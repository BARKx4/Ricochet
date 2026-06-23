# Appendix F: Glossary

## Purpose

This appendix defines the terms used throughout Learn Ricochet.

## Terms

| Term | Meaning |
| --- | --- |
| Stack | The runtime value stack. Words consume values from it and leave results on it. |
| Word | A named operation such as `println`, `json_decode`, `find_record`, or `tui_write`. |
| Selector | A generated OOP member word such as `email.get` or `email.set`. |
| Binding | A named value created with `var` and read with `$name`. |
| Dynamic `get` | Runtime name lookup, usually shaped as `"name" get`. |
| Block | A quoted executable chunk such as `[ 2 * ]`. |
| Result | Explicit success or failure value. Use `ok?`, `value`, `error`, `unwrap_or`, `map_result`, and `and_then`. |
| Capability | Host permission such as filesystem, HTTP, sockets, TUI, webview, process, PTY, or environment access. |
| Retained resource | Host-managed resource kept behind an id, such as HTTP streams, sockets, process jobs, PTY sessions, or upload streams. Release retained resources when done. |
| MVC | Model-view-controller web app structure. Routes call controller actions, controllers prepare response data, views render HTML, and models map to data. |
| Controller | MVC class that implements request actions and returns responses. |
| Template directive | View syntax such as `{% collection "item" each %}` or `{% condition if %}`. |
| Active Record | Model mapping layer for existing database tables. |
| Migration | Ordered schema change file. Ricochet supports SQL and native migration DSL files. |
| Seed | Ordered data-loading file for development or fixture data. |
| Package | Ricochet dependency project with manifest metadata. |
| Registry | Package index and artifact store. Ricochet supports local/static and hosted registry workflows. |
| Static import | Compile-time import such as `"lib/report" import`. |
| Dynamic import | Runtime import such as `"math_tools/stats" import_dynamic`, returning a `Result`. |
| Macro | Compile-time code generation hook invoked explicitly with `"name" macro_call`. |
| Bytecode | Compiled `.rcob` artifact produced by `rco build`. |
| Image | Persistent VM-state snapshot used by REPL image workflows. |
| Source emission | Readable source-like view emitted from bytecode by `rco emit-source`. |
| DAP | Debug Adapter Protocol integration used by editor debuggers. |
| LSP | Language Server Protocol integration used for diagnostics, hover, completion, symbols, formatting, and rename. |
| TUI | Terminal UI app using `tui_*` words and `rco tui`. |
| GUI | Desktop webview app using `webview_*` words and `rco gui` or `rco package --gui`. |
| Update channel | Release metadata JSON that describes candidate or stable artifacts, verification methods, rollout, and rollback policy. |
