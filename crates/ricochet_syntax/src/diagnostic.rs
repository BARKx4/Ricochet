use crate::{LexError, ParseError, Span, TokenKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDiagnostic {
    pub file: String,
    pub span: Span,
    pub message: String,
    pub help: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePosition {
    pub line: usize,
    pub character: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRange {
    pub start: SourcePosition,
    pub end: SourcePosition,
}

impl SourceDiagnostic {
    pub fn new(file: impl Into<String>, span: Span, message: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            span,
            message: message.into(),
            help: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn render(&self, source: &str) -> String {
        render_source_diagnostic(self, source)
    }
}

pub fn parse_error_diagnostic(
    file: impl Into<String>,
    source: &str,
    error: &ParseError,
) -> SourceDiagnostic {
    let file = file.into();
    match error {
        ParseError::Lex(error) => lex_error_diagnostic(file, error),
        ParseError::Unexpected { found, span } => SourceDiagnostic::new(
            file,
            *span,
            format!("unexpected token {}", token_label(found)),
        ),
        ParseError::Expected {
            expected,
            found,
            span,
        } => SourceDiagnostic::new(
            file,
            *span,
            format!("expected {expected}, found {}", token_label(found)),
        ),
        ParseError::InvalidNumber { literal, span } => {
            SourceDiagnostic::new(file, *span, format!("invalid number literal {literal:?}"))
        }
        ParseError::MissingWhileCondition { span } => {
            SourceDiagnostic::new(file, *span, "while requires a condition before it")
        }
    }
    .with_source_bounds(source)
}

pub fn render_source_diagnostic(diagnostic: &SourceDiagnostic, source: &str) -> String {
    let bounded = diagnostic.clone().with_source_bounds(source);
    let line_starts = line_starts(source);
    let (line, column) = line_column(&line_starts, bounded.span.start);
    let text = source_line(source, line).unwrap_or("");
    let caret_count = caret_width(source, &line_starts, bounded.span).max(1);
    let gutter_width = line.to_string().len();
    let mut output = String::new();

    output.push_str(&format!("error: {}\n", bounded.message));
    output.push_str(&format!(" --> {}:{line}:{column}\n", bounded.file));
    output.push_str(&format!("{:>gutter_width$} |\n", ""));
    output.push_str(&format!("{line:>gutter_width$} | {text}\n"));
    output.push_str(&format!(
        "{:>gutter_width$} | {}{}\n",
        "",
        " ".repeat(column.saturating_sub(1)),
        "^".repeat(caret_count)
    ));
    if let Some(help) = &bounded.help {
        output.push_str(&format!("help: {help}\n"));
    }
    output.trim_end().to_string()
}

pub fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

pub fn line_column(line_starts: &[usize], offset: usize) -> (usize, usize) {
    let line_index = match line_starts.binary_search(&offset) {
        Ok(index) => index,
        Err(index) => index.saturating_sub(1),
    };
    let line_start = line_starts.get(line_index).copied().unwrap_or(0);
    (line_index + 1, offset.saturating_sub(line_start) + 1)
}

pub fn utf16_range_for_span(source: &str, span: Span) -> SourceRange {
    let bounded = SourceDiagnostic::new("", span, "").with_source_bounds(source);
    SourceRange {
        start: utf16_position(source, bounded.span.start),
        end: utf16_position(source, bounded.span.end),
    }
}

pub fn utf16_position(source: &str, offset: usize) -> SourcePosition {
    let offset = offset.min(source.len());
    let starts = line_starts(source);
    let line_index = match starts.binary_search(&offset) {
        Ok(index) => index,
        Err(index) => index.saturating_sub(1),
    };
    let line_start = starts.get(line_index).copied().unwrap_or(0);
    let character = source[line_start..offset]
        .chars()
        .map(char::len_utf16)
        .sum();
    SourcePosition {
        line: line_index,
        character,
    }
}

fn lex_error_diagnostic(file: String, error: &LexError) -> SourceDiagnostic {
    match error {
        LexError::UnterminatedString(position) => {
            SourceDiagnostic::new(file, point_span(*position), "unterminated string literal")
        }
        LexError::UnterminatedComment(position) => {
            SourceDiagnostic::new(file, point_span(*position), "unterminated doc comment")
        }
        LexError::InvalidStringEscape { escape, position } => SourceDiagnostic::new(
            file,
            point_span(*position),
            format!("invalid string escape \\{escape}"),
        ),
        LexError::EmptyReference(position) => {
            SourceDiagnostic::new(file, point_span(*position), "empty reference")
        }
        LexError::LeadingExclamationWord { word, position } => SourceDiagnostic::new(
            file,
            word_span(*position, word),
            format!("leading ! word {word:?} is not supported"),
        )
        .with_help("use ordinary word names; collection mutators are push, put, insert_at, remove, remove_at, and clear_items"),
        LexError::InvalidWord { word, position } => SourceDiagnostic::new(
            file,
            word_span(*position, word),
            format!("invalid word {word:?}"),
        )
        .with_help("use _ for word separators; - is reserved for subtraction and negative numbers"),
    }
}

fn source_line(source: &str, line: usize) -> Option<&str> {
    source
        .lines()
        .nth(line.saturating_sub(1))
        .map(|line| line.trim_end_matches('\r'))
}

fn caret_width(source: &str, line_starts: &[usize], span: Span) -> usize {
    let line_index = match line_starts.binary_search(&span.start) {
        Ok(index) => index,
        Err(index) => index.saturating_sub(1),
    };
    let line_end = line_starts
        .get(line_index + 1)
        .copied()
        .unwrap_or(source.len());
    span.end
        .min(line_end)
        .saturating_sub(span.start)
        .max(usize::from(span.end > span.start))
}

fn token_label(token: &TokenKind) -> String {
    match token {
        TokenKind::Symbol(value) => format!("symbol {value:?}"),
        TokenKind::DotWord(value) => format!("leading-dot word {value:?}"),
        TokenKind::Reference(value) => format!("reference {value:?}"),
        TokenKind::String(value) => format!("string {value:?}"),
        TokenKind::Number(value) => format!("number {value:?}"),
        TokenKind::DocComment(_) => "doc comment".to_string(),
        TokenKind::LeftParen => "'('".to_string(),
        TokenKind::RightParen => "')'".to_string(),
        TokenKind::LeftBracket => "'['".to_string(),
        TokenKind::RightBracket => "']'".to_string(),
        TokenKind::Arrow => "'->'".to_string(),
        TokenKind::Newline => "newline".to_string(),
        TokenKind::Eof => "end of file".to_string(),
    }
}

fn point_span(position: usize) -> Span {
    Span {
        start: position,
        end: position.saturating_add(1),
    }
}

fn word_span(position: usize, word: &str) -> Span {
    Span {
        start: position,
        end: position
            .saturating_add(word.len())
            .max(position.saturating_add(1)),
    }
}

trait BoundDiagnostic {
    fn with_source_bounds(self, source: &str) -> Self;
}

impl BoundDiagnostic for SourceDiagnostic {
    fn with_source_bounds(mut self, source: &str) -> Self {
        let length = source.len();
        self.span.start = self.span.start.min(length);
        self.span.end = self
            .span
            .end
            .max(self.span.start.saturating_add(1))
            .min(length);
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_module;

    use super::*;

    #[test]
    fn parse_error_diagnostic_renders_line_and_caret() {
        let source = "Widget Model Subclass\n  \"email\" Accessor\n";
        let error = parse_module(source).expect_err("missing end should fail");
        let rendered = parse_error_diagnostic("Widget.rco", source, &error).render(source);

        assert!(rendered.contains("error: expected end, found end of file"));
        assert!(rendered.contains("--> Widget.rco:3:1"));
        assert!(rendered.contains("| ^"));
    }

    #[test]
    fn lex_error_diagnostic_renders_string_site() {
        let source = "\"unterminated";
        let error = parse_module(source).expect_err("unterminated string should fail");
        let rendered = parse_error_diagnostic("bad.rco", source, &error).render(source);

        assert!(rendered.contains("error: unterminated string literal"));
        assert!(rendered.contains("--> bad.rco:1:1"));
    }

    #[test]
    fn utf16_ranges_use_zero_based_lsp_positions() {
        let source = "a\n😀 nope\n";
        let span = Span { start: 7, end: 11 };

        assert_eq!(
            utf16_range_for_span(source, span),
            SourceRange {
                start: SourcePosition {
                    line: 1,
                    character: 3,
                },
                end: SourcePosition {
                    line: 1,
                    character: 7,
                },
            }
        );
    }
}
