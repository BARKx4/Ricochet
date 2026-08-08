//! Preserved evidence prototype for ADR-001.
//!
//! This crate is deliberately isolated from the Ricochet 1 parser. It proves
//! properties of the proposed Ricochet 2 surface without becoming a
//! compatibility promise or a production compiler frontend.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    fn cover(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Whitespace,
    Newline,
    Comment,
    String,
    Number,
    Reference,
    Word,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftAngle,
    RightAngle,
    Comma,
    Colon,
    Plus,
    Arrow,
}

impl TokenKind {
    fn is_parser_trivia(&self) -> bool {
        matches!(self, Self::Whitespace | Self::Newline | Self::Comment)
    }

    fn is_spacing_trivia(&self) -> bool {
        matches!(self, Self::Whitespace | Self::Newline)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cst {
    pub tokens: Vec<Token>,
}

impl Cst {
    pub fn recover_source(&self) -> String {
        self.tokens
            .iter()
            .map(|token| token.text.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error => formatter.write_str("error"),
            Self::Warning => formatter.write_str("warning"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    fn error(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ast {
    pub nodes: Vec<AstNode>,
}

impl Ast {
    pub fn declaration_count(&self) -> usize {
        fn count(nodes: &[AstNode]) -> usize {
            nodes
                .iter()
                .map(|node| match node {
                    AstNode::Declaration(declaration) => 1 + count(&declaration.body),
                    AstNode::Match(node) => node.arms.iter().map(|arm| count(&arm.body)).sum(),
                    AstNode::If(node) => count(&node.then_body) + count(&node.else_body),
                    AstNode::Block(node) => count(&node.body),
                    AstNode::Binding(_) | AstNode::Expression(_) => 0,
                })
                .sum()
        }

        count(&self.nodes)
    }

    pub fn declarations(&self) -> Vec<&Declaration> {
        fn collect<'a>(nodes: &'a [AstNode], output: &mut Vec<&'a Declaration>) {
            for node in nodes {
                match node {
                    AstNode::Declaration(declaration) => {
                        output.push(declaration);
                        collect(&declaration.body, output);
                    }
                    AstNode::Match(node) => {
                        for arm in &node.arms {
                            collect(&arm.body, output);
                        }
                    }
                    AstNode::If(node) => {
                        collect(&node.then_body, output);
                        collect(&node.else_body, output);
                    }
                    AstNode::Block(node) => collect(&node.body, output),
                    AstNode::Binding(_) | AstNode::Expression(_) => {}
                }
            }
        }

        let mut output = Vec::new();
        collect(&self.nodes, &mut output);
        output
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AstNode {
    Declaration(Declaration),
    Binding(Binding),
    Match(MatchNode),
    If(IfNode),
    Block(BlockNode),
    Expression(Expression),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationKind {
    Function,
    Method,
    Record,
    Enum,
    Trait,
    Class,
    Subclass,
    Implements,
    Field,
    Case,
}

impl DeclarationKind {
    pub fn canonical_word(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "Method",
            Self::Record => "Record",
            Self::Enum => "Enum",
            Self::Trait => "Trait",
            Self::Class => "Class",
            Self::Subclass => "Subclass",
            Self::Implements => "Implements",
            Self::Field => "Field",
            Self::Case => "Case",
        }
    }

    fn opens_block(self, modifiers: &[String]) -> bool {
        match self {
            Self::Field | Self::Case => false,
            Self::Method if modifiers.iter().any(|modifier| modifier == "required") => false,
            _ => true,
        }
    }
}

impl fmt::Display for DeclarationKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.canonical_word())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub kind: DeclarationKind,
    pub name: String,
    pub related_type: Option<TypeRef>,
    pub generics: Vec<GenericParameter>,
    pub signature: Option<Signature>,
    pub modifiers: Vec<String>,
    pub body: Vec<AstNode>,
    pub header_span: Span,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericParameter {
    pub name: String,
    pub bounds: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    pub source: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedType {
    pub name: Option<String>,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub inputs: Vec<NamedType>,
    pub outputs: Vec<NamedType>,
    pub effects: Vec<String>,
    pub span: Span,
}

impl Signature {
    pub fn stack_row(&self) -> String {
        fn side(values: &[NamedType]) -> String {
            if values.is_empty() {
                return "[]".to_string();
            }
            let values = values
                .iter()
                .map(|value| match &value.name {
                    Some(name) => format!("{name}: {}", value.ty.source),
                    None => value.ty.source.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{values}]")
        }

        let mut row = format!("{} -> {}", side(&self.inputs), side(&self.outputs));
        if !self.effects.is_empty() {
            row.push_str(" uses {");
            row.push_str(&self.effects.join(", "));
            row.push('}');
        }
        row
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub name: String,
    pub operation: BindingOperation,
    pub annotation: Option<TypeRef>,
    pub value_source: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingOperation {
    Let,
    Var,
    Set,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchNode {
    pub scrutinee: String,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: String,
    pub body: Vec<AstNode>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfNode {
    pub condition: String,
    pub then_body: Vec<AstNode>,
    pub else_body: Vec<AstNode>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Each,
    With,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockNode {
    pub kind: BlockKind,
    pub header: String,
    pub body: Vec<AstNode>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    pub source: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    pub cst: Cst,
    pub ast: Ast,
    pub diagnostics: Vec<Diagnostic>,
}

impl Analysis {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }
}

pub fn analyze(source: &str) -> Analysis {
    let (cst, mut diagnostics) = lex(source);
    validate_balanced_delimiters(&cst, &mut diagnostics);
    let mut parser = Parser::new(&cst, &mut diagnostics);
    let ast = parser.parse();
    diagnostics.sort_by_key(|diagnostic| (diagnostic.span.start, diagnostic.code));
    Analysis {
        cst,
        ast,
        diagnostics,
    }
}

pub fn format_source(source: &str) -> String {
    let (cst, _) = lex(source);
    let lines = lines_from_cst(&cst);
    let mut frames = Vec::new();
    let mut output = Vec::new();

    for line in lines {
        let significant = format_tokens(&line.tokens);
        if significant.is_empty() {
            output.push(String::new());
            continue;
        }

        let starts_end = is_exact_word(&significant, "end");
        let starts_else = is_exact_word(&significant, "else");
        let starts_when = ends_with_word(&significant, "when");
        let starts_close_bracket = significant
            .first()
            .is_some_and(|token| token.kind == TokenKind::RightBracket);

        if starts_end {
            if matches!(frames.last(), Some(FormatFrame::MatchArm)) {
                frames.pop();
                if matches!(frames.last(), Some(FormatFrame::Match)) {
                    frames.pop();
                }
            } else {
                frames.pop();
            }
        } else if starts_else {
            if matches!(frames.last(), Some(FormatFrame::IfArm)) {
                frames.pop();
            }
        } else if starts_when {
            if matches!(frames.last(), Some(FormatFrame::MatchArm)) {
                frames.pop();
            }
        } else if starts_close_bracket && matches!(frames.last(), Some(FormatFrame::Bracket)) {
            frames.pop();
        }

        let rendered = render_tokens(&significant);
        output.push(format!("{}{}", "  ".repeat(frames.len()), rendered));

        if starts_else {
            frames.push(FormatFrame::IfArm);
        } else if starts_when {
            frames.push(FormatFrame::MatchArm);
        } else if opens_declaration_block(&significant) {
            frames.push(FormatFrame::Block);
        } else if ends_with_word(&significant, "match") {
            frames.push(FormatFrame::Match);
        } else if ends_with_word(&significant, "if") {
            frames.push(FormatFrame::IfArm);
        } else if ends_with_word(&significant, "each") || ends_with_word(&significant, "with") {
            frames.push(FormatFrame::Block);
        } else if has_unclosed_left_bracket(&significant) {
            frames.push(FormatFrame::Bracket);
        }
    }

    while output.first().is_some_and(String::is_empty) {
        output.remove(0);
    }
    while output.last().is_some_and(String::is_empty) {
        output.pop();
    }
    output.push(String::new());
    output.join("\n")
}

pub fn line_and_column(source: &str, offset: usize) -> (usize, usize) {
    let mut clamped = offset.min(source.len());
    while !source.is_char_boundary(clamped) {
        clamped -= 1;
    }
    let prefix = &source[..clamped];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let column = source[line_start..clamped].chars().count() + 1;
    (line, column)
}

fn lex(source: &str) -> (Cst, Vec<Diagnostic>) {
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();

    while index < bytes.len() {
        let start = index;
        match bytes[index] {
            b' ' | b'\t' => {
                index += 1;
                while matches!(bytes.get(index), Some(b' ' | b'\t')) {
                    index += 1;
                }
                push_token(source, &mut tokens, TokenKind::Whitespace, start, index);
            }
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                index += 2;
                push_token(source, &mut tokens, TokenKind::Newline, start, index);
            }
            b'\r' => {
                index += 1;
                push_token(source, &mut tokens, TokenKind::Whitespace, start, index);
            }
            b'\n' => {
                index += 1;
                push_token(source, &mut tokens, TokenKind::Newline, start, index);
            }
            b'(' if bytes.get(index + 1) == Some(&b'(') => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b')' && bytes[index + 1] == b')')
                {
                    index += 1;
                }
                if index + 1 < bytes.len() {
                    index += 2;
                } else {
                    index = bytes.len();
                    diagnostics.push(Diagnostic::error(
                        "R2L001",
                        "unterminated (( ... )) comment",
                        Span { start, end: index },
                    ));
                }
                push_token(source, &mut tokens, TokenKind::Comment, start, index);
            }
            b'"' => {
                index += 1;
                let mut terminated = false;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' if index + 1 < bytes.len() => index += 2,
                        b'"' => {
                            index += 1;
                            terminated = true;
                            break;
                        }
                        _ => index += 1,
                    }
                }
                if !terminated {
                    diagnostics.push(Diagnostic::error(
                        "R2L002",
                        "unterminated string literal",
                        Span { start, end: index },
                    ));
                }
                push_token(source, &mut tokens, TokenKind::String, start, index);
            }
            b'-' if bytes.get(index + 1) == Some(&b'>') => {
                index += 2;
                push_token(source, &mut tokens, TokenKind::Arrow, start, index);
            }
            b'-' if bytes.get(index + 1).is_some_and(u8::is_ascii_digit) => {
                index = scan_number(bytes, index);
                push_token(source, &mut tokens, TokenKind::Number, start, index);
            }
            byte if byte.is_ascii_digit() => {
                index = scan_number(bytes, index);
                push_token(source, &mut tokens, TokenKind::Number, start, index);
            }
            b'(' => single_token(source, &mut tokens, TokenKind::LeftParen, &mut index),
            b')' => single_token(source, &mut tokens, TokenKind::RightParen, &mut index),
            b'[' => single_token(source, &mut tokens, TokenKind::LeftBracket, &mut index),
            b']' => single_token(source, &mut tokens, TokenKind::RightBracket, &mut index),
            b'<' => single_token(source, &mut tokens, TokenKind::LeftAngle, &mut index),
            b'>' => single_token(source, &mut tokens, TokenKind::RightAngle, &mut index),
            b',' => single_token(source, &mut tokens, TokenKind::Comma, &mut index),
            b':' => single_token(source, &mut tokens, TokenKind::Colon, &mut index),
            b'+' => single_token(source, &mut tokens, TokenKind::Plus, &mut index),
            _ => {
                index += 1;
                while index < bytes.len() && !is_word_boundary(bytes, index) {
                    index += 1;
                }
                let text = &source[start..index];
                let kind = if text.starts_with('$') {
                    if text.len() == 1 {
                        diagnostics.push(Diagnostic::error(
                            "R2L003",
                            "a binding read requires a name after '$'",
                            Span { start, end: index },
                        ));
                    }
                    TokenKind::Reference
                } else {
                    TokenKind::Word
                };
                if text.starts_with('.') {
                    diagnostics.push(Diagnostic::error(
                        "R2L004",
                        "leading-dot words are not Ricochet 2 syntax; put the receiver first",
                        Span { start, end: index },
                    ));
                }
                if text.contains('-') && text != "-" {
                    diagnostics.push(Diagnostic::error(
                        "R2L005",
                        "multiword names use '_' because '-' is reserved for subtraction",
                        Span { start, end: index },
                    ));
                }
                if text.starts_with('!') && text != "!=" {
                    diagnostics.push(Diagnostic::error(
                        "R2L006",
                        "leading '!' words are not part of the canonical typed surface",
                        Span { start, end: index },
                    ));
                }
                push_token(source, &mut tokens, kind, start, index);
            }
        }
    }

    (Cst { tokens }, diagnostics)
}

fn push_token(source: &str, tokens: &mut Vec<Token>, kind: TokenKind, start: usize, end: usize) {
    tokens.push(Token {
        kind,
        text: source[start..end].to_string(),
        span: Span { start, end },
    });
}

fn single_token(source: &str, tokens: &mut Vec<Token>, kind: TokenKind, index: &mut usize) {
    let start = *index;
    *index += 1;
    push_token(source, tokens, kind, start, *index);
}

fn is_word_boundary(bytes: &[u8], index: usize) -> bool {
    matches!(
        bytes[index],
        b' ' | b'\t'
            | b'\r'
            | b'\n'
            | b'('
            | b')'
            | b'['
            | b']'
            | b'<'
            | b'>'
            | b','
            | b':'
            | b'+'
            | b'"'
    ) || (bytes[index] == b'-' && bytes.get(index + 1) == Some(&b'>'))
}

fn scan_number(bytes: &[u8], mut index: usize) -> usize {
    if bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if bytes.get(index) == Some(&b'.') && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        let mut exponent = index + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        if bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            index = exponent + 1;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
        }
    }
    index
}

fn validate_balanced_delimiters(cst: &Cst, diagnostics: &mut Vec<Diagnostic>) {
    let mut stack: Vec<(&TokenKind, Span)> = Vec::new();
    for token in &cst.tokens {
        match token.kind {
            TokenKind::LeftParen | TokenKind::LeftBracket => {
                stack.push((&token.kind, token.span));
            }
            TokenKind::RightParen => {
                if !matches!(stack.last(), Some((TokenKind::LeftParen, _))) {
                    diagnostics.push(Diagnostic::error("R2P001", "unmatched ')'", token.span));
                } else {
                    stack.pop();
                }
            }
            TokenKind::RightBracket => {
                if !matches!(stack.last(), Some((TokenKind::LeftBracket, _))) {
                    diagnostics.push(Diagnostic::error("R2P002", "unmatched ']'", token.span));
                } else {
                    stack.pop();
                }
            }
            _ => {}
        }
    }
    for (kind, span) in stack {
        let delimiter = match kind {
            TokenKind::LeftParen => '(',
            TokenKind::LeftBracket => '[',
            _ => unreachable!("only opening delimiters enter the stack"),
        };
        diagnostics.push(Diagnostic::error(
            "R2P003",
            format!("unclosed '{delimiter}'"),
            span,
        ));
    }
}

#[derive(Debug, Clone)]
struct Line {
    tokens: Vec<Token>,
    span: Span,
}

fn lines_from_cst(cst: &Cst) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut end = 0;

    for token in &cst.tokens {
        if token.kind == TokenKind::Newline {
            lines.push(Line {
                tokens: std::mem::take(&mut tokens),
                span: Span {
                    start,
                    end: token.span.start,
                },
            });
            start = token.span.end;
            end = start;
        } else {
            if tokens.is_empty() {
                start = token.span.start;
            }
            end = token.span.end;
            tokens.push(token.clone());
        }
    }

    if !tokens.is_empty() || lines.is_empty() {
        lines.push(Line {
            tokens,
            span: Span { start, end },
        });
    }
    lines
}

fn parser_tokens(tokens: &[Token]) -> Vec<Token> {
    tokens
        .iter()
        .filter(|token| !token.kind.is_parser_trivia())
        .cloned()
        .collect()
}

fn format_tokens(tokens: &[Token]) -> Vec<Token> {
    tokens
        .iter()
        .filter(|token| !token.kind.is_spacing_trivia())
        .cloned()
        .collect()
}

#[derive(Debug, Clone, Copy, Default)]
struct Stops {
    end: bool,
    else_word: bool,
    when: bool,
}

struct Parser<'a> {
    lines: Vec<Line>,
    position: usize,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn new(cst: &Cst, diagnostics: &'a mut Vec<Diagnostic>) -> Self {
        Self {
            lines: lines_from_cst(cst),
            position: 0,
            diagnostics,
        }
    }

    fn parse(&mut self) -> Ast {
        let nodes = self.parse_items(Stops::default());
        Ast { nodes }
    }

    fn parse_items(&mut self, stops: Stops) -> Vec<AstNode> {
        let mut nodes = Vec::new();
        while self.position < self.lines.len() {
            let tokens = parser_tokens(&self.lines[self.position].tokens);
            if tokens.is_empty() {
                self.position += 1;
                continue;
            }
            if (stops.end && is_exact_word(&tokens, "end"))
                || (stops.else_word && is_exact_word(&tokens, "else"))
                || (stops.when && ends_with_word(&tokens, "when"))
            {
                break;
            }
            if is_exact_word(&tokens, "end")
                || is_exact_word(&tokens, "else")
                || ends_with_word(&tokens, "when")
            {
                let word = tokens
                    .last()
                    .map_or("terminator", |token| token.text.as_str());
                self.diagnostics.push(Diagnostic::error(
                    "R2P004",
                    format!("unexpected '{word}'"),
                    self.lines[self.position].span,
                ));
                self.position += 1;
                continue;
            }
            nodes.push(self.parse_node(tokens));
        }
        nodes
    }

    fn parse_node(&mut self, tokens: Vec<Token>) -> AstNode {
        if is_generic_header(&tokens) {
            return self.parse_generic_declaration(tokens);
        }
        if declaration_kind(&tokens).is_some() {
            return self.parse_declaration(tokens, Vec::new(), None);
        }
        if ends_with_word(&tokens, "match") {
            return self.parse_match(tokens);
        }
        if ends_with_word(&tokens, "if") {
            return self.parse_if(tokens);
        }
        if ends_with_word(&tokens, "each") {
            return self.parse_block(tokens, BlockKind::Each);
        }
        if ends_with_word(&tokens, "with") {
            return self.parse_block(tokens, BlockKind::With);
        }
        if let Some(binding) = parse_binding(&tokens, self.diagnostics) {
            self.position += 1;
            return AstNode::Binding(binding);
        }

        let line = self.lines[self.position].clone();
        self.position += 1;
        AstNode::Expression(Expression {
            source: render_tokens(&tokens),
            span: line.span,
        })
    }

    fn parse_generic_declaration(&mut self, tokens: Vec<Token>) -> AstNode {
        let generic_line = self.lines[self.position].clone();
        let generics = parse_generics(&tokens, self.diagnostics);
        self.position += 1;

        while self.position < self.lines.len()
            && parser_tokens(&self.lines[self.position].tokens).is_empty()
        {
            self.position += 1;
        }

        if self.position >= self.lines.len() {
            self.diagnostics.push(Diagnostic::error(
                "R2P005",
                "a generic parameter header must be followed by a declaration",
                generic_line.span,
            ));
            return AstNode::Expression(Expression {
                source: render_tokens(&tokens),
                span: generic_line.span,
            });
        }

        let declaration_tokens = parser_tokens(&self.lines[self.position].tokens);
        if declaration_kind(&declaration_tokens).is_none() {
            self.diagnostics.push(Diagnostic::error(
                "R2P005",
                "a generic parameter header must be followed by a declaration",
                generic_line.span,
            ));
            return AstNode::Expression(Expression {
                source: render_tokens(&tokens),
                span: generic_line.span,
            });
        }

        self.parse_declaration(declaration_tokens, generics, Some(generic_line.span))
    }

    fn parse_declaration(
        &mut self,
        tokens: Vec<Token>,
        generics: Vec<GenericParameter>,
        generic_span: Option<Span>,
    ) -> AstNode {
        let header_line = self.lines[self.position].clone();
        let (kind, canonical_case) = declaration_kind(&tokens)
            .expect("parse_declaration is called only for declaration heads");
        if !canonical_case {
            let token = tokens.last().expect("a declaration has a terminal word");
            self.diagnostics.push(Diagnostic::error(
                "R2P006",
                format!(
                    "object declaration meta word must be spelled '{}'",
                    kind.canonical_word()
                ),
                token.span,
            ));
        }
        let mut declaration = parse_declaration_header(
            &tokens,
            kind,
            generics,
            generic_span.map_or(header_line.span, |span| span.cover(header_line.span)),
            self.diagnostics,
        );
        self.position += 1;

        if kind.opens_block(&declaration.modifiers) {
            declaration.body = self.parse_items(Stops {
                end: true,
                ..Stops::default()
            });
            if self.position < self.lines.len()
                && is_exact_word(&parser_tokens(&self.lines[self.position].tokens), "end")
            {
                declaration.span = declaration.span.cover(self.lines[self.position].span);
                self.position += 1;
            } else {
                self.diagnostics.push(Diagnostic::error(
                    "R2P007",
                    format!(
                        "{} '{}' requires a lowercase 'end'",
                        declaration.kind, declaration.name
                    ),
                    declaration.header_span,
                ));
            }
        }

        AstNode::Declaration(declaration)
    }

    fn parse_match(&mut self, tokens: Vec<Token>) -> AstNode {
        let header = self.lines[self.position].clone();
        let scrutinee = render_tokens(&tokens[..tokens.len() - 1]);
        self.position += 1;
        let mut arms = Vec::new();
        let mut end_span = header.span;

        while self.position < self.lines.len() {
            let arm_tokens = parser_tokens(&self.lines[self.position].tokens);
            if arm_tokens.is_empty() {
                self.position += 1;
                continue;
            }
            if is_exact_word(&arm_tokens, "end") {
                end_span = self.lines[self.position].span;
                self.position += 1;
                return AstNode::Match(MatchNode {
                    scrutinee,
                    arms,
                    span: header.span.cover(end_span),
                });
            }
            if !ends_with_word(&arm_tokens, "when") {
                self.diagnostics.push(Diagnostic::error(
                    "R2P008",
                    "a match body must begin with a postfix 'pattern when' arm",
                    self.lines[self.position].span,
                ));
                self.position += 1;
                continue;
            }

            let arm_header = self.lines[self.position].clone();
            let pattern = render_tokens(&arm_tokens[..arm_tokens.len() - 1]);
            self.position += 1;
            let body = self.parse_items(Stops {
                end: true,
                when: true,
                ..Stops::default()
            });
            let arm_end = body.last().map_or(arm_header.span, ast_node_span);
            arms.push(MatchArm {
                pattern,
                body,
                span: arm_header.span.cover(arm_end),
            });
        }

        self.diagnostics.push(Diagnostic::error(
            "R2P009",
            "match requires a lowercase 'end'",
            header.span,
        ));
        AstNode::Match(MatchNode {
            scrutinee,
            arms,
            span: header.span.cover(end_span),
        })
    }

    fn parse_if(&mut self, tokens: Vec<Token>) -> AstNode {
        let header = self.lines[self.position].clone();
        let condition = render_tokens(&tokens[..tokens.len() - 1]);
        self.position += 1;
        let then_body = self.parse_items(Stops {
            end: true,
            else_word: true,
            ..Stops::default()
        });
        let mut else_body = Vec::new();
        if self.position < self.lines.len()
            && is_exact_word(&parser_tokens(&self.lines[self.position].tokens), "else")
        {
            self.position += 1;
            else_body = self.parse_items(Stops {
                end: true,
                ..Stops::default()
            });
        }
        let end_span = self.consume_required_end("if", header.span);
        AstNode::If(IfNode {
            condition,
            then_body,
            else_body,
            span: header.span.cover(end_span),
        })
    }

    fn parse_block(&mut self, tokens: Vec<Token>, kind: BlockKind) -> AstNode {
        let header = self.lines[self.position].clone();
        let header_source = render_tokens(&tokens[..tokens.len() - 1]);
        self.position += 1;
        let body = self.parse_items(Stops {
            end: true,
            ..Stops::default()
        });
        let label = match kind {
            BlockKind::Each => "each",
            BlockKind::With => "with",
        };
        let end_span = self.consume_required_end(label, header.span);
        AstNode::Block(BlockNode {
            kind,
            header: header_source,
            body,
            span: header.span.cover(end_span),
        })
    }

    fn consume_required_end(&mut self, label: &str, header_span: Span) -> Span {
        if self.position < self.lines.len()
            && is_exact_word(&parser_tokens(&self.lines[self.position].tokens), "end")
        {
            let span = self.lines[self.position].span;
            self.position += 1;
            span
        } else {
            self.diagnostics.push(Diagnostic::error(
                "R2P007",
                format!("{label} requires a lowercase 'end'"),
                header_span,
            ));
            header_span
        }
    }
}

fn parse_declaration_header(
    tokens: &[Token],
    kind: DeclarationKind,
    generics: Vec<GenericParameter>,
    header_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> Declaration {
    let terminal = tokens.len() - 1;
    let mut name = "<missing>".to_string();
    let mut generics = generics;
    let mut related_type = None;
    let mut signature = None;
    let mut modifiers = Vec::new();

    match kind {
        DeclarationKind::Function | DeclarationKind::Method => {
            if let Some((left, right)) = outer_signature(tokens) {
                signature = parse_signature(&tokens[left..=right], diagnostics);
                if let Some(name_token) = tokens.get(right + 1) {
                    name = name_token.text.clone();
                    modifiers = tokens[right + 2..terminal]
                        .iter()
                        .map(|token| token.text.clone())
                        .collect();
                    validate_modifiers(&tokens[right + 2..terminal], diagnostics);
                } else {
                    diagnostics.push(Diagnostic::error(
                        "R2P010",
                        "a callable signature must be followed by its name",
                        tokens[left].span,
                    ));
                }
            } else {
                diagnostics.push(Diagnostic::error(
                    "R2P010",
                    "a function or Method requires a parenthesized typed signature",
                    header_span,
                ));
            }
        }
        DeclarationKind::Field => {
            let modifier_start = trailing_modifier_start(tokens, terminal);
            if let Some(name_token) = tokens.first() {
                name = name_token.text.clone();
            }
            if modifier_start > 1 {
                related_type = type_from_complete_slice(&tokens[1..modifier_start], diagnostics);
            } else {
                diagnostics.push(Diagnostic::error(
                    "R2P011",
                    "a Field requires a name followed by a type",
                    header_span,
                ));
            }
            modifiers = tokens[modifier_start..terminal]
                .iter()
                .map(|token| token.text.clone())
                .collect();
            validate_modifiers(&tokens[modifier_start..terminal], diagnostics);
        }
        DeclarationKind::Case => {
            if let Some(name_token) = tokens.first() {
                name = name_token.text.clone();
            }
            if terminal == 0 {
                diagnostics.push(Diagnostic::error(
                    "R2P012",
                    "a Case requires a constructor name",
                    header_span,
                ));
            } else if terminal > 1 {
                let has_payload_delimiters = tokens.get(1).map(|token| &token.kind)
                    == Some(&TokenKind::LeftParen)
                    && tokens.get(terminal - 1).map(|token| &token.kind)
                        == Some(&TokenKind::RightParen);
                if has_payload_delimiters && terminal > 3 {
                    related_type = type_from_complete_slice(&tokens[2..terminal - 1], diagnostics);
                } else if has_payload_delimiters && terminal == 3 {
                    diagnostics.push(Diagnostic::error(
                        "R2P012",
                        "a Case payload cannot be empty",
                        tokens[1].span.cover(tokens[2].span),
                    ));
                } else {
                    diagnostics.push(Diagnostic::error(
                        "R2P012",
                        "a Case payload uses Constructor(Type)",
                        tokens[1].span,
                    ));
                }
            }
        }
        DeclarationKind::Subclass | DeclarationKind::Implements => {
            let modifier_start = trailing_modifier_start(tokens, terminal);
            let mut cursor = 0;
            if let Some(first) = parse_type(tokens, &mut cursor, modifier_start, diagnostics) {
                name = first.source;
            }
            related_type = parse_type(tokens, &mut cursor, modifier_start, diagnostics);
            if related_type.is_none() {
                diagnostics.push(Diagnostic::error(
                    "R2P013",
                    format!("{kind} requires both a subject type and a related type"),
                    header_span,
                ));
            }
            if cursor != modifier_start {
                diagnostics.push(Diagnostic::error(
                    "R2P014",
                    format!("unexpected tokens in {kind} declaration head"),
                    tokens[cursor].span,
                ));
            }
            modifiers = tokens[modifier_start..terminal]
                .iter()
                .map(|token| token.text.clone())
                .collect();
            validate_modifiers(&tokens[modifier_start..terminal], diagnostics);
        }
        DeclarationKind::Record
        | DeclarationKind::Enum
        | DeclarationKind::Trait
        | DeclarationKind::Class => {
            let modifier_start = trailing_modifier_start(tokens, terminal);
            if modifier_start > 0 {
                let (declared_name, declared_generics) =
                    parse_declared_type_name(&tokens[..modifier_start], diagnostics);
                name = declared_name;
                generics.extend(declared_generics);
            } else {
                diagnostics.push(Diagnostic::error(
                    "R2P015",
                    format!("{kind} requires a type name"),
                    header_span,
                ));
            }
            modifiers = tokens[modifier_start..terminal]
                .iter()
                .map(|token| token.text.clone())
                .collect();
            validate_modifiers(&tokens[modifier_start..terminal], diagnostics);
        }
    }

    Declaration {
        kind,
        name,
        related_type,
        generics,
        signature,
        modifiers,
        body: Vec::new(),
        header_span,
        span: header_span,
    }
}

fn outer_signature(tokens: &[Token]) -> Option<(usize, usize)> {
    let left = tokens
        .iter()
        .position(|token| token.kind == TokenKind::LeftParen)?;
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate().skip(left) {
        match token.kind {
            TokenKind::LeftParen => depth += 1,
            TokenKind::RightParen => {
                depth -= 1;
                if depth == 0 {
                    return Some((left, index));
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_signature(tokens: &[Token], diagnostics: &mut Vec<Diagnostic>) -> Option<Signature> {
    if tokens.len() < 2 {
        return None;
    }
    let inner = &tokens[1..tokens.len() - 1];
    let Some(arrow) = inner
        .iter()
        .position(|token| token.kind == TokenKind::Arrow)
    else {
        diagnostics.push(Diagnostic::error(
            "R2P016",
            "typed signatures require exactly one '->' stack transition",
            tokens[0].span.cover(tokens[tokens.len() - 1].span),
        ));
        return None;
    };
    if inner[arrow + 1..]
        .iter()
        .any(|token| token.kind == TokenKind::Arrow)
    {
        diagnostics.push(Diagnostic::error(
            "R2P016",
            "typed signatures require exactly one '->' stack transition",
            inner[arrow].span,
        ));
    }

    let uses = inner[arrow + 1..]
        .iter()
        .position(|token| token.kind == TokenKind::Word && token.text == "uses")
        .map(|offset| arrow + 1 + offset);
    let output_end = uses.unwrap_or(inner.len());
    let inputs = parse_named_types(&inner[..arrow], true, diagnostics);
    let outputs = parse_named_types(&inner[arrow + 1..output_end], false, diagnostics);
    let effects = uses.map_or_else(Vec::new, |uses_index| {
        inner[uses_index + 1..]
            .iter()
            .filter_map(|token| {
                if token.kind == TokenKind::Word {
                    Some(token.text.clone())
                } else {
                    diagnostics.push(Diagnostic::error(
                        "R2P017",
                        "effect names after 'uses' must be words",
                        token.span,
                    ));
                    None
                }
            })
            .collect()
    });
    if let Some(uses_index) = uses.filter(|_| effects.is_empty()) {
        diagnostics.push(Diagnostic::error(
            "R2P017",
            "'uses' requires at least one effect name",
            inner[uses_index].span,
        ));
    }

    Some(Signature {
        inputs,
        outputs,
        effects,
        span: tokens[0].span.cover(tokens[tokens.len() - 1].span),
    })
}

fn parse_named_types(
    tokens: &[Token],
    require_names: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<NamedType> {
    let mut values = Vec::new();
    let mut cursor = 0;
    while cursor < tokens.len() {
        let name = if tokens.get(cursor + 1).is_some_and(|token| {
            token.kind == TokenKind::Colon && tokens[cursor].kind == TokenKind::Word
        }) {
            let name = tokens[cursor].text.clone();
            cursor += 2;
            Some(name)
        } else {
            if require_names {
                diagnostics.push(Diagnostic::error(
                    "R2P018",
                    "input stack entries require 'name: Type'",
                    tokens[cursor].span,
                ));
            }
            None
        };
        let before = cursor;
        if let Some(ty) = parse_type(tokens, &mut cursor, tokens.len(), diagnostics) {
            values.push(NamedType { name, ty });
        } else {
            if cursor == before {
                cursor += 1;
            }
            diagnostics.push(Diagnostic::error(
                "R2P019",
                "expected a type in the stack signature",
                tokens[before].span,
            ));
        }
    }
    values
}

fn parse_type(
    tokens: &[Token],
    cursor: &mut usize,
    end: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<TypeRef> {
    let start = *cursor;
    let base = tokens.get(*cursor)?;
    if base.kind != TokenKind::Word {
        return None;
    }
    *cursor += 1;
    if *cursor < end && tokens[*cursor].kind == TokenKind::LeftAngle {
        *cursor += 1;
        loop {
            if *cursor >= end {
                diagnostics.push(Diagnostic::error(
                    "R2P020",
                    "unclosed generic type argument list",
                    tokens[start].span,
                ));
                break;
            }
            if tokens[*cursor].kind == TokenKind::RightAngle {
                *cursor += 1;
                break;
            }
            if parse_type(tokens, cursor, end, diagnostics).is_none() {
                diagnostics.push(Diagnostic::error(
                    "R2P021",
                    "expected a type argument",
                    tokens[*cursor].span,
                ));
                *cursor += 1;
            }
            if *cursor < end && tokens[*cursor].kind == TokenKind::Comma {
                *cursor += 1;
            } else if *cursor < end && tokens[*cursor].kind != TokenKind::RightAngle {
                diagnostics.push(Diagnostic::error(
                    "R2P022",
                    "generic type arguments must be separated by ','",
                    tokens[*cursor].span,
                ));
            }
        }
    }
    let consumed_end = (*cursor).min(end);
    if consumed_end == start {
        return None;
    }
    Some(TypeRef {
        source: render_tokens(&tokens[start..consumed_end]),
        span: tokens[start].span.cover(tokens[consumed_end - 1].span),
    })
}

fn type_from_complete_slice(
    tokens: &[Token],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<TypeRef> {
    let mut cursor = 0;
    let value = parse_type(tokens, &mut cursor, tokens.len(), diagnostics);
    if cursor < tokens.len() {
        diagnostics.push(Diagnostic::error(
            "R2P023",
            "unexpected tokens after type",
            tokens[cursor].span,
        ));
    }
    value
}

fn parse_generics(tokens: &[Token], diagnostics: &mut Vec<Diagnostic>) -> Vec<GenericParameter> {
    if tokens.len() < 3
        || tokens.first().map(|token| &token.kind) != Some(&TokenKind::LeftAngle)
        || tokens.last().map(|token| &token.kind) != Some(&TokenKind::RightAngle)
    {
        return Vec::new();
    }
    let mut parameters = Vec::new();
    let mut start = 1;
    let mut depth = 0;
    for index in 1..tokens.len() {
        match tokens[index].kind {
            TokenKind::LeftAngle => depth += 1,
            TokenKind::RightAngle if depth > 0 => depth -= 1,
            TokenKind::Comma | TokenKind::RightAngle if depth == 0 => {
                if start < index {
                    if let Some(parameter) =
                        parse_generic_parameter(&tokens[start..index], diagnostics)
                    {
                        parameters.push(parameter);
                    }
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    parameters
}

fn parse_generic_parameter(
    tokens: &[Token],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<GenericParameter> {
    let name = tokens.first()?;
    if name.kind != TokenKind::Word {
        diagnostics.push(Diagnostic::error(
            "R2P024",
            "generic parameters begin with a type parameter name",
            name.span,
        ));
        return None;
    }
    let mut bounds = Vec::new();
    if tokens.len() > 1 {
        if tokens.get(1).map(|token| &token.kind) != Some(&TokenKind::Colon) {
            diagnostics.push(Diagnostic::error(
                "R2P025",
                "generic bounds follow ':'",
                tokens[1].span,
            ));
        } else {
            let mut expect_bound = true;
            for token in &tokens[2..] {
                if expect_bound && token.kind == TokenKind::Word {
                    bounds.push(token.text.clone());
                    expect_bound = false;
                } else if !expect_bound && token.kind == TokenKind::Plus {
                    expect_bound = true;
                } else {
                    diagnostics.push(Diagnostic::error(
                        "R2P026",
                        "generic bounds use 'Bound + Bound'",
                        token.span,
                    ));
                }
            }
            if expect_bound {
                diagnostics.push(Diagnostic::error(
                    "R2P026",
                    "generic bounds cannot end with '+'",
                    tokens[tokens.len() - 1].span,
                ));
            }
        }
    }
    Some(GenericParameter {
        name: name.text.clone(),
        bounds,
        span: name.span.cover(tokens[tokens.len() - 1].span),
    })
}

fn parse_binding(tokens: &[Token], diagnostics: &mut Vec<Diagnostic>) -> Option<Binding> {
    let operation = match tokens.last()?.text.as_str() {
        "let" => BindingOperation::Let,
        "var" => BindingOperation::Var,
        "set" => BindingOperation::Set,
        _ => return None,
    };
    if tokens.len() < 2 {
        diagnostics.push(Diagnostic::error(
            "R2P027",
            "binding operations require a target name",
            tokens.last()?.span,
        ));
        return Some(Binding {
            name: "<missing>".to_string(),
            operation,
            annotation: None,
            value_source: String::new(),
            span: tokens.last()?.span,
        });
    }
    let operation_index = tokens.len() - 1;
    let annotation_colon = tokens[..operation_index]
        .iter()
        .rposition(|token| token.kind == TokenKind::Colon);
    let (name_index, annotation) = if let Some(colon) = annotation_colon {
        if colon == 0 || colon + 1 == operation_index {
            diagnostics.push(Diagnostic::error(
                "R2P027",
                "an annotated binding uses 'name: Type' before the binding word",
                tokens[colon].span,
            ));
            (operation_index - 1, None)
        } else {
            let annotation =
                type_from_complete_slice(&tokens[colon + 1..operation_index], diagnostics);
            (colon - 1, annotation)
        }
    } else {
        (operation_index - 1, None)
    };
    let name = tokens[name_index].text.clone();
    let value_source = render_tokens(&tokens[..name_index]);
    Some(Binding {
        name,
        operation,
        annotation,
        value_source,
        span: tokens[0].span.cover(tokens[operation_index].span),
    })
}

fn declaration_kind(tokens: &[Token]) -> Option<(DeclarationKind, bool)> {
    let word = tokens.last()?.text.as_str();
    let exact = match word {
        "function" => return Some((DeclarationKind::Function, true)),
        "Method" => DeclarationKind::Method,
        "Record" => DeclarationKind::Record,
        "Enum" => DeclarationKind::Enum,
        "Trait" => DeclarationKind::Trait,
        "Class" => DeclarationKind::Class,
        "Subclass" => DeclarationKind::Subclass,
        "Implements" => DeclarationKind::Implements,
        "Field" => DeclarationKind::Field,
        "Case" => DeclarationKind::Case,
        _ => {
            let lowered = word.to_ascii_lowercase();
            let kind = match lowered.as_str() {
                "function" => DeclarationKind::Function,
                "method" => DeclarationKind::Method,
                "record" => DeclarationKind::Record,
                "enum" => DeclarationKind::Enum,
                "trait" => DeclarationKind::Trait,
                "class" => DeclarationKind::Class,
                "subclass" => DeclarationKind::Subclass,
                "implements" => DeclarationKind::Implements,
                "field" => DeclarationKind::Field,
                "case" => DeclarationKind::Case,
                _ => return None,
            };
            return Some((kind, false));
        }
    };
    Some((exact, true))
}

fn parse_declared_type_name(
    tokens: &[Token],
    diagnostics: &mut Vec<Diagnostic>,
) -> (String, Vec<GenericParameter>) {
    let Some(base) = tokens.first() else {
        return ("<missing>".to_string(), Vec::new());
    };
    if tokens.len() > 3
        && base.kind == TokenKind::Word
        && tokens[1].kind == TokenKind::LeftAngle
        && tokens.last().map(|token| &token.kind) == Some(&TokenKind::RightAngle)
    {
        return (base.text.clone(), parse_generics(&tokens[1..], diagnostics));
    }
    (render_tokens(tokens), Vec::new())
}

fn is_generic_header(tokens: &[Token]) -> bool {
    tokens.first().map(|token| &token.kind) == Some(&TokenKind::LeftAngle)
        && tokens.last().map(|token| &token.kind) == Some(&TokenKind::RightAngle)
}

fn trailing_modifier_start(tokens: &[Token], terminal: usize) -> usize {
    let mut start = terminal;
    while start > 0 && modifier_rank(&tokens[start - 1].text).is_some() {
        start -= 1;
    }
    start
}

fn validate_modifiers(tokens: &[Token], diagnostics: &mut Vec<Diagnostic>) {
    let mut previous = 0;
    let mut seen = std::collections::BTreeSet::new();
    for token in tokens {
        let Some(rank) = modifier_rank(&token.text) else {
            diagnostics.push(Diagnostic::error(
                "R2P028",
                format!("unknown declaration modifier '{}'", token.text),
                token.span,
            ));
            continue;
        };
        if !seen.insert(token.text.as_str()) {
            diagnostics.push(Diagnostic::error(
                "R2P029",
                format!("duplicate declaration modifier '{}'", token.text),
                token.span,
            ));
        }
        if rank < previous {
            diagnostics.push(Diagnostic::error(
                "R2P030",
                "modifier order is visibility, storage/dispatch, then shape",
                token.span,
            ));
        }
        previous = previous.max(rank);
    }
}

fn modifier_rank(word: &str) -> Option<u8> {
    match word {
        "public" | "protected" | "private" => Some(1),
        "static" => Some(2),
        "abstract" | "final" | "required" => Some(3),
        _ => None,
    }
}

fn is_exact_word(tokens: &[Token], word: &str) -> bool {
    tokens.len() == 1 && tokens[0].kind == TokenKind::Word && tokens[0].text == word
}

fn ends_with_word(tokens: &[Token], word: &str) -> bool {
    tokens
        .last()
        .is_some_and(|token| token.kind == TokenKind::Word && token.text == word)
}

fn opens_declaration_block(tokens: &[Token]) -> bool {
    let Some((kind, _)) = declaration_kind(tokens) else {
        return false;
    };
    let terminal = tokens.len() - 1;
    let modifier_start = if matches!(kind, DeclarationKind::Function | DeclarationKind::Method) {
        outer_signature(tokens).map_or(terminal, |(_, right)| right + 2)
    } else {
        trailing_modifier_start(tokens, terminal)
    };
    let modifiers = tokens[modifier_start.min(terminal)..terminal]
        .iter()
        .map(|token| token.text.clone())
        .collect::<Vec<_>>();
    kind.opens_block(&modifiers)
}

fn has_unclosed_left_bracket(tokens: &[Token]) -> bool {
    let mut depth = 0_i32;
    for token in tokens {
        match token.kind {
            TokenKind::LeftBracket => depth += 1,
            TokenKind::RightBracket => depth -= 1,
            _ => {}
        }
    }
    depth > 0
}

#[derive(Debug, Clone, Copy)]
enum FormatFrame {
    Block,
    Match,
    MatchArm,
    IfArm,
    Bracket,
}

fn render_tokens(tokens: &[Token]) -> String {
    let signature = tokens
        .first()
        .is_some_and(|token| token.kind == TokenKind::LeftParen)
        && (ends_with_word(tokens, "function") || ends_with_word(tokens, "Method"));
    let inline_bracket = tokens
        .iter()
        .any(|token| token.kind == TokenKind::LeftBracket)
        && tokens
            .iter()
            .any(|token| token.kind == TokenKind::RightBracket);
    let mut output = String::new();
    let mut signature_depth = 0_i32;

    for token in tokens {
        match token.kind {
            TokenKind::LeftParen => {
                trim_trailing_spaces(&mut output);
                output.push('(');
                if signature && signature_depth == 0 {
                    output.push(' ');
                }
                signature_depth += 1;
            }
            TokenKind::RightParen => {
                signature_depth -= 1;
                trim_trailing_spaces(&mut output);
                if signature && signature_depth == 0 && !output.ends_with("( ") {
                    output.push(' ');
                }
                output.push(')');
            }
            TokenKind::LeftAngle => {
                trim_trailing_spaces(&mut output);
                output.push('<');
            }
            TokenKind::RightAngle => {
                trim_trailing_spaces(&mut output);
                output.push('>');
            }
            TokenKind::LeftBracket => {
                ensure_space_if_needed(&mut output);
                output.push('[');
                if inline_bracket {
                    output.push(' ');
                }
            }
            TokenKind::RightBracket => {
                trim_trailing_spaces(&mut output);
                if inline_bracket && !output.ends_with("[ ") {
                    output.push(' ');
                }
                output.push(']');
            }
            TokenKind::Comma => {
                trim_trailing_spaces(&mut output);
                output.push_str(", ");
            }
            TokenKind::Colon => {
                trim_trailing_spaces(&mut output);
                output.push_str(": ");
            }
            TokenKind::Plus | TokenKind::Arrow => {
                ensure_space_if_needed(&mut output);
                output.push_str(&token.text);
                output.push(' ');
            }
            TokenKind::Comment => {
                ensure_space_if_needed(&mut output);
                output.push_str(token.text.trim());
            }
            _ => {
                let attached = output.ends_with('(')
                    || output.ends_with('<')
                    || (output.ends_with('[') && !inline_bracket);
                if !attached {
                    ensure_space_if_needed(&mut output);
                }
                output.push_str(&token.text);
            }
        }
    }
    trim_trailing_spaces(&mut output);
    output
}

fn ensure_space_if_needed(output: &mut String) {
    if !output.is_empty() && !output.ends_with(char::is_whitespace) {
        output.push(' ');
    }
}

fn trim_trailing_spaces(output: &mut String) {
    while output.ends_with(' ') {
        output.pop();
    }
}

fn ast_node_span(node: &AstNode) -> Span {
    match node {
        AstNode::Declaration(node) => node.span,
        AstNode::Binding(node) => node.span,
        AstNode::Match(node) => node.span,
        AstNode::If(node) => node.span,
        AstNode::Block(node) => node.span,
        AstNode::Expression(node) => node.span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADR_001: &str =
        include_str!("../../../architecture/adr/ADR-001-typed-postfix-surface.md");
    const VALID_PROOF: &str = include_str!("../fixtures/typed_postfix.ricochet");
    const INVALID_PROOF: &str = include_str!("../fixtures/invalid_surface.ricochet");

    #[test]
    fn lossless_cst_recovers_every_byte_and_token_identity() {
        let analysis = analyze(VALID_PROOF);
        let recovered = analysis.cst.recover_source();
        assert_eq!(recovered, VALID_PROOF);
        assert_eq!(analyze(&recovered).cst.tokens, analysis.cst.tokens);
    }

    #[test]
    fn formatter_is_idempotent_for_proof_corpus() {
        let once = format_source(VALID_PROOF);
        let twice = format_source(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn every_ricochet_example_in_adr_001_round_trips_and_formats() {
        let examples = ricochet_fences(ADR_001);
        assert_eq!(
            examples.len(),
            27,
            "keep this count explicit as the ADR changes"
        );
        for (index, example) in examples.iter().enumerate() {
            let analysis = analyze(example);
            assert_eq!(analysis.cst.recover_source(), *example, "example {index}");
            assert!(
                !analysis.ast.nodes.is_empty(),
                "example {index} needs an AST"
            );
            assert!(
                analysis.diagnostics.is_empty(),
                "example {index} diagnostics: {:#?}",
                analysis.diagnostics
            );
            let formatted = format_source(example);
            assert_eq!(format_source(&formatted), formatted, "example {index}");
        }
    }

    #[test]
    fn typed_declarations_lower_to_explicit_stack_rows() {
        let analysis = analyze(VALID_PROOF);
        assert!(!analysis.has_errors(), "{:#?}", analysis.diagnostics);
        let declarations = analysis.ast.declarations();
        let rows = declarations
            .iter()
            .filter_map(|declaration| {
                declaration
                    .signature
                    .as_ref()
                    .map(|signature| (declaration.name.as_str(), signature.stack_row()))
            })
            .collect::<Vec<_>>();
        assert!(rows.contains(&(
            "fetch",
            "[url: Url] -> [Result<Response, HttpError>] uses {async, network}".to_string()
        )));
        assert!(rows.contains(&(
            "div_rem",
            "[dividend: Int, divisor: Int] -> [quotient: Int, remainder: Int]".to_string()
        )));

        let option = declarations
            .iter()
            .find(|declaration| declaration.kind == DeclarationKind::Enum)
            .expect("the proof corpus declares Option<T>");
        assert_eq!(option.name, "Option");
        assert_eq!(option.generics[0].name, "T");
        let some = declarations
            .iter()
            .find(|declaration| {
                declaration.kind == DeclarationKind::Case && declaration.name == "Some"
            })
            .expect("the proof corpus declares Some(T)");
        assert_eq!(
            some.related_type.as_ref().map(|ty| ty.source.as_str()),
            Some("T")
        );

        let cache = analysis.ast.nodes.iter().find_map(|node| match node {
            AstNode::Binding(binding) if binding.name == "cache" => Some(binding),
            _ => None,
        });
        assert_eq!(
            cache
                .and_then(|binding| binding.annotation.as_ref())
                .map(|ty| ty.source.as_str()),
            Some("Map<String, Int>")
        );
    }

    #[test]
    fn canonical_spelling_failures_are_explicit() {
        let analysis = analyze("( -> Int ) bad public Function\nend\n-bad\n");
        let codes = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();
        assert_eq!(codes, vec!["R2P006", "R2L005"]);
    }

    #[test]
    fn deliberate_errors_have_stable_codes_and_spans() {
        let analysis = analyze(INVALID_PROOF);
        let actual = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.span.start, diagnostic.span.end))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                ("R2P016", 0, 28),
                ("R2P030", 100, 106),
                ("R2L004", 134, 144),
                ("R2L005", 145, 153),
                ("R2P007", 155, 203),
            ]
        );
    }

    #[test]
    fn mutation_fuzzing_never_panics() {
        let alphabet = [
            ' ', '\n', '(', ')', '[', ']', '<', '>', ':', ',', '$', '"', 'a', '0',
        ];
        let mut state = 0x5eed_u64;
        let mut sample = VALID_PROOF.chars().take(240).collect::<Vec<_>>();
        for _ in 0..4_000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let action = (state % 3) as usize;
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let index = (state as usize) % (sample.len() + 1);
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let value = alphabet[(state as usize) % alphabet.len()];
            match action {
                0 if sample.len() < 512 => sample.insert(index, value),
                1 if !sample.is_empty() => {
                    sample.remove(index.min(sample.len() - 1));
                }
                2 if !sample.is_empty() => {
                    let replacement = index.min(sample.len() - 1);
                    sample[replacement] = value;
                }
                _ => {}
            }
            let source = sample.iter().collect::<String>();
            let result = std::panic::catch_unwind(|| {
                let analysis = analyze(&source);
                let _ = analysis.cst.recover_source();
                let _ = format_source(&source);
            });
            assert!(result.is_ok(), "parser panicked for mutation: {source:?}");
        }
    }

    fn ricochet_fences(markdown: &str) -> Vec<String> {
        let mut examples = Vec::new();
        let mut current = None;
        for line in markdown.lines() {
            if line == "```ricochet" {
                current = Some(String::new());
            } else if line == "```" {
                if let Some(example) = current.take() {
                    examples.push(example);
                }
            } else if let Some(example) = &mut current {
                example.push_str(line);
                example.push('\n');
            }
        }
        examples
    }
}
