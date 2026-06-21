# Editor And Debugging

A VS Code-compatible extension for `.rco` files lives in `editors/vscode`. It
registers the `source.ricochet` scope, highlights Ricochet comments, strings,
`$name` binding reads, postfix selectors, declarations, control flow, async
words, route verbs, core built-ins, and collection types, and launches `rco lsp`
for live editor support.

`rco lsp` speaks stdio Language Server Protocol and provides live diagnostics,
completion, hover, go-to-definition, document symbols, semantic tokens,
document formatting, quick fixes, prepare-rename, and single-document rename
support. The VS Code extension exposes `ricochet.server.path` when `rco` is not
on `PATH`, plus `Ricochet: Restart Language Server` for local toolchain
rebuilds. The same diagnostics warn when old `name get` variable reads should
be written as `$name`, flag leading-dot legacy syntax such as `self .email get`,
and offer quick fixes for both; keep `"name" get` only when the variable name
is intentionally data on the stack.

Use `rco lint [--json] [path]` to run those same compile and syntax diagnostics
over a file or directory in CI. `rco fmt` formats current syntax without hiding
unsupported legacy forms.

Terminal debug sessions accept `step`, `next`, `out`, `continue`, `abort`,
`stack`, `locals`, `globals`, `self`, `tasks`, `tasks --tree`,
`task <id> stack`, and `task <id> locals`; `rco debug --json` streams the same
event contract as JSON Lines for editor adapters. Task pause snapshots include
operation labels, status predicates, optional fault text, and worker
frame/source/opcode/stack/locals details. `rco debug-adapter` speaks Debug
Adapter Protocol over stdio, maps source breakpoints and step into/over/out
controls to the VM debugger, and exposes stack, locals, globals, `self`, and
expandable task scopes to IDEs. The VS Code live debugger stack panel follows
nested DAP variable references, so the `Tasks` scope opens into task detail
fields and worker-frame snapshots instead of stopping at the top-level task
summary.

`rco debug-tui --smoke` renders a deterministic read-only terminal snapshot of
the first debugger pause, including source line, stack, locals, globals, and
task snapshots. Without `--smoke`, `rco debug-tui` renders a fresh snapshot at
each pause and accepts `step`, `next`, `out`, `continue`, or `abort`; repeat
`--command ACTION` for deterministic scripted sessions. `rco debug-web
--smoke` writes the same snapshot as standalone HTML, while `rco debug-web`
serves a loopback-only browser debugger shell with Server-Sent Events at
`/events` and JSON controls at `/control`.

`rco serve --debug` also installs an MVC request-fault pause hook. Before a
controller action, view render, or response metadata failure becomes an HTTP
500 response, the server prints a `FAULT request ...` line with the method,
path, controller/action, revision, stage, and error text.

`Ricochet: Run With Stack Visualizer` runs the active `.rco` file with `rco run
--trace-file` and opens a separate IDE panel for the instruction timeline,
stack, locals, globals, and `self` values; live VS Code debug sessions also
update `Ricochet: Show Debugger Stack` from DAP scopes. See
`docs/debugger-integrations.md` for the shared trace and DAP contracts.

`rco words` prints the built-in editor/LSP word inventory; `rco words --json`
emits it for tooling. Maintainers can run `rco words --check` from the source
tree to compare `docs/reference/app.js`, the TextMate grammar, and the embedded
LSP inventory. `scripts/validate-editor-assets.ps1` checks the grammar and VS
Code wiring against the reference word catalog, and release archives include
this folder under `editors/vscode`. When adding or renaming a public word, use
`docs/adding-words.md` to keep VM dispatch, tests, docs, LSP completions, and
editor grammar in sync.
