use ricochet_bytecode::{Chunk, Op, SourceSpan};
use ricochet_syntax::{
    parse_module, ClassDecl, Expr, Item, MethodDecl, Module, ParseError, Span,
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
            Item::Expr(expr) => self.compile_expr(expr),
            Item::Function(function) => Err(CompileError::Unsupported(format!(
                "function declaration {}",
                function.name
            ))),
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
            self.compile_class_body_item(item, class.span)?;
        }

        self.chunk.push(Op::EndClass, span);
        Ok(())
    }

    fn compile_class_body_item(
        &mut self,
        item: &Item,
        fallback_span: Span,
    ) -> Result<(), CompileError> {
        match item {
            Item::Method(method) => self.compile_method_decl(method),
            Item::Expr(Expr::Sequence(exprs)) if is_field_declaration(exprs) => {
                let name = declaration_name(&exprs[0]).expect("field declaration checked");
                self.chunk
                    .push(Op::AddField(name), self.source_span(fallback_span));
                Ok(())
            }
            Item::Expr(Expr::Sequence(exprs)) if is_block_method_declaration(exprs) => {
                let name = declaration_name(&exprs[0]).expect("method declaration checked");
                let Expr::Block(body) = &exprs[1] else {
                    unreachable!("method declaration checked");
                };
                let block = self.compile_block_chunk(body, fallback_span)?;
                let block = self.chunk.push_block(block);
                self.chunk.push(
                    Op::AddMethod { name, block },
                    self.source_span(fallback_span),
                );
                Ok(())
            }
            Item::Expr(expr) => self.compile_expr(expr),
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
        if let Some(args) = &method.args {
            return Err(CompileError::Unsupported(format!(
                "method {} argument declarations are not supported by compiler lowering yet: {}",
                method.name,
                format_args_decl(args)
            )));
        }

        let block = self.compile_block_chunk(&method.body, method.span)?;
        let block = self.chunk.push_block(block);
        self.chunk.push(
            Op::AddMethod {
                name: method.name.clone(),
                block,
            },
            self.source_span(method.span),
        );
        Ok(())
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

    fn compile_if(
        &mut self,
        then_body: &[Expr],
        else_body: &[Expr],
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

    fn compile_exprs(&mut self, exprs: &[Expr]) -> Result<(), CompileError> {
        let mut index = 0;
        while index < exprs.len() {
            if let Some((name, operator)) = variable_binding_pair(exprs, index) {
                self.push(Op::PushString(name))?;
                self.push(Op::CallWord(operator))?;
                index += 2;
            } else {
                self.compile_expr(&exprs[index])?;
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
            word => self.push(Op::CallWord(word.to_string())),
        }
    }

    fn compile_block_chunk(
        &self,
        exprs: &[Expr],
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

fn is_field_declaration(exprs: &[Expr]) -> bool {
    exprs.len() == 2
        && declaration_name(&exprs[0]).is_some()
        && matches!(&exprs[1], Expr::Symbol(word) if word == "field")
}

fn is_block_method_declaration(exprs: &[Expr]) -> bool {
    exprs.len() == 3
        && declaration_name(&exprs[0]).is_some()
        && matches!(&exprs[1], Expr::Block(_))
        && matches!(&exprs[2], Expr::BangWord(word) if word == "!method")
}

fn declaration_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Symbol(name) | Expr::String(name) => Some(name.clone()),
        _ => None,
    }
}

fn variable_binding_pair(exprs: &[Expr], index: usize) -> Option<(String, String)> {
    let Expr::Symbol(name) = exprs.get(index)? else {
        return None;
    };
    let Expr::Symbol(operator) = exprs.get(index + 1)? else {
        return None;
    };

    matches!(operator.as_str(), "get" | "set" | "var")
        .then(|| (name.clone(), operator.clone()))
}

fn format_args_decl(args: &ricochet_syntax::ArgsDecl) -> String {
    let inputs = if args.inputs.is_empty() {
        "none".to_string()
    } else {
        args.inputs.join(" ")
    };
    let outputs = if args.outputs.is_empty() {
        "none".to_string()
    } else {
        args.outputs.join(" ")
    };
    format!("inputs=({inputs}) outputs=({outputs})")
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
    use ricochet_bytecode::Op;

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
    fn compiles_class_fields_and_block_method_mutations() {
        let source = r#"
          User Model subclass
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
                Op::AddField("email".to_string()),
                Op::AddMethod {
                    name: "displayName".to_string(),
                    block: 0,
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
    fn rejects_class_methods_with_args_until_bytecode_can_preserve_them() {
        let source = r#"
          Transfer Service subclass
            ( amount target -> Result ) transfer method
              amount target send
            end
          end
        "#;

        let err = compile_source("services/transfer.rco", source).expect_err("compile fails");

        match err {
            CompileError::Unsupported(message) => {
                assert!(message.contains("transfer"));
                assert!(message.contains("argument declarations"));
            }
            other => panic!("expected unsupported method args, got {other:?}"),
        }
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
    fn method_block_debug_spans_inherit_declaration_span() {
        let source = r#"
          User Model subclass
            displayName method
              self .email get
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
            vec![3, 3, 3, 3]
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
                },
                Op::EndClass,
            ]
        );
        assert_eq!(
            chunk.blocks[0].ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushString("title".to_string()),
                Op::CallWord("var".to_string()),
                Op::PushString("Hello Ricochet".to_string()),
                Op::PushString("title".to_string()),
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
    fn flattens_nested_expression_sequences_in_source_order() {
        let mut compiler = Compiler::new("test.rco");
        compiler
            .compile_expr(&Expr::Sequence(vec![
                Expr::Number(1),
                Expr::Sequence(vec![
                    Expr::Number(2),
                    Expr::String("done".to_string()),
                    Expr::Symbol("finish".to_string()),
                ]),
                Expr::BangWord("!push".to_string()),
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
