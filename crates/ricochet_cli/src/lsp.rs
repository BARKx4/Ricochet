use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};

use anyhow::{bail, Context, Result};
use ricochet_compiler::compile_source;
use ricochet_syntax::{
    format_source, lex, parse_module, utf16_range_for_span, Expr, Item as SyntaxItem, Module,
    SourcePosition, Span, SpannedExpr, Token, TokenKind,
};
use serde_json::{json, Value};

const TOKEN_TYPES: &[&str] = &[
    "namespace",
    "type",
    "class",
    "function",
    "method",
    "property",
    "variable",
    "keyword",
    "string",
    "number",
    "comment",
    "operator",
];

const WORD_DOCS: &[WordDoc] = &[
    WordDoc::new(
        "Subclass",
        "class declaration",
        "Declare a class with postfix class syntax: `User Model Subclass ... end`.",
    ),
    WordDoc::new(
        "Accessor",
        "class declaration",
        "Declare generated `name.get` and `name.set` selectors inside a class body.",
    ),
    WordDoc::new(
        "Field",
        "class declaration",
        "Declare stored instance state inside a class body.",
    ),
    WordDoc::new(
        "Table",
        "model declaration",
        "Bind a model class to a database table.",
    ),
    WordDoc::new(
        "Method",
        "class declaration",
        "Declare a class method with `[ body ] \"name\" Method`.",
    ),
    WordDoc::new(
        "function",
        "declaration",
        "Declare a top-level function with postfix function syntax.",
    ),
    WordDoc::new(
        "end",
        "control",
        "Close a class, function, method body, `if`, or `while` block.",
    ),
    WordDoc::new(
        "if",
        "control",
        "Execute the following body when the value below it is truthy.",
    ),
    WordDoc::new(
        "else",
        "control",
        "Start the false branch of an `if` expression.",
    ),
    WordDoc::new(
        "while",
        "control",
        "Repeat a body while the condition below it is truthy.",
    ),
    WordDoc::new(
        "break",
        "control",
        "Exit the nearest enclosing `while` loop.",
    ),
    WordDoc::new(
        "continue",
        "control",
        "Continue the nearest enclosing `while` loop.",
    ),
    WordDoc::new("get", "binding", "Read a binding name from the stack."),
    WordDoc::new(
        "set",
        "binding",
        "Write a binding name/value pair from the stack.",
    ),
    WordDoc::new(
        "var",
        "binding",
        "Create or update a local binding from the stack.",
    ),
    WordDoc::new(
        "at",
        "collection",
        "Read a collection key/index: `request \"method\" at`.",
    ),
    WordDoc::new(
        "put!",
        "collection",
        "Mutate a map with container/key/value order: `settings \"theme\" \"dark\" put!`.",
    ),
    WordDoc::new(
        "push!",
        "collection",
        "Mutate a collection with container/value order: `items value push!`.",
    ),
    WordDoc::new(
        "map",
        "collection",
        "Declare a map binding or create map-oriented state.",
    ),
    WordDoc::new(
        "array",
        "collection",
        "Declare an array binding or create array-oriented state.",
    ),
    WordDoc::new("ok", "result", "Wrap a value in an ok result."),
    WordDoc::new("fail", "result", "Wrap a value in an error result."),
    WordDoc::new("value", "result", "Unwrap an ok result or raise its error."),
    WordDoc::new("error", "result", "Extract the error side of a result."),
    WordDoc::new("spawn", "async", "Start a block as a Ricochet task."),
    WordDoc::new("await", "async", "Wait for a task and push its result."),
    WordDoc::new("await-all", "async", "Wait for all tasks in an array."),
    WordDoc::new(
        "release-task",
        "async",
        "Release retained completed task state after awaiting it.",
    ),
    WordDoc::new(
        "runtime_capabilities",
        "capabilities",
        "Return a map describing enabled runtime capabilities.",
    ),
    WordDoc::new(
        "env_get",
        "environment",
        "Read an environment variable through the environment capability.",
    ),
    WordDoc::new(
        "env_set",
        "environment",
        "Set an environment variable in the current Ricochet process.",
    ),
    WordDoc::new(
        "secret_env",
        "secrets",
        "Build an environment-backed secret reference map.",
    ),
    WordDoc::new(
        "secret_literal",
        "secrets",
        "Build a literal secret reference map for tests and fixtures.",
    ),
    WordDoc::new(
        "secret_resolve",
        "secrets",
        "Resolve a secret reference through the appropriate capability.",
    ),
    WordDoc::new(
        "config_get",
        "config",
        "Read a required value from a config map by key or nested path.",
    ),
    WordDoc::new(
        "fs_read_text",
        "filesystem",
        "Read a text file through the filesystem host capability.",
    ),
    WordDoc::new(
        "fs_write_text",
        "filesystem",
        "Write text through the filesystem host capability.",
    ),
    WordDoc::new(
        "http_request",
        "http",
        "Perform a structured HTTP request map through the HTTP host capability.",
    ),
    WordDoc::new(
        "http_request_new",
        "http",
        "Create a structured HTTP request map.",
    ),
    WordDoc::new(
        "http_header_put",
        "http",
        "Add or update a validated header in a request map.",
    ),
    WordDoc::new(
        "http_bearer_auth",
        "http",
        "Add an Authorization bearer header to a request map.",
    ),
    WordDoc::new(
        "http_json_body",
        "http",
        "Set a Ricochet value as a request map's JSON body.",
    ),
    WordDoc::new(
        "http_timeout",
        "http",
        "Set a bounded timeout on a request map.",
    ),
    WordDoc::new(
        "http_request_task",
        "http",
        "Start a structured HTTP request as a Ricochet task.",
    ),
    WordDoc::new(
        "process_spawn",
        "process",
        "Run a bounded child process through the process capability.",
    ),
    WordDoc::new(
        "process_start",
        "process",
        "Start a retained long-running child process job.",
    ),
    WordDoc::new(
        "process_env_put",
        "process",
        "Add or update a child process environment entry in an options map.",
    ),
    WordDoc::new(
        "pty_start",
        "pty",
        "Start a PTY session through the PTY host capability.",
    ),
    WordDoc::new(
        "approval_create",
        "approval",
        "Create an exactly-once approval record.",
    ),
    WordDoc::new(
        "approval_claim",
        "approval",
        "Claim an approval record exactly once.",
    ),
    WordDoc::new(
        "tui_write",
        "tui",
        "Write text to the terminal UI capability.",
    ),
    WordDoc::new(
        "webview_window",
        "webview",
        "Create a native webview document/window value.",
    ),
    WordDoc::new(
        "webview_action",
        "webview",
        "Describe a GUI action name and callback word.",
    ),
    WordDoc::new(
        "webview_window_state",
        "webview",
        "Create a webview document with explicit state and actions.",
    ),
    WordDoc::new(
        "json-decode",
        "json",
        "Decode JSON text into Ricochet values.",
    ),
    WordDoc::new(
        "json-encode",
        "json",
        "Encode a Ricochet value as JSON text.",
    ),
    WordDoc::new("println", "io", "Print a value and a newline."),
    WordDoc::new("debug", "io", "Print debug output for a value."),
    WordDoc::new("self", "oop", "Push the current receiver inside a method."),
    WordDoc::new("new", "oop", "Instantiate a class."),
    WordDoc::new(
        "send",
        "oop",
        "Call a selector whose name is known at runtime.",
    ),
];

pub fn run_lsp_server(trace: bool) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    run_lsp(stdin.lock(), stdout.lock(), trace)
}

pub(crate) fn run_lsp<R: BufRead, W: Write>(
    mut reader: R,
    mut writer: W,
    trace: bool,
) -> Result<()> {
    let mut server = LspServer::default();
    while let Some(message) = read_message(&mut reader)? {
        if trace {
            eprintln!("<- {}", message);
        }
        let outgoing = server.handle_message(message)?;
        for response in outgoing {
            if trace {
                eprintln!("-> {}", response);
            }
            write_message(&mut writer, &response)?;
        }
        if server.exit_requested {
            break;
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct LspServer {
    documents: BTreeMap<String, LspDocument>,
    shutdown_requested: bool,
    exit_requested: bool,
}

#[derive(Debug, Clone)]
struct LspDocument {
    uri: String,
    source: String,
    version: Option<i64>,
}

#[derive(Debug, Clone)]
struct WordDoc {
    label: &'static str,
    detail: &'static str,
    documentation: &'static str,
}

impl WordDoc {
    const fn new(label: &'static str, detail: &'static str, documentation: &'static str) -> Self {
        Self {
            label,
            detail,
            documentation,
        }
    }
}

#[derive(Debug, Clone)]
struct SymbolDef {
    name: String,
    kind: SymbolKind,
    span: Span,
    docs: Vec<String>,
    children: Vec<SymbolDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SymbolKind {
    Class,
    Function,
    Method,
    Property,
    Field,
    Table,
}

#[derive(Debug, Clone)]
struct TokenHit {
    label: String,
    span: Span,
}

impl LspServer {
    fn handle_message(&mut self, message: Value) -> Result<Vec<Value>> {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Ok(Vec::new());
        };
        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or(Value::Null);

        match method {
            "initialize" => Ok(vec![response(id, initialize_result())]),
            "initialized" => Ok(Vec::new()),
            "shutdown" => {
                self.shutdown_requested = true;
                Ok(vec![response(id, Value::Null)])
            }
            "exit" => {
                self.exit_requested = true;
                Ok(Vec::new())
            }
            "textDocument/didOpen" => self.did_open(&params),
            "textDocument/didChange" => self.did_change(&params),
            "textDocument/didSave" => self.did_save(&params),
            "textDocument/didClose" => self.did_close(&params),
            "textDocument/completion" => Ok(vec![response(
                id,
                json!({ "isIncomplete": false, "items": self.completions(&params) }),
            )]),
            "textDocument/hover" => Ok(vec![response(id, self.hover(&params))]),
            "textDocument/definition" => Ok(vec![response(id, self.definition(&params))]),
            "textDocument/documentSymbol" => Ok(vec![response(id, self.document_symbols(&params))]),
            "textDocument/semanticTokens/full" => {
                Ok(vec![response(id, self.semantic_tokens(&params))])
            }
            "textDocument/formatting" => Ok(vec![response(id, self.formatting(&params))]),
            "textDocument/prepareRename" => Ok(vec![response(id, self.prepare_rename(&params))]),
            "textDocument/rename" => Ok(vec![response(id, self.rename(&params))]),
            _ if id.is_some() => Ok(vec![error_response(
                id,
                -32601,
                format!("unsupported Ricochet LSP method {method}"),
            )]),
            _ => Ok(Vec::new()),
        }
    }

    fn did_open(&mut self, params: &Value) -> Result<Vec<Value>> {
        let Some(text_document) = params.get("textDocument") else {
            return Ok(Vec::new());
        };
        let Some(uri) = text_document.get("uri").and_then(Value::as_str) else {
            return Ok(Vec::new());
        };
        let source = text_document
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let version = text_document.get("version").and_then(Value::as_i64);
        let document = LspDocument {
            uri: uri.to_string(),
            source,
            version,
        };
        let notification = publish_diagnostics(&document);
        self.documents.insert(uri.to_string(), document);
        Ok(vec![notification])
    }

    fn did_change(&mut self, params: &Value) -> Result<Vec<Value>> {
        let Some(text_document) = params.get("textDocument") else {
            return Ok(Vec::new());
        };
        let Some(uri) = text_document.get("uri").and_then(Value::as_str) else {
            return Ok(Vec::new());
        };
        let Some(document) = self.documents.get_mut(uri) else {
            return Ok(Vec::new());
        };
        if let Some(version) = text_document.get("version").and_then(Value::as_i64) {
            document.version = Some(version);
        }
        if let Some(change) = params
            .get("contentChanges")
            .and_then(Value::as_array)
            .and_then(|changes| changes.last())
            .and_then(|change| change.get("text"))
            .and_then(Value::as_str)
        {
            document.source = change.to_string();
        }
        Ok(vec![publish_diagnostics(document)])
    }

    fn did_save(&mut self, params: &Value) -> Result<Vec<Value>> {
        let Some(uri) = params
            .get("textDocument")
            .and_then(|document| document.get("uri"))
            .and_then(Value::as_str)
        else {
            return Ok(Vec::new());
        };
        if let Some(text) = params.get("text").and_then(Value::as_str) {
            if let Some(document) = self.documents.get_mut(uri) {
                document.source = text.to_string();
            }
        }
        Ok(self
            .documents
            .get(uri)
            .map(publish_diagnostics)
            .into_iter()
            .collect())
    }

    fn did_close(&mut self, params: &Value) -> Result<Vec<Value>> {
        let Some(uri) = params
            .get("textDocument")
            .and_then(|document| document.get("uri"))
            .and_then(Value::as_str)
        else {
            return Ok(Vec::new());
        };
        self.documents.remove(uri);
        Ok(vec![json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri,
                "diagnostics": [],
            }
        })])
    }

    fn document_for_params(&self, params: &Value) -> Option<&LspDocument> {
        let uri = params
            .get("textDocument")
            .and_then(|document| document.get("uri"))
            .and_then(Value::as_str)?;
        self.documents.get(uri)
    }

    fn position_for_params(params: &Value) -> Option<SourcePosition> {
        let position = params.get("position")?;
        Some(SourcePosition {
            line: position.get("line")?.as_u64()? as usize,
            character: position.get("character")?.as_u64()? as usize,
        })
    }

    fn completions(&self, params: &Value) -> Vec<Value> {
        let mut items = WORD_DOCS
            .iter()
            .map(|entry| {
                json!({
                    "label": entry.label,
                    "kind": completion_kind(entry.label),
                    "detail": entry.detail,
                    "documentation": {
                        "kind": "markdown",
                        "value": entry.documentation,
                    },
                })
            })
            .collect::<Vec<_>>();

        if let Some(document) = self.document_for_params(params) {
            let mut symbols = BTreeMap::new();
            for symbol in symbols_for_source(&document.source) {
                insert_symbol_completion(&mut symbols, &symbol);
            }
            for (_, symbol) in symbols {
                items.push(symbol);
            }
        }
        items
    }

    fn hover(&self, params: &Value) -> Value {
        let Some(document) = self.document_for_params(params) else {
            return Value::Null;
        };
        let Some(position) = Self::position_for_params(params) else {
            return Value::Null;
        };
        let Some(hit) = token_at_position(&document.source, position) else {
            return Value::Null;
        };

        if let Some(entry) = WORD_DOCS.iter().find(|entry| entry.label == hit.label) {
            return json!({
                "contents": {
                    "kind": "markdown",
                    "value": format!("**{}**\n\n{}\n\n_{}_", entry.label, entry.documentation, entry.detail),
                },
                "range": lsp_range(&document.source, hit.span),
            });
        }

        if let Some(symbol) = find_symbol(&symbols_for_source(&document.source), &hit.label) {
            let docs = if symbol.docs.is_empty() {
                String::new()
            } else {
                format!("\n\n{}", symbol.docs.join("\n"))
            };
            return json!({
                "contents": {
                    "kind": "markdown",
                    "value": format!("**{}**\n\n{}{}", symbol.name, symbol.kind.detail(), docs),
                },
                "range": lsp_range(&document.source, hit.span),
            });
        }

        Value::Null
    }

    fn definition(&self, params: &Value) -> Value {
        let Some(document) = self.document_for_params(params) else {
            return Value::Null;
        };
        let Some(position) = Self::position_for_params(params) else {
            return Value::Null;
        };
        let Some(hit) = token_at_position(&document.source, position) else {
            return Value::Null;
        };
        let symbols = symbols_for_source(&document.source);
        let Some(symbol) = find_symbol(&symbols, &hit.label) else {
            return Value::Null;
        };
        json!({
            "uri": document.uri,
            "range": lsp_range(&document.source, symbol.span),
        })
    }

    fn document_symbols(&self, params: &Value) -> Value {
        let Some(document) = self.document_for_params(params) else {
            return json!([]);
        };
        let symbols = symbols_for_source(&document.source)
            .into_iter()
            .map(|symbol| document_symbol(&document.source, symbol))
            .collect::<Vec<_>>();
        json!(symbols)
    }

    fn semantic_tokens(&self, params: &Value) -> Value {
        let Some(document) = self.document_for_params(params) else {
            return json!({ "data": [] });
        };
        json!({ "data": semantic_token_data(&document.source) })
    }

    fn formatting(&self, params: &Value) -> Value {
        let Some(document) = self.document_for_params(params) else {
            return json!([]);
        };
        let Ok(formatted) = format_source(&document.source) else {
            return json!([]);
        };
        if formatted == document.source {
            return json!([]);
        }
        json!([{
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": position_json(document_end_position(&document.source)),
            },
            "newText": formatted,
        }])
    }

    fn prepare_rename(&self, params: &Value) -> Value {
        let Some(document) = self.document_for_params(params) else {
            return Value::Null;
        };
        let Some(position) = Self::position_for_params(params) else {
            return Value::Null;
        };
        let Some(hit) = token_at_position(&document.source, position) else {
            return Value::Null;
        };
        if is_renameable(&hit.label) {
            json!(lsp_range(&document.source, hit.span))
        } else {
            Value::Null
        }
    }

    fn rename(&self, params: &Value) -> Value {
        let Some(document) = self.document_for_params(params) else {
            return Value::Null;
        };
        let Some(position) = Self::position_for_params(params) else {
            return Value::Null;
        };
        let Some(new_name) = params.get("newName").and_then(Value::as_str) else {
            return Value::Null;
        };
        if !is_renameable(new_name) {
            return Value::Null;
        }
        let Some(hit) = token_at_position(&document.source, position) else {
            return Value::Null;
        };
        if !is_renameable(&hit.label) {
            return Value::Null;
        }

        let edits = rename_edits(&document.source, &hit.label, new_name);
        if edits.is_empty() {
            return Value::Null;
        }
        let mut changes = serde_json::Map::new();
        changes.insert(document.uri.clone(), Value::Array(edits));
        json!({ "changes": Value::Object(changes) })
    }
}

impl SymbolKind {
    fn lsp_kind(self) -> u8 {
        match self {
            SymbolKind::Class => 5,
            SymbolKind::Function => 12,
            SymbolKind::Method => 6,
            SymbolKind::Property | SymbolKind::Field | SymbolKind::Table => 7,
        }
    }

    fn detail(self) -> &'static str {
        match self {
            SymbolKind::Class => "class",
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Property => "accessor",
            SymbolKind::Field => "field",
            SymbolKind::Table => "table",
        }
    }
}

fn initialize_result() -> Value {
    json!({
        "serverInfo": {
            "name": "Ricochet Language Server",
            "version": ricochet_syntax::crate_version(),
        },
        "capabilities": {
            "textDocumentSync": {
                "openClose": true,
                "change": 1,
                "save": { "includeText": true },
            },
            "completionProvider": {
                "resolveProvider": false,
                "triggerCharacters": [".", "_", "\""],
            },
            "hoverProvider": true,
            "definitionProvider": true,
            "documentSymbolProvider": true,
            "semanticTokensProvider": {
                "legend": {
                    "tokenTypes": TOKEN_TYPES,
                    "tokenModifiers": [],
                },
                "full": true,
            },
            "documentFormattingProvider": true,
            "renameProvider": {
                "prepareProvider": true,
            },
        },
    })
}

fn response(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    })
}

fn error_response(id: Option<Value>, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": {
            "code": code,
            "message": message.into(),
        },
    })
}

fn publish_diagnostics(document: &LspDocument) -> Value {
    let diagnostics = match compile_source(&document.uri, &document.source) {
        Ok(_) => Vec::new(),
        Err(error) => vec![crate::compile_error_lsp_diagnostic(
            &document.uri,
            &document.source,
            &error,
        )],
    };
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": document.uri,
            "version": document.version,
            "diagnostics": diagnostics,
        }
    })
}

fn symbols_for_source(source: &str) -> Vec<SymbolDef> {
    let Ok(module) = parse_module(source) else {
        return Vec::new();
    };
    module_symbols(&module)
}

fn module_symbols(module: &Module) -> Vec<SymbolDef> {
    module
        .items
        .iter()
        .filter_map(symbol_for_item)
        .collect::<Vec<_>>()
}

fn symbol_for_item(item: &SyntaxItem) -> Option<SymbolDef> {
    match item {
        SyntaxItem::Class(class) => Some(SymbolDef {
            name: class.name.clone(),
            kind: SymbolKind::Class,
            span: class.span,
            docs: class.docs.clone(),
            children: class
                .body
                .iter()
                .filter_map(symbol_for_class_body_item)
                .collect(),
        }),
        SyntaxItem::Function(function) => Some(SymbolDef {
            name: function.name.clone(),
            kind: SymbolKind::Function,
            span: function.span,
            docs: function.docs.clone(),
            children: Vec::new(),
        }),
        SyntaxItem::Method(method) => Some(SymbolDef {
            name: method.name.clone(),
            kind: SymbolKind::Method,
            span: method.span,
            docs: method.docs.clone(),
            children: Vec::new(),
        }),
        SyntaxItem::Expr { .. } => None,
    }
}

fn symbol_for_class_body_item(item: &SyntaxItem) -> Option<SymbolDef> {
    match item {
        SyntaxItem::Method(method) => Some(SymbolDef {
            name: method.name.clone(),
            kind: SymbolKind::Method,
            span: method.span,
            docs: method.docs.clone(),
            children: Vec::new(),
        }),
        SyntaxItem::Expr {
            expr: Expr::Sequence(exprs),
            span,
            docs,
        } => class_body_sequence_symbol(exprs, *span, docs.clone()),
        SyntaxItem::Class(class) => Some(SymbolDef {
            name: class.name.clone(),
            kind: SymbolKind::Class,
            span: class.span,
            docs: class.docs.clone(),
            children: Vec::new(),
        }),
        SyntaxItem::Function(function) => Some(SymbolDef {
            name: function.name.clone(),
            kind: SymbolKind::Function,
            span: function.span,
            docs: function.docs.clone(),
            children: Vec::new(),
        }),
        SyntaxItem::Expr { .. } => None,
    }
}

fn class_body_sequence_symbol(
    exprs: &[SpannedExpr],
    span: Span,
    docs: Vec<String>,
) -> Option<SymbolDef> {
    match exprs {
        [name, operator] if matches!(&operator.expr, Expr::Symbol(word) if word == "Table") => {
            Some(SymbolDef {
                name: declaration_name(name)?,
                kind: SymbolKind::Table,
                span,
                docs,
                children: Vec::new(),
            })
        }
        [name, operator] if matches!(&operator.expr, Expr::Symbol(word) if word == "Accessor") => {
            Some(SymbolDef {
                name: declaration_name(name)?,
                kind: SymbolKind::Property,
                span,
                docs,
                children: Vec::new(),
            })
        }
        [name, operator] if matches!(&operator.expr, Expr::Symbol(word) if word == "Field") => {
            Some(SymbolDef {
                name: declaration_name(name)?,
                kind: SymbolKind::Field,
                span,
                docs,
                children: Vec::new(),
            })
        }
        [block, name, operator] if matches!((&block.expr, &operator.expr), (Expr::Block(_), Expr::Symbol(word)) if word == "Method") => {
            Some(SymbolDef {
                name: declaration_name(name)?,
                kind: SymbolKind::Method,
                span,
                docs,
                children: Vec::new(),
            })
        }
        [args, block, name, operator] if matches!((&args.expr, &block.expr, &operator.expr), (Expr::Args(_), Expr::Block(_), Expr::Symbol(word)) if word == "Method") => {
            Some(SymbolDef {
                name: declaration_name(name)?,
                kind: SymbolKind::Method,
                span,
                docs,
                children: Vec::new(),
            })
        }
        _ => None,
    }
}

fn declaration_name(expression: &SpannedExpr) -> Option<String> {
    match &expression.expr {
        Expr::Symbol(name) | Expr::String(name) => Some(name.clone()),
        _ => None,
    }
}

fn document_symbol(source: &str, symbol: SymbolDef) -> Value {
    json!({
        "name": symbol.name,
        "kind": symbol.kind.lsp_kind(),
        "detail": symbol.kind.detail(),
        "range": lsp_range(source, symbol.span),
        "selectionRange": lsp_range(source, symbol.span),
        "children": symbol.children.into_iter().map(|child| document_symbol(source, child)).collect::<Vec<_>>(),
    })
}

fn find_symbol<'a>(symbols: &'a [SymbolDef], name: &str) -> Option<&'a SymbolDef> {
    for symbol in symbols {
        if symbol.name == name {
            return Some(symbol);
        }
        if let Some(child) = find_symbol(&symbol.children, name) {
            return Some(child);
        }
    }
    None
}

fn insert_symbol_completion(symbols: &mut BTreeMap<String, Value>, symbol: &SymbolDef) {
    symbols.insert(
        symbol.name.clone(),
        json!({
            "label": symbol.name,
            "kind": match symbol.kind {
                SymbolKind::Class => 7,
                SymbolKind::Function => 3,
                SymbolKind::Method => 2,
                SymbolKind::Property | SymbolKind::Field | SymbolKind::Table => 10,
            },
            "detail": symbol.kind.detail(),
        }),
    );
    for child in &symbol.children {
        insert_symbol_completion(symbols, child);
    }
}

fn completion_kind(label: &str) -> u8 {
    match label {
        "Subclass" | "Accessor" | "Field" | "Table" | "Method" | "function" => 14,
        "if" | "else" | "while" | "end" | "break" | "continue" => 14,
        _ => 3,
    }
}

fn token_at_position(source: &str, position: SourcePosition) -> Option<TokenHit> {
    let offset = offset_for_position(source, position)?;
    let tokens = lex(source).ok()?;
    tokens
        .into_iter()
        .filter(|token| token.span.start <= offset && offset <= token.span.end)
        .find_map(|token| token_hit(source, token))
}

fn token_hit(source: &str, token: Token) -> Option<TokenHit> {
    match token.kind {
        TokenKind::Reference(name) => Some(TokenHit {
            label: name,
            span: Span {
                start: token.span.start.saturating_add(1),
                end: token.span.end,
            },
        }),
        TokenKind::Symbol(label) | TokenKind::BangWord(label) | TokenKind::DotWord(label) => {
            Some(TokenHit {
                label,
                span: token.span,
            })
        }
        TokenKind::String(_) => Some(TokenHit {
            label: source[token.span.start..token.span.end].to_string(),
            span: token.span,
        }),
        _ => None,
    }
}

fn semantic_token_data(source: &str) -> Vec<usize> {
    let Ok(tokens) = lex(source) else {
        return Vec::new();
    };
    let mut encoded = Vec::new();
    let mut previous_line = 0usize;
    let mut previous_start = 0usize;
    for token in tokens {
        let Some(token_type) = semantic_token_type(&token.kind) else {
            continue;
        };
        let range = utf16_range_for_span(source, token.span);
        if range.start.line != range.end.line {
            continue;
        }
        let length = range.end.character.saturating_sub(range.start.character);
        if length == 0 {
            continue;
        }
        let delta_line = range.start.line.saturating_sub(previous_line);
        let delta_start = if delta_line == 0 {
            range.start.character.saturating_sub(previous_start)
        } else {
            range.start.character
        };
        encoded.extend([delta_line, delta_start, length, token_type, 0]);
        previous_line = range.start.line;
        previous_start = range.start.character;
    }
    encoded
}

fn semantic_token_type(kind: &TokenKind) -> Option<usize> {
    match kind {
        TokenKind::Symbol(word) if is_capitalized(word) => Some(2),
        TokenKind::Symbol(word) if is_keyword(word) => Some(7),
        TokenKind::Symbol(word) if word.contains('.') => Some(4),
        TokenKind::Symbol(_) | TokenKind::BangWord(_) | TokenKind::Reference(_) => Some(6),
        TokenKind::DotWord(_) => Some(11),
        TokenKind::String(_) => Some(8),
        TokenKind::Number(_) => Some(9),
        TokenKind::DocComment(_) => Some(10),
        TokenKind::LeftParen
        | TokenKind::RightParen
        | TokenKind::LeftBracket
        | TokenKind::RightBracket
        | TokenKind::Arrow => Some(11),
        TokenKind::Newline | TokenKind::Eof => None,
    }
}

fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "if" | "else"
            | "while"
            | "end"
            | "break"
            | "continue"
            | "function"
            | "Subclass"
            | "Accessor"
            | "Field"
            | "Table"
            | "Method"
    )
}

fn is_capitalized(word: &str) -> bool {
    word.chars().next().is_some_and(char::is_uppercase)
}

fn rename_edits(source: &str, old_name: &str, new_name: &str) -> Vec<Value> {
    let Ok(tokens) = lex(source) else {
        return Vec::new();
    };
    tokens
        .into_iter()
        .filter_map(|token| {
            let span = match token.kind {
                TokenKind::Reference(name) if name == old_name => Span {
                    start: token.span.start.saturating_add(1),
                    end: token.span.end,
                },
                TokenKind::Symbol(name) | TokenKind::BangWord(name) | TokenKind::DotWord(name) => {
                    if name == old_name {
                        token.span
                    } else if let Some(selector_suffix) = name.strip_prefix(old_name) {
                        if selector_suffix.starts_with('.') {
                            Span {
                                start: token.span.start,
                                end: token.span.start + old_name.len(),
                            }
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                _ => return None,
            };
            Some(json!({
                "range": lsp_range(source, span),
                "newText": new_name,
            }))
        })
        .collect()
}

fn is_renameable(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '?' | '!' | '.')
        })
        && !WORD_DOCS.iter().any(|entry| entry.label == name)
}

fn lsp_range(source: &str, span: Span) -> Value {
    let range = utf16_range_for_span(source, span);
    json!({
        "start": position_json(range.start),
        "end": position_json(range.end),
    })
}

fn position_json(position: SourcePosition) -> Value {
    json!({
        "line": position.line,
        "character": position.character,
    })
}

fn document_end_position(source: &str) -> SourcePosition {
    ricochet_syntax::diagnostic::utf16_position(source, source.len())
}

fn offset_for_position(source: &str, position: SourcePosition) -> Option<usize> {
    let mut line_start = 0usize;
    let mut current_line = 0usize;
    for (index, byte) in source.bytes().enumerate() {
        if current_line == position.line {
            break;
        }
        if byte == b'\n' {
            current_line += 1;
            line_start = index + 1;
        }
    }
    if current_line != position.line {
        return None;
    }
    let line_end = source[line_start..]
        .find('\n')
        .map(|relative| line_start + relative)
        .unwrap_or(source.len());
    let mut utf16 = 0usize;
    for (relative, character) in source[line_start..line_end].char_indices() {
        if utf16 >= position.character {
            return Some(line_start + relative);
        }
        utf16 += character.len_utf16();
    }
    Some(line_end)
}

fn read_message<R: BufRead>(reader: &mut R) -> Result<Option<Value>> {
    let mut content_length = None;
    let mut saw_header = false;
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            if saw_header {
                bail!("unexpected EOF while reading LSP headers");
            }
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        saw_header = true;
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .context("invalid LSP Content-Length")?,
            );
        }
    }
    let content_length = content_length.context("missing LSP Content-Length header")?;
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .context("failed to parse LSP JSON message")
        .map(Some)
}

fn write_message<W: Write>(writer: &mut W, value: &Value) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn lsp_server_publishes_diagnostics_and_answers_completion() {
        let uri = "file:///workspace/User.rco";
        let input = messages(&[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"ricochet","version":1,"text":"User Model Subclass\n  \"email\" Accessor\n"}}}),
            json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":uri},"position":{"line":1,"character":4}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}),
            json!({"jsonrpc":"2.0","method":"exit","params":null}),
        ]);
        let mut output = Vec::new();

        run_lsp(Cursor::new(input), &mut output, false).expect("LSP server should run");

        let messages = parse_messages(&output);
        assert_eq!(messages[0]["id"], 1);
        assert_eq!(
            messages[0]["result"]["capabilities"]["hoverProvider"], true,
            "initialize should advertise hover support"
        );
        assert_eq!(messages[1]["method"], "textDocument/publishDiagnostics");
        assert_eq!(
            messages[1]["params"]["diagnostics"][0]["message"],
            "expected end, found end of file"
        );
        assert_eq!(messages[2]["id"], 2);
        let completions = messages[2]["result"]["items"]
            .as_array()
            .expect("completion response should contain items");
        assert!(
            completions.iter().any(|item| item["label"] == "Accessor"),
            "completion should include Ricochet words"
        );
    }

    #[test]
    fn lsp_analysis_returns_symbols_formatting_and_rename_edits() {
        let uri = "file:///workspace/User.rco";
        let source =
            "User Model Subclass\nemail Accessor\n[ self email.get ] \"displayName\" Method\nend\n";
        let input = messages(&[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"ricochet","version":1,"text":source}}}),
            json!({"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":uri}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"textDocument/formatting","params":{"textDocument":{"uri":uri},"options":{"tabSize":2,"insertSpaces":true}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"textDocument/rename","params":{"textDocument":{"uri":uri},"position":{"line":1,"character":1},"newName":"contactEmail"}}),
            json!({"jsonrpc":"2.0","id":5,"method":"shutdown","params":null}),
            json!({"jsonrpc":"2.0","method":"exit","params":null}),
        ]);
        let mut output = Vec::new();

        run_lsp(Cursor::new(input), &mut output, false).expect("LSP server should run");

        let messages = parse_messages(&output);
        let symbols = messages
            .iter()
            .find(|message| message["id"] == 2)
            .expect("document symbol response should exist")["result"]
            .as_array()
            .expect("symbols should be an array");
        assert_eq!(symbols[0]["name"], "User");
        assert!(
            symbols[0]["children"]
                .as_array()
                .expect("class should have children")
                .iter()
                .any(|child| child["name"] == "displayName"),
            "method symbol should be nested under class"
        );
        let formatting = messages
            .iter()
            .find(|message| message["id"] == 3)
            .expect("formatting response should exist")["result"]
            .as_array()
            .expect("formatting should be an array");
        assert!(formatting[0]["newText"]
            .as_str()
            .expect("format edit should contain newText")
            .contains("  email Accessor"));
        let rename = messages
            .iter()
            .find(|message| message["id"] == 4)
            .expect("rename response should exist");
        let edits = rename["result"]["changes"][uri]
            .as_array()
            .expect("rename response should contain edits");
        assert!(
            edits.len() >= 2,
            "rename should update declaration and selector/reference uses"
        );
    }

    fn messages(values: &[Value]) -> Vec<u8> {
        let mut output = Vec::new();
        for value in values {
            write_message(&mut output, value).expect("message should serialize");
        }
        output
    }

    fn parse_messages(output: &[u8]) -> Vec<Value> {
        let mut cursor = Cursor::new(output);
        let mut messages = Vec::new();
        while let Some(message) = read_message(&mut cursor).expect("message should parse") {
            messages.push(message);
        }
        messages
    }
}
