# Ricochet VS Code Grammar

This folder contains a VS Code-compatible extension for Ricochet `.rco` source
files. It contributes a TextMate grammar, a language-client bridge that
launches `rco lsp` over stdio, and a Ricochet debug configuration that launches
`rco debug-adapter` over stdio.

The grammar registers the `source.ricochet` scope and highlights:

- `(( ... ))` Ricochet comments/doc comments
- strings, valid escapes, and invalid escape sequences
- positive and negative integer numbers
- `$name` binding reads
- postfix selectors such as `email.get`, `displayName`, and `status`
- mutator words such as `push`, `put`, `insert_at`, and `remove`
- declaration, control-flow, async, route, webview, stack, and core built-in words
- Args arrows and block/argument delimiters

The language server adds live diagnostics, completion, hover, go-to-definition,
document symbols, semantic tokens, document formatting, quick fixes,
prepare-rename, and single-document rename support. The current quick fix
rewrites legacy `name get` variable reads to `$name` and leading-dot syntax
such as `self .email get` to postfix selectors such as `self email.get`.

The extension also includes `Ricochet: Run With Stack Visualizer`, which runs
the active `.rco` file with `rco run --trace-file` and opens a separate IDE
panel showing the recorded instruction timeline, stack, locals, globals, and
`self` values. Live Ricochet debug sessions use the same configured `rco`
binary, support source breakpoints and step controls, and update `Ricochet:
Show Debugger Stack` with the paused frame, stack, locals, globals, `self`, and
task scopes.

To try it locally, open this folder as an extension development host:

```powershell
cd .\editors\vscode
npm install
code --extensionDevelopmentPath .
```

If `rco` is not on `PATH`, set `ricochet.server.path` to the built executable,
for example `E:\LLM Projects\Ricochet\target\debug\rco.exe`.

From the repository root, run `rco words --check` after adding or renaming
built-in words. It compares the reference docs, TextMate grammar, and curated
LSP inventory so editor support does not drift from the language surface.
