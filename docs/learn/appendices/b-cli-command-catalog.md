# Appendix B: CLI Command Catalog

This appendix groups `rco` commands by workflow and points you back to the chapters that teach them.

Print current help at any time:

```bash
rco --help
rco run --help
```

## Command families

| Workflow | Commands | Learn path |
| --- | --- | --- |
| Run code | `run`, `repl`, `check`, `test` | Chapters 01, 02, 12 |
| Start a project | `new`, `doctor`, `routes`, `serve` | Chapters 23 through 27, 37 |
| Debug and inspect | `debug`, `debug-tui`, `debug-web`, `debug-adapter`, `lsp-diagnostics` | Chapters 13 and 32 |
| Build artifacts | `build`, `run-bytecode`, `image`, `emit-source` | Chapter 33 |
| Package apps | `package`, `gui`, `tui` | Chapters 21, 22, 34, 36, 38 |
| Database lifecycle | `migrate`, `seed` | Chapters 26 and 37 |
| Packages | `add`, `install`, `verify`, `audit` | Chapter 29 |
| Registries | `publish`, `registry`, `search` | Chapter 30 |
| Docs and style | `doc`, `fmt`, `lint`, `words` | Chapters 12 and 32 |
| Language server | `lsp` | Chapter 32 |

## First commands to try

For a script:

```bash
rco run path/to/app.rco
rco lint path/to/app.rco
```

For an MVC app:

```bash
rco routes path/to/app
rco doctor path/to/app
rco test path/to/app
```

For a package project:

```bash
rco install
rco verify
rco audit
```

For a release artifact:

```bash
rco package path/to/app.rco --output app.exe
```

Add `--tui` for terminal apps, `--gui` for desktop webview apps, and `--gui --mvc` for MVC projects packaged as local-server desktop apps.
