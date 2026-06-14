# Ricochet VS Code Grammar

This folder contains a VS Code-compatible TextMate grammar for Ricochet
`.rco` source files.

The grammar registers the `source.ricochet` scope and highlights:

- `(( ... ))` Ricochet comments/doc comments
- strings, valid escapes, and invalid escape sequences
- integer numbers
- `$name` binding reads
- dot-method dispatch such as `.find`, `.push!`, and `.get-task`
- bang words such as `!method`, `!push`, and `!put`
- declaration, control-flow, async, route, webview, stack, and core built-in words
- Args arrows and block/argument delimiters

To try it locally, open this folder as an extension development host:

```powershell
code --extensionDevelopmentPath .\editors\vscode
```
