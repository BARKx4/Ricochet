use crate::token::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Class(ClassDecl),
    Method(MethodDecl),
    Function(FunctionDecl),
    Macro(MacroDecl),
    Expr {
        expr: Expr,
        span: Span,
        docs: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub name: String,
    pub superclass: String,
    pub body: Vec<Item>,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MethodDecl {
    pub name: String,
    pub args: Option<ArgsDecl>,
    pub body: Vec<SpannedExpr>,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub name: String,
    pub args: Option<ArgsDecl>,
    pub body: Vec<SpannedExpr>,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MacroDecl {
    pub name: String,
    pub args: Option<ArgsDecl>,
    pub body: Vec<SpannedExpr>,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArgsDecl {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpannedExpr {
    pub expr: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Symbol(String),
    BangWord(String),
    DotWord(String),
    Reference(String),
    String(String),
    Number(i64),
    Float(f64),
    Args(ArgsDecl),
    Block(Vec<SpannedExpr>),
    Sequence(Vec<SpannedExpr>),
    If {
        then_body: Vec<SpannedExpr>,
        else_body: Vec<SpannedExpr>,
    },
    While {
        condition: Vec<SpannedExpr>,
        body: Vec<SpannedExpr>,
    },
}
