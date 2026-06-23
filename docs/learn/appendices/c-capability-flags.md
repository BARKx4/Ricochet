# Appendix C: Capability Flags

## Purpose

This appendix summarizes the host capability flags used by local scripts and
tests. Capabilities decide what a program can ask the host process to do.

## Profiles

| Profile | Default behavior |
| --- | --- |
| `trusted` | Enables filesystem, HTTP, TUI, webview, environment, sleep, and workspace-style local development powers. Process, PTY, and raw sockets remain opt-in. |
| `sandboxed` | Starts with broad host powers disabled. Open only the specific roots, hosts, variables, or UI surfaces the program needs. |

## Common Flags

| Need | Flags |
| --- | --- |
| Disable filesystem | `--no-fs` |
| Restrict filesystem | `--fs-root PATH` |
| Read-only filesystem | `--fs-readonly` |
| Disable HTTP | `--no-http` |
| Restrict HTTP hosts | `--http-allow-host HOST` |
| Enable process execution | `--allow-process` |
| Restrict process and PTY cwd | `--process-root PATH` |
| Enable PTY sessions | `--allow-pty` |
| Disable or enable terminal UI | `--no-tui`, `--allow-tui` |
| Disable or enable webview GUI | `--no-webview`, `--allow-webview` |
| Disable environment access | `--no-env` |
| Restrict environment names | `--env-allow NAME` |
| Disable sleep | `--no-sleep` |
| Enable raw TCP/WebSocket sockets | `--allow-sockets` |
| Restrict socket hosts | `--socket-allow-host HOST` |

## Examples

Run a file-reading script in a sandbox rooted at the repo:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run --capability-profile sandboxed --fs-root . examples/learn/17-files-workspaces-env-and-secrets/settings-loader.rco
```

Run a local HTTP example against loopback only:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run --capability-profile sandboxed --http-allow-host 127.0.0.1 app.rco http://127.0.0.1:3000
```

Run a terminal app with the terminal capability:

```powershell
cargo run -q -p ricochet_cli --bin rco -- tui examples/learn/36-capstone-tui/service_dashboard/dashboard.rco
```

Run a process/PTY example with explicit opt-ins:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run --allow-process --allow-pty examples/learn/20-processes-and-ptys/tool-runner.rco
```

## Safety Rule

Treat destructive words such as `fs_delete` and `workspace_delete` as explicit
operator actions. Resolve and inspect paths before writing, moving, or deleting
anything.

Status: drafted from current `rco run --help`.
