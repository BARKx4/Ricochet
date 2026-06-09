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
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub name: String,
    pub superclass: String,
    pub body: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MethodDecl {
    pub name: String,
    pub args: Option<ArgsDecl>,
    pub body: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub name: String,
    pub args: Option<ArgsDecl>,
    pub body: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArgsDecl {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Symbol(String),
    BangWord(String),
    DotWord(String),
    String(String),
    Number(i64),
    Args(ArgsDecl),
    Block(Vec<Expr>),
    Sequence(Vec<Expr>),
    If { then_body: Vec<Expr>, else_body: Vec<Expr> },
}
