pub mod ast;
pub mod formatter;
pub mod lexer;
pub mod parser;
pub mod token;

pub use ast::*;
pub use formatter::format_source;
pub use lexer::{lex, LexError};
pub use parser::{parse_module, ParseError};
pub use token::{Span, Token, TokenKind};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
