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
    Parser { tokens, pos: 0 }.parse_module()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn parse_module(&mut self) -> Result<Module, ParseError> {
        let mut items = Vec::new();
        self.skip_newlines();
        while !self.at_eof() {
            items.push(self.parse_item()?);
            self.skip_newlines();
        }
        Ok(Module { items })
    }

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        if let Some(class) = self.try_parse_class()? {
            return Ok(Item::Class(class));
        }
        if let Some(function) = self.try_parse_function()? {
            return Ok(Item::Function(function));
        }
        if let Some(method) = self.try_parse_method()? {
            return Ok(Item::Method(method));
        }
        let expression = self.parse_expr_item()?;
        Ok(Item::Expr {
            expr: expression.expr,
            span: expression.span,
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
        if !self.consume_symbol("subclass") {
            self.pos = checkpoint;
            return Ok(None);
        }

        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            if self.consume_symbol("end") {
                let end = self.previous_span().end;
                return Ok(Some(ClassDecl {
                    name,
                    superclass,
                    body,
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
            span: Span {
                start: start.start,
                end,
            },
        }))
    }

    fn try_parse_method(&mut self) -> Result<Option<MethodDecl>, ParseError> {
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
        if !self.consume_symbol("method") {
            self.pos = checkpoint;
            return Ok(None);
        }

        let body = self.parse_expr_body_until_end()?;
        let end = self.previous_span().end;
        Ok(Some(MethodDecl {
            name,
            args,
            body,
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
                start: exprs.first().expect("expression sequence is non-empty").span.start,
                end: exprs.last().expect("expression sequence is non-empty").span.end,
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
            TokenKind::String(s) => Expr::String(s),
            TokenKind::Number(n) => {
                let value = n.parse().map_err(|_| ParseError::InvalidNumber {
                    literal: n,
                    span: token.span,
                })?;
                Expr::Number(value)
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
        self.expect_kind(expected, |kind| {
            matches!(kind, TokenKind::Symbol(s) if s == expected)
        })
    }

    fn expect_left_paren(&mut self) -> Result<(), ParseError> {
        self.expect_kind("left paren", |kind| matches!(kind, TokenKind::LeftParen))
    }

    fn expect_right_paren(&mut self) -> Result<(), ParseError> {
        self.expect_kind("right paren", |kind| matches!(kind, TokenKind::RightParen))
    }

    fn expect_right_bracket(&mut self) -> Result<(), ParseError> {
        self.expect_kind("right bracket", |kind| matches!(kind, TokenKind::RightBracket))
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
        while matches!(self.peek_kind(), TokenKind::Newline | TokenKind::DocComment(_)) {
            self.pos += 1;
        }
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
    match statement {
        SpannedExpr {
            expr: Expr::Sequence(expressions),
            ..
        } => body.extend(expressions),
        expression => body.push(expression),
    }
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
          User Model subclass
            users table
            email field
            displayName method
              self .email get
            end
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
                        Expr::Symbol("users".to_string()),
                        Expr::Symbol("table".to_string()),
                    ]
                ));
                assert!(matches!(
                    &class.body[1],
                    Item::Expr {
                        expr: Expr::Sequence(exprs),
                        ..
                    } if unspan(exprs) == vec![
                        Expr::Symbol("email".to_string()),
                        Expr::Symbol("field".to_string()),
                    ]
                ));
                match &class.body[2] {
                    Item::Method(method) => {
                        assert_eq!(method.name, "displayName");
                        assert_eq!(
                            unspan(&method.body),
                            vec![
                                Expr::Symbol("self".to_string()),
                                Expr::DotWord(".email".to_string()),
                                Expr::Symbol("get".to_string()),
                            ]
                        );
                    }
                    other => panic!("expected method, got {other:?}"),
                }
            }
            other => panic!("expected class, got {other:?}"),
        }
    }

    #[test]
    fn parses_block_method_mutation() {
        let src = r#""index" [ ctx get "home/index" swap view ] !method"#;
        let module = parse_module(src).expect("parse succeeds");
        assert_eq!(module.items.len(), 1);
        match &module.items[0] {
            Item::Expr {
                expr: Expr::Sequence(exprs),
                ..
            } => {
                assert_eq!(exprs.len(), 3);
                assert_eq!(exprs[0].expr, Expr::String("index".to_string()));
                assert!(matches!(exprs[1].expr, Expr::Block(_)));
                assert_eq!(exprs[2].expr, Expr::BangWord("!method".to_string()));
            }
            other => panic!("expected expression sequence, got {other:?}"),
        }
    }

    #[test]
    fn parses_multiline_block_with_trivia_before_close() {
        let src = r#"
          "index" [
            (( fetch context ))
            ctx get
          ] !method
        "#;

        let module = parse_module(src).expect("parse succeeds");
        assert_eq!(module.items.len(), 1);
        match &module.items[0] {
            Item::Expr {
                expr: Expr::Sequence(exprs),
                ..
            } => match &exprs[1].expr {
                Expr::Block(block) => {
                    assert_eq!(
                        unspan(block),
                        vec![
                            Expr::Symbol("ctx".to_string()),
                            Expr::Symbol("get".to_string()),
                        ]
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
          ) transfer method
            amount get
          end
        "#;

        let module = parse_module(src).expect("parse succeeds");
        assert_eq!(module.items.len(), 1);
        match &module.items[0] {
            Item::Method(method) => {
                let args = method.args.as_ref().expect("args parsed");
                assert_eq!(args.inputs, vec!["amount", "target"]);
                assert_eq!(args.outputs, vec!["Result"]);
            }
            other => panic!("expected method, got {other:?}"),
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
            expr:
                Expr::While {
                    condition,
                    body,
                },
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
        let continue_if = body.iter().find_map(|expression| match &expression.expr {
            Expr::If { then_body, .. }
                if then_body
                    .iter()
                    .any(|item| item.expr == Expr::Symbol("continue".to_string())) =>
            {
                Some(())
            }
            _ => None,
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
                assert_eq!(
                    unspan(&function.body),
                    vec![Expr::String("hi".to_string())]
                );
            }
            other => panic!("expected function, got {other:?}"),
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
