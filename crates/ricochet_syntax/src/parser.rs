use crate::ast::*;
use crate::lexer::{lex, LexError};
use crate::token::{Span, Token, TokenKind};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error(transparent)]
    Lex(#[from] LexError),
    #[error("unexpected token {found:?} at {span:?}")]
    Unexpected { found: TokenKind, span: Span },
    #[error("expected {expected}, found {found:?} at {span:?}")]
    Expected {
        expected: &'static str,
        found: TokenKind,
        span: Span,
    },
    #[error("invalid number literal {literal:?} at {span:?}")]
    InvalidNumber { literal: String, span: Span },
    #[error("while requires a condition before it at {span:?}")]
    MissingWhileCondition { span: Span },
}

pub fn parse_module(source: &str) -> Result<Module, ParseError> {
    let tokens = lex(source)?;
    Parser {
        tokens,
        pos: 0,
        pending_docs: Vec::new(),
    }
    .parse_module()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    pending_docs: Vec<String>,
}

impl Parser {
    fn parse_module(&mut self) -> Result<Module, ParseError> {
        let mut items = Vec::new();
        loop {
            self.skip_item_trivia();
            if self.at_eof() {
                break;
            }
            items.push(self.parse_item()?);
        }
        Ok(Module { items })
    }

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        let docs = self.take_pending_docs();
        if let Some(class) = self.try_parse_class()? {
            return Ok(Item::Class(ClassDecl { docs, ..class }));
        }
        if let Some(macro_decl) = self.try_parse_macro()? {
            return Ok(Item::Macro(MacroDecl { docs, ..macro_decl }));
        }
        if let Some(function) = self.try_parse_function()? {
            return Ok(Item::Function(FunctionDecl { docs, ..function }));
        }
        let expression = self.parse_expr_item()?;
        Ok(Item::Expr {
            expr: expression.expr,
            span: expression.span,
            docs,
        })
    }

    fn try_parse_class(&mut self) -> Result<Option<ClassDecl>, ParseError> {
        self.skip_newlines();
        let checkpoint = self.pos;
        let start = self.current_span();
        let Some(name) = self.peek_symbol_like() else {
            return Ok(None);
        };
        self.advance();
        let Some(superclass) = self.peek_symbol_like() else {
            self.pos = checkpoint;
            return Ok(None);
        };
        self.advance();
        if !self.consume_symbol("Subclass") {
            self.pos = checkpoint;
            return Ok(None);
        }

        let mut body = Vec::new();
        loop {
            self.skip_item_trivia();
            if self.consume_symbol("end") {
                let end = self.previous_span().end;
                return Ok(Some(ClassDecl {
                    name,
                    superclass,
                    body,
                    docs: Vec::new(),
                    span: Span {
                        start: start.start,
                        end,
                    },
                }));
            }
            if self.at_eof() {
                let token = self.current_token().clone();
                return Err(ParseError::Expected {
                    expected: "end",
                    found: token.kind,
                    span: token.span,
                });
            }
            body.push(self.parse_item()?);
        }
    }

    fn try_parse_macro(&mut self) -> Result<Option<MacroDecl>, ParseError> {
        self.skip_newlines();
        let checkpoint = self.pos;
        let start = self.current_span();
        let TokenKind::String(name) = self.peek_kind().clone() else {
            return Ok(None);
        };
        self.advance();
        if !self.consume_symbol("Macro") {
            self.pos = checkpoint;
            return Ok(None);
        }

        self.skip_newlines();
        let args = if matches!(self.peek_kind(), TokenKind::LeftParen) {
            let args = self.parse_args()?;
            self.skip_newlines();
            Some(args)
        } else {
            None
        };

        if !matches!(self.peek_kind(), TokenKind::LeftBracket) {
            let token = self.current_token().clone();
            return Err(ParseError::Expected {
                expected: "macro body block",
                found: token.kind,
                span: token.span,
            });
        }
        self.advance();
        let body = self.parse_block_exprs()?;
        self.expect_right_bracket()?;
        self.skip_newlines();
        self.expect_symbol("end")?;
        let end = self.previous_span().end;
        Ok(Some(MacroDecl {
            name,
            args,
            body,
            docs: Vec::new(),
            span: Span {
                start: start.start,
                end,
            },
        }))
    }

    fn try_parse_function(&mut self) -> Result<Option<FunctionDecl>, ParseError> {
        self.skip_newlines();
        let checkpoint = self.pos;
        let start = self.current_span();
        let args = if matches!(self.peek_kind(), TokenKind::LeftParen) {
            Some(self.parse_args()?)
        } else {
            None
        };
        let Some(name) = self.peek_symbol_like() else {
            self.pos = checkpoint;
            return Ok(None);
        };
        self.advance();
        if !self.consume_symbol("function") {
            self.pos = checkpoint;
            return Ok(None);
        }

        let body = self.parse_expr_body_until_end()?;
        let end = self.previous_span().end;
        Ok(Some(FunctionDecl {
            name,
            args,
            body,
            docs: Vec::new(),
            span: Span {
                start: start.start,
                end,
            },
        }))
    }

    fn parse_expr_body_until_end(&mut self) -> Result<Vec<SpannedExpr>, ParseError> {
        let body = self.parse_exprs_until(&["end"], "end")?;
        self.expect_symbol("end")?;
        Ok(body)
    }

    fn parse_expr_item(&mut self) -> Result<SpannedExpr, ParseError> {
        let mut exprs = vec![self.parse_expr()?];
        loop {
            if self.consume_symbol("while") {
                let body = self.parse_exprs_until(&["end"], "while terminator")?;
                self.expect_symbol("end")?;
                let span = Span {
                    start: exprs
                        .first()
                        .expect("while condition is non-empty")
                        .span
                        .start,
                    end: self.previous_span().end,
                };
                return Ok(SpannedExpr {
                    expr: Expr::While {
                        condition: exprs,
                        body,
                    },
                    span,
                });
            }

            if matches!(
                self.peek_kind(),
                TokenKind::Newline
                    | TokenKind::DocComment(_)
                    | TokenKind::Eof
                    | TokenKind::RightBracket
            ) || matches!(self.peek_kind(), TokenKind::Symbol(s) if s == "else" || s == "end")
            {
                break;
            }

            exprs.push(self.parse_expr()?);
        }

        if exprs.len() == 1 {
            Ok(exprs.remove(0))
        } else {
            let span = Span {
                start: exprs
                    .first()
                    .expect("expression sequence is non-empty")
                    .span
                    .start,
                end: exprs
                    .last()
                    .expect("expression sequence is non-empty")
                    .span
                    .end,
            };
            Ok(SpannedExpr {
                expr: Expr::Sequence(exprs),
                span,
            })
        }
    }

    fn parse_expr(&mut self) -> Result<SpannedExpr, ParseError> {
        self.skip_newlines();
        let token = self.advance();
        let start = token.span.start;
        let expr = match token.kind {
            TokenKind::Symbol(s) if s == "if" => self.parse_if_expr()?,
            TokenKind::Symbol(s) if s == "while" => {
                return Err(ParseError::MissingWhileCondition { span: token.span });
            }
            TokenKind::Symbol(s) => Expr::Symbol(s),
            TokenKind::BangWord(s) => Expr::BangWord(s),
            TokenKind::DotWord(s) => Expr::DotWord(s),
            TokenKind::Reference(s) => Expr::Reference(s),
            TokenKind::String(s) => Expr::String(s),
            TokenKind::Number(n) => {
                if is_float_literal(&n) {
                    let value = n.parse::<f64>().map_err(|_| ParseError::InvalidNumber {
                        literal: n.clone(),
                        span: token.span,
                    })?;
                    if !value.is_finite() {
                        return Err(ParseError::InvalidNumber {
                            literal: n,
                            span: token.span,
                        });
                    }
                    Expr::Float(value)
                } else {
                    let value = n.parse().map_err(|_| ParseError::InvalidNumber {
                        literal: n,
                        span: token.span,
                    })?;
                    Expr::Number(value)
                }
            }
            TokenKind::LeftParen => {
                self.pos = self.pos.saturating_sub(1);
                Expr::Args(self.parse_args()?)
            }
            TokenKind::LeftBracket => {
                let exprs = self.parse_block_exprs()?;
                self.expect_right_bracket()?;
                Expr::Block(exprs)
            }
            other => {
                return Err(ParseError::Unexpected {
                    found: other,
                    span: token.span,
                });
            }
        };

        Ok(SpannedExpr {
            expr,
            span: Span {
                start,
                end: self.previous_span().end,
            },
        })
    }

    fn parse_if_expr(&mut self) -> Result<Expr, ParseError> {
        let then_body = self.parse_exprs_until(&["else", "end"], "if terminator")?;
        let else_body = if self.consume_symbol("else") {
            self.parse_exprs_until(&["end"], "if terminator")?
        } else {
            Vec::new()
        };
        self.expect_symbol("end")?;

        Ok(Expr::If {
            then_body,
            else_body,
        })
    }

    fn parse_exprs_until(
        &mut self,
        stop_symbols: &[&str],
        expected: &'static str,
    ) -> Result<Vec<SpannedExpr>, ParseError> {
        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Symbol(s) if stop_symbols.contains(&s.as_str()))
            {
                break;
            }
            if self.at_eof() {
                let token = self.current_token().clone();
                return Err(ParseError::Expected {
                    expected,
                    found: token.kind,
                    span: token.span,
                });
            }
            push_statement(&mut body, self.parse_expr_item()?);
        }
        Ok(body)
    }

    fn parse_block_exprs(&mut self) -> Result<Vec<SpannedExpr>, ParseError> {
        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::RightBracket) {
                break;
            }
            if self.at_eof() {
                let token = self.current_token().clone();
                return Err(ParseError::Expected {
                    expected: "right bracket",
                    found: token.kind,
                    span: token.span,
                });
            }
            push_statement(&mut body, self.parse_expr_item()?);
        }
        Ok(body)
    }

    fn parse_args(&mut self) -> Result<ArgsDecl, ParseError> {
        self.expect_left_paren()?;
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        let mut in_outputs = false;
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::RightParen | TokenKind::Eof) {
                break;
            }
            let token = self.advance();
            match token.kind {
                TokenKind::Arrow => in_outputs = true,
                TokenKind::Symbol(s) => {
                    if in_outputs {
                        outputs.push(s);
                    } else {
                        inputs.push(s);
                    }
                }
                other => {
                    return Err(ParseError::Unexpected {
                        found: other,
                        span: token.span,
                    });
                }
            }
        }
        self.expect_right_paren()?;
        Ok(ArgsDecl { inputs, outputs })
    }

    fn consume_symbol(&mut self, expected: &str) -> bool {
        if matches!(self.peek_kind(), TokenKind::Symbol(s) if s == expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_symbol(&mut self, expected: &'static str) -> Result<(), ParseError> {
        self.expect_kind(
            expected,
            |kind| matches!(kind, TokenKind::Symbol(s) if s == expected),
        )
    }

    fn expect_left_paren(&mut self) -> Result<(), ParseError> {
        self.expect_kind("left paren", |kind| matches!(kind, TokenKind::LeftParen))
    }

    fn expect_right_paren(&mut self) -> Result<(), ParseError> {
        self.expect_kind("right paren", |kind| matches!(kind, TokenKind::RightParen))
    }

    fn expect_right_bracket(&mut self) -> Result<(), ParseError> {
        self.expect_kind("right bracket", |kind| {
            matches!(kind, TokenKind::RightBracket)
        })
    }

    fn expect_kind(
        &mut self,
        expected: &'static str,
        predicate: impl FnOnce(&TokenKind) -> bool,
    ) -> Result<(), ParseError> {
        let token = self.advance();
        if predicate(&token.kind) {
            Ok(())
        } else {
            Err(ParseError::Expected {
                expected,
                found: token.kind,
                span: token.span,
            })
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(
            self.peek_kind(),
            TokenKind::Newline | TokenKind::DocComment(_)
        ) {
            self.pos += 1;
        }
    }

    fn skip_item_trivia(&mut self) {
        loop {
            match self.peek_kind() {
                TokenKind::Newline => self.pos += 1,
                TokenKind::DocComment(text) => {
                    self.pending_docs.push(text.clone());
                    self.pos += 1;
                }
                _ => break,
            }
        }
    }

    fn take_pending_docs(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_docs)
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.current_token().kind
    }

    fn peek_symbol_like(&self) -> Option<String> {
        match self.peek_kind() {
            TokenKind::Symbol(s) | TokenKind::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    fn advance(&mut self) -> Token {
        let token = self.current_token().clone();
        self.pos += 1;
        token
    }

    fn current_token(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn current_span(&self) -> Span {
        self.current_token().span
    }

    fn previous_span(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1).min(self.tokens.len() - 1)].span
    }
}

fn push_statement(body: &mut Vec<SpannedExpr>, statement: SpannedExpr) {
    body.push(statement);
}

fn is_float_literal(literal: &str) -> bool {
    literal.contains('.') || literal.contains('e') || literal.contains('E')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Item};

    fn unspan(expressions: &[SpannedExpr]) -> Vec<Expr> {
        expressions
            .iter()
            .map(|expression| expression.expr.clone())
            .collect()
    }

    #[test]
    fn parses_class_with_field_and_method() {
        let src = r#"
          User Model Subclass
            "users" Table
            "email" Accessor
            [
              self email.get
            ] "displayName" Method
          end
        "#;

        let module = parse_module(src).expect("parse succeeds");
        assert_eq!(module.items.len(), 1);
        match &module.items[0] {
            Item::Class(class) => {
                assert_eq!(class.name, "User");
                assert_eq!(class.superclass, "Model");
                assert_eq!(class.body.len(), 3);
                assert!(matches!(
                    &class.body[0],
                    Item::Expr {
                        expr: Expr::Sequence(exprs),
                        ..
                    } if unspan(exprs) == vec![
                        Expr::String("users".to_string()),
                        Expr::Symbol("Table".to_string()),
                    ]
                ));
                assert!(matches!(
                    &class.body[1],
                    Item::Expr {
                        expr: Expr::Sequence(exprs),
                        ..
                    } if unspan(exprs) == vec![
                        Expr::String("email".to_string()),
                        Expr::Symbol("Accessor".to_string()),
                    ]
                ));
                assert!(matches!(
                    &class.body[2],
                    Item::Expr {
                        expr: Expr::Sequence(exprs),
                        ..
                    } if matches!(&exprs[0].expr, Expr::Block(_))
                        && exprs[1].expr == Expr::String("displayName".to_string())
                        && exprs[2].expr == Expr::Symbol("Method".to_string())
                ));
            }
            other => panic!("expected class, got {other:?}"),
        }
    }

    #[test]
    fn parses_block_method_mutation() {
        let src = r#"[ ctx get "home/index" swap view ] "index" Method"#;
        let module = parse_module(src).expect("parse succeeds");
        assert_eq!(module.items.len(), 1);
        match &module.items[0] {
            Item::Expr {
                expr: Expr::Sequence(exprs),
                ..
            } => {
                assert_eq!(exprs.len(), 3);
                assert!(matches!(exprs[0].expr, Expr::Block(_)));
                assert_eq!(exprs[1].expr, Expr::String("index".to_string()));
                assert_eq!(exprs[2].expr, Expr::Symbol("Method".to_string()));
            }
            other => panic!("expected expression sequence, got {other:?}"),
        }
    }

    #[test]
    fn parses_multiline_block_with_trivia_before_close() {
        let src = r#"
          [
            (( fetch context ))
            ctx get
          ] "index" Method
        "#;

        let module = parse_module(src).expect("parse succeeds");
        assert_eq!(module.items.len(), 1);
        match &module.items[0] {
            Item::Expr {
                expr: Expr::Sequence(exprs),
                ..
            } => match &exprs[0].expr {
                Expr::Block(block) => {
                    assert_eq!(
                        unspan(block),
                        vec![Expr::Sequence(vec![
                            SpannedExpr {
                                expr: Expr::Symbol("ctx".to_string()),
                                span: Span { start: 57, end: 60 },
                            },
                            SpannedExpr {
                                expr: Expr::Symbol("get".to_string()),
                                span: Span { start: 61, end: 64 },
                            },
                        ])]
                    );
                }
                other => panic!("expected block, got {other:?}"),
            },
            other => panic!("expected expression sequence, got {other:?}"),
        }
    }

    #[test]
    fn parses_multiline_args_with_trivia_before_close() {
        let src = r#"
          (
            amount target
            -> Result
          ) [
            amount get
          ] "transfer" Method
        "#;

        let module = parse_module(src).expect("parse succeeds");
        assert_eq!(module.items.len(), 1);
        match &module.items[0] {
            Item::Expr {
                expr: Expr::Sequence(exprs),
                ..
            } => {
                let Expr::Args(args) = &exprs[0].expr else {
                    panic!("expected args");
                };
                assert_eq!(args.inputs, vec!["amount", "target"]);
                assert_eq!(args.outputs, vec!["Result"]);
            }
            other => panic!("expected expression sequence, got {other:?}"),
        }
    }

    #[test]
    fn parses_dollar_references() {
        let module = parse_module(r#"$ctx "params" at "id" at"#).expect("parse succeeds");

        match &module.items[0] {
            Item::Expr {
                expr: Expr::Sequence(exprs),
                ..
            } => {
                assert_eq!(
                    unspan(exprs),
                    vec![
                        Expr::Reference("ctx".to_string()),
                        Expr::String("params".to_string()),
                        Expr::Symbol("at".to_string()),
                        Expr::String("id".to_string()),
                        Expr::Symbol("at".to_string()),
                    ]
                );
            }
            other => panic!("expected expression sequence, got {other:?}"),
        }
    }

    #[test]
    fn parses_negative_number_literals() {
        let module = parse_module("-1 -9223372036854775808").expect("parse succeeds");

        match &module.items[0] {
            Item::Expr {
                expr: Expr::Sequence(exprs),
                ..
            } => {
                assert_eq!(
                    unspan(exprs),
                    vec![Expr::Number(-1), Expr::Number(i64::MIN)]
                );
            }
            other => panic!("expected expression sequence, got {other:?}"),
        }
    }

    #[test]
    fn parses_float_literals() {
        let module = parse_module("1.5 -0.25 6e2 -7.5e-1").expect("parse succeeds");

        match &module.items[0] {
            Item::Expr {
                expr: Expr::Sequence(exprs),
                ..
            } => {
                assert_eq!(
                    unspan(exprs),
                    vec![
                        Expr::Float(1.5),
                        Expr::Float(-0.25),
                        Expr::Float(600.0),
                        Expr::Float(-0.75)
                    ]
                );
            }
            other => panic!("expected expression sequence, got {other:?}"),
        }
    }

    #[test]
    fn preserves_doc_comments_on_declarations_and_fields() {
        let src = r#"
          (( User model docs ))
          User Model Subclass
            (( Email field docs ))
            "email" Accessor

            (( Display name docs ))
            [
              self email.get
            ] "displayName" Method
          end

          (( Helper docs ))
          helper function
            "ok"
          end
        "#;

        let module = parse_module(src).expect("parse succeeds");
        match &module.items[0] {
            Item::Class(class) => {
                assert_eq!(class.docs, vec!["User model docs"]);
                match &class.body[0] {
                    Item::Expr { docs, .. } => assert_eq!(docs, &vec!["Email field docs"]),
                    other => panic!("expected field expression, got {other:?}"),
                }
                match &class.body[1] {
                    Item::Expr { docs, .. } => assert_eq!(docs, &vec!["Display name docs"]),
                    other => panic!("expected method expression, got {other:?}"),
                }
            }
            other => panic!("expected class, got {other:?}"),
        }

        match &module.items[1] {
            Item::Function(function) => assert_eq!(function.docs, vec!["Helper docs"]),
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn parses_postfix_if_else_expression() {
        let src = r#"true if "yes" else "no" end"#;
        let module = parse_module(src).expect("parse succeeds");

        let Item::Expr {
            expr: Expr::Sequence(exprs),
            ..
        } = &module.items[0]
        else {
            panic!("expected expression sequence");
        };
        assert_eq!(exprs[0].expr, Expr::Symbol("true".to_string()));
        let Expr::If {
            then_body,
            else_body,
        } = &exprs[1].expr
        else {
            panic!("expected if expression");
        };
        assert_eq!(unspan(then_body), vec![Expr::String("yes".to_string())]);
        assert_eq!(unspan(else_body), vec![Expr::String("no".to_string())]);
    }

    #[test]
    fn parses_postfix_while_with_loop_control() {
        let src = r#"
          count get 10 < while
            count get 1 + count set
            count get 5 = if
              continue
            end
            break
          end
        "#;
        let module = parse_module(src).expect("parse succeeds");

        let Item::Expr {
            expr: Expr::While { condition, body },
            ..
        } = &module.items[0]
        else {
            panic!("expected while expression");
        };

        assert_eq!(
            unspan(condition),
            vec![
                Expr::Symbol("count".to_string()),
                Expr::Symbol("get".to_string()),
                Expr::Number(10),
                Expr::Symbol("<".to_string()),
            ]
        );
        let continue_if = body.iter().find_map(|expression| {
            let expressions = match &expression.expr {
                Expr::Sequence(expressions) => expressions.as_slice(),
                _ => std::slice::from_ref(expression),
            };
            expressions
                .iter()
                .find_map(|expression| match &expression.expr {
                    Expr::If { then_body, .. }
                        if then_body
                            .iter()
                            .any(|item| item.expr == Expr::Symbol("continue".to_string())) =>
                    {
                        Some(())
                    }
                    _ => None,
                })
        });
        assert_eq!(continue_if, Some(()));
        assert_eq!(
            body.last().map(|expression| &expression.expr),
            Some(&Expr::Symbol("break".to_string()))
        );
    }

    #[test]
    fn rejects_while_without_a_condition() {
        let err = parse_module("while\n  true\nend").expect_err("parse fails");
        assert!(matches!(err, ParseError::MissingWhileCondition { .. }));
    }

    #[test]
    fn parses_top_level_function_declaration() {
        let src = r#"
          hello function
            "hi"
          end
        "#;

        let module = parse_module(src).expect("parse succeeds");

        assert_eq!(module.items.len(), 1);
        match &module.items[0] {
            Item::Function(function) => {
                assert_eq!(function.name, "hello");
                assert_eq!(function.args, None);
                assert_eq!(unspan(&function.body), vec![Expr::String("hi".to_string())]);
            }
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn parses_macro_declaration_without_args() {
        let module = parse_module(
            r#"
              "unless" Macro
                [
                  "ok"
                ]
              end
            "#,
        )
        .expect("parse succeeds");

        assert_eq!(module.items.len(), 1);
        match &module.items[0] {
            Item::Macro(macro_decl) => {
                assert_eq!(macro_decl.name, "unless");
                assert_eq!(macro_decl.args, None);
                assert_eq!(
                    unspan(&macro_decl.body),
                    vec![Expr::String("ok".to_string())]
                );
            }
            other => panic!("expected macro, got {other:?}"),
        }
    }

    #[test]
    fn parses_macro_declaration_with_args() {
        let module = parse_module(
            r#"
              "unless" Macro
                ( condition body -> expansion )
                [
                  condition body
                ]
              end
            "#,
        )
        .expect("parse succeeds");

        match &module.items[0] {
            Item::Macro(macro_decl) => {
                let args = macro_decl.args.as_ref().expect("args should parse");
                assert_eq!(args.inputs, vec!["condition", "body"]);
                assert_eq!(args.outputs, vec!["expansion"]);
                match &macro_decl.body[0].expr {
                    Expr::Sequence(exprs) => assert_eq!(
                        unspan(exprs),
                        vec![
                            Expr::Symbol("condition".to_string()),
                            Expr::Symbol("body".to_string()),
                        ]
                    ),
                    other => panic!("expected macro body sequence, got {other:?}"),
                }
            }
            other => panic!("expected macro, got {other:?}"),
        }
    }

    #[test]
    fn attaches_docs_to_macro_declaration() {
        let module = parse_module(
            r#"
              (( Run a block when a condition is false. ))
              "unless" Macro
                []
              end
            "#,
        )
        .expect("parse succeeds");

        match &module.items[0] {
            Item::Macro(macro_decl) => {
                assert_eq!(
                    macro_decl.docs,
                    vec!["Run a block when a condition is false."]
                );
            }
            other => panic!("expected macro, got {other:?}"),
        }
    }

    #[test]
    fn rejects_macro_declaration_without_body_block() {
        let err = parse_module(r#""unless" Macro end"#).expect_err("parse fails");
        match err {
            ParseError::Expected { expected, .. } => {
                assert_eq!(expected, "macro body block");
            }
            other => panic!("expected macro body error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_macro_declaration_without_end() {
        let err = parse_module(r#""unless" Macro [ "ok" ]"#).expect_err("parse fails");
        match err {
            ParseError::Expected { expected, .. } => {
                assert_eq!(expected, "end");
            }
            other => panic!("expected end error, got {other:?}"),
        }
    }

    #[test]
    fn macro_call_stays_ordinary_expression_sequence() {
        let module = parse_module(r#"$ready [ "not ready" println ] "unless" macro_call"#)
            .expect("parse succeeds");

        match &module.items[0] {
            Item::Expr {
                expr: Expr::Sequence(exprs),
                ..
            } => {
                assert_eq!(exprs[0].expr, Expr::Reference("ready".to_string()));
                assert!(matches!(exprs[1].expr, Expr::Block(_)));
                assert_eq!(exprs[2].expr, Expr::String("unless".to_string()));
                assert_eq!(exprs[3].expr, Expr::Symbol("macro_call".to_string()));
            }
            other => panic!("expected expression sequence, got {other:?}"),
        }
    }

    #[test]
    fn bare_macro_name_stays_ordinary_expression_sequence() {
        let module = parse_module("name Macro").expect("parse succeeds");

        match &module.items[0] {
            Item::Expr {
                expr: Expr::Sequence(exprs),
                ..
            } => {
                assert_eq!(
                    unspan(exprs),
                    vec![
                        Expr::Symbol("name".to_string()),
                        Expr::Symbol("Macro".to_string()),
                    ]
                );
            }
            other => panic!("expected expression sequence, got {other:?}"),
        }
    }

    #[test]
    fn rejects_overflowing_number_literal() {
        let err = parse_module("9223372036854775808").expect_err("parse fails");
        match err {
            ParseError::InvalidNumber { literal, .. } => {
                assert_eq!(literal, "9223372036854775808");
            }
            other => panic!("expected invalid number, got {other:?}"),
        }
    }
}
