# Ricochet VS Code Grammar

This folder contains a VS Code-compatible extension for Ricochet `.rco` source
files. It contributes both a TextMate grammar and a language-client bridge that
launches `rco lsp` over stdio.

The grammar registers the `source.ricochet` scope and highlights:

- `(( ... ))` Ricochet comments/doc comments
- strings, valid escapes, and invalid escape sequences
- positive and negative integer numbers
- `$name` binding reads
- postfix selectors such as `email.get`, `displayName`, and `status`
- mutator words such as `push!`, `put!`, `insert!`, and `remove!`
- declaration, control-flow, async, route, webview, stack, and core built-in words
- Args arrows and block/argument delimiters

The language server adds live diagnostics, completion, hover, go-to-definition,
document symbols, semantic tokens, document formatting, prepare-rename, and
single-document rename support.

The extension also includes `Ricochet: Run With Stack Visualizer`, which runs
the active `.rco` file with `rco run --trace-file` and opens a separate IDE
panel showing the recorded instruction timeline, stack, locals, globals, and
`self` values. This is trace-backed today and is intended to evolve into the
same visual surface used by live debugger sessions.

To try it locally, open this folder as an extension development host:

```powershell
cd .\editors\vscode
npm install
code --extensionDevelopmentPath .
```

If `rco` is not on `PATH`, set `ricochet.server.path` to the built executable,
for example `E:\LLM Projects\Ricochet\target\debug\rco.exe`.
