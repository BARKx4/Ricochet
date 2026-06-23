# Appendix B: CLI Command Catalog

## Purpose

This appendix groups `rco` commands by workflow and points you back to the
chapters that teach them.

Print current help at any time:

```powershell
cargo run -q -p ricochet_cli --bin rco -- --help
cargo run -q -p ricochet_cli --bin rco -- run --help
```

## Command Families

| Workflow | Commands | Learn path |
| --- | --- | --- |
| Start a project | `new`, `doctor`, `routes`, `serve` | Chapters 23 through 27, 37 |
| Run code | `run`, `repl`, `check`, `test` | Chapters 01, 02, 12 |
| Debug and inspect | `debug`, `debug-tui`, `debug-web`, `debug-adapter`, `lsp-diagnostics` | Chapters 13 and 32 |
| Build artifacts | `build`, `run-bytecode`, `image`, `emit-source` | Chapter 33 |
| Package apps | `package`, `gui`, `tui` | Chapters 21, 22, 34, 36, 38 |
| Database lifecycle | `migrate`, `seed` | Chapters 26 and 37 |
| Packages | `add`, `install`, `verify`, `audit` | Chapter 29 |
| Registries | `publish`, `registry`, `search` | Chapter 30 |
| Docs and style | `doc`, `fmt`, `lint`, `words` | Chapters 12 and 32 |
| Benchmarks | `bench` | Reference docs and contributor workflows |
| Language server | `lsp` | Chapter 32 |

## First Commands To Try

For a script:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run path/to/app.rco
cargo run -q -p ricochet_cli --bin rco -- lint path/to/app.rco
```

For an MVC app:

```powershell
cargo run -q -p ricochet_cli --bin rco -- routes path/to/app
cargo run -q -p ricochet_cli --bin rco -- doctor path/to/app
cargo run -q -p ricochet_cli --bin rco -- test path/to/app
```

For a package project:

```powershell
cargo run -q -p ricochet_cli --bin rco -- install
cargo run -q -p ricochet_cli --bin rco -- verify
cargo run -q -p ricochet_cli --bin rco -- audit
```

For a release artifact:

```powershell
cargo run -q -p ricochet_cli --bin rco -- package path/to/app.rco --output app.exe
```

Add `--tui` for terminal apps, `--gui` for desktop webview apps, and
`--gui --mvc` for MVC projects packaged as local-server desktop apps.

Status: drafted from current `rco --help`.
