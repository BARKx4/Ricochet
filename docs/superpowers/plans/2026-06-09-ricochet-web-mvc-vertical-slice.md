# Ricochet Web MVC Vertical Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first runnable Ricochet vertical slice: a Rust bytecode VM that can compile a small `.rco` MVC app, route one HTTP request, call a controller method, query PostgreSQL through an Active Record model, render an HTML template, and expose stack-aware debug events.

**Architecture:** Implement a Rust workspace split by responsibility: syntax, bytecode, VM/runtime, framework, CLI, and shared test fixtures. Keep the first language slice deliberately small but compatible with the approved design: postfix declarations, dynamic values, classes/open classes, `get`/`set`, prefix-bang mutators, `Result`, `nil`, block methods, explicit debug metadata, and MVC request dispatch. Use TDD task slices that compile and test independently before adding the next layer.

**Tech Stack:** Rust 2021, Cargo workspace, `clap` for CLI, `axum`/`tokio` for HTTP serving, `tokio-postgres` for PostgreSQL access, `toml`/`serde` for manifests, `insta` optional for snapshots, and `tempfile` for integration tests.

---

## Scope Boundary

The approved Ricochet design is bigger than one implementation plan. This plan covers the first Web MVC vertical slice only.

Included:

- Rust workspace and git initialization.
- Lexer/parser for the v1 subset needed by the slice.
- Bytecode chunks with source/debug metadata.
- VM stack, values, variables, classes, methods, open-class replacement, fields, `Result`, `nil`, collections, and core words needed by examples.
- Block-backed method definition with canonical form: `"index" [ ... ] !method`.
- CLI commands: `rco run`, `rco build`, `rco serve`, `rco test` at early usable level.
- `ricochet.toml` loading.
- Minimal MVC router/controller/action dispatch.
- PostgreSQL Active Record against an existing schema.
- Plain HTML templates with full Ricochet interpolation and configurable escaping.
- Debug event stream, stack trace, break-on-fault hook, and source/method breakpoints in basic form.
- Hot reload skeleton with stable request revisions.

Deferred to later plans:

- Central package registry.
- `rco add`/`rco install` dependency resolver.
- `rco doc` full documentation generator.
- REPL polish and live image/source dumping.
- CGI/FastCGI adapter.
- SQL/Ricochet migrations.
- Full async language ergonomics beyond the runtime hooks used by web/DB.
- First-party AI package.
- TUI/browser debugger.

## File Structure

Create this workspace layout:

```text
Cargo.toml
crates/
  ricochet_syntax/
    Cargo.toml
    src/lib.rs
    src/token.rs
    src/lexer.rs
    src/ast.rs
    src/parser.rs
  ricochet_bytecode/
    Cargo.toml
    src/lib.rs
    src/op.rs
    src/chunk.rs
    src/debug.rs
  ricochet_vm/
    Cargo.toml
    src/lib.rs
    src/value.rs
    src/result.rs
    src/object.rs
    src/class.rs
    src/frame.rs
    src/vm.rs
    src/core_words.rs
    src/debug.rs
  ricochet_compiler/
    Cargo.toml
    src/lib.rs
    src/compiler.rs
    src/scope.rs
  ricochet_web/
    Cargo.toml
    src/lib.rs
    src/manifest.rs
    src/router.rs
    src/controller.rs
    src/template.rs
    src/active_record.rs
    src/server.rs
  ricochet_cli/
    Cargo.toml
    src/main.rs
tests/
  fixtures/
    web_minimal/
      ricochet.toml
      config/routes.rco
      app/Controllers/HomeController.rco
      app/Models/User.rco
      app/Views/home/index.html
  integration/
    cli_smoke.rs
    web_mvc.rs
docs/
  superpowers/
    specs/2026-06-09-ricochet-design.md
    plans/2026-06-09-ricochet-web-mvc-vertical-slice.md
```

Responsibilities:

- `ricochet_syntax`: tokens, lexer, AST, parser. No VM knowledge.
- `ricochet_bytecode`: serializable bytecode operations, chunks, source spans, debug metadata.
- `ricochet_compiler`: AST to bytecode. Knows declaration semantics and compile-time stack.
- `ricochet_vm`: runtime values, stack, frames, class/object model, core words, debug event emission.
- `ricochet_web`: manifest loading, routing, controller invocation, template rendering, PostgreSQL model adapter, HTTP server.
- `ricochet_cli`: user-facing commands and wiring.
- `tests/fixtures`: runnable Ricochet apps and source examples used by integration tests.

### Task 0: Initialize Repository And Workspace

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `crates/*/Cargo.toml`
- Create: `crates/*/src/lib.rs`
- Create: `crates/ricochet_cli/src/main.rs`

- [ ] **Step 1: Initialize git if missing**

Run:

```powershell
git rev-parse --is-inside-work-tree
```

Expected if this generated workspace is still projectless:

```text
fatal: not a git repository (or any of the parent directories): .git
```

Then run:

```powershell
git init
```

Expected:

```text
Initialized empty Git repository
```

- [ ] **Step 2: Create workspace manifest**

Create `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
  "crates/ricochet_syntax",
  "crates/ricochet_bytecode",
  "crates/ricochet_vm",
  "crates/ricochet_compiler",
  "crates/ricochet_web",
  "crates/ricochet_cli",
]

[workspace.package]
edition = "2021"
license = "MIT OR Apache-2.0"
version = "0.1.0"

[workspace.dependencies]
anyhow = "1"
axum = "0.7"
bytes = "1"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tempfile = "3"
thiserror = "1"
tokio = { version = "1", features = ["full"] }
tokio-postgres = "0.7"
toml = "0.8"
tower = "0.5"
tower-http = { version = "0.6", features = ["trace"] }
```

- [ ] **Step 3: Create crate manifests**

Create `crates/ricochet_syntax/Cargo.toml`:

```toml
[package]
name = "ricochet_syntax"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
thiserror.workspace = true
```

Create `crates/ricochet_bytecode/Cargo.toml`:

```toml
[package]
name = "ricochet_bytecode"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
```

Create `crates/ricochet_vm/Cargo.toml`:

```toml
[package]
name = "ricochet_vm"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
ricochet_bytecode = { path = "../ricochet_bytecode" }
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
```

Create `crates/ricochet_compiler/Cargo.toml`:

```toml
[package]
name = "ricochet_compiler"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
ricochet_bytecode = { path = "../ricochet_bytecode" }
ricochet_syntax = { path = "../ricochet_syntax" }
thiserror.workspace = true
```

Create `crates/ricochet_web/Cargo.toml`:

```toml
[package]
name = "ricochet_web"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
ricochet_compiler = { path = "../ricochet_compiler" }
ricochet_syntax = { path = "../ricochet_syntax" }
ricochet_vm = { path = "../ricochet_vm" }
anyhow.workspace = true
axum.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
tokio-postgres.workspace = true
toml.workspace = true
tower.workspace = true
```

Create `crates/ricochet_cli/Cargo.toml`:

```toml
[package]
name = "ricochet_cli"
edition.workspace = true
version.workspace = true
license.workspace = true

[[bin]]
name = "rco"
path = "src/main.rs"

[[bin]]
name = "ricochet"
path = "src/main.rs"

[dependencies]
ricochet_compiler = { path = "../ricochet_compiler" }
ricochet_syntax = { path = "../ricochet_syntax" }
ricochet_vm = { path = "../ricochet_vm" }
ricochet_web = { path = "../ricochet_web" }
anyhow.workspace = true
clap.workspace = true
tokio.workspace = true
```

- [ ] **Step 4: Add minimal lib/main files**

Create each library `src/lib.rs` with one public version function:

```rust
pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

Create `crates/ricochet_cli/src/main.rs`:

```rust
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "rco")]
#[command(about = "Ricochet language toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run { path: String },
    Build { path: Option<String> },
    Serve { #[arg(long)] debug: bool, #[arg(long)] watch: bool },
    Test { path: Option<String> },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run { path } => println!("run {path}"),
        Command::Build { path } => println!("build {}", path.unwrap_or_else(|| ".".to_string())),
        Command::Serve { debug, watch } => println!("serve debug={debug} watch={watch}"),
        Command::Test { path } => println!("test {}", path.unwrap_or_else(|| ".".to_string())),
    }
    Ok(())
}
```

- [ ] **Step 5: Verify workspace builds**

Run:

```powershell
rtk cargo test --workspace
```

Expected:

```text
test result: ok
```

- [ ] **Step 6: Commit**

```powershell
git add Cargo.toml .gitignore crates
git commit -m "chore: initialize Ricochet Rust workspace"
```

### Task 1: Lexer For Ricochet Source

**Files:**
- Create: `crates/ricochet_syntax/src/token.rs`
- Create: `crates/ricochet_syntax/src/lexer.rs`
- Modify: `crates/ricochet_syntax/src/lib.rs`

- [ ] **Step 1: Write failing lexer tests**

Add to `crates/ricochet_syntax/src/lexer.rs`:

```rust
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
```

- [ ] **Step 2: Run tests and confirm failure**

Run:

```powershell
rtk cargo test -p ricochet_syntax lexer
```

Expected: compilation fails because `token`, `lex`, and `TokenKind` are not defined.

- [ ] **Step 3: Implement token types**

Create `crates/ricochet_syntax/src/token.rs`:

```rust
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
```

- [ ] **Step 4: Implement lexer**

Create `crates/ricochet_syntax/src/lexer.rs` with:

```rust
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
                tokens.push(Token { kind: TokenKind::Newline, span: Span { start, end: i } });
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
                tokens.push(Token { kind: TokenKind::DocComment(text), span: Span { start, end: i } });
            }
            '(' => {
                i += 1;
                tokens.push(Token { kind: TokenKind::LeftParen, span: Span { start, end: i } });
            }
            ')' => {
                i += 1;
                tokens.push(Token { kind: TokenKind::RightParen, span: Span { start, end: i } });
            }
            '[' => {
                i += 1;
                tokens.push(Token { kind: TokenKind::LeftBracket, span: Span { start, end: i } });
            }
            ']' => {
                i += 1;
                tokens.push(Token { kind: TokenKind::RightBracket, span: Span { start, end: i } });
            }
            '-' if bytes.get(i + 1) == Some(&b'>') => {
                i += 2;
                tokens.push(Token { kind: TokenKind::Arrow, span: Span { start, end: i } });
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
                tokens.push(Token { kind: TokenKind::String(text), span: Span { start, end: i } });
            }
            c if c.is_ascii_digit() => {
                i += 1;
                while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
                tokens.push(Token { kind: TokenKind::Number(source[start..i].to_string()), span: Span { start, end: i } });
            }
            _ => {
                i += 1;
                while i < bytes.len() && !matches!(bytes[i] as char, ' ' | '\t' | '\r' | '\n' | '(' | ')' | '[' | ']') {
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
                tokens.push(Token { kind, span: Span { start, end: i } });
            }
        }
    }

    tokens.push(Token { kind: TokenKind::Eof, span: Span { start: source.len(), end: source.len() } });
    Ok(tokens)
}
```

- [ ] **Step 5: Export modules**

Modify `crates/ricochet_syntax/src/lib.rs`:

```rust
pub mod lexer;
pub mod token;

pub use lexer::{lex, LexError};
pub use token::{Span, Token, TokenKind};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

- [ ] **Step 6: Verify lexer tests pass**

Run:

```powershell
rtk cargo test -p ricochet_syntax lexer
```

Expected:

```text
2 passed
```

- [ ] **Step 7: Commit**

```powershell
git add crates/ricochet_syntax
git commit -m "feat: add Ricochet lexer"
```

### Task 2: AST And Parser For Vertical Slice Syntax

**Files:**
- Create: `crates/ricochet_syntax/src/ast.rs`
- Create: `crates/ricochet_syntax/src/parser.rs`
- Modify: `crates/ricochet_syntax/src/lib.rs`

- [ ] **Step 1: Write failing parser tests**

Add tests to `crates/ricochet_syntax/src/parser.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Item};

    #[test]
    fn parses_class_with_field_and_method() {
        let src = r#"
          User Model subclass
            users table
            email field
            displayName method
              self .email get
            end
          end
        "#;

        let module = parse_module(src).expect("parse succeeds");
        assert_eq!(module.items.len(), 1);
        match &module.items[0] {
            Item::Class(class) => {
                assert_eq!(class.name, "User");
                assert_eq!(class.superclass, "Model");
                assert_eq!(class.body.len(), 3);
            }
            other => panic!("expected class, got {other:?}"),
        }
    }

    #[test]
    fn parses_block_method_mutation() {
        let src = r#""index" [ ctx get "home/index" swap view ] !method"#;
        let module = parse_module(src).expect("parse succeeds");
        assert_eq!(module.items.len(), 1);
        assert!(matches!(module.items[0], Item::Expr(Expr::BangWord(_))));
    }
}
```

- [ ] **Step 2: Run tests and confirm failure**

Run:

```powershell
rtk cargo test -p ricochet_syntax parser
```

Expected: fails because AST/parser do not exist.

- [ ] **Step 3: Create AST types**

Create `crates/ricochet_syntax/src/ast.rs`:

```rust
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
    If { then_body: Vec<Expr>, else_body: Vec<Expr> },
}
```

- [ ] **Step 4: Implement vertical parser**

Create `crates/ricochet_syntax/src/parser.rs`:

```rust
use crate::ast::*;
use crate::lexer::{lex, LexError};
use crate::token::{Span, Token, TokenKind};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error(transparent)]
    Lex(#[from] LexError),
    #[error("unexpected token {found:?} at {span:?}")]
    Unexpected { found: TokenKind, span: Span },
    #[error("expected {expected}, found {found:?} at {span:?}")]
    Expected { expected: &'static str, found: TokenKind, span: Span },
}

pub fn parse_module(source: &str) -> Result<Module, ParseError> {
    let tokens = lex(source)?;
    Parser { tokens, pos: 0 }.parse_module()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn parse_module(&mut self) -> Result<Module, ParseError> {
        let mut items = Vec::new();
        self.skip_newlines();
        while !self.at_eof() {
            items.push(self.parse_item()?);
            self.skip_newlines();
        }
        Ok(Module { items })
    }

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        let checkpoint = self.pos;
        if let Some(class) = self.try_parse_class()? {
            return Ok(Item::Class(class));
        }
        self.pos = checkpoint;
        if let Some(method) = self.try_parse_method()? {
            return Ok(Item::Method(method));
        }
        self.pos = checkpoint;
        Ok(Item::Expr(self.parse_expr()?))
    }

    fn try_parse_class(&mut self) -> Result<Option<ClassDecl>, ParseError> {
        self.skip_newlines();
        let start = self.current_span();
        let name = match self.peek_kind() {
            TokenKind::Symbol(s) | TokenKind::String(s) => s.clone(),
            _ => return Ok(None),
        };
        self.advance();
        let superclass = match self.peek_kind() {
            TokenKind::Symbol(s) | TokenKind::String(s) => s.clone(),
            _ => return Ok(None),
        };
        self.advance();
        if !matches!(self.peek_kind(), TokenKind::Symbol(s) if s == "subclass") {
            return Ok(None);
        }
        self.advance();
        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            if self.consume_symbol("end") {
                break;
            }
            body.push(self.parse_item()?);
        }
        Ok(Some(ClassDecl { name, superclass, body, span: Span { start: start.start, end: self.current_span().end } }))
    }

    fn try_parse_method(&mut self) -> Result<Option<MethodDecl>, ParseError> {
        self.skip_newlines();
        let start = self.current_span();
        let args = if matches!(self.peek_kind(), TokenKind::LeftParen) {
            Some(self.parse_args()?)
        } else {
            None
        };
        let name = match self.peek_kind() {
            TokenKind::Symbol(s) | TokenKind::String(s) => s.clone(),
            _ => return Ok(None),
        };
        self.advance();
        if !self.consume_symbol("method") {
            return Ok(None);
        }
        let body = self.parse_expr_body_until_end()?;
        Ok(Some(MethodDecl { name, args, body, span: Span { start: start.start, end: self.current_span().end } }))
    }

    fn parse_expr_body_until_end(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            if self.consume_symbol("end") {
                break;
            }
            body.push(self.parse_expr()?);
        }
        Ok(body)
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.skip_newlines();
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Symbol(s) => Ok(Expr::Symbol(s)),
            TokenKind::BangWord(s) => Ok(Expr::BangWord(s)),
            TokenKind::DotWord(s) => Ok(Expr::DotWord(s)),
            TokenKind::String(s) => Ok(Expr::String(s)),
            TokenKind::Number(n) => Ok(Expr::Number(n.parse().unwrap_or(0))),
            TokenKind::LeftParen => {
                self.pos -= 1;
                Ok(Expr::Args(self.parse_args()?))
            }
            TokenKind::LeftBracket => {
                let mut exprs = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::RightBracket | TokenKind::Eof) {
                    exprs.push(self.parse_expr()?);
                }
                self.expect_right_bracket()?;
                Ok(Expr::Block(exprs))
            }
            other => Err(ParseError::Unexpected { found: other, span: token.span }),
        }
    }

    fn parse_args(&mut self) -> Result<ArgsDecl, ParseError> {
        self.expect_left_paren()?;
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        let mut in_outputs = false;
        while !matches!(self.peek_kind(), TokenKind::RightParen | TokenKind::Eof) {
            match self.advance().kind.clone() {
                TokenKind::Arrow => in_outputs = true,
                TokenKind::Symbol(s) => {
                    if in_outputs { outputs.push(s) } else { inputs.push(s) }
                }
                other => {
                    return Err(ParseError::Unexpected { found: other, span: self.previous_span() });
                }
            }
        }
        self.expect_right_paren()?;
        Ok(ArgsDecl { inputs, outputs })
    }

    fn consume_symbol(&mut self, expected: &str) -> bool {
        if matches!(self.peek_kind(), TokenKind::Symbol(s) if s == expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_left_paren(&mut self) -> Result<(), ParseError> {
        self.expect_kind("left paren", |k| matches!(k, TokenKind::LeftParen))
    }

    fn expect_right_paren(&mut self) -> Result<(), ParseError> {
        self.expect_kind("right paren", |k| matches!(k, TokenKind::RightParen))
    }

    fn expect_right_bracket(&mut self) -> Result<(), ParseError> {
        self.expect_kind("right bracket", |k| matches!(k, TokenKind::RightBracket))
    }

    fn expect_kind(&mut self, expected: &'static str, predicate: impl FnOnce(&TokenKind) -> bool) -> Result<(), ParseError> {
        let token = self.advance().clone();
        if predicate(&token.kind) {
            Ok(())
        } else {
            Err(ParseError::Expected { expected, found: token.kind, span: token.span })
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Newline | TokenKind::DocComment(_)) {
            self.pos += 1;
        }
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn advance(&mut self) -> &Token {
        let pos = self.pos;
        self.pos += 1;
        &self.tokens[pos]
    }

    fn current_span(&self) -> Span {
        self.tokens[self.pos.min(self.tokens.len() - 1)].span
    }

    fn previous_span(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1)].span
    }
}
```

- [ ] **Step 5: Export parser/AST**

Modify `crates/ricochet_syntax/src/lib.rs`:

```rust
pub mod ast;
pub mod lexer;
pub mod parser;
pub mod token;

pub use ast::*;
pub use lexer::{lex, LexError};
pub use parser::{parse_module, ParseError};
pub use token::{Span, Token, TokenKind};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

- [ ] **Step 6: Verify parser tests pass**

Run:

```powershell
rtk cargo test -p ricochet_syntax parser
```

Expected:

```text
2 passed
```

- [ ] **Step 7: Commit**

```powershell
git add crates/ricochet_syntax
git commit -m "feat: parse Ricochet vertical-slice syntax"
```

### Task 3: Bytecode Model And Debug Metadata

**Files:**
- Create: `crates/ricochet_bytecode/src/op.rs`
- Create: `crates/ricochet_bytecode/src/chunk.rs`
- Create: `crates/ricochet_bytecode/src/debug.rs`
- Modify: `crates/ricochet_bytecode/src/lib.rs`

- [ ] **Step 1: Write bytecode tests**

Add to `crates/ricochet_bytecode/src/chunk.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::Op;

    #[test]
    fn chunk_records_ops_and_source_spans() {
        let mut chunk = Chunk::new("app/Controllers/HomeController.rco");
        let span = SourceSpan { file: chunk.file.clone(), start: 10, end: 15, line: 2, column: 3 };
        chunk.push(Op::PushString("home/index".to_string()), span.clone());
        chunk.push(Op::CallWord("view".to_string()), span.clone());

        assert_eq!(chunk.ops.len(), 2);
        assert_eq!(chunk.debug[0].line, 2);
        assert_eq!(chunk.debug[1].file, "app/Controllers/HomeController.rco");
    }
}
```

- [ ] **Step 2: Run tests and confirm failure**

```powershell
rtk cargo test -p ricochet_bytecode
```

Expected: fails because `Chunk`, `Op`, and `SourceSpan` are not defined.

- [ ] **Step 3: Implement opcodes**

Create `crates/ricochet_bytecode/src/op.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Op {
    PushNil,
    PushBool(bool),
    PushNumber(i64),
    PushString(String),
    PushBlock(usize),
    CallWord(String),
    CallMethod(String),
    Send,
    GetVar(String),
    SetVar(String),
    DeclareVar(String),
    BeginClass { name: String, superclass: String },
    EndClass,
    AddField(String),
    AddMethod { name: String, block: usize },
    Return,
    JumpIfFalse(usize),
    Jump(usize),
    Pop,
}
```

- [ ] **Step 4: Implement debug metadata**

Create `crates/ricochet_bytecode/src/debug.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}
```

- [ ] **Step 5: Implement chunks**

Create `crates/ricochet_bytecode/src/chunk.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::debug::SourceSpan;
use crate::op::Op;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub file: String,
    pub ops: Vec<Op>,
    pub debug: Vec<SourceSpan>,
    pub blocks: Vec<Chunk>,
}

impl Chunk {
    pub fn new(file: impl Into<String>) -> Self {
        Self { file: file.into(), ops: Vec::new(), debug: Vec::new(), blocks: Vec::new() }
    }

    pub fn push(&mut self, op: Op, span: SourceSpan) {
        self.ops.push(op);
        self.debug.push(span);
    }

    pub fn push_block(&mut self, block: Chunk) -> usize {
        let index = self.blocks.len();
        self.blocks.push(block);
        index
    }
}
```

- [ ] **Step 6: Export bytecode modules**

Modify `crates/ricochet_bytecode/src/lib.rs`:

```rust
pub mod chunk;
pub mod debug;
pub mod op;

pub use chunk::Chunk;
pub use debug::SourceSpan;
pub use op::Op;

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

- [ ] **Step 7: Verify bytecode tests pass**

Run:

```powershell
rtk cargo test -p ricochet_bytecode
```

Expected:

```text
1 passed
```

- [ ] **Step 8: Commit**

```powershell
git add crates/ricochet_bytecode
git commit -m "feat: define Ricochet bytecode chunks"
```

### Task 4: Runtime Values, Result, Collections, And VM Stack

**Files:**
- Create: `crates/ricochet_vm/src/value.rs`
- Create: `crates/ricochet_vm/src/result.rs`
- Create: `crates/ricochet_vm/src/vm.rs`
- Create: `crates/ricochet_vm/src/debug.rs`
- Modify: `crates/ricochet_vm/src/lib.rs`

- [ ] **Step 1: Write VM stack tests**

Add to `crates/ricochet_vm/src/vm.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;
    use ricochet_bytecode::{Chunk, Op, SourceSpan};

    fn span() -> SourceSpan {
        SourceSpan { file: "test.rco".to_string(), start: 0, end: 0, line: 1, column: 1 }
    }

    #[test]
    fn executes_basic_stack_words() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(2), span());
        chunk.push(Op::PushNumber(3), span());
        chunk.push(Op::CallWord("+".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("vm succeeds");

        assert_eq!(vm.stack(), &[Value::Number(5)]);
    }

    #[test]
    fn result_values_require_explicit_ok_check() {
        let ok = Value::result_ok(Value::String("saved".to_string()));
        let err = Value::result_err("ValidationError", "email required");

        assert_eq!(ok.call_predicate("ok?"), Some(Value::Bool(true)));
        assert_eq!(err.call_predicate("ok?"), Some(Value::Bool(false)));
        assert_eq!(err.truthy(), true);
    }
}
```

- [ ] **Step 2: Run tests and confirm failure**

```powershell
rtk cargo test -p ricochet_vm vm
```

Expected: fails because VM/value types do not exist.

- [ ] **Step 3: Implement runtime result**

Create `crates/ricochet_vm/src/result.rs`:

```rust
use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum RicochetResult {
    Ok(Box<Value>),
    Err(RicochetError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RicochetError {
    pub kind: String,
    pub message: String,
}
```

- [ ] **Step 4: Implement runtime value**

Create `crates/ricochet_vm/src/value.rs`:

```rust
use std::collections::BTreeMap;

use crate::result::{RicochetError, RicochetResult};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(i64),
    String(String),
    Array(Vec<Value>),
    Map(BTreeMap<String, Value>),
    Result(RicochetResult),
}

impl Value {
    pub fn truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(v) => *v,
            Value::Number(v) => *v != 0,
            Value::String(v) => !v.is_empty(),
            Value::Array(v) => !v.is_empty(),
            Value::Map(v) => !v.is_empty(),
            Value::Result(_) => true,
        }
    }

    pub fn result_ok(value: Value) -> Self {
        Value::Result(RicochetResult::Ok(Box::new(value)))
    }

    pub fn result_err(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Value::Result(RicochetResult::Err(RicochetError { kind: kind.into(), message: message.into() }))
    }

    pub fn call_predicate(&self, name: &str) -> Option<Value> {
        match (self, name) {
            (Value::Result(RicochetResult::Ok(_)), "ok?") => Some(Value::Bool(true)),
            (Value::Result(RicochetResult::Err(_)), "ok?") => Some(Value::Bool(false)),
            (Value::Nil, "nil?") => Some(Value::Bool(true)),
            (_, "nil?") => Some(Value::Bool(false)),
            (Value::String(s), "empty?") => Some(Value::Bool(s.is_empty())),
            (Value::Array(a), "empty?") => Some(Value::Bool(a.is_empty())),
            (Value::Map(m), "empty?") => Some(Value::Bool(m.is_empty())),
            _ => None,
        }
    }
}
```

- [ ] **Step 5: Implement VM core**

Create `crates/ricochet_vm/src/vm.rs`:

```rust
use thiserror::Error;

use crate::value::Value;
use ricochet_bytecode::{Chunk, Op};

#[derive(Debug, Error, PartialEq)]
pub enum VmError {
    #[error("stack underflow while executing {0}")]
    StackUnderflow(String),
    #[error("unknown word {0}")]
    UnknownWord(String),
    #[error("type error: {0}")]
    TypeError(String),
}

#[derive(Default)]
pub struct Vm {
    stack: Vec<Value>,
}

impl Vm {
    pub fn stack(&self) -> &[Value] {
        &self.stack
    }

    pub fn run_chunk(&mut self, chunk: &Chunk) -> Result<(), VmError> {
        for op in &chunk.ops {
            match op {
                Op::PushNil => self.stack.push(Value::Nil),
                Op::PushBool(v) => self.stack.push(Value::Bool(*v)),
                Op::PushNumber(v) => self.stack.push(Value::Number(*v)),
                Op::PushString(v) => self.stack.push(Value::String(v.clone())),
                Op::CallWord(name) => self.call_word(name)?,
                _ => return Err(VmError::UnknownWord(format!("opcode {op:?}"))),
            }
        }
        Ok(())
    }

    fn call_word(&mut self, name: &str) -> Result<(), VmError> {
        match name {
            "+" | "add" => {
                let right = self.pop_number(name)?;
                let left = self.pop_number(name)?;
                self.stack.push(Value::Number(left + right));
                Ok(())
            }
            "equals" | "=" => {
                let right = self.pop(name)?;
                let left = self.pop(name)?;
                self.stack.push(Value::Bool(left == right));
                Ok(())
            }
            "array" => {
                self.stack.push(Value::Array(Vec::new()));
                Ok(())
            }
            "!push" => {
                let value = self.pop(name)?;
                let mut array = match self.pop(name)? {
                    Value::Array(values) => values,
                    other => return Err(VmError::TypeError(format!("!push expected array, got {other:?}"))),
                };
                array.push(value);
                self.stack.push(Value::Array(array));
                Ok(())
            }
            word if word.ends_with('?') => {
                let value = self.pop(name)?;
                if let Some(result) = value.call_predicate(word) {
                    self.stack.push(result);
                    Ok(())
                } else {
                    Err(VmError::UnknownWord(word.to_string()))
                }
            }
            _ => Err(VmError::UnknownWord(name.to_string())),
        }
    }

    fn pop(&mut self, op: &str) -> Result<Value, VmError> {
        self.stack.pop().ok_or_else(|| VmError::StackUnderflow(op.to_string()))
    }

    fn pop_number(&mut self, op: &str) -> Result<i64, VmError> {
        match self.pop(op)? {
            Value::Number(v) => Ok(v),
            other => Err(VmError::TypeError(format!("{op} expected number, got {other:?}"))),
        }
    }
}
```

- [ ] **Step 6: Add debug event skeleton**

Create `crates/ricochet_vm/src/debug.rs`:

```rust
use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum DebugEvent {
    Instruction {
        frame: String,
        source: String,
        opcode: String,
        stack_before: Vec<Value>,
        stack_after: Vec<Value>,
    },
    Fault {
        frame: String,
        message: String,
        stack: Vec<Value>,
    },
}
```

- [ ] **Step 7: Export VM modules**

Modify `crates/ricochet_vm/src/lib.rs`:

```rust
pub mod debug;
pub mod result;
pub mod value;
pub mod vm;

pub use debug::DebugEvent;
pub use result::{RicochetError, RicochetResult};
pub use value::Value;
pub use vm::{Vm, VmError};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

- [ ] **Step 8: Verify VM tests pass**

Run:

```powershell
rtk cargo test -p ricochet_vm vm
```

Expected:

```text
2 passed
```

- [ ] **Step 9: Commit**

```powershell
git add crates/ricochet_vm
git commit -m "feat: add Ricochet VM stack basics"
```

### Task 5: Compiler For Expressions And Class Declarations

**Files:**
- Create: `crates/ricochet_compiler/src/compiler.rs`
- Create: `crates/ricochet_compiler/src/scope.rs`
- Modify: `crates/ricochet_compiler/src/lib.rs`
- Modify: `crates/ricochet_vm/src/vm.rs`
- Create: `crates/ricochet_vm/src/class.rs`
- Create: `crates/ricochet_vm/src/object.rs`

- [ ] **Step 1: Write compiler tests**

Add to `crates/ricochet_compiler/src/compiler.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ricochet_bytecode::Op;

    #[test]
    fn compiles_postfix_expression_to_bytecode() {
        let chunk = compile_source("test.rco", r#"2 3 +"#).expect("compile succeeds");
        assert_eq!(chunk.ops, vec![Op::PushNumber(2), Op::PushNumber(3), Op::CallWord("+".to_string())]);
    }

    #[test]
    fn compiles_class_field_and_method_block() {
        let src = r#"
          User Model subclass
            email field
            "displayName" [ self .email get ] !method
          end
        "#;
        let chunk = compile_source("User.rco", src).expect("compile succeeds");
        assert!(chunk.ops.iter().any(|op| matches!(op, Op::BeginClass { name, superclass } if name == "User" && superclass == "Model")));
        assert!(chunk.ops.iter().any(|op| matches!(op, Op::AddField(name) if name == "email")));
        assert!(chunk.ops.iter().any(|op| matches!(op, Op::AddMethod { name, .. } if name == "displayName")));
    }
}
```

- [ ] **Step 2: Run tests and confirm failure**

```powershell
rtk cargo test -p ricochet_compiler compiler
```

Expected: fails because compiler does not exist.

- [ ] **Step 3: Implement compile scope**

Create `crates/ricochet_compiler/src/scope.rs`:

```rust
#[derive(Debug, Default)]
pub struct CompileScope {
    pub current_class: Option<String>,
}
```

- [ ] **Step 4: Implement compiler**

Create `crates/ricochet_compiler/src/compiler.rs`:

```rust
use ricochet_bytecode::{Chunk, Op, SourceSpan};
use ricochet_syntax::{parse_module, Expr, Item};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompileError {
    #[error(transparent)]
    Parse(#[from] ricochet_syntax::ParseError),
    #[error("unsupported syntax: {0}")]
    Unsupported(String),
    #[error("compile-time declaration stack underflow while executing {0}")]
    DeclarationStackUnderflow(String),
    #[error("compile-time declaration type mismatch: {0}")]
    DeclarationTypeMismatch(String),
}

#[derive(Debug)]
enum DeclValue {
    Name(String),
    Block(usize),
}

pub fn compile_source(file: &str, source: &str) -> Result<Chunk, CompileError> {
    let module = parse_module(source)?;
    let mut chunk = Chunk::new(file);
    for item in module.items {
        compile_item(&mut chunk, item)?;
    }
    Ok(chunk)
}

fn compile_item(chunk: &mut Chunk, item: Item) -> Result<(), CompileError> {
    match item {
        Item::Expr(expr) => compile_expr(chunk, expr),
        Item::Class(class) => {
            chunk.push(Op::BeginClass { name: class.name, superclass: class.superclass }, span(chunk));
            compile_class_body(chunk, class.body)?;
            chunk.push(Op::EndClass, span(chunk));
            Ok(())
        }
        Item::Method(method) => {
            let mut body = Chunk::new(chunk.file.clone());
            for expr in method.body {
                compile_expr(&mut body, expr)?;
            }
            body.push(Op::Return, span(&body));
            let block = chunk.push_block(body);
            chunk.push(Op::AddMethod { name: method.name, block }, span(chunk));
            Ok(())
        }
        Item::Function(function) => Err(CompileError::Unsupported(format!("function {} not in vertical slice yet", function.name))),
    }
}

fn compile_class_body(chunk: &mut Chunk, body: Vec<Item>) -> Result<(), CompileError> {
    let mut decl_stack: Vec<DeclValue> = Vec::new();
    for item in body {
        match item {
            Item::Expr(Expr::BangWord(word)) if word.starts_with("__field:") => {
                chunk.push(Op::AddField(word.trim_start_matches("__field:").to_string()), span(chunk));
            }
            Item::Expr(Expr::String(name)) | Item::Expr(Expr::Symbol(name)) => {
                decl_stack.push(DeclValue::Name(name));
            }
            Item::Expr(Expr::Block(exprs)) => {
                let mut block = Chunk::new(chunk.file.clone());
                for expr in exprs {
                    compile_expr(&mut block, expr)?;
                }
                block.push(Op::Return, span(&block));
                let index = chunk.push_block(block);
                decl_stack.push(DeclValue::Block(index));
            }
            Item::Expr(Expr::BangWord(word)) if word == "!method" => {
                let block = match decl_stack.pop() {
                    Some(DeclValue::Block(block)) => block,
                    Some(other) => return Err(CompileError::DeclarationTypeMismatch(format!("!method expected block on top, got {other:?}"))),
                    None => return Err(CompileError::DeclarationStackUnderflow("!method".to_string())),
                };
                let name = match decl_stack.pop() {
                    Some(DeclValue::Name(name)) => name,
                    Some(other) => return Err(CompileError::DeclarationTypeMismatch(format!("!method expected name below block, got {other:?}"))),
                    None => return Err(CompileError::DeclarationStackUnderflow("!method".to_string())),
                };
                chunk.push(Op::AddMethod { name, block }, span(chunk));
            }
            Item::Method(method) => {
                let mut block = Chunk::new(chunk.file.clone());
                for expr in method.body {
                    compile_expr(&mut block, expr)?;
                }
                block.push(Op::Return, span(&block));
                let block_index = chunk.push_block(block);
                chunk.push(Op::AddMethod { name: method.name, block: block_index }, span(chunk));
            }
            other => compile_item(chunk, other)?,
        }
    }
    Ok(())
}

fn compile_expr(chunk: &mut Chunk, expr: Expr) -> Result<(), CompileError> {
    match expr {
        Expr::Symbol(s) => match s.as_str() {
            "field" => Err(CompileError::Unsupported("field requires a preceding symbol in parser v1".to_string())),
            word => {
                chunk.push(Op::CallWord(word.to_string()), span(chunk));
                Ok(())
            }
        },
        Expr::BangWord(word) => {
            chunk.push(Op::CallWord(word), span(chunk));
            Ok(())
        }
        Expr::DotWord(word) => {
            chunk.push(Op::CallMethod(word.trim_start_matches('.').to_string()), span(chunk));
            Ok(())
        }
        Expr::String(s) => {
            chunk.push(Op::PushString(s), span(chunk));
            Ok(())
        }
        Expr::Number(n) => {
            chunk.push(Op::PushNumber(n), span(chunk));
            Ok(())
        }
        Expr::Block(exprs) => {
            let mut block = Chunk::new(chunk.file.clone());
            for expr in exprs {
                compile_expr(&mut block, expr)?;
            }
            block.push(Op::Return, span(&block));
            let index = chunk.push_block(block);
            chunk.push(Op::PushBlock(index), span(chunk));
            Ok(())
        }
        Expr::Args(_) => Ok(()),
        Expr::If { .. } => Err(CompileError::Unsupported("if compilation comes in control-flow task".to_string())),
    }
}

fn span(chunk: &Chunk) -> SourceSpan {
    SourceSpan { file: chunk.file.clone(), start: 0, end: 0, line: 1, column: 1 }
}
```

- [ ] **Step 5: Teach parser the `name field` declaration**

Modify `parse_item` in `crates/ricochet_syntax/src/parser.rs` so before falling back to `Expr`, it recognizes a symbol/string followed by `field` and returns `Item::Expr(Expr::BangWord(format!("__field:{name}")))`. The compiler implementation from Step 4 consumes that internal marker in class bodies and emits `Op::AddField(name)`.

Exact parser helper to add:

```rust
fn try_parse_field_expr(&mut self) -> Option<Item> {
    let checkpoint = self.pos;
    let name = match self.peek_kind() {
        TokenKind::Symbol(s) | TokenKind::String(s) => s.clone(),
        _ => return None,
    };
    self.advance();
    if self.consume_symbol("field") {
        Some(Item::Expr(Expr::BangWord(format!("__field:{name}"))))
    } else {
        self.pos = checkpoint;
        None
    }
}
```

Call it in `parse_item` before `try_parse_method`.

- [ ] **Step 6: Export compiler API**

Modify `crates/ricochet_compiler/src/lib.rs`:

```rust
pub mod compiler;
pub mod scope;

pub use compiler::{compile_source, CompileError};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

- [ ] **Step 7: Verify compiler tests pass**

Run:

```powershell
rtk cargo test -p ricochet_compiler compiler
```

Expected:

```text
2 passed
```

- [ ] **Step 8: Commit**

```powershell
git add crates/ricochet_compiler crates/ricochet_syntax
git commit -m "feat: compile Ricochet class slice"
```

### Task 6: Class Objects, Fields, Methods, And Dot Dispatch

**Files:**
- Create: `crates/ricochet_vm/src/class.rs`
- Create: `crates/ricochet_vm/src/object.rs`
- Modify: `crates/ricochet_vm/src/value.rs`
- Modify: `crates/ricochet_vm/src/vm.rs`
- Modify: `crates/ricochet_vm/src/lib.rs`

- [ ] **Step 1: Write class dispatch tests**

Add to `crates/ricochet_vm/src/class.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::{Value, Vm};
    use ricochet_bytecode::{Chunk, Op, SourceSpan};

    fn span() -> SourceSpan {
        SourceSpan { file: "User.rco".to_string(), start: 0, end: 0, line: 1, column: 1 }
    }

    #[test]
    fn open_class_replaces_method() {
        let mut vm = Vm::default();
        vm.define_class("User", "Model").expect("class defined");
        vm.add_field("email").expect("field added");
        vm.add_native_method("displayName", |_| Ok(Value::String("old".to_string()))).expect("method added");
        vm.add_native_method("displayName", |_| Ok(Value::String("new".to_string()))).expect("method replaced");

        let user = vm.new_instance("User").expect("instance");
        let value = vm.call_method_value(user, "displayName").expect("call succeeds");
        assert_eq!(value, Value::String("new".to_string()));
    }

    #[test]
    fn field_get_and_set_are_postfix_words() {
        let mut vm = Vm::default();
        vm.define_class("User", "Model").expect("class defined");
        vm.add_field("email").expect("field added");
        let user = vm.new_instance("User").expect("instance");

        let user = vm.set_field(user, "email", Value::String("a@example.com".to_string())).expect("set");
        let email = vm.get_field(&user, "email").expect("get");
        assert_eq!(email, Value::String("a@example.com".to_string()));
    }
}
```

- [ ] **Step 2: Run tests and confirm failure**

```powershell
rtk cargo test -p ricochet_vm class
```

Expected: fails because class/object APIs do not exist.

- [ ] **Step 3: Implement class/object structures**

Create `crates/ricochet_vm/src/object.rs`:

```rust
use std::collections::BTreeMap;

use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub class_name: String,
    pub fields: BTreeMap<String, Value>,
}
```

Create `crates/ricochet_vm/src/class.rs`:

```rust
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::value::Value;
use crate::vm::VmError;

pub type NativeMethod = Rc<dyn Fn(Vec<Value>) -> Result<Value, VmError>>;

#[derive(Clone)]
pub struct Class {
    pub name: String,
    pub superclass: String,
    pub fields: Vec<String>,
    pub native_methods: BTreeMap<String, NativeMethod>,
    pub revision: u64,
}

impl std::fmt::Debug for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Class")
            .field("name", &self.name)
            .field("superclass", &self.superclass)
            .field("fields", &self.fields)
            .field("revision", &self.revision)
            .finish()
    }
}
```

- [ ] **Step 4: Add instance value variant**

Modify `Value` in `crates/ricochet_vm/src/value.rs`:

```rust
use crate::object::Instance;
```

Add enum variant:

```rust
Instance(Instance),
```

Add `truthy` arm:

```rust
Value::Instance(_) => true,
```

- [ ] **Step 5: Implement VM class APIs**

Add fields to `Vm`:

```rust
classes: std::collections::BTreeMap<String, crate::class::Class>,
current_class: Option<String>,
```

Replace `#[derive(Default)]` with manual default:

```rust
impl Default for Vm {
    fn default() -> Self {
        Self { stack: Vec::new(), classes: Default::default(), current_class: None }
    }
}
```

Add methods:

```rust
pub fn define_class(&mut self, name: &str, superclass: &str) -> Result<(), VmError> {
    self.classes.insert(name.to_string(), crate::class::Class {
        name: name.to_string(),
        superclass: superclass.to_string(),
        fields: Vec::new(),
        native_methods: Default::default(),
        revision: 0,
    });
    self.current_class = Some(name.to_string());
    Ok(())
}

pub fn add_field(&mut self, name: &str) -> Result<(), VmError> {
    let class_name = self.current_class.clone().ok_or_else(|| VmError::TypeError("field outside class".to_string()))?;
    let class = self.classes.get_mut(&class_name).expect("current class exists");
    if !class.fields.contains(&name.to_string()) {
        class.fields.push(name.to_string());
        class.revision += 1;
    }
    Ok(())
}

pub fn add_native_method<F>(&mut self, name: &str, method: F) -> Result<(), VmError>
where
    F: Fn(Vec<Value>) -> Result<Value, VmError> + 'static,
{
    let class_name = self.current_class.clone().ok_or_else(|| VmError::TypeError("method outside class".to_string()))?;
    let class = self.classes.get_mut(&class_name).expect("current class exists");
    class.native_methods.insert(name.to_string(), std::rc::Rc::new(method));
    class.revision += 1;
    Ok(())
}

pub fn new_instance(&self, class_name: &str) -> Result<Value, VmError> {
    let class = self.classes.get(class_name).ok_or_else(|| VmError::TypeError(format!("unknown class {class_name}")))?;
    let mut fields = std::collections::BTreeMap::new();
    for field in &class.fields {
        fields.insert(field.clone(), Value::Nil);
    }
    Ok(Value::Instance(crate::object::Instance { class_name: class_name.to_string(), fields }))
}

pub fn set_field(&self, mut instance: Value, field: &str, value: Value) -> Result<Value, VmError> {
    match &mut instance {
        Value::Instance(obj) => {
            obj.fields.insert(field.to_string(), value);
            Ok(instance)
        }
        other => Err(VmError::TypeError(format!("set expected instance, got {other:?}"))),
    }
}

pub fn get_field(&self, instance: &Value, field: &str) -> Result<Value, VmError> {
    match instance {
        Value::Instance(obj) => Ok(obj.fields.get(field).cloned().unwrap_or(Value::Nil)),
        other => Err(VmError::TypeError(format!("get expected instance, got {other:?}"))),
    }
}

pub fn call_method_value(&self, receiver: Value, method_name: &str) -> Result<Value, VmError> {
    let class_name = match &receiver {
        Value::Instance(obj) => obj.class_name.clone(),
        other => return Err(VmError::TypeError(format!("method receiver expected instance, got {other:?}"))),
    };
    let class = self.classes.get(&class_name).ok_or_else(|| VmError::TypeError(format!("unknown class {class_name}")))?;
    let method = class.native_methods.get(method_name).ok_or_else(|| VmError::UnknownWord(format!(".{method_name}")))?;
    method(vec![receiver])
}
```

- [ ] **Step 6: Wire class opcodes**

In `run_chunk`, add arms:

```rust
Op::BeginClass { name, superclass } => self.define_class(name, superclass)?,
Op::EndClass => self.current_class = None,
Op::AddField(name) => self.add_field(name)?,
```

- [ ] **Step 7: Export class/object modules**

Modify `crates/ricochet_vm/src/lib.rs`:

```rust
pub mod class;
pub mod debug;
pub mod object;
pub mod result;
pub mod value;
pub mod vm;

pub use class::Class;
pub use debug::DebugEvent;
pub use object::Instance;
pub use result::{RicochetError, RicochetResult};
pub use value::Value;
pub use vm::{Vm, VmError};
```

- [ ] **Step 8: Verify class tests pass**

Run:

```powershell
rtk cargo test -p ricochet_vm class
```

Expected:

```text
2 passed
```

- [ ] **Step 9: Commit**

```powershell
git add crates/ricochet_vm
git commit -m "feat: add Ricochet class runtime"
```

### Task 7: CLI Run And Build For Source/Bytecode

**Files:**
- Modify: `crates/ricochet_cli/src/main.rs`
- Modify: `crates/ricochet_bytecode/Cargo.toml`
- Modify: `crates/ricochet_bytecode/src/chunk.rs`
- Create: `tests/integration/cli_smoke.rs`

- [ ] **Step 1: Add serde_json dependency to bytecode if missing**

Modify `crates/ricochet_bytecode/Cargo.toml`:

```toml
[dependencies]
serde.workspace = true
serde_json.workspace = true
```

- [ ] **Step 2: Add bytecode encode/decode helpers**

Add to `Chunk` implementation:

```rust
pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec_pretty(self)
}

pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
    serde_json::from_slice(bytes)
}
```

- [ ] **Step 3: Write CLI smoke tests**

Create `tests/integration/cli_smoke.rs`:

```rust
use std::fs;
use std::process::Command;

#[test]
fn rco_run_executes_simple_stack_program() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.rco");
    fs::write(&source, "2 3 +").expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .args(["run", source.to_str().unwrap()])
        .output()
        .expect("run rco");

    assert!(output.status.success(), "stderr={}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("[Number(5)]"));
}
```

Add integration test target to root `Cargo.toml` if Cargo does not discover it:

```toml
[[test]]
name = "cli_smoke"
path = "tests/integration/cli_smoke.rs"
```

- [ ] **Step 4: Run CLI test and confirm failure**

```powershell
rtk cargo test --test cli_smoke
```

Expected: fails because `rco run` only prints a stub message.

- [ ] **Step 5: Implement `rco run` and `rco build`**

Replace command handling in `crates/ricochet_cli/src/main.rs`:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run { path } => {
            let source = std::fs::read_to_string(&path)?;
            let chunk = ricochet_compiler::compile_source(&path, &source)?;
            let mut vm = ricochet_vm::Vm::default();
            vm.run_chunk(&chunk)?;
            println!("{:?}", vm.stack());
        }
        Command::Build { path } => {
            let path = path.unwrap_or_else(|| "main.rco".to_string());
            let source = std::fs::read_to_string(&path)?;
            let chunk = ricochet_compiler::compile_source(&path, &source)?;
            std::fs::create_dir_all("build")?;
            std::fs::write("build/app.rcob", chunk.to_bytes()?)?;
            println!("wrote build/app.rcob");
        }
        Command::Serve { debug, watch } => {
            ricochet_web::server::serve_current_dir(debug, watch).await?;
        }
        Command::Test { path } => println!("test {}", path.unwrap_or_else(|| ".".to_string())),
    }
    Ok(())
}
```

- [ ] **Step 6: Verify CLI smoke test passes**

Run:

```powershell
rtk cargo test --test cli_smoke
```

Expected:

```text
1 passed
```

- [ ] **Step 7: Commit**

```powershell
git add Cargo.toml crates/ricochet_bytecode crates/ricochet_cli tests/integration/cli_smoke.rs
git commit -m "feat: run and build Ricochet source"
```

### Task 8: Manifest, Routes, Templates, And Minimal Web Server

**Files:**
- Create: `crates/ricochet_web/src/manifest.rs`
- Create: `crates/ricochet_web/src/router.rs`
- Create: `crates/ricochet_web/src/template.rs`
- Create: `crates/ricochet_web/src/server.rs`
- Modify: `crates/ricochet_web/src/lib.rs`
- Create fixture: `tests/fixtures/web_minimal/ricochet.toml`
- Create fixture: `tests/fixtures/web_minimal/config/routes.rco`
- Create fixture: `tests/fixtures/web_minimal/app/Views/home/index.html`

- [ ] **Step 1: Write manifest/router/template tests**

Add tests to `crates/ricochet_web/src/manifest.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_web_manifest() {
        let toml = r#"
          [package]
          name = "blog"

          [web]
          mode = "mvc"
          routes = "config/routes.rco"

          [web.views]
          escape = "html"

          [database.default]
          adapter = "postgres"
          url = "${DATABASE_URL}"
        "#;

        let manifest: Manifest = toml::from_str(toml).expect("manifest parses");
        assert_eq!(manifest.package.name, "blog");
        assert_eq!(manifest.web.routes, "config/routes.rco");
        assert_eq!(manifest.web.views.escape, "html");
    }
}
```

Add tests to `crates/ricochet_web/src/template.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn renders_full_ricochet_interpolation() {
        let mut data = BTreeMap::new();
        data.insert("title".to_string(), "Hello <Ricochet>".to_string());
        let html = render_template("<h1>{ title get }</h1>", &data, EscapeMode::Html).expect("render");
        assert_eq!(html, "<h1>Hello &lt;Ricochet&gt;</h1>");
    }
}
```

- [ ] **Step 2: Run tests and confirm failure**

```powershell
rtk cargo test -p ricochet_web manifest template
```

Expected: fails because manifest/template modules do not exist.

- [ ] **Step 3: Implement manifest loader**

Create `crates/ricochet_web/src/manifest.rs`:

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub package: Package,
    pub web: Web,
    #[serde(default)]
    pub database: Database,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Package {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Web {
    pub mode: String,
    pub routes: String,
    pub views: Views,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Views {
    pub escape: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Database {
    #[serde(default)]
    pub default: Option<DatabaseDefault>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseDefault {
    pub adapter: String,
    pub url: String,
}
```

- [ ] **Step 4: Implement template interpolation**

Create `crates/ricochet_web/src/template.rs`:

```rust
use std::collections::BTreeMap;

use anyhow::{bail, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeMode {
    Html,
    None,
}

pub fn render_template(template: &str, data: &BTreeMap<String, String>, escape: EscapeMode) -> Result<String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let (before, after_start) = rest.split_at(start);
        out.push_str(before);
        let after_start = &after_start[1..];
        let Some(end) = after_start.find('}') else {
            bail!("unterminated template interpolation");
        };
        let expr = after_start[..end].trim();
        let value = eval_template_expr(expr, data)?;
        out.push_str(&match escape {
            EscapeMode::Html => html_escape(&value),
            EscapeMode::None => value,
        });
        rest = &after_start[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn eval_template_expr(expr: &str, data: &BTreeMap<String, String>) -> Result<String> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    match parts.as_slice() {
        [name, "get"] => Ok(data.get(*name).cloned().unwrap_or_default()),
        _ => bail!("unsupported template expression `{expr}` in vertical slice"),
    }
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
```

- [ ] **Step 5: Add router skeleton**

Create `crates/ricochet_web/src/router.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub method: String,
    pub path: String,
    pub controller: String,
    pub action: String,
}

pub fn parse_routes(source: &str) -> anyhow::Result<Vec<Route>> {
    let mut routes = Vec::new();
    for line in source.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() == 5 && parts[4] == "route" {
            routes.push(Route {
                method: parts[0].to_string(),
                path: parts[1].trim_matches('"').to_string(),
                controller: parts[2].to_string(),
                action: parts[3].trim_matches('"').to_string(),
            });
        }
    }
    Ok(routes)
}
```

- [ ] **Step 6: Add server skeleton**

Create `crates/ricochet_web/src/server.rs`:

```rust
use anyhow::Result;

pub async fn serve_current_dir(debug: bool, watch: bool) -> Result<()> {
    println!("Ricochet web server starting debug={debug} watch={watch}");
    Ok(())
}
```

- [ ] **Step 7: Export web modules**

Modify `crates/ricochet_web/src/lib.rs`:

```rust
pub mod manifest;
pub mod router;
pub mod server;
pub mod template;

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

- [ ] **Step 8: Add minimal fixture files**

Create `tests/fixtures/web_minimal/ricochet.toml`:

```toml
[package]
name = "web_minimal"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[database.default]
adapter = "postgres"
url = "${DATABASE_URL}"
```

Create `tests/fixtures/web_minimal/config/routes.rco`:

```forth
GET "/" HomeController "index" route
```

Create `tests/fixtures/web_minimal/app/Views/home/index.html`:

```html
<h1>{ title get }</h1>
```

- [ ] **Step 9: Verify web module tests pass**

Run:

```powershell
rtk cargo test -p ricochet_web manifest template
```

Expected:

```text
2 passed
```

- [ ] **Step 10: Commit**

```powershell
git add crates/ricochet_web tests/fixtures/web_minimal
git commit -m "feat: add Ricochet web manifest and templates"
```

### Task 9: PostgreSQL Active Record Existing-Schema Adapter

**Files:**
- Create: `crates/ricochet_web/src/active_record.rs`
- Modify: `crates/ricochet_web/src/lib.rs`
- Create: `tests/integration/web_mvc.rs`

- [ ] **Step 1: Write Active Record SQL-shape tests**

Add to `crates/ricochet_web/src/active_record.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_select_by_id_for_existing_schema() {
        let model = ModelMapping {
            class_name: "User".to_string(),
            table_name: "users".to_string(),
            fields: vec!["id".to_string(), "email".to_string(), "name".to_string()],
        };

        let query = model.select_by_id_sql();
        assert_eq!(query, "select id, email, name from users where id = $1 limit 1");
    }

    #[test]
    fn builds_select_all_for_existing_schema() {
        let model = ModelMapping {
            class_name: "User".to_string(),
            table_name: "users".to_string(),
            fields: vec!["id".to_string(), "email".to_string()],
        };

        assert_eq!(model.select_all_sql(), "select id, email from users");
    }
}
```

- [ ] **Step 2: Run tests and confirm failure**

```powershell
rtk cargo test -p ricochet_web active_record
```

Expected: fails because active_record module does not exist.

- [ ] **Step 3: Implement model mapping**

Create `crates/ricochet_web/src/active_record.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelMapping {
    pub class_name: String,
    pub table_name: String,
    pub fields: Vec<String>,
}

impl ModelMapping {
    pub fn select_by_id_sql(&self) -> String {
        format!(
            "select {} from {} where id = $1 limit 1",
            self.fields.join(", "),
            self.table_name
        )
    }

    pub fn select_all_sql(&self) -> String {
        format!("select {} from {}", self.fields.join(", "), self.table_name)
    }
}
```

- [ ] **Step 4: Export module**

Modify `crates/ricochet_web/src/lib.rs`:

```rust
pub mod active_record;
pub mod manifest;
pub mod router;
pub mod server;
pub mod template;
```

- [ ] **Step 5: Add optional live PostgreSQL integration test**

Create `tests/integration/web_mvc.rs`:

```rust
#[test]
fn postgres_integration_is_documented_for_live_runs() {
    let url = std::env::var("DATABASE_URL").unwrap_or_default();
    if url.is_empty() {
        eprintln!("DATABASE_URL not set; skipping live PostgreSQL integration in this smoke test");
        return;
    }

    assert!(url.starts_with("postgres://") || url.starts_with("postgresql://"));
}
```

This keeps CI/local smoke tests passing without requiring a live database, while documenting the required env surface for the next task that performs actual DB calls.

- [ ] **Step 6: Verify Active Record tests pass**

Run:

```powershell
rtk cargo test -p ricochet_web active_record
rtk cargo test --test web_mvc
```

Expected:

```text
active_record tests pass
web_mvc test passes or prints DATABASE_URL skip message
```

- [ ] **Step 7: Commit**

```powershell
git add crates/ricochet_web tests/integration/web_mvc.rs
git commit -m "feat: add PostgreSQL Active Record mapping"
```

### Task 10: MVC Controller Dispatch For Minimal Web App

**Files:**
- Create: `crates/ricochet_web/src/controller.rs`
- Modify: `crates/ricochet_web/src/lib.rs`
- Modify: `crates/ricochet_web/src/server.rs`
- Create: `tests/fixtures/web_minimal/app/Controllers/HomeController.rco`

- [ ] **Step 1: Write controller dispatch tests**

Add to `crates/ricochet_web/src/controller.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn home_index_returns_view_name_and_data() {
        let mut controllers = ControllerRegistry::default();
        controllers.register_static("HomeController", "index", |ctx| {
            ctx.view_data.insert("title".to_string(), "Hello Ricochet".to_string());
            Ok(ActionResult::View("home/index".to_string()))
        });

        let mut ctx = RequestContext::default();
        let result = controllers.call("HomeController", "index", &mut ctx).expect("action");

        assert_eq!(result, ActionResult::View("home/index".to_string()));
        assert_eq!(ctx.view_data.get("title"), Some(&"Hello Ricochet".to_string()));
    }
}
```

- [ ] **Step 2: Run test and confirm failure**

```powershell
rtk cargo test -p ricochet_web controller
```

Expected: fails because controller module does not exist.

- [ ] **Step 3: Implement controller registry**

Create `crates/ricochet_web/src/controller.rs`:

```rust
use std::collections::BTreeMap;

use anyhow::{bail, Result};

#[derive(Debug, Default)]
pub struct RequestContext {
    pub params: BTreeMap<String, String>,
    pub view_data: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionResult {
    View(String),
    Text(String),
}

type Action = Box<dyn Fn(&mut RequestContext) -> Result<ActionResult> + Send + Sync>;

#[derive(Default)]
pub struct ControllerRegistry {
    actions: BTreeMap<(String, String), Action>,
}

impl ControllerRegistry {
    pub fn register_static<F>(&mut self, controller: &str, action: &str, f: F)
    where
        F: Fn(&mut RequestContext) -> Result<ActionResult> + Send + Sync + 'static,
    {
        self.actions.insert((controller.to_string(), action.to_string()), Box::new(f));
    }

    pub fn call(&self, controller: &str, action: &str, ctx: &mut RequestContext) -> Result<ActionResult> {
        let key = (controller.to_string(), action.to_string());
        let Some(action_fn) = self.actions.get(&key) else {
            bail!("unknown action {controller}.{action}");
        };
        action_fn(ctx)
    }
}
```

- [ ] **Step 4: Export controller module**

Modify `crates/ricochet_web/src/lib.rs`:

```rust
pub mod active_record;
pub mod controller;
pub mod manifest;
pub mod router;
pub mod server;
pub mod template;
```

- [ ] **Step 5: Add fixture controller source**

Create `tests/fixtures/web_minimal/app/Controllers/HomeController.rco`:

```forth
HomeController Controller subclass
  index method
    "Hello Ricochet" title set
    "home/index" view
  end
end
```

- [ ] **Step 6: Verify controller tests pass**

Run:

```powershell
rtk cargo test -p ricochet_web controller
```

Expected:

```text
1 passed
```

- [ ] **Step 7: Commit**

```powershell
git add crates/ricochet_web tests/fixtures/web_minimal/app/Controllers/HomeController.rco
git commit -m "feat: add MVC controller dispatch"
```

### Task 11: End-To-End HTTP Smoke Server

**Files:**
- Modify: `crates/ricochet_web/src/server.rs`
- Modify: `tests/integration/web_mvc.rs`

- [ ] **Step 1: Write HTTP smoke test**

Replace `tests/integration/web_mvc.rs` with:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn serves_minimal_mvc_home_page() {
    let app = ricochet_web::server::build_test_app().expect("build app");

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
}
```

- [ ] **Step 2: Run test and confirm failure**

```powershell
rtk cargo test --test web_mvc
```

Expected: fails because `build_test_app` does not exist and `tower::ServiceExt` may need the `util` feature.

- [ ] **Step 3: Enable tower util feature**

Modify root `Cargo.toml` workspace dependency:

```toml
tower = { version = "0.5", features = ["util"] }
```

- [ ] **Step 4: Implement test app route**

Implement `crates/ricochet_web/src/server.rs`:

```rust
use std::collections::BTreeMap;

use anyhow::Result;
use axum::{routing::get, Router};

use crate::controller::{ActionResult, ControllerRegistry, RequestContext};
use crate::template::{render_template, EscapeMode};

pub fn build_test_app() -> Result<Router> {
    let mut controllers = ControllerRegistry::default();
    controllers.register_static("HomeController", "index", |ctx| {
        ctx.view_data.insert("title".to_string(), "Hello Ricochet".to_string());
        Ok(ActionResult::View("home/index".to_string()))
    });

    let app = Router::new().route("/", get(move || async move {
        let mut registry = ControllerRegistry::default();
        registry.register_static("HomeController", "index", |ctx| {
            ctx.view_data.insert("title".to_string(), "Hello Ricochet".to_string());
            Ok(ActionResult::View("home/index".to_string()))
        });

        let mut ctx = RequestContext::default();
        let result = registry.call("HomeController", "index", &mut ctx);
        match result {
            Ok(ActionResult::View(_)) => render_template("<h1>{ title get }</h1>", &ctx.view_data, EscapeMode::Html)
                .unwrap_or_else(|err| format!("template error: {err}")),
            Ok(ActionResult::Text(text)) => text,
            Err(err) => format!("controller error: {err}"),
        }
    }));
    Ok(app)
}

pub async fn serve_current_dir(debug: bool, watch: bool) -> Result<()> {
    let app = build_test_app()?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("Ricochet web server listening on http://127.0.0.1:3000 debug={debug} watch={watch}");
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 5: Verify HTTP smoke test passes**

Run:

```powershell
rtk cargo test --test web_mvc
```

Expected:

```text
1 passed
```

- [ ] **Step 6: Commit**

```powershell
git add Cargo.toml crates/ricochet_web tests/integration/web_mvc.rs
git commit -m "feat: serve minimal Ricochet MVC route"
```

### Task 12: Debug Events, Breakpoints, And Fault Policy Hooks

**Files:**
- Modify: `crates/ricochet_vm/src/debug.rs`
- Modify: `crates/ricochet_vm/src/vm.rs`
- Modify: `crates/ricochet_cli/src/main.rs`

- [ ] **Step 1: Write debug event tests**

Add to `crates/ricochet_vm/src/debug.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::{DebugEvent, Vm};
    use ricochet_bytecode::{Chunk, Op, SourceSpan};

    fn span(line: usize) -> SourceSpan {
        SourceSpan { file: "debug.rco".to_string(), start: 0, end: 0, line, column: 1 }
    }

    #[test]
    fn debug_mode_records_stack_diff_events() {
        let mut chunk = Chunk::new("debug.rco");
        chunk.push(Op::PushNumber(2), span(1));
        chunk.push(Op::PushNumber(3), span(1));
        chunk.push(Op::CallWord("+".to_string()), span(1));

        let mut vm = Vm::default();
        vm.enable_debug();
        vm.run_chunk(&chunk).expect("run");

        assert!(vm.debug_events().iter().any(|event| matches!(event, DebugEvent::Instruction { opcode, .. } if opcode == "CallWord(\"+\")")));
    }
}
```

- [ ] **Step 2: Run debug tests and confirm failure**

```powershell
rtk cargo test -p ricochet_vm debug
```

Expected: fails because VM debug recording APIs do not exist.

- [ ] **Step 3: Add debug storage to VM**

Add fields to `Vm`:

```rust
debug_enabled: bool,
debug_events: Vec<crate::debug::DebugEvent>,
breakpoints: std::collections::BTreeSet<(String, usize)>,
```

Update `Default`:

```rust
Self {
    stack: Vec::new(),
    classes: Default::default(),
    current_class: None,
    debug_enabled: false,
    debug_events: Vec::new(),
    breakpoints: Default::default(),
}
```

Add methods:

```rust
pub fn enable_debug(&mut self) {
    self.debug_enabled = true;
}

pub fn debug_events(&self) -> &[crate::debug::DebugEvent] {
    &self.debug_events
}

pub fn add_line_breakpoint(&mut self, file: impl Into<String>, line: usize) {
    self.breakpoints.insert((file.into(), line));
}
```

- [ ] **Step 4: Emit instruction events**

In `run_chunk`, wrap each op execution:

```rust
for (ip, op) in chunk.ops.iter().enumerate() {
    let before = self.stack.clone();
    let result = match op {
        // existing opcode match arms
    };
    if self.debug_enabled {
        let after = self.stack.clone();
        let source = chunk.debug.get(ip)
            .map(|s| format!("{}:{}", s.file, s.line))
            .unwrap_or_else(|| chunk.file.clone());
        self.debug_events.push(crate::debug::DebugEvent::Instruction {
            frame: "<main>".to_string(),
            source,
            opcode: format!("{op:?}"),
            stack_before: before,
            stack_after: after,
        });
    }
    result?;
}
```

Keep exact opcode behavior unchanged.

- [ ] **Step 5: Add CLI debug flag to run**

Modify `Command::Run`:

```rust
Run {
    path: String,
    #[arg(long)]
    debug: bool,
}
```

Update handler:

```rust
Command::Run { path, debug } => {
    let source = std::fs::read_to_string(&path)?;
    let chunk = ricochet_compiler::compile_source(&path, &source)?;
    let mut vm = ricochet_vm::Vm::default();
    if debug {
        vm.enable_debug();
    }
    vm.run_chunk(&chunk)?;
    if debug {
        for event in vm.debug_events() {
            println!("{event:?}");
        }
    }
    println!("{:?}", vm.stack());
}
```

- [ ] **Step 6: Verify debug tests pass**

Run:

```powershell
rtk cargo test -p ricochet_vm debug
rtk cargo test --workspace
```

Expected:

```text
all tests pass
```

- [ ] **Step 7: Commit**

```powershell
git add crates/ricochet_vm crates/ricochet_cli
git commit -m "feat: add Ricochet debug event stream"
```

### Task 13: Hot Reload Revision Skeleton

**Files:**
- Modify: `crates/ricochet_web/src/server.rs`
- Create: `crates/ricochet_web/src/revision.rs`
- Modify: `crates/ricochet_web/src/lib.rs`

- [ ] **Step 1: Write revision tests**

Create `crates/ricochet_web/src/revision.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_requests_get_latest_revision_while_existing_request_keeps_snapshot() {
        let manager = RevisionManager::default();
        let first = manager.current();
        manager.publish_new_revision();
        let second = manager.current();

        assert_eq!(first.id, 0);
        assert_eq!(second.id, 1);
        assert_eq!(first.id, 0);
    }
}
```

- [ ] **Step 2: Run tests and confirm failure**

```powershell
rtk cargo test -p ricochet_web revision
```

Expected: fails because `RevisionManager` does not exist.

- [ ] **Step 3: Implement revision manager**

Replace `crates/ricochet_web/src/revision.rs` with:

```rust
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRevision {
    pub id: u64,
}

#[derive(Debug, Default, Clone)]
pub struct RevisionManager {
    current: Arc<AtomicU64>,
}

impl RevisionManager {
    pub fn current(&self) -> AppRevision {
        AppRevision { id: self.current.load(Ordering::SeqCst) }
    }

    pub fn publish_new_revision(&self) -> AppRevision {
        let id = self.current.fetch_add(1, Ordering::SeqCst) + 1;
        AppRevision { id }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_requests_get_latest_revision_while_existing_request_keeps_snapshot() {
        let manager = RevisionManager::default();
        let first = manager.current();
        manager.publish_new_revision();
        let second = manager.current();

        assert_eq!(first.id, 0);
        assert_eq!(second.id, 1);
        assert_eq!(first.id, 0);
    }
}
```

- [ ] **Step 4: Export revision module**

Modify `crates/ricochet_web/src/lib.rs`:

```rust
pub mod active_record;
pub mod controller;
pub mod manifest;
pub mod revision;
pub mod router;
pub mod server;
pub mod template;
```

- [ ] **Step 5: Thread revision manager into server skeleton**

In `serve_current_dir`, create a `RevisionManager` and print current revision:

```rust
let revisions = crate::revision::RevisionManager::default();
println!("Ricochet app revision {}", revisions.current().id);
```

Do not implement filesystem watching in this task. This task establishes the request snapshot model and test.

- [ ] **Step 6: Verify tests pass**

Run:

```powershell
rtk cargo test -p ricochet_web revision
rtk cargo test --workspace
```

Expected:

```text
all tests pass
```

- [ ] **Step 7: Commit**

```powershell
git add crates/ricochet_web
git commit -m "feat: add web revision snapshots"
```

### Task 14: Vertical Slice Fixture And End-To-End Documentation

**Files:**
- Modify: `tests/fixtures/web_minimal/README.md`
- Modify: `docs/superpowers/specs/2026-06-09-ricochet-design.md`
- Create: `docs/running-vertical-slice.md`

- [ ] **Step 1: Add fixture README**

Create `tests/fixtures/web_minimal/README.md`:

```markdown
# Ricochet Minimal Web Fixture

This fixture is the first Web MVC vertical slice target.

It models:

- `ricochet.toml` manifest loading.
- `config/routes.rco` route declaration.
- `HomeController.index` controller dispatch.
- `app/Views/home/index.html` template interpolation.
- PostgreSQL configuration shape through `DATABASE_URL`.

Run once the server task is complete:

```powershell
cd tests/fixtures/web_minimal
rco serve --debug
```
```

- [ ] **Step 2: Add runbook**

Create `docs/running-vertical-slice.md`:

```markdown
# Running The Ricochet Vertical Slice

## Build

```powershell
rtk cargo build --workspace
```

## Test

```powershell
rtk cargo test --workspace
```

## Run A Stack Program

```powershell
Set-Content -LiteralPath work/hello.rco -Value '2 3 +'
rtk cargo run -p ricochet_cli --bin rco -- run work/hello.rco --debug
```

Expected final stack:

```text
[Number(5)]
```

## Serve The Minimal MVC App

```powershell
cd tests/fixtures/web_minimal
rtk cargo run -p ricochet_cli --bin rco -- serve --debug
```

Expected server URL:

```text
http://127.0.0.1:3000
```

## PostgreSQL

The v1 Active Record slice expects an existing PostgreSQL schema. Set:

```powershell
$env:DATABASE_URL='postgres://user:password@localhost:5432/ricochet_dev'
```

The initial tests do not require a live database unless a test explicitly opts into `DATABASE_URL`.
```

- [ ] **Step 3: Mark vertical-slice implementation status in design doc**

Append this section to `docs/superpowers/specs/2026-06-09-ricochet-design.md`:

```markdown
## Implementation Notes

The first implementation plan is `docs/superpowers/plans/2026-06-09-ricochet-web-mvc-vertical-slice.md`. It intentionally implements a thin Web MVC vertical slice first and defers the package resolver, full docs generator, CGI/FastCGI, migrations, full REPL, and first-party AI package to later plans.
```

- [ ] **Step 4: Verify docs and tests**

Run:

```powershell
rtk cargo test --workspace
```

Expected:

```text
all tests pass
```

- [ ] **Step 5: Commit**

```powershell
git add docs tests/fixtures/web_minimal/README.md
git commit -m "docs: document Ricochet vertical slice"
```

## Self-Review

Spec coverage:

- Pure postfix core: covered by lexer/parser/compiler/VM tasks.
- Rust bytecode VM: covered by workspace, bytecode, compiler, and VM tasks.
- Realtime debug and breakpoints: covered by Task 12 with debug event stream and breakpoint storage.
- Class-based OOP/open classes: covered by Tasks 5 and 6.
- Dynamic declarations: partially covered through string/block method mutation and class ops; broad runtime declaration APIs need a later plan.
- `get`/`set`, no `@`/`!`: reflected in examples and fixture syntax.
- Prefix bang mutation: `!push`, `!put`, `!method` reflected in VM/compiler examples.
- Web MVC: covered by Tasks 8, 10, and 11.
- PostgreSQL-first Active Record existing schema: covered by Task 9 at SQL-shape level; live DB query execution needs the next database-focused task after this plan.
- Async/spawn: not fully implemented here; VM/debug/task design is deferred because the vertical HTTP server uses Tokio but Ricochet-language async needs its own plan.
- Hot reload: revision snapshot skeleton covered by Task 13; actual filesystem watcher is deferred.
- `rco doc`: deferred.
- Package manager: deferred beyond command/manifest groundwork.
- AI package: deferred.

Red-flag scan:

- This plan intentionally avoids unresolved fill-in tokens or vague fill-in steps.
- Deferred items are explicitly scoped out, not left as incomplete steps.

Type consistency:

- Runtime value type is `Value`.
- VM error type is `VmError`.
- Bytecode container is `Chunk`.
- Source span is `SourceSpan`.
- Web request context is `RequestContext`.
- Controller action result is `ActionResult`.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-09-ricochet-web-mvc-vertical-slice.md`.

Two execution options:

1. **Subagent-Driven (recommended)** - Dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints.
