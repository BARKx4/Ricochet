# Learn Ricochet Manual Map

This map tracks the public source structure for the Learn Ricochet manual. Status values describe the source manual only: `planned` means the page has its production-facing skeleton, `drafted` means prose and examples have been written, and `validated` means examples and coverage checks have passed.

## Part I: Orientation And Core Mental Model

Chapters 00 through 03 introduce Ricochet, the `rco` workflow, and the postfix stack model.

## Part II: Language Core And Feedback Loop

Chapters 04 through 13 cover values, data, math, strings, collections, results, control flow, OOP, tests, linting, formatting, and early debugging.

## Part III: Host Capabilities And Local App Surfaces

Chapters 14 through 22 cover time, async tasks, host capability boundaries, files, HTTP, sockets, processes, PTYs, terminal UI, and desktop GUI surfaces.

## Part IV: MVC, Data, Auth, Forms, And AI

Chapters 23 through 28 cover Ricochet MVC applications, routing, templates, uploads, databases, migrations, sessions, forms, authentication helpers, and AI integration.

## Part V: Packages, Registries, Macros, Tooling, And Release

Chapters 29 through 34 cover dependency workflows, package registries, macros, debugger/editor tools, bytecode/images/source emission, packaging, release artifacts, and update metadata.

## Part VI: Capstone Applications

Chapters 35 through 38 bring the manual together through complete CLI, TUI, MVC, and GUI applications.

## Chapter Map

| Part | Chapter | Title | Source | Status | Example path |
| --- | ---: | --- | --- | --- | --- |
| Part I: Orientation And Core Mental Model | 00 | Orientation | [00-orientation.md](chapters/00-orientation.md) | drafted | None |
| Part I: Orientation And Core Mental Model | 01 | Hello World | [01-hello-world.md](chapters/01-hello-world.md) | drafted | `examples/learn/01-hello-world/main.rco` |
| Part I: Orientation And Core Mental Model | 02 | Running Ricochet | [02-running-ricochet.md](chapters/02-running-ricochet.md) | drafted | REPL and CLI commands |
| Part I: Orientation And Core Mental Model | 03 | Postfix Stack Thinking | [03-postfix-stack-thinking.md](chapters/03-postfix-stack-thinking.md) | drafted | `examples/learn/03-stack/main.rco` |
| Part II: Language Core And Feedback Loop | 04 | Values And Literals | [04-values-and-literals.md](chapters/04-values-and-literals.md) | drafted | `examples/learn/04-values/value-tour.rco` |
| Part II: Language Core And Feedback Loop | 05 | Bindings And Data | [05-bindings-and-data.md](chapters/05-bindings-and-data.md) | drafted | `examples/learn/05-bindings-and-data/profile-card.rco` |
| Part II: Language Core And Feedback Loop | 06 | Numbers, Math, And Truth | [06-numbers-math-and-truth.md](chapters/06-numbers-math-and-truth.md) | drafted | `examples/learn/06-numbers-math-and-truth/budget.rco` |
| Part II: Language Core And Feedback Loop | 07 | Strings, JSON, And Regex | [07-strings-json-and-regex.md](chapters/07-strings-json-and-regex.md) | drafted | `examples/learn/07-strings-json-and-regex/log-cleaner.rco` |
| Part II: Language Core And Feedback Loop | 08 | Collections | [08-collections.md](chapters/08-collections.md) | drafted | `examples/learn/08-collections/main.rco` |
| Part II: Language Core And Feedback Loop | 09 | Results And Errors | [09-results-and-errors.md](chapters/09-results-and-errors.md) | drafted | `examples/learn/09-results-and-errors/config-loader.rco` |
| Part II: Language Core And Feedback Loop | 10 | Control Flow, Functions, And Blocks | [10-control-flow-functions-and-blocks.md](chapters/10-control-flow-functions-and-blocks.md) | drafted | `examples/learn/10-control-flow/main.rco` |
| Part II: Language Core And Feedback Loop | 11 | OOP And Dispatch | [11-oop-and-dispatch.md](chapters/11-oop-and-dispatch.md) | drafted | `examples/learn/11-oop/main.rco` |
| Part II: Language Core And Feedback Loop | 12 | Testing, Linting, And Formatting | [12-testing-linting-and-formatting.md](chapters/12-testing-linting-and-formatting.md) | drafted | `examples/learn/12-testing-linting-and-formatting/main.rco` |
| Part II: Language Core And Feedback Loop | 13 | Introspection And Debug Basics | [13-introspection-and-debug-basics.md](chapters/13-introspection-and-debug-basics.md) | drafted | `examples/learn/13-introspection-and-debug-basics/debug-tour.rco` |
| Part III: Host Capabilities And Local App Surfaces | 14 | Date, Time, And Duration | [14-date-time-and-duration.md](chapters/14-date-time-and-duration.md) | drafted | `examples/learn/14-date-time-and-duration/reminder.rco` |
| Part III: Host Capabilities And Local App Surfaces | 15 | Async And Tasks | [15-async-and-tasks.md](chapters/15-async-and-tasks.md) | drafted | `examples/learn/15-async/parallel-checks.rco` |
| Part III: Host Capabilities And Local App Surfaces | 16 | Capabilities And Sandboxing | [16-capabilities-and-sandboxing.md](chapters/16-capabilities-and-sandboxing.md) | drafted | `examples/learn/16-capabilities-and-sandboxing/capability-report.rco` |
| Part III: Host Capabilities And Local App Surfaces | 17 | Files, Workspaces, Environment, Config, And Secrets | [17-files-workspaces-env-and-secrets.md](chapters/17-files-workspaces-env-and-secrets.md) | drafted | `examples/learn/17-files-workspaces-env-and-secrets/settings-loader.rco` |
| Part III: Host Capabilities And Local App Surfaces | 18 | HTTP And Streams | [18-http-and-streams.md](chapters/18-http-and-streams.md) | drafted | `examples/learn/18-http-streams/api-client.rco` |
| Part III: Host Capabilities And Local App Surfaces | 19 | TCP And WebSocket Sockets | [19-tcp-and-websocket-sockets.md](chapters/19-tcp-and-websocket-sockets.md) | drafted | `examples/learn/19-sockets/tcp_echo.rco`; `examples/learn/19-sockets/ws_echo.rco` |
| Part III: Host Capabilities And Local App Surfaces | 20 | Processes And PTYs | [20-processes-and-ptys.md](chapters/20-processes-and-ptys.md) | drafted | `examples/learn/20-processes-and-ptys/tool-runner.rco` |
| Part III: Host Capabilities And Local App Surfaces | 21 | Terminal UI | [21-terminal-ui.md](chapters/21-terminal-ui.md) | drafted | `examples/learn/21-tui/task-dashboard.rco` |
| Part III: Host Capabilities And Local App Surfaces | 22 | Webview And Desktop GUI | [22-webview-and-desktop-gui.md](chapters/22-webview-and-desktop-gui.md) | drafted | `examples/learn/22-gui/notes_gui.rco` |
| Part IV: MVC, Data, Auth, Forms, And AI | 23 | MVC First App | [23-mvc-first-app.md](chapters/23-mvc-first-app.md) | drafted | `examples/learn/23-mvc/first_app` |
| Part IV: MVC, Data, Auth, Forms, And AI | 24 | Routes, Controllers, And Responses | [24-routes-controllers-and-responses.md](chapters/24-routes-controllers-and-responses.md) | drafted | `examples/learn/23-mvc/controllers` |
| Part IV: MVC, Data, Auth, Forms, And AI | 25 | Templates, Static Assets, And Uploads | [25-templates-static-assets-and-uploads.md](chapters/25-templates-static-assets-and-uploads.md) | drafted | `examples/learn/23-mvc/templates_uploads` |
| Part IV: MVC, Data, Auth, Forms, And AI | 26 | Data, Active Record, And Migrations | [26-data-active-record-and-migrations.md](chapters/26-data-active-record-and-migrations.md) | drafted | `examples/learn/26-data/contacts_app` |
| Part IV: MVC, Data, Auth, Forms, And AI | 27 | Sessions, Forms, Auth, And Passwords | [27-sessions-forms-auth-and-passwords.md](chapters/27-sessions-forms-auth-and-passwords.md) | drafted | `examples/learn/27-auth-forms/login_flow` |
| Part IV: MVC, Data, Auth, Forms, And AI | 28 | AI Capabilities And The AI Package | [28-ai-capabilities-and-ai-package.md](chapters/28-ai-capabilities-and-ai-package.md) | drafted | `examples/learn/28-ai/fake_provider_chat` |
| Part V: Packages, Registries, Macros, Tooling, And Release | 29 | Packages, Imports, And Dependencies | [29-packages-imports-and-dependencies.md](chapters/29-packages-imports-and-dependencies.md) | drafted | `examples/learn/29-packages/local_math_package` |
| Part V: Packages, Registries, Macros, Tooling, And Release | 30 | Registries, Publish, Yank, And Mirror | [30-registries-publish-yank-and-mirror.md](chapters/30-registries-publish-yank-and-mirror.md) | drafted | `examples/learn/30-registries/local_registry_lab` |
| Part V: Packages, Registries, Macros, Tooling, And Release | 31 | Macros And Expansion | [31-macros-and-expansion.md](chapters/31-macros-and-expansion.md) | drafted | `examples/learn/31-macros/route_macro` |
| Part V: Packages, Registries, Macros, Tooling, And Release | 32 | Debugger, DAP, LSP, And Editor Tools | [32-debugger-dap-lsp-and-editor-tools.md](chapters/32-debugger-dap-lsp-and-editor-tools.md) | drafted | `examples/learn/32-debugger-editor/debuggable_app.rco` |
| Part V: Packages, Registries, Macros, Tooling, And Release | 33 | Bytecode, Images, And Source Emission | [33-bytecode-images-and-source-emission.md](chapters/33-bytecode-images-and-source-emission.md) | drafted | `examples/learn/33-bytecode-images-and-source-emission/image_lab.rco` |
| Part V: Packages, Registries, Macros, Tooling, And Release | 34 | Packaging, Release, And Updates | [34-packaging-release-and-updates.md](chapters/34-packaging-release-and-updates.md) | drafted | `examples/learn/34-packaging-release-and-updates/run-lab.ps1` |
| Part VI: Capstone Applications | 35 | Capstone CLI Tool | [35-capstone-cli-tool.md](chapters/35-capstone-cli-tool.md) | drafted | `examples/learn/35-capstone-cli/worklog` |
| Part VI: Capstone Applications | 36 | Capstone TUI Dashboard | [36-capstone-tui-dashboard.md](chapters/36-capstone-tui-dashboard.md) | drafted | `examples/learn/36-capstone-tui/service_dashboard` |
| Part VI: Capstone Applications | 37 | Capstone MVC App | [37-capstone-mvc-app.md](chapters/37-capstone-mvc-app.md) | drafted | `examples/learn/37-capstone-mvc/project_journal` |
| Part VI: Capstone Applications | 38 | Capstone Packaged GUI App | [38-capstone-packaged-gui-app.md](chapters/38-capstone-packaged-gui-app.md) | drafted | `examples/learn/38-capstone-gui/personal_ledger` |

## Appendices

| Appendix | Title | Source | Status | Purpose |
| --- | --- | --- | --- | --- |
| A | Word Catalog | [a-word-catalog.md](appendices/a-word-catalog.md) | drafted | Compact coverage table for the live word inventory. |
| B | CLI Command Catalog | [b-cli-command-catalog.md](appendices/b-cli-command-catalog.md) | drafted | Command lookup by workflow. |
| C | Capability Flags | [c-capability-flags.md](appendices/c-capability-flags.md) | drafted | Host-power flags and safety boundaries. |
| D | Syntax Guardrails | [d-syntax-guardrails.md](appendices/d-syntax-guardrails.md) | drafted | Postfix syntax shapes and common corrections. |
| E | Troubleshooting | [e-troubleshooting.md](appendices/e-troubleshooting.md) | drafted | Common error messages and recovery paths. |
| F | Glossary | [f-glossary.md](appendices/f-glossary.md) | drafted | Short definitions for terms used across the manual. |
