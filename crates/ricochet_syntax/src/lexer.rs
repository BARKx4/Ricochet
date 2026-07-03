use crate::token::{Span, Token, TokenKind};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LexError {
    #[error("unterminated string at byte {0}")]
    UnterminatedString(usize),
    #[error("unterminated comment at byte {0}")]
    UnterminatedComment(usize),
    #[error("invalid string escape \\{escape} at byte {position}")]
    InvalidStringEscape { escape: char, position: usize },
    #[error("empty reference at byte {0}")]
    EmptyReference(usize),
    #[error("leading ! words are not supported at byte {position}: {word:?}")]
    LeadingExclamationWord { word: String, position: usize },
    #[error("invalid word {word:?} at byte {position}")]
    InvalidWord { word: String, position: usize },
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
                let text = decode_string(&source[body_start..i], body_start)?;
                i += 1;
                tokens.push(Token {
                    kind: TokenKind::String(text),
                    span: Span { start, end: i },
                });
            }
            '-' if bytes
                .get(i + 1)
                .is_some_and(|byte| (*byte as char).is_ascii_digit()) =>
            {
                i = scan_number_literal(bytes, i);
                tokens.push(Token {
                    kind: TokenKind::Number(source[start..i].to_string()),
                    span: Span { start, end: i },
                });
            }
            c if c.is_ascii_digit() => {
                i = scan_number_literal(bytes, i);
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
                if word.starts_with('-') && word != "-" {
                    return Err(LexError::InvalidWord {
                        word,
                        position: start,
                    });
                }
                let kind = if word.starts_with('!') {
                    return Err(LexError::LeadingExclamationWord {
                        word,
                        position: start,
                    });
                } else if word.starts_with('.') {
                    TokenKind::DotWord(word)
                } else if let Some(name) = word.strip_prefix('$') {
                    if name.is_empty() {
                        return Err(LexError::EmptyReference(start));
                    }
                    TokenKind::Reference(name.to_string())
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

fn scan_number_literal(bytes: &[u8], mut i: usize) -> usize {
    if bytes.get(i) == Some(&b'-') {
        i += 1;
    }

    while i < bytes.len() && is_ascii_digit(bytes[i]) {
        i += 1;
    }

    if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(|byte| is_ascii_digit(*byte)) {
        i += 2;
        while i < bytes.len() && is_ascii_digit(bytes[i]) {
            i += 1;
        }
    }

    if bytes
        .get(i)
        .is_some_and(|byte| matches!(*byte, b'e' | b'E'))
    {
        let mut exponent = i + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        if bytes
            .get(exponent)
            .is_some_and(|byte| is_ascii_digit(*byte))
        {
            i = exponent + 1;
            while i < bytes.len() && is_ascii_digit(bytes[i]) {
                i += 1;
            }
        }
    }

    i
}

fn is_ascii_digit(byte: u8) -> bool {
    (byte as char).is_ascii_digit()
}

fn decode_string(value: &str, offset: usize) -> Result<String, LexError> {
    let mut decoded = String::new();
    let mut characters = value.char_indices();

    while let Some((index, character)) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }

        let Some((_, escape)) = characters.next() else {
            return Err(LexError::InvalidStringEscape {
                escape: '\\',
                position: offset + index,
            });
        };
        decoded.push(match escape {
            '"' => '"',
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            escape => {
                return Err(LexError::InvalidStringEscape {
                    escape,
                    position: offset + index,
                });
            }
        });
    }

    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenKind;

    #[test]
    fn lexes_postfix_declarations_comments_and_blocks() {
        let src = r#"
          (( doc comment ))
          User Model Subclass
            "name" Accessor
            [ ctx get "home/index" swap view ] "index" Method
          end
        "#;

        let tokens = lex(src).expect("lexing succeeds");
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind.clone()).collect();

        assert!(kinds.contains(&TokenKind::DocComment("doc comment".to_string())));
        assert!(kinds.contains(&TokenKind::Symbol("User".to_string())));
        assert!(kinds.contains(&TokenKind::Symbol("Subclass".to_string())));
        assert!(kinds.contains(&TokenKind::String("index".to_string())));
        assert!(kinds.contains(&TokenKind::LeftBracket));
        assert!(kinds.contains(&TokenKind::RightBracket));
        assert!(kinds.contains(&TokenKind::Symbol("Method".to_string())));
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

    #[test]
    fn lexes_dollar_reference_words() {
        let tokens = lex("$users .count").expect("lexing succeeds");
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind.clone()).collect();

        assert_eq!(kinds[0], TokenKind::Reference("users".to_string()));
        assert_eq!(kinds[1], TokenKind::DotWord(".count".to_string()));
    }

    #[test]
    fn lexes_negative_numbers_without_stealing_subtraction() {
        let tokens = lex("-1 - -2").expect("lexing succeeds");
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind.clone()).collect();

        assert_eq!(kinds[0], TokenKind::Number("-1".to_string()));
        assert_eq!(kinds[1], TokenKind::Symbol("-".to_string()));
        assert_eq!(kinds[2], TokenKind::Number("-2".to_string()));
    }

    #[test]
    fn lexes_float_literals_without_stealing_dot_selectors() {
        let tokens = lex("1.5 -0.25 6e2 -7.5e-1 user email.get").expect("lexing succeeds");
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind.clone()).collect();

        assert_eq!(kinds[0], TokenKind::Number("1.5".to_string()));
        assert_eq!(kinds[1], TokenKind::Number("-0.25".to_string()));
        assert_eq!(kinds[2], TokenKind::Number("6e2".to_string()));
        assert_eq!(kinds[3], TokenKind::Number("-7.5e-1".to_string()));
        assert_eq!(kinds[4], TokenKind::Symbol("user".to_string()));
        assert_eq!(kinds[5], TokenKind::Symbol("email.get".to_string()));
    }

    #[test]
    fn rejects_dash_prefixed_words() {
        assert!(lex("-name").is_err());
    }

    #[test]
    fn rejects_leading_exclamation_words() {
        let word = format!("{}{}", "!", "push");

        assert_eq!(
            lex(&word),
            Err(LexError::LeadingExclamationWord { word, position: 0 })
        );
    }

    #[test]
    fn decodes_modern_string_escapes() {
        let tokens = lex(r#""quote: \" slash: \\ line:\n tab:\t""#).expect("lexing succeeds");

        assert_eq!(
            tokens[0].kind,
            TokenKind::String("quote: \" slash: \\ line:\n tab:\t".to_string())
        );
    }
}
