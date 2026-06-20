use ricochet_bytecode::{ArgsSpec, Chunk, Op, SourceSpan};
use ricochet_syntax::{
    line_column, line_starts, parse_error_diagnostic, parse_module, ArgsDecl, ClassDecl, Expr,
    Item, MacroDecl, MethodDecl, Module, ParseError, SourceDiagnostic, Span, SpannedExpr,
};
use std::collections::HashMap;
use thiserror::Error;

const MAX_MACRO_EXPANSION_DEPTH: usize = 32;
const MAX_SAME_MACRO_RECURSION: usize = 8;
const MAX_MACRO_EVALUATOR_STEPS: usize = 10_000;
const MAX_GENERATED_AST_NODES: usize = 100_000;

#[derive(Debug, Error, PartialEq)]
pub enum CompileError {
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error("unsupported compiler feature: {feature}")]
    Unsupported {
        feature: String,
        span: Span,
        help: Option<String>,
    },
    #[error("{word} can only be used inside a loop")]
    LoopControlOutsideLoop { word: String, span: Span },
}

#[derive(Debug, Clone, PartialEq)]
pub struct MacroExpansion {
    pub module_id: String,
    pub module: Module,
    pub macro_tables: Vec<MacroTableSummary>,
    pub trace: Vec<MacroExpansionTraceEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MacroTableSummary {
    pub module_id: String,
    pub macros: Vec<MacroSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MacroSummary {
    pub name: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub docs: Vec<String>,
    pub span: Span,
    pub body_span: Option<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroExpansionTraceEntry {
    pub id: String,
    pub module_id: String,
    pub macro_name: String,
    pub invocation_span: Span,
    pub name_span: Span,
    pub definition_span: Span,
    pub depth: usize,
    pub argument_count: usize,
    pub output_node_count: usize,
}

pub fn compile_source(file: &str, source: &str) -> Result<Chunk, CompileError> {
    let expansion = expand_source(file, source)?;
    let mut compiler = Compiler::from_source(file, source);
    compiler.compile_module(&expansion.module)?;
    Ok(compiler.finish())
}

pub fn expand_source(module_id: &str, source: &str) -> Result<MacroExpansion, CompileError> {
    let module = parse_module(source)?;
    expand_module(module_id, &module)
}

pub fn expand_module(module_id: &str, module: &Module) -> Result<MacroExpansion, CompileError> {
    let module_id = normalize_module_id(module_id);
    let mut expander = MacroExpander::new(&module_id, module)?;
    let module = expander.expand_module(module)?;
    Ok(expander.finish(module))
}

pub fn format_compile_error(file: &str, source: &str, error: &CompileError) -> String {
    match error {
        CompileError::Parse(error) => parse_error_diagnostic(file, source, error).render(source),
        CompileError::Unsupported {
            feature,
            span,
            help,
        } => {
            let mut diagnostic = SourceDiagnostic::new(
                file,
                *span,
                format!("unsupported compiler feature: {feature}"),
            );
            if let Some(help) = help {
                diagnostic = diagnostic.with_help(help.clone());
            }
            diagnostic.render(source)
        }
        CompileError::LoopControlOutsideLoop { word, span } => SourceDiagnostic::new(
            file,
            *span,
            format!("{word} can only be used inside a loop"),
        )
        .render(source),
    }
}

#[derive(Clone)]
struct MacroDefinition {
    name: String,
    inputs: Vec<String>,
    outputs: Vec<String>,
    docs: Vec<String>,
    body: Vec<SpannedExpr>,
    span: Span,
}

struct MacroExpander {
    module_id: String,
    macros: HashMap<String, MacroDefinition>,
    generated_ast_nodes: usize,
    trace: Vec<MacroExpansionTraceEntry>,
}

#[derive(Clone)]
struct ExpandedSegment {
    exprs: Vec<SpannedExpr>,
    span: Span,
}

impl ExpandedSegment {
    fn new(exprs: Vec<SpannedExpr>, fallback_span: Span) -> Self {
        let span = exprs_span(&exprs).unwrap_or(fallback_span);
        Self { exprs, span }
    }

    fn single(expr: SpannedExpr) -> Self {
        Self {
            span: expr.span,
            exprs: vec![expr],
        }
    }

    fn as_operand(&self) -> SpannedExpr {
        match self.exprs.as_slice() {
            [expr] => expr.clone(),
            _ => SpannedExpr {
                expr: Expr::Sequence(self.exprs.clone()),
                span: self.span,
            },
        }
    }

    fn string_literal(&self) -> Option<(&str, Span)> {
        match self.exprs.as_slice() {
            [SpannedExpr {
                expr: Expr::String(value),
                span,
            }] => Some((value, *span)),
            _ => None,
        }
    }
}

impl MacroExpander {
    fn new(module_id: &str, module: &Module) -> Result<Self, CompileError> {
        let mut macros = HashMap::new();
        for item in &module.items {
            let Item::Macro(macro_decl) = item else {
                continue;
            };

            if macros.contains_key(&macro_decl.name) {
                return Err(CompileError::Unsupported {
                    feature: format!(
                        "ambiguous compile-time macro {:?}: duplicate local declarations",
                        macro_decl.name
                    ),
                    span: macro_decl.span,
                    help: Some(
                        "keep one local macro declaration for each macro name in this file"
                            .to_string(),
                    ),
                });
            }

            macros.insert(
                macro_decl.name.clone(),
                MacroDefinition::from_decl(macro_decl),
            );
        }

        Ok(Self {
            module_id: module_id.to_string(),
            macros,
            generated_ast_nodes: 0,
            trace: Vec::new(),
        })
    }

    fn finish(self, module: Module) -> MacroExpansion {
        MacroExpansion {
            module_id: self.module_id.clone(),
            module,
            macro_tables: vec![self.macro_table_summary()],
            trace: self.trace,
        }
    }

    fn macro_table_summary(&self) -> MacroTableSummary {
        let mut macros = self
            .macros
            .values()
            .map(|macro_def| MacroSummary {
                name: macro_def.name.clone(),
                inputs: macro_def.inputs.clone(),
                outputs: macro_def.outputs.clone(),
                docs: macro_def.docs.clone(),
                span: macro_def.span,
                body_span: exprs_span(&macro_def.body),
            })
            .collect::<Vec<_>>();
        macros.sort_by(|left, right| left.name.cmp(&right.name));
        MacroTableSummary {
            module_id: self.module_id.clone(),
            macros,
        }
    }

    fn expand_module(&mut self, module: &Module) -> Result<Module, CompileError> {
        let mut items = Vec::new();
        let mut stack = Vec::new();
        for item in &module.items {
            if let Some(item) = self.expand_top_level_item(item, &mut stack)? {
                items.push(item);
            }
        }
        Ok(Module { items })
    }

    fn expand_top_level_item(
        &mut self,
        item: &Item,
        stack: &mut Vec<String>,
    ) -> Result<Option<Item>, CompileError> {
        match item {
            Item::Macro(_) => Ok(None),
            Item::Class(class) => Ok(Some(Item::Class(self.expand_class(class, stack)?))),
            Item::Method(method) => Ok(Some(Item::Method(self.expand_method(method, stack)?))),
            Item::Function(function) => Ok(Some(Item::Function(ricochet_syntax::FunctionDecl {
                name: function.name.clone(),
                args: function.args.clone(),
                body: self.expand_exprs(&function.body, stack, 0)?,
                docs: function.docs.clone(),
                span: function.span,
            }))),
            Item::Expr { expr, span, docs } => Ok(Some(Item::Expr {
                expr: self.expand_expr(expr, stack, 0)?,
                span: *span,
                docs: docs.clone(),
            })),
        }
    }

    fn expand_class(
        &mut self,
        class: &ClassDecl,
        stack: &mut Vec<String>,
    ) -> Result<ClassDecl, CompileError> {
        let mut body = Vec::new();
        for item in &class.body {
            body.push(self.expand_class_body_item(item, stack)?);
        }
        Ok(ClassDecl {
            name: class.name.clone(),
            superclass: class.superclass.clone(),
            body,
            docs: class.docs.clone(),
            span: class.span,
        })
    }

    fn expand_class_body_item(
        &mut self,
        item: &Item,
        stack: &mut Vec<String>,
    ) -> Result<Item, CompileError> {
        match item {
            Item::Macro(macro_decl) => Err(CompileError::Unsupported {
                feature: "macro declarations are only supported at top level in this local macro slice"
                    .to_string(),
                span: macro_decl.span,
                help: Some(
                    "move the macro declaration to the top level of the file and invoke it from the class body"
                        .to_string(),
                ),
            }),
            Item::Class(class) => Ok(Item::Class(self.expand_class(class, stack)?)),
            Item::Method(method) => Ok(Item::Method(self.expand_method(method, stack)?)),
            Item::Function(function) => Ok(Item::Function(ricochet_syntax::FunctionDecl {
                name: function.name.clone(),
                args: function.args.clone(),
                body: self.expand_exprs(&function.body, stack, 0)?,
                docs: function.docs.clone(),
                span: function.span,
            })),
            Item::Expr { expr, span, docs } => Ok(Item::Expr {
                expr: self.expand_expr(expr, stack, 0)?,
                span: *span,
                docs: docs.clone(),
            }),
        }
    }

    fn expand_method(
        &mut self,
        method: &MethodDecl,
        stack: &mut Vec<String>,
    ) -> Result<MethodDecl, CompileError> {
        Ok(MethodDecl {
            name: method.name.clone(),
            args: method.args.clone(),
            body: self.expand_exprs(&method.body, stack, 0)?,
            docs: method.docs.clone(),
            span: method.span,
        })
    }

    fn expand_expr(
        &mut self,
        expr: &Expr,
        stack: &mut Vec<String>,
        depth: usize,
    ) -> Result<Expr, CompileError> {
        match expr {
            Expr::Block(body) => Ok(Expr::Block(self.expand_exprs(body, stack, depth)?)),
            Expr::Sequence(exprs) => Ok(Expr::Sequence(self.expand_exprs(exprs, stack, depth)?)),
            Expr::If {
                then_body,
                else_body,
            } => Ok(Expr::If {
                then_body: self.expand_exprs(then_body, stack, depth)?,
                else_body: self.expand_exprs(else_body, stack, depth)?,
            }),
            Expr::While { condition, body } => Ok(Expr::While {
                condition: self.expand_exprs(condition, stack, depth)?,
                body: self.expand_exprs(body, stack, depth)?,
            }),
            _ => Ok(expr.clone()),
        }
    }

    fn expand_spanned_expr(
        &mut self,
        expr: &SpannedExpr,
        stack: &mut Vec<String>,
        depth: usize,
    ) -> Result<SpannedExpr, CompileError> {
        Ok(SpannedExpr {
            expr: self.expand_expr(&expr.expr, stack, depth)?,
            span: expr.span,
        })
    }

    fn expand_exprs(
        &mut self,
        exprs: &[SpannedExpr],
        stack: &mut Vec<String>,
        depth: usize,
    ) -> Result<Vec<SpannedExpr>, CompileError> {
        let mut output = Vec::<ExpandedSegment>::new();

        for expr in exprs {
            if matches!(&expr.expr, Expr::Symbol(word) if word == "macro_call") {
                self.expand_macro_call(expr.span, &mut output, stack, depth)?;
            } else {
                output.push(ExpandedSegment::single(
                    self.expand_spanned_expr(expr, stack, depth)?,
                ));
            }
        }

        Ok(flatten_segments(output))
    }

    fn expand_macro_call(
        &mut self,
        call_span: Span,
        output: &mut Vec<ExpandedSegment>,
        stack: &mut Vec<String>,
        depth: usize,
    ) -> Result<(), CompileError> {
        let Some(name_segment) = output.last() else {
            return Err(CompileError::Unsupported {
                feature: "macro_call requires a literal macro name immediately before it"
                    .to_string(),
                span: call_span,
                help: Some("use MacroOperand* \"name\" macro_call".to_string()),
            });
        };

        let Some((name, name_span)) = name_segment.string_literal() else {
            return Err(CompileError::Unsupported {
                feature: "nonliteral compile-time macro names are not supported".to_string(),
                span: name_segment.span,
                help: Some("use a string literal macro name before macro_call".to_string()),
            });
        };

        let Some(macro_def) = self.macros.get(name).cloned() else {
            return Err(CompileError::Unsupported {
                feature: format!("unknown compile-time macro {name:?}"),
                span: name_span,
                help: Some(
                    "declare a local top-level macro with \"name\" Macro before invoking macro_call"
                        .to_string(),
                ),
            });
        };

        if depth >= MAX_MACRO_EXPANSION_DEPTH {
            return Err(CompileError::Unsupported {
                feature: format!(
                    "macro expansion depth limit {MAX_MACRO_EXPANSION_DEPTH} exceeded while expanding {:?}",
                    macro_def.name
                ),
                span: name_span,
                help: Some("shorten the macro expansion chain or remove recursive expansion".to_string()),
            });
        }

        let same_macro_depth = stack
            .iter()
            .filter(|macro_name| macro_name.as_str() == macro_def.name)
            .count();
        if same_macro_depth >= MAX_SAME_MACRO_RECURSION {
            return Err(CompileError::Unsupported {
                feature: format!(
                    "same-macro recursion limit {MAX_SAME_MACRO_RECURSION} exceeded while expanding {:?}",
                    macro_def.name
                ),
                span: name_span,
                help: Some("make the macro expansion terminate before recursively invoking itself".to_string()),
            });
        }

        let arg_count = macro_def.inputs.len();
        if output.len() < arg_count + 1 {
            return Err(CompileError::Unsupported {
                feature: format!(
                    "compile-time macro {:?} expected {arg_count} operand(s), but invocation has fewer",
                    macro_def.name
                ),
                span: name_span,
                help: Some("place the macro operands immediately before the literal macro name".to_string()),
            });
        }

        let operand_start = output.len() - 1 - arg_count;
        let operands = output[operand_start..output.len() - 1]
            .iter()
            .map(ExpandedSegment::as_operand)
            .collect::<Vec<_>>();
        output.truncate(operand_start);

        let mut bindings = HashMap::new();
        for (name, operand) in macro_def.inputs.iter().zip(operands) {
            bindings.insert(name.clone(), operand);
        }

        stack.push(macro_def.name.clone());
        let mut evaluator = MacroEvaluator::new(&bindings);
        let expansion = evaluator.evaluate(&macro_def)?;
        self.record_generated_nodes(&expansion, name_span)?;
        let expansion = self.expand_exprs(&expansion, stack, depth + 1)?;
        let output_node_count = count_spanned_exprs(&expansion);
        self.trace.push(MacroExpansionTraceEntry {
            id: trace_id(self.trace.len(), &macro_def.name, call_span),
            module_id: self.module_id.clone(),
            macro_name: macro_def.name.clone(),
            invocation_span: call_span,
            name_span,
            definition_span: macro_def.span,
            depth,
            argument_count: arg_count,
            output_node_count,
        });
        stack.pop();

        output.push(ExpandedSegment::new(expansion, name_span));
        Ok(())
    }

    fn record_generated_nodes(
        &mut self,
        exprs: &[SpannedExpr],
        span: Span,
    ) -> Result<(), CompileError> {
        self.generated_ast_nodes += count_spanned_exprs(exprs);
        if self.generated_ast_nodes > MAX_GENERATED_AST_NODES {
            return Err(CompileError::Unsupported {
                feature: format!(
                    "generated macro AST node limit {MAX_GENERATED_AST_NODES} exceeded"
                ),
                span,
                help: Some("reduce macro expansion output for this source file".to_string()),
            });
        }
        Ok(())
    }
}

impl MacroDefinition {
    fn from_decl(macro_decl: &MacroDecl) -> Self {
        Self {
            name: macro_decl.name.clone(),
            inputs: macro_decl
                .args
                .as_ref()
                .map(|args| args.inputs.clone())
                .unwrap_or_default(),
            outputs: macro_decl
                .args
                .as_ref()
                .map(|args| args.outputs.clone())
                .unwrap_or_default(),
            docs: macro_decl.docs.clone(),
            body: macro_decl.body.clone(),
            span: macro_decl.span,
        }
    }
}

#[derive(Clone)]
enum MacroValue {
    Ast(SpannedExpr),
    AstList(Vec<SpannedExpr>),
    QuotedBlock(Vec<SpannedExpr>),
    String,
    Number,
    Float,
    Bool,
    Nil,
}

struct MacroEvaluator<'a> {
    bindings: &'a HashMap<String, SpannedExpr>,
    stack: Vec<MacroValue>,
    steps: usize,
}

impl<'a> MacroEvaluator<'a> {
    fn new(bindings: &'a HashMap<String, SpannedExpr>) -> Self {
        Self {
            bindings,
            stack: Vec::new(),
            steps: 0,
        }
    }

    fn evaluate(&mut self, macro_def: &MacroDefinition) -> Result<Vec<SpannedExpr>, CompileError> {
        self.eval_exprs(&macro_def.body, macro_def.span)?;
        let Some(value) = self.stack.pop() else {
            return Err(CompileError::Unsupported {
                feature: format!(
                    "compile-time macro {:?} did not produce quoted AST",
                    macro_def.name
                ),
                span: macro_def.span,
                help: Some(
                    "end the macro body with a quoted block followed by quote_ast".to_string(),
                ),
            });
        };
        if !self.stack.is_empty() {
            return Err(CompileError::Unsupported {
                feature: format!(
                    "compile-time macro {:?} left extra values on the evaluator stack",
                    macro_def.name
                ),
                span: macro_def.span,
                help: Some("return exactly one quoted AST value from the macro body".to_string()),
            });
        }

        match value {
            MacroValue::AstList(exprs) => Ok(exprs),
            MacroValue::Ast(expr) => Ok(vec![expr]),
            _ => Err(CompileError::Unsupported {
                feature: format!(
                    "compile-time macro {:?} returned a scalar instead of quoted AST",
                    macro_def.name
                ),
                span: macro_def.span,
                help: Some("use quote_ast to turn a quoted block into expansion AST".to_string()),
            }),
        }
    }

    fn eval_exprs(
        &mut self,
        exprs: &[SpannedExpr],
        default_span: Span,
    ) -> Result<(), CompileError> {
        for expr in exprs {
            self.eval_spanned_expr(expr, default_span)?;
        }
        Ok(())
    }

    fn eval_spanned_expr(
        &mut self,
        expr: &SpannedExpr,
        default_span: Span,
    ) -> Result<(), CompileError> {
        self.bump(expr.span)?;
        match &expr.expr {
            Expr::Sequence(exprs) => self.eval_exprs(exprs, expr.span),
            Expr::Block(body) => {
                self.stack.push(MacroValue::QuotedBlock(body.clone()));
                Ok(())
            }
            Expr::Reference(name) => {
                let Some(value) = self.bindings.get(name) else {
                    return Err(CompileError::Unsupported {
                        feature: format!("unknown compile-time macro argument ${name}"),
                        span: expr.span,
                        help: Some(
                            "macro bodies can read only declared macro arguments in this slice"
                                .to_string(),
                        ),
                    });
                };
                self.stack.push(MacroValue::Ast(value.clone()));
                Ok(())
            }
            Expr::String(_) => {
                self.stack.push(MacroValue::String);
                Ok(())
            }
            Expr::Number(_) => {
                self.stack.push(MacroValue::Number);
                Ok(())
            }
            Expr::Float(_) => {
                self.stack.push(MacroValue::Float);
                Ok(())
            }
            Expr::Symbol(word) => self.eval_symbol(word, expr.span),
            Expr::BangWord(word) => self.fail_unallowlisted(word, expr.span),
            Expr::DotWord(word) => self.fail_unallowlisted(word, expr.span),
            Expr::Args(_) => Err(CompileError::Unsupported {
                feature: "argument declarations are not supported inside compile-time macro bodies"
                    .to_string(),
                span: expr.span,
                help: None,
            }),
            Expr::If { .. } | Expr::While { .. } => Err(CompileError::Unsupported {
                feature:
                    "control flow is not supported inside compile-time macro bodies in this slice"
                        .to_string(),
                span: if expr.span.start == expr.span.end {
                    default_span
                } else {
                    expr.span
                },
                help: Some("use quote_ast to emit runtime control flow instead".to_string()),
            }),
        }
    }

    fn eval_symbol(&mut self, word: &str, span: Span) -> Result<(), CompileError> {
        match word {
            "quote_ast" => self.eval_quote_ast(span),
            "true" | "false" => {
                self.stack.push(MacroValue::Bool);
                Ok(())
            }
            "nil" => {
                self.stack.push(MacroValue::Nil);
                Ok(())
            }
            _ => self.fail_unallowlisted(word, span),
        }
    }

    fn eval_quote_ast(&mut self, span: Span) -> Result<(), CompileError> {
        let Some(value) = self.stack.pop() else {
            return Err(CompileError::Unsupported {
                feature: "quote_ast requires a quoted block".to_string(),
                span,
                help: Some("place a block literal immediately before quote_ast".to_string()),
            });
        };
        let MacroValue::QuotedBlock(body) = value else {
            return Err(CompileError::Unsupported {
                feature: "quote_ast can only convert quoted block literals".to_string(),
                span,
                help: Some("place a block literal immediately before quote_ast".to_string()),
            });
        };

        let quoted = self.quote_ast_body(&body)?;
        self.stack
            .push(MacroValue::AstList(flatten_quoted_block(quoted)));
        Ok(())
    }

    fn quote_ast_body(&mut self, body: &[SpannedExpr]) -> Result<Vec<SpannedExpr>, CompileError> {
        self.quote_ast_exprs(body)
    }

    fn quote_ast_exprs(&mut self, exprs: &[SpannedExpr]) -> Result<Vec<SpannedExpr>, CompileError> {
        let mut output = Vec::new();
        let mut index = 0;

        while index < exprs.len() {
            if let Some(next) = exprs.get(index + 1) {
                if matches!(&next.expr, Expr::Symbol(word) if word == "ast_splice") {
                    let Expr::Reference(name) = &exprs[index].expr else {
                        return Err(CompileError::Unsupported {
                            feature: "ast_splice requires a macro argument reference immediately before it"
                                .to_string(),
                            span: next.span,
                            help: Some("use $arg ast_splice inside quote_ast output".to_string()),
                        });
                    };
                    let Some(value) = self.bindings.get(name) else {
                        return Err(CompileError::Unsupported {
                            feature: format!("unknown compile-time macro argument ${name}"),
                            span: exprs[index].span,
                            help: Some(
                                "declare the argument in the macro Args input list before splicing it"
                                    .to_string(),
                            ),
                        });
                    };
                    self.bump(next.span)?;
                    output.push(value.clone());
                    index += 2;
                    continue;
                }
            }

            output.push(self.quote_ast_expr(&exprs[index])?);
            index += 1;
        }

        Ok(output)
    }

    fn quote_ast_expr(&mut self, expr: &SpannedExpr) -> Result<SpannedExpr, CompileError> {
        self.bump(expr.span)?;
        let quoted = match &expr.expr {
            Expr::Block(body) => Expr::Block(self.quote_ast_body(body)?),
            Expr::Sequence(exprs) => Expr::Sequence(self.quote_ast_exprs(exprs)?),
            Expr::If {
                then_body,
                else_body,
            } => Expr::If {
                then_body: self.quote_ast_body(then_body)?,
                else_body: self.quote_ast_body(else_body)?,
            },
            Expr::While { condition, body } => Expr::While {
                condition: self.quote_ast_body(condition)?,
                body: self.quote_ast_body(body)?,
            },
            Expr::Symbol(word) if word == "ast_splice" => {
                return Err(CompileError::Unsupported {
                    feature: "ast_splice requires a macro argument reference immediately before it"
                        .to_string(),
                    span: expr.span,
                    help: Some("use $arg ast_splice inside quote_ast output".to_string()),
                });
            }
            _ => expr.expr.clone(),
        };
        Ok(SpannedExpr {
            expr: quoted,
            span: expr.span,
        })
    }

    fn bump(&mut self, span: Span) -> Result<(), CompileError> {
        self.steps += 1;
        if self.steps > MAX_MACRO_EVALUATOR_STEPS {
            return Err(CompileError::Unsupported {
                feature: format!("macro evaluator step limit {MAX_MACRO_EVALUATOR_STEPS} exceeded"),
                span,
                help: Some("simplify the compile-time macro body".to_string()),
            });
        }
        Ok(())
    }

    fn fail_unallowlisted(&self, word: &str, span: Span) -> Result<(), CompileError> {
        Err(CompileError::Unsupported {
            feature: format!("compile-time macro body used unallowlisted word {word:?}"),
            span,
            help: Some(
                "macro bodies are fail-closed and cannot call runtime words or host capabilities"
                    .to_string(),
            ),
        })
    }
}

fn flatten_quoted_block(mut exprs: Vec<SpannedExpr>) -> Vec<SpannedExpr> {
    if exprs.len() == 1 {
        if let Expr::Sequence(sequence) = exprs.remove(0).expr {
            return sequence;
        }
    }
    exprs
}

fn flatten_segments(segments: Vec<ExpandedSegment>) -> Vec<SpannedExpr> {
    segments
        .into_iter()
        .flat_map(|segment| segment.exprs)
        .collect()
}

fn exprs_span(exprs: &[SpannedExpr]) -> Option<Span> {
    Some(Span {
        start: exprs.first()?.span.start,
        end: exprs.last()?.span.end,
    })
}

fn count_spanned_exprs(exprs: &[SpannedExpr]) -> usize {
    exprs.iter().map(count_spanned_expr).sum()
}

fn count_spanned_expr(expr: &SpannedExpr) -> usize {
    1 + match &expr.expr {
        Expr::Block(body) | Expr::Sequence(body) => count_spanned_exprs(body),
        Expr::If {
            then_body,
            else_body,
        } => count_spanned_exprs(then_body) + count_spanned_exprs(else_body),
        Expr::While { condition, body } => {
            count_spanned_exprs(condition) + count_spanned_exprs(body)
        }
        _ => 0,
    }
}

fn normalize_module_id(module_id: &str) -> String {
    let module_id = module_id.replace('\\', "/");
    if module_id.is_empty() {
        "<source>".to_string()
    } else {
        module_id
    }
}

fn trace_id(index: usize, macro_name: &str, span: Span) -> String {
    let safe_name = macro_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("local:{index}:{safe_name}:{}-{}", span.start, span.end)
}

struct Compiler {
    chunk: Chunk,
    line_starts: Vec<usize>,
    default_span: Span,
    loop_stack: Vec<LoopContext>,
}

struct LoopContext {
    continue_target: usize,
    break_jumps: Vec<usize>,
}

impl Compiler {
    fn new(file: impl Into<String>) -> Self {
        Self {
            chunk: Chunk::new(file),
            line_starts: vec![0],
            default_span: Span { start: 0, end: 0 },
            loop_stack: Vec::new(),
        }
    }

    fn from_source(file: impl Into<String>, source: &str) -> Self {
        let mut compiler = Self::new(file);
        compiler.line_starts = line_starts(source);
        compiler
    }

    fn finish(self) -> Chunk {
        self.chunk
    }

    fn compile_module(&mut self, module: &Module) -> Result<(), CompileError> {
        for item in &module.items {
            self.compile_item(item)?;
        }
        Ok(())
    }

    fn compile_item(&mut self, item: &Item) -> Result<(), CompileError> {
        match item {
            Item::Class(class) => self.compile_class(class),
            Item::Expr { expr, span, .. } => self.compile_expr_item(expr, *span),
            Item::Function(function) => self.compile_function_decl(function),
            Item::Macro(macro_decl) => Err(CompileError::Unsupported {
                feature: format!(
                    "compile-time macros are not implemented yet: macro declaration {:?}",
                    macro_decl.name
                ),
                span: macro_decl.span,
                help: Some(
                    "macro declarations parse for editor support, but expansion and lowering will land in a later compiler slice"
                        .to_string(),
                ),
            }),
            Item::Method(method) => Err(CompileError::Unsupported {
                feature: format!("top-level method declaration {}", method.name),
                span: method.span,
                help: Some(
                    "methods must be declared inside a class body with postfix Method syntax"
                        .to_string(),
                ),
            }),
        }
    }

    fn compile_class(&mut self, class: &ClassDecl) -> Result<(), CompileError> {
        let span = self.source_span(class.span);
        self.chunk.push(
            Op::BeginClass {
                name: class.name.clone(),
                superclass: class.superclass.clone(),
            },
            span.clone(),
        );

        for item in &class.body {
            self.compile_class_body_item(item)?;
        }

        self.chunk.push(Op::EndClass, span);
        Ok(())
    }

    fn compile_function_decl(
        &mut self,
        function: &ricochet_syntax::FunctionDecl,
    ) -> Result<(), CompileError> {
        let block = self.compile_block_chunk(&function.body, function.span)?;
        let block = self.chunk.push_block(block);
        self.chunk.push(
            Op::AddFunction {
                name: function.name.clone(),
                block,
                args: function.args.as_ref().map(args_spec),
            },
            self.source_span(function.span),
        );
        Ok(())
    }

    fn compile_class_body_item(&mut self, item: &Item) -> Result<(), CompileError> {
        match item {
            Item::Method(method) => self.compile_method_decl(method),
            Item::Expr {
                expr: Expr::Sequence(exprs),
                ..
            } if table_declaration(exprs).is_some() => {
                let name = table_declaration(exprs).expect("table declaration checked");
                self.push_at(Op::PushString(name), exprs[0].span);
                self.push_at(Op::CallWord("Table".to_string()), exprs[1].span);
                Ok(())
            }
            Item::Expr {
                expr: Expr::Sequence(exprs),
                span,
                ..
            } if is_field_declaration(exprs) => {
                let name = declaration_name(&exprs[0]).expect("field declaration checked");
                self.chunk.push(Op::AddField(name), self.source_span(*span));
                Ok(())
            }
            Item::Expr {
                expr: Expr::Sequence(exprs),
                span,
                ..
            } if is_accessor_declaration(exprs) => {
                let name = declaration_name(&exprs[0]).expect("accessor declaration checked");
                self.chunk
                    .push(Op::AddAccessor(name), self.source_span(*span));
                Ok(())
            }
            Item::Expr {
                expr: Expr::Sequence(exprs),
                span,
                ..
            } if block_method_declaration(exprs).is_some() => {
                let (args, name, body) =
                    block_method_declaration(exprs).expect("method declaration checked");
                let block = self.compile_block_chunk(body, *span)?;
                let block = self.chunk.push_block(block);
                self.chunk.push(
                    Op::AddMethod {
                        name,
                        block,
                        args: args.map(args_spec),
                    },
                    self.source_span(*span),
                );
                Ok(())
            }
            Item::Expr { expr, span, .. } => self.compile_expr_item(expr, *span),
            Item::Class(class) => Err(CompileError::Unsupported {
                feature: format!("nested class declaration {}", class.name),
                span: class.span,
                help: Some("move nested classes to the top level".to_string()),
            }),
            Item::Function(function) => Err(CompileError::Unsupported {
                feature: format!("function declaration {}", function.name),
                span: function.span,
                help: Some("move function declarations to the top level".to_string()),
            }),
            Item::Macro(macro_decl) => Err(CompileError::Unsupported {
                feature: format!(
                    "compile-time macros are not implemented yet: macro declaration {:?}",
                    macro_decl.name
                ),
                span: macro_decl.span,
                help: Some(
                    "macro declarations inside class bodies are parsed for editor support, but macro expansion is not available yet"
                        .to_string(),
                ),
            }),
        }
    }

    fn compile_method_decl(&mut self, method: &MethodDecl) -> Result<(), CompileError> {
        let block = self.compile_block_chunk(&method.body, method.span)?;
        let block = self.chunk.push_block(block);
        self.chunk.push(
            Op::AddMethod {
                name: method.name.clone(),
                block,
                args: method.args.as_ref().map(args_spec),
            },
            self.source_span(method.span),
        );
        Ok(())
    }

    fn compile_expr_item(&mut self, expr: &Expr, span: Span) -> Result<(), CompileError> {
        let previous = self.default_span;
        self.default_span = span;
        let result = self.compile_expr(expr);
        self.default_span = previous;
        result
    }

    fn compile_expr(&mut self, expr: &Expr) -> Result<(), CompileError> {
        match expr {
            Expr::Symbol(word) => self.compile_symbol(word),
            Expr::BangWord(word) => self.push(Op::CallWord(word.clone())),
            Expr::DotWord(word) => Err(CompileError::Unsupported {
                feature: format!("leading-dot method syntax {word:?}"),
                span: self.default_span,
                help: Some(
                    "use postfix selectors, for example: user email.get or http_request"
                        .to_string(),
                ),
            }),
            Expr::Reference(name) => {
                self.push(Op::PushString(name.clone()))?;
                self.push(Op::CallWord("get".to_string()))
            }
            Expr::String(value) => self.push(Op::PushString(value.clone())),
            Expr::Number(value) => self.push(Op::PushNumber(*value)),
            Expr::Float(value) => self.push(Op::PushFloat(*value)),
            Expr::Block(body) => {
                let block = self.compile_block_chunk(body, self.default_span)?;
                let block = self.chunk.push_block(block);
                self.push(Op::PushBlock(block))
            }
            Expr::Sequence(exprs) => self.compile_exprs(exprs),
            Expr::Args(_) => Err(CompileError::Unsupported {
                feature: "argument declarations".to_string(),
                span: self.default_span,
                help: Some("remove the signature and pop arguments from the stack".to_string()),
            }),
            Expr::If {
                then_body,
                else_body,
            } => self.compile_if(then_body, else_body),
            Expr::While { condition, body } => self.compile_while(condition, body),
        }
    }

    fn compile_spanned_expr(&mut self, expression: &SpannedExpr) -> Result<(), CompileError> {
        let previous = self.default_span;
        self.default_span = expression.span;
        let result = self.compile_expr(&expression.expr);
        self.default_span = previous;
        result
    }

    fn compile_if(
        &mut self,
        then_body: &[SpannedExpr],
        else_body: &[SpannedExpr],
    ) -> Result<(), CompileError> {
        let jump_if_false = self.chunk.instructions.len();
        self.push(Op::JumpIfFalse(usize::MAX))?;

        self.compile_exprs(then_body)?;

        let jump_over_else = self.chunk.instructions.len();
        self.push(Op::Jump(usize::MAX))?;

        let else_start = self.chunk.instructions.len();
        self.compile_exprs(else_body)?;

        let end = self.chunk.instructions.len();
        self.chunk.instructions[jump_if_false].op = Op::JumpIfFalse(else_start);
        self.chunk.instructions[jump_over_else].op = Op::Jump(end);

        Ok(())
    }

    fn compile_while(
        &mut self,
        condition: &[SpannedExpr],
        body: &[SpannedExpr],
    ) -> Result<(), CompileError> {
        let condition_start = self.chunk.instructions.len();
        self.compile_exprs(condition)?;

        let exit_jump = self.chunk.instructions.len();
        self.push(Op::JumpIfFalse(usize::MAX))?;

        self.loop_stack.push(LoopContext {
            continue_target: condition_start,
            break_jumps: Vec::new(),
        });
        let body_result = self.compile_exprs(body);
        let loop_context = self
            .loop_stack
            .pop()
            .expect("loop context is present while compiling loop body");
        body_result?;

        self.push(Op::Jump(condition_start))?;
        let loop_end = self.chunk.instructions.len();
        self.chunk.instructions[exit_jump].op = Op::JumpIfFalse(loop_end);
        for break_jump in loop_context.break_jumps {
            self.chunk.instructions[break_jump].op = Op::Jump(loop_end);
        }

        Ok(())
    }

    fn compile_exprs(&mut self, exprs: &[SpannedExpr]) -> Result<(), CompileError> {
        let mut index = 0;
        while index < exprs.len() {
            if let Some((name, operator)) = variable_binding_pair(exprs, index) {
                self.push_at(Op::PushString(name), exprs[index].span);
                self.push_at(Op::CallWord(operator), exprs[index + 1].span);
                index += 2;
            } else {
                self.compile_spanned_expr(&exprs[index])?;
                index += 1;
            }
        }

        Ok(())
    }

    fn compile_symbol(&mut self, word: &str) -> Result<(), CompileError> {
        match word {
            "nil" => self.push(Op::PushNil),
            "true" => self.push(Op::PushBool(true)),
            "false" => self.push(Op::PushBool(false)),
            "return" => self.push(Op::Return),
            "break" => {
                if self.loop_stack.is_empty() {
                    return Err(CompileError::LoopControlOutsideLoop {
                        word: word.to_string(),
                        span: self.default_span,
                    });
                }
                let jump = self.chunk.instructions.len();
                self.push(Op::Jump(usize::MAX))?;
                self.loop_stack
                    .last_mut()
                    .expect("loop context checked before break")
                    .break_jumps
                    .push(jump);
                Ok(())
            }
            "continue" => {
                let target = self
                    .loop_stack
                    .last()
                    .map(|context| context.continue_target)
                    .ok_or_else(|| CompileError::LoopControlOutsideLoop {
                        word: word.to_string(),
                        span: self.default_span,
                    })?;
                self.push(Op::Jump(target))
            }
            word => self.push(Op::CallWord(word.to_string())),
        }
    }

    fn compile_block_chunk(
        &self,
        exprs: &[SpannedExpr],
        default_span: Span,
    ) -> Result<Chunk, CompileError> {
        let mut compiler = Compiler {
            chunk: Chunk::new(self.chunk.file.clone()),
            line_starts: self.line_starts.clone(),
            default_span,
            loop_stack: Vec::new(),
        };

        compiler.compile_exprs(exprs)?;

        compiler.chunk.push(Op::Return, compiler.default_span());
        Ok(compiler.finish())
    }

    fn push(&mut self, op: Op) -> Result<(), CompileError> {
        self.chunk.push(op, self.default_span());
        Ok(())
    }

    fn push_at(&mut self, op: Op, span: Span) {
        self.chunk.push(op, self.source_span(span));
    }

    fn default_span(&self) -> SourceSpan {
        self.source_span(self.default_span)
    }

    fn source_span(&self, span: Span) -> SourceSpan {
        let (line, column) = line_column(&self.line_starts, span.start);
        SourceSpan {
            file: self.chunk.file.clone(),
            start: span.start,
            end: span.end,
            line,
            column,
        }
    }
}

fn is_field_declaration(exprs: &[SpannedExpr]) -> bool {
    exprs.len() == 2
        && declaration_name(&exprs[0]).is_some()
        && matches!(&exprs[1].expr, Expr::Symbol(word) if word == "Field")
}

fn is_accessor_declaration(exprs: &[SpannedExpr]) -> bool {
    exprs.len() == 2
        && declaration_name(&exprs[0]).is_some()
        && matches!(&exprs[1].expr, Expr::Symbol(word) if word == "Accessor")
}

fn table_declaration(exprs: &[SpannedExpr]) -> Option<String> {
    match exprs {
        [name, operator] if matches!(&operator.expr, Expr::Symbol(word) if word == "Table") => {
            declaration_name(name)
        }
        _ => None,
    }
}

fn block_method_declaration(
    exprs: &[SpannedExpr],
) -> Option<(Option<&ArgsDecl>, String, &[SpannedExpr])> {
    match exprs {
        [block, name, operator] => match (&block.expr, &operator.expr) {
            (Expr::Block(body), Expr::Symbol(word)) if word == "Method" => {
                Some((None, declaration_name(name)?, body.as_slice()))
            }
            _ => None,
        },
        [args, block, name, operator] => match (&args.expr, &block.expr, &operator.expr) {
            (Expr::Args(args), Expr::Block(body), Expr::Symbol(word)) if word == "Method" => {
                Some((Some(args), declaration_name(name)?, body.as_slice()))
            }
            _ => None,
        },
        _ => None,
    }
}

fn declaration_name(expression: &SpannedExpr) -> Option<String> {
    match &expression.expr {
        Expr::Symbol(name) | Expr::String(name) => Some(name.clone()),
        _ => None,
    }
}

fn variable_binding_pair(exprs: &[SpannedExpr], index: usize) -> Option<(String, String)> {
    let Expr::Symbol(name) = &exprs.get(index)?.expr else {
        return None;
    };
    let Expr::Symbol(operator) = &exprs.get(index + 1)?.expr else {
        return None;
    };

    matches!(
        operator.as_str(),
        "get" | "set" | "var" | "array" | "list" | "map" | "Array" | "List" | "Map" | "Set"
    )
    .then(|| (name.clone(), operator.clone()))
}

fn args_spec(args: &ArgsDecl) -> ArgsSpec {
    ArgsSpec {
        inputs: args.inputs.clone(),
        outputs: args.outputs.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ricochet_bytecode::{ArgsSpec, Op};

    fn spanned(expr: Expr) -> SpannedExpr {
        SpannedExpr {
            expr,
            span: Span { start: 0, end: 0 },
        }
    }

    #[test]
    fn compiles_ordinary_postfix_sequence() {
        let chunk = compile_source("test.rco", "2 3 +").expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushNumber(2),
                Op::PushNumber(3),
                Op::CallWord("+".to_string())
            ]
        );
    }

    #[test]
    fn compiles_negative_number_literals() {
        let chunk = compile_source("test.rco", "-1 2 +").expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushNumber(-1),
                Op::PushNumber(2),
                Op::CallWord("+".to_string())
            ]
        );
    }

    #[test]
    fn compiles_float_literals() {
        let chunk = compile_source("test.rco", "1.5 2 +").expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushFloat(1.5),
                Op::PushNumber(2),
                Op::CallWord("+".to_string())
            ]
        );
    }

    #[test]
    fn formats_parse_errors_with_source_context() {
        let source = "User Model Subclass\n  \"email\" Accessor\n";
        let error = compile_source("User.rco", source).expect_err("missing end should fail");
        let rendered = format_compile_error("User.rco", source, &error);

        assert!(rendered.contains("error: expected end, found end of file"));
        assert!(rendered.contains("--> User.rco:3:1"));
        assert!(rendered.contains("| ^"));
    }

    #[test]
    fn top_level_expression_debug_spans_follow_source_lines() {
        let chunk = compile_source("test.rco", "2\n3\n+\n").expect("compile succeeds");

        assert_eq!(
            chunk.debug().map(|span| span.line).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn compiles_class_fields_and_block_method_mutations() {
        let source = r#"
          User Model Subclass
            "users" Table
            "email" Accessor
            [ self email.get ] "displayName" Method
          end
        "#;

        let chunk = compile_source("models/user.rco", source).expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::BeginClass {
                    name: "User".to_string(),
                    superclass: "Model".to_string(),
                },
                Op::PushString("users".to_string()),
                Op::CallWord("Table".to_string()),
                Op::AddAccessor("email".to_string()),
                Op::AddMethod {
                    name: "displayName".to_string(),
                    block: 0,
                    args: None,
                },
                Op::EndClass,
            ]
        );
        assert_eq!(chunk.blocks.len(), 1);
        assert_eq!(
            chunk.blocks[0].ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::CallWord("self".to_string()),
                Op::CallWord("email.get".to_string()),
                Op::Return,
            ]
        );
    }

    #[test]
    fn compiles_class_methods_with_args_metadata() {
        let source = r#"
          Transfer Service Subclass
            ( amount target -> Result ) [
              amount target send
            ] "transfer" Method
          end
        "#;

        let chunk = compile_source("services/transfer.rco", source).expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::BeginClass {
                    name: "Transfer".to_string(),
                    superclass: "Service".to_string(),
                },
                Op::AddMethod {
                    name: "transfer".to_string(),
                    block: 0,
                    args: Some(ArgsSpec {
                        inputs: vec!["amount".to_string(), "target".to_string()],
                        outputs: vec!["Result".to_string()],
                    }),
                },
                Op::EndClass,
            ]
        );
    }

    #[test]
    fn compiles_block_method_declarations_with_args_metadata() {
        let source = r#"
          HomeController Controller Subclass
            ( id ctx ) [
              nil title var
              ctx var
              id var
              id get title set
              ctx get
              "home/show" swap view
            ] "show" Method
          end
        "#;

        let chunk = compile_source("controllers/home.rco", source).expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::BeginClass {
                    name: "HomeController".to_string(),
                    superclass: "Controller".to_string(),
                },
                Op::AddMethod {
                    name: "show".to_string(),
                    block: 0,
                    args: Some(ArgsSpec {
                        inputs: vec!["id".to_string(), "ctx".to_string()],
                        outputs: Vec::new(),
                    }),
                },
                Op::EndClass,
            ]
        );
    }

    #[test]
    fn compiles_class_method_declarations_to_add_method_blocks() {
        let source = r#"
          User Model Subclass
            [ self email.get ] "displayName" Method
          end
        "#;

        let chunk = compile_source("models/user.rco", source).expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::BeginClass {
                    name: "User".to_string(),
                    superclass: "Model".to_string(),
                },
                Op::AddMethod {
                    name: "displayName".to_string(),
                    block: 0,
                    args: None,
                },
                Op::EndClass,
            ]
        );
        assert_eq!(
            chunk.blocks[0].ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::CallWord("self".to_string()),
                Op::CallWord("email.get".to_string()),
                Op::Return,
            ]
        );
    }

    #[test]
    fn method_block_debug_spans_follow_each_expression_line() {
        let source = r#"
          User Model Subclass
            [
              self
              email.get
            ] "displayName" Method
          end
        "#;

        let chunk = compile_source("models/user.rco", source).expect("compile succeeds");

        assert_eq!(
            chunk
                .blocks
                .first()
                .expect("method block should be present")
                .debug()
                .map(|span| span.line)
                .collect::<Vec<_>>(),
            vec![4, 5, 3]
        );
    }

    #[test]
    fn compiles_variable_names_before_get_set_and_var_as_strings() {
        let chunk = compile_source(
            "controllers/home.rco",
            r#"amount var 100 amount set amount get"#,
        )
        .expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushString("amount".to_string()),
                Op::CallWord("var".to_string()),
                Op::PushNumber(100),
                Op::PushString("amount".to_string()),
                Op::CallWord("set".to_string()),
                Op::PushString("amount".to_string()),
                Op::CallWord("get".to_string()),
            ]
        );
    }

    #[test]
    fn compiles_controller_context_binding_words_as_variable_names() {
        let chunk = compile_source("controllers/home.rco", r#"ctx get "home/index" swap view"#)
            .expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushString("ctx".to_string()),
                Op::CallWord("get".to_string()),
                Op::PushString("home/index".to_string()),
                Op::CallWord("swap".to_string()),
                Op::CallWord("view".to_string()),
            ]
        );
    }

    #[test]
    fn compiles_dollar_references_as_variable_gets() {
        let chunk = compile_source("controllers/home.rco", r#"$ctx "home/index" swap view"#)
            .expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushString("ctx".to_string()),
                Op::CallWord("get".to_string()),
                Op::PushString("home/index".to_string()),
                Op::CallWord("swap".to_string()),
                Op::CallWord("view".to_string()),
            ]
        );
    }

    #[test]
    fn compiles_dynamic_declarations_from_dollar_references() {
        let chunk = compile_source("test.rco", r#""users" name var $name array $users count"#)
            .expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushString("users".to_string()),
                Op::PushString("name".to_string()),
                Op::CallWord("var".to_string()),
                Op::PushString("name".to_string()),
                Op::CallWord("get".to_string()),
                Op::CallWord("array".to_string()),
                Op::PushString("users".to_string()),
                Op::CallWord("get".to_string()),
                Op::CallWord("count".to_string()),
            ]
        );
    }

    #[test]
    fn compiles_fixture_home_controller_for_mvc_dispatch() {
        let source =
            include_str!("../../../tests/fixtures/web_minimal/app/Controllers/HomeController.rco");

        let chunk = compile_source(
            "tests/fixtures/web_minimal/app/Controllers/HomeController.rco",
            source,
        )
        .expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::BeginClass {
                    name: "HomeController".to_string(),
                    superclass: "Controller".to_string(),
                },
                Op::AddMethod {
                    name: "index".to_string(),
                    block: 0,
                    args: None,
                },
                Op::EndClass,
            ]
        );
        assert_eq!(
            chunk.blocks[0].ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushString("title".to_string()),
                Op::CallWord("var".to_string()),
                Op::PushString("user".to_string()),
                Op::CallWord("var".to_string()),
                Op::PushString("Hello Ricochet".to_string()),
                Op::PushString("title".to_string()),
                Op::CallWord("set".to_string()),
                Op::CallWord("map".to_string()),
                Op::PushString("name".to_string()),
                Op::PushString("Ada <Lovelace>".to_string()),
                Op::CallWord("put!".to_string()),
                Op::PushString("user".to_string()),
                Op::CallWord("set".to_string()),
                Op::PushString("ctx".to_string()),
                Op::CallWord("get".to_string()),
                Op::PushString("home/index".to_string()),
                Op::CallWord("swap".to_string()),
                Op::CallWord("view".to_string()),
                Op::Return,
            ]
        );
    }

    #[test]
    fn compiles_postfix_if_else_to_jump_opcodes() {
        let chunk =
            compile_source("test.rco", r#"true if "yes" else "no" end"#).expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushBool(true),
                Op::JumpIfFalse(4),
                Op::PushString("yes".to_string()),
                Op::Jump(5),
                Op::PushString("no".to_string()),
            ]
        );
    }

    #[test]
    fn compiles_postfix_while_to_backward_jump() {
        let chunk = compile_source(
            "test.rco",
            r#"
              0 count var
              count get 3 < while
                count get 1 + count set
              end
              count get
            "#,
        )
        .expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushNumber(0),
                Op::PushString("count".to_string()),
                Op::CallWord("var".to_string()),
                Op::PushString("count".to_string()),
                Op::CallWord("get".to_string()),
                Op::PushNumber(3),
                Op::CallWord("<".to_string()),
                Op::JumpIfFalse(15),
                Op::PushString("count".to_string()),
                Op::CallWord("get".to_string()),
                Op::PushNumber(1),
                Op::CallWord("+".to_string()),
                Op::PushString("count".to_string()),
                Op::CallWord("set".to_string()),
                Op::Jump(3),
                Op::PushString("count".to_string()),
                Op::CallWord("get".to_string()),
            ]
        );
    }

    #[test]
    fn block_statements_do_not_bind_declarations_across_statement_boundaries() {
        let chunk = compile_source(
            "test.rco",
            r#"
              Probe Object Subclass
                [
                  map bag var
                  bag get "id" "bag" put! drop
                  array events var
                ] "go" Method
              end
            "#,
        )
        .expect("compile succeeds");

        assert_eq!(
            chunk.blocks[0].ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::CallWord("map".to_string()),
                Op::PushString("bag".to_string()),
                Op::CallWord("var".to_string()),
                Op::PushString("bag".to_string()),
                Op::CallWord("get".to_string()),
                Op::PushString("id".to_string()),
                Op::PushString("bag".to_string()),
                Op::CallWord("put!".to_string()),
                Op::CallWord("drop".to_string()),
                Op::CallWord("array".to_string()),
                Op::PushString("events".to_string()),
                Op::CallWord("var".to_string()),
                Op::Return,
            ]
        );
    }

    #[test]
    fn rejects_break_and_continue_outside_a_loop() {
        for word in ["break", "continue"] {
            assert_eq!(
                compile_source("test.rco", word),
                Err(CompileError::LoopControlOutsideLoop {
                    word: word.to_string(),
                    span: Span {
                        start: 0,
                        end: word.len()
                    },
                })
            );
        }
    }

    #[test]
    fn compiles_top_level_function_declaration() {
        let chunk = compile_source("test.rco", r#"hello function "hi" end hello"#)
            .expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::AddFunction {
                    name: "hello".to_string(),
                    block: 0,
                    args: None,
                },
                Op::CallWord("hello".to_string()),
            ]
        );
        assert_eq!(
            chunk.blocks[0].ops().cloned().collect::<Vec<_>>(),
            vec![Op::PushString("hi".to_string()), Op::Return]
        );
    }

    #[test]
    fn compiles_explicit_return_to_return_opcode() {
        let chunk = compile_source(
            "test.rco",
            "answer function\n  42 return\n  99\nend\nanswer\n",
        )
        .expect("compile succeeds");

        assert_eq!(
            chunk.blocks[0].ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushNumber(42),
                Op::Return,
                Op::PushNumber(99),
                Op::Return,
            ]
        );
    }

    #[test]
    fn compiles_top_level_function_args_metadata() {
        let source = r#"
          ( name -> String ) greet function
            name get
          end
        "#;
        let chunk = compile_source("test.rco", source).expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![Op::AddFunction {
                name: "greet".to_string(),
                block: 0,
                args: Some(ArgsSpec {
                    inputs: vec!["name".to_string()],
                    outputs: vec!["String".to_string()],
                }),
            }]
        );
    }

    #[test]
    fn compiles_macro_declaration_to_empty_runtime_chunk() {
        let source = r#"
          "unless" Macro
            [
              "ok"
            ]
          end
        "#;
        let chunk = compile_source("test.rco", source).expect("compile succeeds");

        assert!(chunk.ops().next().is_none());
        assert!(chunk.blocks.is_empty());
    }

    #[test]
    fn rejects_class_body_macro_declaration_for_local_only_slice() {
        let source = r#"
          User Model Subclass
            "displayName" Macro
              [
                "ok"
              ]
            end
          end
        "#;
        let err = compile_source("test.rco", source).expect_err("compile fails");

        match &err {
            CompileError::Unsupported {
                feature,
                span,
                help,
            } => {
                assert!(feature.contains("macro declarations are only supported at top level"));
                assert_eq!(span.start, source.find("\"displayName\"").unwrap());
                assert!(help
                    .as_deref()
                    .is_some_and(|help| help.contains("move the macro declaration")));
            }
            other => panic!("expected unsupported class-body macro declaration, got {other:?}"),
        }
        let diagnostic = format_compile_error("test.rco", source, &err);
        assert!(diagnostic.contains("macro declarations are only supported at top level"));
    }

    #[test]
    fn expands_simple_local_macro_into_runtime_ops() {
        let source = r#"
          "say_ok" Macro
            [
              [ "ok" println ] quote_ast
            ]
          end

          "say_ok" macro_call
        "#;
        let chunk = compile_source("test.rco", source).expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushString("ok".to_string()),
                Op::CallWord("println".to_string()),
            ]
        );
    }

    #[test]
    fn expand_source_returns_macro_table_expanded_module_and_trace() {
        let source = r#"
          (( Say ok. ))
          "say_ok" Macro
            [
              [ "ok" println ] quote_ast
            ]
          end

          "say_ok" macro_call
        "#;
        let expansion = expand_source("src\\macro_test.rco", source).expect("expansion succeeds");

        assert_eq!(expansion.module_id, "src/macro_test.rco");
        assert_eq!(expansion.macro_tables.len(), 1);
        let table = &expansion.macro_tables[0];
        assert_eq!(table.module_id, "src/macro_test.rco");
        assert_eq!(table.macros.len(), 1);
        assert_eq!(table.macros[0].name, "say_ok");
        assert_eq!(table.macros[0].inputs, Vec::<String>::new());
        assert_eq!(table.macros[0].outputs, Vec::<String>::new());
        assert_eq!(table.macros[0].docs, vec!["Say ok.".to_string()]);
        assert_eq!(
            table.macros[0].span.start,
            source.find("\"say_ok\"").unwrap()
        );
        assert!(table.macros[0].body_span.is_some());

        let [Item::Expr { expr, .. }] = expansion.module.items.as_slice() else {
            panic!("expanded module should contain one expression item");
        };
        let Expr::Sequence(exprs) = expr else {
            panic!("expanded expression should remain a sequence");
        };
        assert!(matches!(&exprs[0].expr, Expr::String(value) if value == "ok"));
        assert!(matches!(&exprs[1].expr, Expr::Symbol(word) if word == "println"));

        assert_eq!(expansion.trace.len(), 1);
        let trace = &expansion.trace[0];
        assert_eq!(trace.macro_name, "say_ok");
        assert_eq!(trace.module_id, "src/macro_test.rco");
        assert_eq!(trace.depth, 0);
        assert_eq!(trace.argument_count, 0);
        assert_eq!(trace.output_node_count, 2);
        assert_eq!(trace.definition_span, table.macros[0].span);
        assert_eq!(
            trace.invocation_span.start,
            source.rfind("macro_call").unwrap()
        );
        assert!(!trace.id.contains('\\'));
        assert!(!trace.id.contains('/'));
    }

    #[test]
    fn macro_args_splice_caller_operands_into_expanded_runtime_code() {
        let source = r#"
          "double" Macro
            ( value -> expansion )
            [
              [ $value ast_splice $value ast_splice + ] quote_ast
            ]
          end

          $total "double" macro_call
        "#;
        let chunk = compile_source("test.rco", source).expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushString("total".to_string()),
                Op::CallWord("get".to_string()),
                Op::PushString("total".to_string()),
                Op::CallWord("get".to_string()),
                Op::CallWord("+".to_string()),
            ]
        );
    }

    #[test]
    fn nested_macro_expansion_can_be_used_as_an_operand() {
        let source = r#"
          "inner" Macro
            [
              [ 2 3 + ] quote_ast
            ]
          end

          "wrap" Macro
            ( value -> expansion )
            [
              [ $value ast_splice 4 * ] quote_ast
            ]
          end

          "inner" macro_call "wrap" macro_call
        "#;
        let chunk = compile_source("test.rco", source).expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushNumber(2),
                Op::PushNumber(3),
                Op::CallWord("+".to_string()),
                Op::PushNumber(4),
                Op::CallWord("*".to_string()),
            ]
        );
    }

    #[test]
    fn unknown_compile_time_macro_call_fails_instead_of_runtime_fallback() {
        let source = r#""missing" macro_call"#;
        let err = compile_source("test.rco", source).expect_err("compile fails");

        match &err {
            CompileError::Unsupported { feature, span, .. } => {
                assert!(feature.contains("unknown compile-time macro"));
                assert_eq!(span.start, source.find("\"missing\"").unwrap());
            }
            other => panic!("expected unknown macro unsupported error, got {other:?}"),
        }
        let diagnostic = format_compile_error("test.rco", source, &err);
        assert!(diagnostic.contains("unknown compile-time macro"));
    }

    #[test]
    fn unallowlisted_macro_body_word_fails_closed() {
        let source = r#"
          "bad" Macro
            [
              fs_read_text
            ]
          end

          "bad" macro_call
        "#;
        let err = compile_source("test.rco", source).expect_err("compile fails");

        match &err {
            CompileError::Unsupported { feature, .. } => {
                assert!(feature.contains("compile-time macro body used unallowlisted word"));
                assert!(feature.contains("fs_read_text"));
            }
            other => panic!("expected fail-closed macro evaluator error, got {other:?}"),
        }
    }

    #[test]
    fn same_macro_recursion_limit_fails_clearly() {
        let source = r#"
          "loop" Macro
            [
              [ "loop" macro_call ] quote_ast
            ]
          end

          "loop" macro_call
        "#;
        let err = compile_source("test.rco", source).expect_err("compile fails");

        match &err {
            CompileError::Unsupported { feature, .. } => {
                assert!(feature.contains("same-macro recursion limit"));
                assert!(feature.contains("loop"));
            }
            other => panic!("expected macro recursion limit error, got {other:?}"),
        }
    }

    #[test]
    fn macro_expansion_depth_limit_fails_clearly() {
        let mut source = String::new();
        for index in 0..34 {
            let expansion = if index == 33 {
                r#""done""#.to_string()
            } else {
                format!(r#""m{}" macro_call"#, index + 1)
            };
            source.push_str(&format!(
                r#"
                  "m{index}" Macro
                    [
                      [ {expansion} ] quote_ast
                    ]
                  end
                "#
            ));
        }
        source.push_str(r#""m0" macro_call"#);

        let err = compile_source("test.rco", &source).expect_err("compile fails");

        match &err {
            CompileError::Unsupported { feature, .. } => {
                assert!(feature.contains("macro expansion depth limit"));
            }
            other => panic!("expected macro depth limit error, got {other:?}"),
        }
    }

    #[test]
    fn compiles_top_level_block_literals_to_push_block() {
        let chunk = compile_source("test.rco", r#"[ "ok" ]"#).expect("compile succeeds");

        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![Op::PushBlock(0)]
        );
        assert_eq!(
            chunk.blocks[0].ops().cloned().collect::<Vec<_>>(),
            vec![Op::PushString("ok".to_string()), Op::Return]
        );
    }

    #[test]
    fn block_debug_spans_follow_nested_expression_lines() {
        let chunk = compile_source("test.rco", "[\n  2\n  3\n  +\n]\n").expect("compile succeeds");

        assert_eq!(
            chunk.blocks[0]
                .debug()
                .map(|span| span.line)
                .collect::<Vec<_>>(),
            vec![2, 3, 4, 1]
        );
    }

    #[test]
    fn if_branch_debug_spans_follow_nested_expression_lines() {
        let chunk = compile_source("test.rco", "true if\n  \"yes\"\nelse\n  \"no\"\nend\n")
            .expect("compile succeeds");

        assert_eq!(
            chunk.debug().map(|span| span.line).collect::<Vec<_>>(),
            vec![1, 1, 2, 1, 4]
        );
    }

    #[test]
    fn flattens_nested_expression_sequences_in_source_order() {
        let mut compiler = Compiler::new("test.rco");
        compiler
            .compile_expr(&Expr::Sequence(vec![
                spanned(Expr::Number(1)),
                spanned(Expr::Sequence(vec![
                    spanned(Expr::Number(2)),
                    spanned(Expr::String("done".to_string())),
                    spanned(Expr::Symbol("finish".to_string())),
                ])),
                spanned(Expr::BangWord("!push".to_string())),
            ]))
            .expect("compile succeeds");

        assert_eq!(
            compiler.chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushNumber(1),
                Op::PushNumber(2),
                Op::PushString("done".to_string()),
                Op::CallWord("finish".to_string()),
                Op::CallWord("!push".to_string()),
            ]
        );
    }
}
