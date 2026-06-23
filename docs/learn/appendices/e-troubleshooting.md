# Appendix E: Troubleshooting

This appendix helps you move from an error message to the next useful command. Prefer the smallest check that reproduces the issue.

## Quick checks

| Symptom | Likely cause | Next command |
| --- | --- | --- |
| `rco` not found | Installed binary is missing from `PATH` | Check your install and shell `PATH`; then run `rco --help`. |
| Syntax or parse error | Infix habit, wrong selector shape, bad import string | `rco lint path/to/file.rco` |
| Stack underflow | A word consumed more values than the stack had | Run the smallest script and inspect the line before the failure. |
| Type error | A word received the wrong value kind | Add `type` or `inspect` near the failing value. |
| `Result` unwrap failure | `value` was called on an error result | Check `ok?`, then print `error "kind" at` and `error "message" at`. |
| Filesystem denied | Profile or root disallows the path | Add `--fs-root PATH`, or use `workspace_resolve` to inspect containment. |
| HTTP denied | HTTP disabled or host not allowed | Use `--http-allow-host HOST`. |
| Socket denied | Raw sockets disabled or host not allowed | Use `--allow-sockets` and `--socket-allow-host HOST`. |
| Process or PTY denied | Process/PTY powers are always opt-in | Use `--allow-process`, `--allow-pty`, and optionally `--process-root PATH`. |
| TUI output leaves a messy terminal | App failed before terminal cleanup | Keep load/parse work before `tui_enter`; keep cleanup visible. |
| MVC route missing | Route file, controller, or action name mismatch | `rco routes path/to/app` |
| MVC compile issue | Manifest, controller, model, route, or view problem | `rco doctor path/to/app` |
| Migration confusion | Database state differs from migration files | `rco migrate status path/to/app` |
| Package verify failure | Manifest, lockfile, cache, or integrity mismatch | `rco verify path/to/project` |
| GUI package missing | Package command failed or wrote somewhere else | Check the package root path and output path. |

## Debug commands

Use JSON or smoke previews when you need evidence without a full interactive session:

```bash
rco debug --json --breakpoint 10 path/to/file.rco
rco debug-tui --smoke --breakpoint 10 path/to/file.rco
rco debug-web --smoke --breakpoint 10 path/to/file.rco
```

Use traces when the program runs but behavior is surprising:

```bash
rco run --trace-file trace.json path/to/file.rco
```

## When to fail loudly

If the evidence does not support a logical explanation, stop and collect more diagnostics instead of inventing a cause. Share the command, exact output, current path, and whether local generated state exists.
