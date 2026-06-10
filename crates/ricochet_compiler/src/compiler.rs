use ricochet_bytecode::{ArgsSpec, Chunk, Op, SourceSpan};
use ricochet_syntax::{
    parse_module, ArgsDecl, ClassDecl, Expr, Item, MethodDecl, Module, ParseError, Span,
    SpannedExpr,
};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum CompileError {
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error("unsupported compiler feature: {0}")]
    Unsupported(String),
}

pub fn compile_source(file: &str, source: &str) -> Result<Chunk, CompileError> {
    let module = parse_module(source)?;
    let mut compiler = Compiler::from_source(file, source);
    compiler.compile_module(&module)?;
    Ok(compiler.finish())
}

struct Compiler {
    chunk: Chunk,
    line_starts: Vec<usize>,
    default_span: Span,
}

impl Compiler {
    fn new(file: impl Into<String>) -> Self {
        Self {
            chunk: Chunk::new(file),
            line_starts: vec![0],
            default_span: Span { start: 0, end: 0 },
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
            Item::Expr { expr, span } => self.compile_expr_item(expr, *span),
            Item::Function(function) => self.compile_function_decl(function),
            Item::Method(method) => Err(CompileError::Unsupported(format!(
                "top-level method declaration {}",
                method.name
            ))),
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
                self.push_at(Op::CallWord("table".to_string()), exprs[1].span);
                Ok(())
            }
            Item::Expr {
                expr: Expr::Sequence(exprs),
                span,
            } if is_field_declaration(exprs) => {
                let name = declaration_name(&exprs[0]).expect("field declaration checked");
                self.chunk
                    .push(Op::AddField(name), self.source_span(*span));
                Ok(())
            }
            Item::Expr {
                expr: Expr::Sequence(exprs),
                span,
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
            Item::Expr { expr, span } => self.compile_expr_item(expr, *span),
            Item::Class(class) => Err(CompileError::Unsupported(format!(
                "nested class declaration {}",
                class.name
            ))),
            Item::Function(function) => Err(CompileError::Unsupported(format!(
                "function declaration {}",
                function.name
            ))),
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
            Expr::DotWord(word) => self.push(Op::CallMethod(method_name(word))),
            Expr::String(value) => self.push(Op::PushString(value.clone())),
            Expr::Number(value) => self.push(Op::PushNumber(*value)),
            Expr::Block(body) => {
                let block = self.compile_block_chunk(body, self.default_span)?;
                let block = self.chunk.push_block(block);
                self.push(Op::PushBlock(block))
            }
            Expr::Sequence(exprs) => self.compile_exprs(exprs),
            Expr::Args(_) => Err(CompileError::Unsupported("argument declarations".to_string())),
            Expr::If {
                then_body,
                else_body,
            } => self.compile_if(then_body, else_body),
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
        && matches!(&exprs[1].expr, Expr::Symbol(word) if word == "field")
}

fn table_declaration(exprs: &[SpannedExpr]) -> Option<String> {
    match exprs {
        [name, operator] if matches!(&operator.expr, Expr::Symbol(word) if word == "table") => {
            declaration_name(name)
        }
        _ => None,
    }
}

fn block_method_declaration(
    exprs: &[SpannedExpr],
) -> Option<(Option<&ArgsDecl>, String, &[SpannedExpr])> {
    match exprs {
        [name, block, bang] => match (&block.expr, &bang.expr) {
            (Expr::Block(body), Expr::BangWord(word)) if word == "!method" => {
                Some((None, declaration_name(name)?, body.as_slice()))
            }
            _ => None,
        },
        [args, name, block, bang] => match (&args.expr, &block.expr, &bang.expr) {
            (Expr::Args(args), Expr::Block(body), Expr::BangWord(word)) if word == "!method" => {
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

    matches!(operator.as_str(), "get" | "set" | "var")
        .then(|| (name.clone(), operator.clone()))
}

fn args_spec(args: &ArgsDecl) -> ArgsSpec {
    ArgsSpec {
        inputs: args.inputs.clone(),
        outputs: args.outputs.clone(),
    }
}

fn method_name(word: &str) -> String {
    word.strip_prefix('.').unwrap_or(word).to_string()
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

fn line_column(line_starts: &[usize], offset: usize) -> (usize, usize) {
    let line_index = match line_starts.binary_search(&offset) {
        Ok(index) => index,
        Err(index) => index.saturating_sub(1),
    };
    let line_start = line_starts.get(line_index).copied().unwrap_or(0);
    (line_index + 1, offset.saturating_sub(line_start) + 1)
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
          User Model subclass
            users table
            email field
            "displayName" [ self .email get ] !method
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
                Op::CallWord("table".to_string()),
                Op::AddField("email".to_string()),
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
                Op::CallMethod("email".to_string()),
                Op::CallWord("get".to_string()),
                Op::Return,
            ]
        );
    }

    #[test]
    fn compiles_class_methods_with_args_metadata() {
        let source = r#"
          Transfer Service subclass
            ( amount target -> Result ) transfer method
              amount target send
            end
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
          HomeController Controller subclass
            ( id ctx ) "show" [
              nil title var
              ctx var
              id var
              id get title set
              ctx get
              "home/show" swap view
            ] !method
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
          User Model subclass
            displayName method
              self .email get
            end
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
                Op::CallMethod("email".to_string()),
                Op::CallWord("get".to_string()),
                Op::Return,
            ]
        );
    }

    #[test]
    fn method_block_debug_spans_follow_each_expression_line() {
        let source = r#"
          User Model subclass
            displayName method
              self
              .email
              get
            end
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
            vec![4, 5, 6, 3]
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
        let chunk = compile_source(
            "controllers/home.rco",
            r#"ctx get "home/index" swap view"#,
        )
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
    fn compiles_fixture_home_controller_for_mvc_dispatch() {
        let source = include_str!("../../../tests/fixtures/web_minimal/app/Controllers/HomeController.rco");

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
                Op::CallWord("!put".to_string()),
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
        let chunk = compile_source("test.rco", r#"true if "yes" else "no" end"#)
            .expect("compile succeeds");

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
        let chunk = compile_source(
            "test.rco",
            "true if\n  \"yes\"\nelse\n  \"no\"\nend\n",
        )
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
