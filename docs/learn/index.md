# Learn Ricochet

Learn Ricochet is a beginner-friendly guide to Ricochet, a modern postfix language for scripts, tools, apps, packages, and release workflows.

The guide starts slowly on purpose. If you have never heard of postfix notation or stack languages, begin with **Start Here** before Chapter 00. If you already know stack languages, skim the Start Here pages and move into the chapter sequence.

## Start Here

- [What Ricochet is for](start-here/00-what-is-ricochet-for.md)
- [How postfix reads](start-here/01-how-postfix-reads.md)
- [The stack as a workbench](start-here/02-the-stack-as-a-workbench.md)
- [How to use this guide](start-here/03-how-to-use-this-guide.md)

## Learning path

**Beginner runway:** Chapters 00 through 05 teach the toolchain, postfix reading, the stack, values, bindings, and small data. Do not rush these if Ricochet is your first stack language.

**Core language:** Chapters 06 through 13 teach math, strings, collections, results, control flow, OOP, testing, and debugging.

**Host and app surfaces:** Chapters 14 through 28 teach time, async, capabilities, files, HTTP, sockets, processes, TUI, GUI, MVC, data, auth, forms, and AI package boundaries.

**Professional workflows:** Chapters 29 through 38 teach packages, registries, macros, editor tooling, bytecode, release packaging, and capstone apps.

## Chapters

- [Chapter 00: Welcome to Ricochet](chapters/00-orientation.md)
- [Chapter 01: Your First Program](chapters/01-hello-world.md)
- [Chapter 02: Running Code and Getting Feedback](chapters/02-running-ricochet.md)
- [Chapter 03: How Postfix Reads](chapters/03-postfix-stack-thinking.md)
- [Chapter 04: Values, Literals, and Inspection](chapters/04-values-and-literals.md)
- [Chapter 05: Names, Bindings, and Small Data](chapters/05-bindings-and-data.md)
- [Chapter 06: Numbers, Math, and Truth](chapters/06-numbers-math-and-truth.md)
- [Chapter 07: Strings, JSON, and Regex](chapters/07-strings-json-and-regex.md)
- [Chapter 08: Collections](chapters/08-collections.md)
- [Chapter 09: Results and Errors](chapters/09-results-and-errors.md)
- [Chapter 10: Making Decisions and Reusing Code](chapters/10-control-flow-functions-and-blocks.md)
- [Chapter 11: OOP and Dispatch](chapters/11-oop-and-dispatch.md)
- [Chapter 12: Testing, Linting, and Formatting](chapters/12-testing-linting-and-formatting.md)
- [Chapter 13: Introspection And Debug Basics](chapters/13-introspection-and-debug-basics.md)
- [Chapter 14: Date, Time, And Duration](chapters/14-date-time-and-duration.md)
- [Chapter 15: Async And Tasks](chapters/15-async-and-tasks.md)
- [Chapter 16: Capabilities And Sandboxing](chapters/16-capabilities-and-sandboxing.md)
- [Chapter 17: Files, Workspaces, Environment, Config, And Secrets](chapters/17-files-workspaces-env-and-secrets.md)
- [Chapter 18: HTTP And Streams](chapters/18-http-and-streams.md)
- [Chapter 19: TCP And WebSocket Sockets](chapters/19-tcp-and-websocket-sockets.md)
- [Chapter 20: Processes And PTYs](chapters/20-processes-and-ptys.md)
- [Chapter 21: Terminal UI](chapters/21-terminal-ui.md)
- [Chapter 22: Webview And Desktop GUI](chapters/22-webview-and-desktop-gui.md)
- [Chapter 23: MVC First App](chapters/23-mvc-first-app.md)
- [Chapter 24: Routes, Controllers, And Responses](chapters/24-routes-controllers-and-responses.md)
- [Chapter 25: Templates, Static Assets, And Uploads](chapters/25-templates-static-assets-and-uploads.md)
- [Chapter 26: Data, Active Record, And Migrations](chapters/26-data-active-record-and-migrations.md)
- [Chapter 27: Sessions, Forms, Auth, And Passwords](chapters/27-sessions-forms-auth-and-passwords.md)
- [Chapter 28: AI Capabilities And The AI Package](chapters/28-ai-capabilities-and-ai-package.md)
- [Chapter 29: Packages, Imports, And Dependencies](chapters/29-packages-imports-and-dependencies.md)
- [Chapter 30: Registries, Publish, Yank, And Mirror](chapters/30-registries-publish-yank-and-mirror.md)
- [Chapter 31: Macros And Expansion](chapters/31-macros-and-expansion.md)
- [Chapter 32: Debugger, DAP, LSP, And Editor Tools](chapters/32-debugger-dap-lsp-and-editor-tools.md)
- [Chapter 33: Bytecode, Images, And Source Emission](chapters/33-bytecode-images-and-source-emission.md)
- [Chapter 34: Packaging, Release, And Updates](chapters/34-packaging-release-and-updates.md)
- [Chapter 35: Capstone CLI Tool](chapters/35-capstone-cli-tool.md)
- [Chapter 36: Capstone TUI Dashboard](chapters/36-capstone-tui-dashboard.md)
- [Chapter 37: Capstone MVC App](chapters/37-capstone-mvc-app.md)
- [Chapter 38: Capstone Packaged GUI App](chapters/38-capstone-packaged-gui-app.md)

## Concepts

- [Concept: Application Surfaces](concepts/application-surfaces.md)
- [Concept: Bindings vs. Stack Juggling](concepts/bindings-vs-stack-juggling.md)
- [Concept: Capabilities and Sandboxing](concepts/capabilities-and-sandboxing.md)
- [Concept: Postfix Evaluation](concepts/postfix-evaluation.md)
- [Concept: Results and Errors](concepts/results-and-errors.md)
- [Concept: Stack Effects](concepts/stack-effects.md)

## How-To Guides

- [How to Choose a Data Shape](how-to/choose-a-data-shape.md)
- [How to Install and Run Ricochet](how-to/install-and-run.md)
- [How to Read Diagnostics](how-to/read-diagnostics.md)
- [How to Use the Examples](how-to/use-examples.md)

## Appendices

- [Appendix A: Word Groups at a Glance](appendices/a-word-catalog.md)
- [Appendix B: CLI Command Catalog](appendices/b-cli-command-catalog.md)
- [Appendix C: Capability Flags](appendices/c-capability-flags.md)
- [Appendix D: Syntax Guardrails](appendices/d-syntax-guardrails.md)
- [Appendix E: Troubleshooting](appendices/e-troubleshooting.md)
- [Appendix F: Glossary](appendices/f-glossary.md)

## Style note

The main path uses installed `rco` commands. Source-checkout and documentation-maintenance commands belong in contributor documentation unless a chapter specifically teaches them.
