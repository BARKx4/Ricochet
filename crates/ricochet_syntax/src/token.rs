#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Symbol(String),
    BangWord(String),
    DotWord(String),
    String(String),
    Number(String),
    DocComment(String),
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Arrow,
    Newline,
    Eof,
}
