use crate::token::{Span, Token, TokenKind};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LexError {
    #[error("unterminated string at byte {0}")]
    UnterminatedString(usize),
    #[error("unterminated comment at byte {0}")]
    UnterminatedComment(usize),
}

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut tokens = Vec::new();

    while i < bytes.len() {
        let start = i;
        match bytes[i] as char {
            ' ' | '\t' | '\r' => i += 1,
            '\n' => {
                i += 1;
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    span: Span { start, end: i },
                });
            }
            '(' if bytes.get(i + 1) == Some(&b'(') => {
                i += 2;
                let body_start = i;
                while i + 1 < bytes.len() && !(bytes[i] == b')' && bytes[i + 1] == b')') {
                    i += 1;
                }
                if i + 1 >= bytes.len() {
                    return Err(LexError::UnterminatedComment(start));
                }
                let text = source[body_start..i].trim().to_string();
                i += 2;
                tokens.push(Token {
                    kind: TokenKind::DocComment(text),
                    span: Span { start, end: i },
                });
            }
            '(' => {
                i += 1;
                tokens.push(Token {
                    kind: TokenKind::LeftParen,
                    span: Span { start, end: i },
                });
            }
            ')' => {
                i += 1;
                tokens.push(Token {
                    kind: TokenKind::RightParen,
                    span: Span { start, end: i },
                });
            }
            '[' => {
                i += 1;
                tokens.push(Token {
                    kind: TokenKind::LeftBracket,
                    span: Span { start, end: i },
                });
            }
            ']' => {
                i += 1;
                tokens.push(Token {
                    kind: TokenKind::RightBracket,
                    span: Span { start, end: i },
                });
            }
            '-' if bytes.get(i + 1) == Some(&b'>') => {
                i += 2;
                tokens.push(Token {
                    kind: TokenKind::Arrow,
                    span: Span { start, end: i },
                });
            }
            '"' => {
                i += 1;
                let body_start = i;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if i >= bytes.len() {
                    return Err(LexError::UnterminatedString(start));
                }
                let text = source[body_start..i].to_string();
                i += 1;
                tokens.push(Token {
                    kind: TokenKind::String(text),
                    span: Span { start, end: i },
                });
            }
            c if c.is_ascii_digit() => {
                i += 1;
                while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
                tokens.push(Token {
                    kind: TokenKind::Number(source[start..i].to_string()),
                    span: Span { start, end: i },
                });
            }
            _ => {
                i += 1;
                while i < bytes.len()
                    && !matches!(
                        bytes[i] as char,
                        ' ' | '\t' | '\r' | '\n' | '(' | ')' | '[' | ']'
                    )
                {
                    if bytes[i] == b'-' && bytes.get(i + 1) == Some(&b'>') {
                        break;
                    }
                    i += 1;
                }
                let word = source[start..i].to_string();
                let kind = if word.starts_with('!') {
                    TokenKind::BangWord(word)
                } else if word.starts_with('.') {
                    TokenKind::DotWord(word)
                } else {
                    TokenKind::Symbol(word)
                };
                tokens.push(Token {
                    kind,
                    span: Span { start, end: i },
                });
            }
        }
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span {
            start: source.len(),
            end: source.len(),
        },
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenKind;

    #[test]
    fn lexes_postfix_declarations_comments_and_blocks() {
        let src = r#"
          (( doc comment ))
          User Model subclass
            name field
            "index" [ ctx get "home/index" swap view ] !method
          end
        "#;

        let tokens = lex(src).expect("lexing succeeds");
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind.clone()).collect();

        assert!(kinds.contains(&TokenKind::DocComment("doc comment".to_string())));
        assert!(kinds.contains(&TokenKind::Symbol("User".to_string())));
        assert!(kinds.contains(&TokenKind::Symbol("subclass".to_string())));
        assert!(kinds.contains(&TokenKind::String("index".to_string())));
        assert!(kinds.contains(&TokenKind::LeftBracket));
        assert!(kinds.contains(&TokenKind::RightBracket));
        assert!(kinds.contains(&TokenKind::BangWord("!method".to_string())));
    }

    #[test]
    fn lexes_args_object_and_return_arrow() {
        let src = "( amount target -> Result ) transfer method";
        let tokens = lex(src).expect("lexing succeeds");
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind.clone()).collect();

        assert_eq!(kinds[0], TokenKind::LeftParen);
        assert!(kinds.contains(&TokenKind::Arrow));
        assert!(kinds.contains(&TokenKind::RightParen));
    }
}
