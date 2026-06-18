# Ricochet Showcase Apps

These examples are beta developer targets: they show how the language, packages,
GUI host, debugger, and MVC runtime fit together without claiming production
hardening.

## SQLite Notes MVC

```powershell
rco migrate apply examples/showcase/sqlite_notes
rco serve examples/showcase/sqlite_notes --fs-root examples/showcase/sqlite_notes
```

The app maps a `Note` model to SQLite, renders a notes page, and accepts a
simple form-backed insert.

## Package Auth And Forms

```powershell
rco run examples/showcase/package_auth_forms/main.rco
```

This script consumes the repo-local `@ricochet/auth` and `@ricochet/forms`
packages by local aliases.

## AI Provider Probe

```powershell
rco run examples/showcase/ai_provider_probe/main.rco
rco run --env-allow OPENAI_API_KEY --http-allow-host api.openai.com examples/showcase/ai_provider_probe/live_probe.rco
```

`main.rco` is a dry-run request builder. `live_probe.rco` performs the HTTP
request only when explicitly granted environment and HTTP capabilities.

## GUI Task Monitor

```powershell
rco gui examples/showcase/gui_task_monitor.rco
```

The GUI example uses the v2 webview document contract with explicit state,
actions, and a callback.

## Debugger Demo

```powershell
rco debug --step examples/showcase/debugger_demo.rco
rco debug-adapter
```

Set a source breakpoint on the line that reads `$worker await` to inspect the
stack, globals, and task snapshot from the terminal debugger or an IDE.
