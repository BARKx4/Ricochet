pub mod lexer;
pub mod token;

pub use lexer::{lex, LexError};
pub use token::{Span, Token, TokenKind};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
