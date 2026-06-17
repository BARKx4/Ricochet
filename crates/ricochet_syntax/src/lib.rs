pub mod ast;
pub mod diagnostic;
pub mod formatter;
pub mod lexer;
pub mod parser;
pub mod token;

pub use ast::*;
pub use diagnostic::{
    line_column, line_starts, parse_error_diagnostic, render_source_diagnostic,
    utf16_range_for_span, SourceDiagnostic, SourcePosition, SourceRange,
};
pub use formatter::format_source;
pub use lexer::{lex, LexError};
pub use parser::{parse_module, ParseError};
pub use token::{Span, Token, TokenKind};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
