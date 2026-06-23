# Learn Ricochet Examples

These examples support the Learn Ricochet manual. They are intentionally small,
local, and safe to run from a source checkout while the RC1 feature set is
frozen.

Run any example with the workspace CLI:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/01-hello-world/main.rco
```

## Examples

| Chapter | Example | Command |
| --- | --- | --- |
| 01 | Hello World | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/01-hello-world/main.rco` |
| 03 | Stack Receipt | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/03-stack/main.rco` |
| 04 | Value Tour | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/04-values/value-tour.rco` |
| 05 | Profile Card | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/05-bindings-and-data/profile-card.rco` |
| 06 | Budget Calculator | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/06-numbers-math-and-truth/budget.rco` |
| 07 | Log Cleaner | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/07-strings-json-and-regex/log-cleaner.rco` |
| 08 | Collections Task List | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/08-collections/main.rco` |
| 09 | Results Config Loader | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/09-results-and-errors/config-loader.rco` |
| 10 | Control-Flow Gradebook | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/10-control-flow/main.rco` |
| 11 | OOP Contact Book | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/11-oop/main.rco` |
| 12 | Testing Loop | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/12-testing-linting-and-formatting/main.rco` |
| 13 | Inspect Tour | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/13-introspection-and-debug-basics/debug-tour.rco` |
| 14 | Reminder Time Math | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/14-date-time-and-duration/reminder.rco` |
| 15 | Parallel Checks | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/15-async/parallel-checks.rco` |
| 16 | Capability Report | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/16-capabilities-and-sandboxing/capability-report.rco` |
| 17 | Settings Loader | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/17-files-workspaces-env-and-secrets/settings-loader.rco` |
| 18 | HTTP Client | `powershell -ExecutionPolicy Bypass -File examples/learn/18-http-streams/run-local.ps1` |
| 19 | TCP Echo | `cargo run -q -p ricochet_cli --bin rco -- run --capability-profile sandboxed --socket-allow-host 127.0.0.1 examples/learn/19-sockets/tcp_echo.rco` |
| 19 | WebSocket Echo | `cargo run -q -p ricochet_cli --bin rco -- run --capability-profile sandboxed --socket-allow-host 127.0.0.1 examples/learn/19-sockets/ws_echo.rco` |
| 20 | Process And PTY Runner | `cargo run -q -p ricochet_cli --bin rco -- run --allow-process --allow-pty examples/learn/20-processes-and-ptys/tool-runner.rco` |
| 21 | Task Dashboard TUI | `cargo run -q -p ricochet_cli --bin rco -- tui examples/learn/21-tui/task-dashboard.rco` |
| 22 | Notes GUI Document | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/22-gui/notes_gui.rco` |
| 23 | MVC First App Routes | `cargo run -q -p ricochet_cli --bin rco -- routes examples/learn/23-mvc/first_app` |
| 24 | MVC Controller Responses | `cargo run -q -p ricochet_cli --bin rco -- routes examples/learn/23-mvc/controllers` |
| 25 | Templates, Assets, Uploads | `cargo run -q -p ricochet_cli --bin rco -- doctor examples/learn/23-mvc/templates_uploads` |
| 26 | Contacts Data App | `cargo run -q -p ricochet_cli --bin rco -- migrate status examples/learn/26-data/contacts_app` |
| 27 | Auth And Forms Harness | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/27-auth-forms/login_flow/auth_flow.rco` |
| 28 | AI Fake Provider Harness | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/28-ai/fake_provider_chat/fake_provider.rco` |
| 29 | Local Math Package | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/29-packages/local_math_package/main.rco` |
| 30 | Local Registry Lab | `cargo run -q -p ricochet_cli --bin rco -- registry check examples/learn/30-registries/local_registry_lab/registry` |
| 31 | Route Macro Lab | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/31-macros/route_macro/main.rco` |
| 32 | Debugger Editor App | `cargo run -q -p ricochet_cli --bin rco -- debug-tui --smoke --breakpoint 10 examples/learn/32-debugger-editor/debuggable_app.rco` |
| 33 | Bytecode Image Lab | `powershell -ExecutionPolicy Bypass -File examples/learn/33-bytecode-images-and-source-emission/run-lab.ps1` |
| 34 | Packaging Release Lab | `powershell -ExecutionPolicy Bypass -File examples/learn/34-packaging-release-and-updates/run-lab.ps1` |
| 35 | Worklog CLI Capstone | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/35-capstone-cli/worklog/main.rco` |
| 35 | Worklog CLI Tests | `cargo run -q -p ricochet_cli --bin rco -- test examples/learn/35-capstone-cli/worklog` |
| 36 | Service Dashboard TUI | `cargo run -q -p ricochet_cli --bin rco -- tui examples/learn/36-capstone-tui/service_dashboard/dashboard.rco` |
| 36 | Service Dashboard Tests | `cargo run -q -p ricochet_cli --bin rco -- test examples/learn/36-capstone-tui/service_dashboard/ServiceDashboardTest.rco` |
| 37 | Project Journal Routes | `cargo run -q -p ricochet_cli --bin rco -- routes examples/learn/37-capstone-mvc/project_journal` |
| 37 | Project Journal Doctor | `cargo run -q -p ricochet_cli --bin rco -- doctor examples/learn/37-capstone-mvc/project_journal` |
| 37 | Project Journal Migration Status | `cargo run -q -p ricochet_cli --bin rco -- migrate status examples/learn/37-capstone-mvc/project_journal` |
| 37 | Project Journal Tests | `cargo run -q -p ricochet_cli --bin rco -- test examples/learn/37-capstone-mvc/project_journal` |
| 38 | Personal Ledger GUI | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/38-capstone-gui/personal_ledger/ledger_gui.rco` |
| 38 | Personal Ledger Tests | `cargo run -q -p ricochet_cli --bin rco -- test examples/learn/38-capstone-gui/personal_ledger/LedgerTest.rco` |
| 38 | Personal Ledger Package | `powershell -ExecutionPolicy Bypass -File examples/learn/38-capstone-gui/personal_ledger/run-package.ps1` |

`examples.json` is the runnable manifest used by validation and future manual
tooling.
