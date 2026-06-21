# Ricochet Showcase Apps

These examples are beta developer targets: they show how the language, packages,
GUI host, debugger, and MVC runtime fit together without claiming production
hardening.

## SQLite Notes MVC

```powershell
Push-Location examples/showcase/sqlite_notes
rco migrate apply .
rco serve --fs-root .
```

The app maps a `Note` model to SQLite, renders a notes page, and accepts a
simple form-backed insert.

## Package Auth And Forms

```powershell
rco run examples/showcase/package_auth_forms/main.rco
```

This script consumes the repo-local `@ricochet/auth` and `@ricochet/forms`
packages by local aliases.

## Package Macro Queue Report

```powershell
rco run examples/showcase/package_macro_queue_report/main.rco
Push-Location examples/showcase/package_macro_queue_report
rco expand main.rco --json
```

This showcase keeps the app postfix-shaped while importing a local package
macro module through a manifest dependency. The package exports the public
`install_queue_report` macro, keeps `_count_line` private, and leaves
`rco expand --json` with canonical package-aware module metadata for the locked
dependency.

## AI Provider Probe

```powershell
rco run examples/showcase/ai_provider_probe/main.rco
rco run examples/showcase/ai_provider_probe/fake_provider.rco
rco run examples/showcase/ai_provider_probe/local_model_request.rco
rco run examples/showcase/ai_provider_probe/ollama_native_request.rco
rco run --env-allow OPENAI_API_KEY --http-allow-host api.openai.com examples/showcase/ai_provider_probe/live_probe.rco
```

`main.rco` is an OpenAI-compatible dry-run request builder. `fake_provider.rco`
executes the package provider flow through an injected fake executor, including
retry and response normalization. `local_model_request.rco` builds a local
OpenAI-compatible request for endpoints such as Ollama or LM Studio when they
expose `/v1/chat/completions`. `ollama_native_request.rco` builds Ollama's
native `/api/chat` request shape. `live_probe.rco` performs the HTTP request
only when explicitly granted environment and HTTP capabilities.

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
