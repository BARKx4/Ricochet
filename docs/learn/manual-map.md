# Learn Ricochet Manual Map

This map is a learner-facing navigation page. It describes what each chapter teaches and where it fits in the course. It intentionally avoids source-maintenance status labels.

## Recommended path for new readers

1. Read the Start Here pages.
2. Complete Chapters 00 through 10 in order.
3. Read Chapter 12 before using host capabilities or app frameworks.
4. Choose the app surface you need: CLI, TUI, GUI, MVC, packages, or release packaging.

## Parts

| Part | Chapters | Purpose |
| --- | --- | --- |
| Part I: Orientation and Core Mental Model | 00-03 | Welcome, first program, running code, postfix reading, and the stack. |
| Part II: Language Core and Feedback Loop | 04-13 | Values, bindings, data, math, text, collections, results, control flow, OOP, tests, and debugging. |
| Part III: Host Capabilities and Local App Surfaces | 14-22 | Time, async, capabilities, files, HTTP, sockets, processes, TUI, and GUI. |
| Part IV: MVC, Data, Auth, Forms, and AI | 23-28 | Web app layers, persistence, sessions, forms, and AI package boundaries. |
| Part V: Packages, Registries, Macros, Tooling, and Release | 29-34 | Reuse, distribution, expansion, editor tools, bytecode, images, and release packaging. |
| Part VI: Capstone Applications | 35-38 | Complete CLI, TUI, MVC, and packaged GUI apps. |

## Chapter map

| Chapter | Title | What the reader learns |
| --- | --- | --- |
| [00](chapters/00-orientation.md) | Chapter 00: Welcome to Ricochet | What Ricochet is, how the guide is organized, and why postfix order matters. |
| [01](chapters/01-hello-world.md) | Chapter 01: Your First Program | How to run the smallest useful Ricochet script. |
| [02](chapters/02-running-ricochet.md) | Chapter 02: Running Code and Getting Feedback | How to use `rco run`, `rco repl`, help, and diagnostics. |
| [03](chapters/03-postfix-stack-thinking.md) | Chapter 03: How Postfix Reads | How to trace postfix code and use the starter stack vocabulary. |
| [04](chapters/04-values-and-literals.md) | Chapter 04: Values, Literals, and Inspection | How to recognize and inspect runtime values. |
| [05](chapters/05-bindings-and-data.md) | Chapter 05: Names, Bindings, and Small Data | How to use bindings, arrays, and maps to keep stack code readable. |
| [06](chapters/06-numbers-math-and-truth.md) | Chapter 06: Numbers, Math, and Truth | How postfix arithmetic, comparisons, assertions, and conversions work. |
| [07](chapters/07-strings-json-and-regex.md) | Chapter 07: Strings, JSON, and Regex | How to process strings, JSON, and regex results. |
| [08](chapters/08-collections.md) | Chapter 08: Collections | How to use arrays, maps, sets, ranges, mutation, and block-based collection words. |
| [09](chapters/09-results-and-errors.md) | Chapter 09: Results and Errors | How explicit `Result` values model success and failure. |
| [10](chapters/10-control-flow-functions-and-blocks.md) | Chapter 10: Making Decisions and Reusing Code | How conditionals, loops, blocks, and functions shape programs. |
| [11](chapters/11-oop-and-dispatch.md) | Chapter 11: OOP and Dispatch | How classes, accessors, methods, and dispatch work. |
| [12](chapters/12-testing-linting-and-formatting.md) | Chapter 12: Testing, Linting, and Formatting | How tests, linting, and formatting create a feedback loop. |
| [13](chapters/13-introspection-and-debug-basics.md) | Chapter 13: Introspection And Debug Basics | Once programs have names, data, functions, and tests, you need tools for seeing what the runtime sees. Introspection and debugging make stack behavior observable instead of mysterious. |
| [14](chapters/14-date-time-and-duration.md) | Chapter 14: Date, Time, And Duration | Dates and durations are common in reminders, logs, packages, sessions, and release workflows. This chapter teaches time as data before you use it inside larger applications. |
| [15](chapters/15-async-and-tasks.md) | Chapter 15: Async And Tasks | Async work lets a program wait for several things without turning every wait into a blocking pause. The important beginner model is that a task is a value you can start, wait on, and reason about. |
| [16](chapters/16-capabilities-and-sandboxing.md) | Chapter 16: Capabilities And Sandboxing | Host capabilities are the safety boundary between Ricochet code and your machine. You should understand that boundary before reading files, calling HTTP, opening sockets, or launching processes. |
| [17](chapters/17-files-workspaces-env-and-secrets.md) | Chapter 17: Files, Workspaces, Environment, Config, And Secrets | Useful scripts often need configuration, files, environment variables, and secrets. Ricochet treats those as explicit host interactions that should be scoped and checked. |
| [18](chapters/18-http-and-streams.md) | Chapter 18: HTTP And Streams | HTTP connects Ricochet programs to services. Streams let programs handle data progressively instead of pretending every response is a small string. |
| [19](chapters/19-tcp-and-websocket-sockets.md) | Chapter 19: TCP And WebSocket Sockets | Raw sockets and WebSockets are lower-level network tools. They are powerful, so this chapter emphasizes explicit hosts, loopback practice, and cleanup. |
| [20](chapters/20-processes-and-ptys.md) | Chapter 20: Processes And PTYs | Process and PTY control lets Ricochet coordinate external tools. This is high-trust host power, so the chapter teaches deliberate opt-ins and clear boundaries. |
| [21](chapters/21-terminal-ui.md) | Chapter 21: Terminal UI | Terminal UIs are local applications with visual state. The same stack and collection habits now drive frames, rows, keys, and cleanup. |
| [22](chapters/22-webview-and-desktop-gui.md) | Chapter 22: Webview And Desktop GUI | Desktop GUI work introduces a browser-like surface while keeping your application local. The key model is data flowing between Ricochet and a webview document. |
| [23](chapters/23-mvc-first-app.md) | Chapter 23: MVC First App | MVC apps are the first large application shape in the guide. Routes, controllers, views, and models are easier when you see them as named layers around the same Ricochet language. |
| [24](chapters/24-routes-controllers-and-responses.md) | Chapter 24: Routes, Controllers, And Responses | Routes turn HTTP requests into controller actions. This chapter teaches how requests become data and how controller methods return responses. |
| [25](chapters/25-templates-static-assets-and-uploads.md) | Chapter 25: Templates, Static Assets, And Uploads | Templates and assets turn controller data into user-facing pages. Uploads add a host-resource boundary, so the chapter keeps size, path, and cleanup concerns visible. |
| [26](chapters/26-data-active-record-and-migrations.md) | Chapter 26: Data, Active Record, And Migrations | Persistent data requires a stronger mental model than maps in memory. Models, migrations, and records connect Ricochet objects to database state. |
| [27](chapters/27-sessions-forms-auth-and-passwords.md) | Chapter 27: Sessions, Forms, Auth, And Passwords | Forms and sessions are where web apps meet user identity. This chapter teaches the flow without hiding password and session safety behind magic. |
| [28](chapters/28-ai-capabilities-and-ai-package.md) | Chapter 28: AI Capabilities And The AI Package | AI features are application boundaries: prompts, providers, transcripts, tool permissions, and fallback behavior. The safest model is explicit capability, explicit provider, explicit result. |
| [29](chapters/29-packages-imports-and-dependencies.md) | Chapter 29: Packages, Imports, And Dependencies | Packages let Ricochet projects reuse code. Imports, manifests, locks, verification, and audit commands keep reuse understandable and reproducible. |
| [30](chapters/30-registries-publish-yank-and-mirror.md) | Chapter 30: Registries, Publish, Yank, And Mirror | Registries turn packages into shared artifacts. This chapter teaches local registry practice before public publishing concerns. |
| [31](chapters/31-macros-and-expansion.md) | Chapter 31: Macros And Expansion | Macros generate code before runtime. They are powerful, so the chapter teaches ordinary functions and packages first, then introduces expansion as a deliberate tool. |
| [32](chapters/32-debugger-dap-lsp-and-editor-tools.md) | Chapter 32: Debugger, DAP, LSP, And Editor Tools | Large projects need editor feedback, traces, breakpoints, and diagnostics. This chapter shows the professional tooling surface around Ricochet programs. |
| [33](chapters/33-bytecode-images-and-source-emission.md) | Chapter 33: Bytecode, Images, And Source Emission | Bytecode and images are runtime artifacts. Understanding them helps you separate source, compiled code, saved VM state, and emitted source views. |
| [34](chapters/34-packaging-release-and-updates.md) | Chapter 34: Packaging, Release, And Updates | Release work turns a working app into something another person can install. Packaging, signatures, metadata, and update channels are part of that handoff. |
| [35](chapters/35-capstone-cli-tool.md) | Chapter 35: Capstone CLI Tool | The CLI capstone combines data, results, files, tests, and reporting into a complete command-line tool. |
| [36](chapters/36-capstone-tui-dashboard.md) | Chapter 36: Capstone TUI Dashboard | The TUI capstone combines terminal rendering, data loading, tests, and safe cleanup into a complete local dashboard. |
| [37](chapters/37-capstone-mvc-app.md) | Chapter 37: Capstone MVC App | The MVC capstone combines routes, controllers, views, data, migrations, seeds, and tests into a complete web application. |
| [38](chapters/38-capstone-packaged-gui-app.md) | Chapter 38: Capstone Packaged GUI App | The final capstone combines GUI state, local data, tests, and packaging into a desktop-style application you can hand to someone else. |
