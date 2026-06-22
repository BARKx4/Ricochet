# Learn Ricochet Manual Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a production-facing "Learn Ricochet" user manual that takes a new reader from Hello World through the full current Ricochet language, runtime, word catalog, host capabilities, MVC/web stack, TUI/GUI surfaces, package system, registry workflows, debugging tools, and release packaging.

**Architecture:** Treat the manual as a guided learning layer beside the existing reference. Keep the canonical tutorial source in `docs/learn`, keep runnable examples in `examples/learn`, generate or mirror static HTML under `docs/reference/learn` so it is served with the official reference, and enforce coverage with a word/feature matrix generated from `rco words --json` plus `docs/feature-map.md`.

**Tech Stack:** Ricochet Markdown/HTML docs, static reference CSS/JS in `docs/reference`, runnable `.rco` examples, PowerShell validation scripts, `rco words --json`, `rco words --check`, `rco run`, `rco test`, `rco new`, `rco serve`, `rco package`, and the existing GitHub Pages docs workflow.

---

## Global Constraints

- Keep the manual public and production-facing. Do not include agent handoff notes, local memory references, internal roadmap anxiety, or private cleanup history.
- Treat `v0.1.19-rc.1` as the frozen feature surface for the path to 1.0. Do not add or imply new Ricochet features while creating the manual; immediate future work should stay limited to fixes, security hardening, documentation, validation, and release readiness.
- Preserve the postfix/RPN teaching model from the first chapter onward.
- Do not introduce leading-dot syntax, receiver-first pseudo-object host APIs, dash-prefixed public word names, or examples that conflict with `AGENTS.md`.
- Every code sample must be runnable, or explicitly marked as a shell command, file tree, template fragment, or conceptual stack diagram.
- Every public word from `rco words --json` must appear in exactly one primary teaching chapter or appendix coverage entry.
- Every major feature family from `docs/feature-map.md` must have a guided chapter, a runnable example, or an explicit appendix entry.
- Host capability chapters must teach safety boundaries before powerful operations, especially filesystem deletion, workspace mutation, process, PTY, TCP, WebSocket, and HTTP.
- MVC examples must distinguish local beta scaffold auth from production auth helpers.
- Keep chapters short enough for linear reading. Use "Deepen It" sections and appendices for dense reference material.
- Update `docs/feature-map.md` only if the manual work discovers a stale feature claim.

---

## Target File Structure

Create or update these paths:

```text
docs/
  learn/
    index.md
    manual-map.md
    word-coverage.json
    chapters/
      00-orientation.md
      01-hello-world.md
      02-running-ricochet.md
      03-postfix-stack-thinking.md
      04-values-and-literals.md
      05-bindings-and-data.md
      06-numbers-math-and-truth.md
      07-strings-json-and-regex.md
      08-collections.md
      09-results-and-errors.md
      10-control-flow-functions-and-blocks.md
      11-oop-and-dispatch.md
      12-testing-linting-and-formatting.md
      13-introspection-and-debug-basics.md
      14-date-time-and-duration.md
      15-async-and-tasks.md
      16-capabilities-and-sandboxing.md
      17-files-workspaces-env-and-secrets.md
      18-http-and-streams.md
      19-tcp-and-websocket-sockets.md
      20-processes-and-ptys.md
      21-terminal-ui.md
      22-webview-and-desktop-gui.md
      23-mvc-first-app.md
      24-routes-controllers-and-responses.md
      25-templates-static-assets-and-uploads.md
      26-data-active-record-and-migrations.md
      27-sessions-forms-auth-and-passwords.md
      28-ai-capabilities-and-ai-package.md
      29-packages-imports-and-dependencies.md
      30-registries-publish-yank-and-mirror.md
      31-macros-and-expansion.md
      32-debugger-dap-lsp-and-editor-tools.md
      33-bytecode-images-and-source-emission.md
      34-packaging-release-and-updates.md
      35-capstone-cli-tool.md
      36-capstone-tui-dashboard.md
      37-capstone-mvc-app.md
      38-capstone-packaged-gui-app.md
    appendices/
      a-word-catalog.md
      b-cli-command-catalog.md
      c-capability-flags.md
      d-syntax-guardrails.md
      e-troubleshooting.md
      f-glossary.md
  reference/
    learn/
      index.html
      chapters/*.html
      appendices/*.html
    guides/index.html
  wiki/
    README.md
examples/
  learn/
    01-hello-world/
    03-stack/
    08-collections/
    10-control-flow/
    11-oop/
    15-async/
    18-http-streams/
    19-sockets/
    21-tui/
    22-gui/
    23-mvc/
    26-data/
    27-auth-forms/
    28-ai/
    29-packages/
    31-macros/
    35-capstone-cli/
    36-capstone-tui/
    37-capstone-mvc/
    38-capstone-gui/
scripts/
  validate-learn-manual.ps1
  render-learn-manual.ps1
```

If `scripts/render-learn-manual.ps1` would become too complex, replace it with a small Rust helper under the existing CLI or a dedicated docs helper. Do not hand-maintain dozens of duplicate Markdown and HTML chapters without a validation story.

---

## Manual Shape

The manual should read as a course:

1. Part I: language orientation and core mental model.
2. Part II: word families, values, control flow, OOP, tests, and debugging basics.
3. Part III: host capabilities and local application surfaces.
4. Part IV: MVC, data, auth, forms, and AI.
5. Part V: packages, registries, macros, tooling, bytecode, packaging, and release.
6. Part VI: capstone applications.
7. Appendices: compact lookup tables and troubleshooting.

Every chapter should use this local structure:

```markdown
# Chapter NN: Title

## What You Will Build
## Concepts
## Words Introduced
## Guided Example
## Try It
## Common Mistakes
## What You Know Now
```

For advanced feature chapters, add:

```markdown
## Safety Notes
## Production Notes
## Reference Links
```

---

## Chapter-By-Chapter Design

### Chapter 00: Orientation

**Definition:** Establish what Ricochet is, why postfix programming matters, and how the manual relates to the reference.

**Complete Means:**

- Explains Ricochet as a real language and toolchain, not a toy shell.
- Links to installation and contribution instructions without duplicating all release docs.
- Explains the reader path: scripts, CLI tools, TUI, GUI, MVC, packages.
- Introduces the difference between tutorial, reference, and wiki.

**Guided Example:** None. Keep this chapter short and welcoming.

**Primary Coverage:** `rco`, reference site, docs layout, examples layout.

### Chapter 01: Hello World

**Definition:** The first Ricochet program and the smallest useful stack mental model.

**Complete Means:**

- Shows a one-line file that prints a string.
- Shows `rco run`.
- Shows the same expression in the REPL.
- Uses a stack diagram for `"Hello, Ricochet" print`.
- Avoids teaching variables or control flow too early.

**Guided Example:** `examples/learn/01-hello-world/hello.rco`.

**Primary Coverage:** first script, `print`, stack input/output notation, comments.

### Chapter 02: Running Ricochet

**Definition:** Teach how to run source files, use the REPL, inspect help, and read diagnostics.

**Complete Means:**

- Covers `rco run`, `rco repl`, `rco --help`, `rco run --help`.
- Teaches comments, doc comments, string escapes, integer and float literals.
- Shows what a parse error and a runtime stack/type error look like.
- Introduces the examples directory and expected command prompt style.

**Guided Example:** `examples/learn/01-hello-world/repl-notes.rco`.

**Primary Coverage:** CLI basics, diagnostics, source file conventions.

### Chapter 03: Postfix Stack Thinking

**Definition:** Build fluency with stack movement before larger programs.

**Complete Means:**

- Teaches stack diagrams using before/after tables.
- Introduces stack words one small problem at a time.
- Explains when stack manipulation is helpful and when naming a value is clearer.
- Includes exercises that ask the reader to predict the stack.

**Guided Example:** `examples/learn/03-stack/receipt.rco`.

**Words Introduced:** `swap`, `dup`, `drop`, `over`, `rot`, `nip`, `tuck`, `pick`, `roll`, `depth`, `clear`.

### Chapter 04: Values And Literals

**Definition:** Teach Ricochet's runtime values and truthiness policy from a user's point of view.

**Complete Means:**

- Covers nil, booleans, integers, floats, strings, arrays, lists, maps, sets, classes, instances, blocks, tasks, results, regexes, and capabilities at a high level.
- Explains that decimal or exponent literals are floats and plain integer literals are signed integer numbers.
- Teaches truthiness with numbers, floats, strings, collections, nil, booleans, results, NaN, and infinities as implemented.
- Defers mutation and construction details to later chapters.

**Guided Example:** `examples/learn/04-values/value-tour.rco`.

**Primary Coverage:** `nil`, `true`, `false`, `nil?`, `empty?`, `type`, `inspect`.

### Chapter 05: Bindings And Data

**Definition:** Teach variables, named data, arrays, and maps.

**Complete Means:**

- Teaches `var` and `$name` reads.
- Teaches dynamic `"name" get` and `"name" value set` only after static reads are clear.
- Covers `array`, `map`, dynamic data lookup, and mutation.
- Explains when to choose a map over a class or model.

**Guided Example:** `examples/learn/05-bindings-and-data/profile-card.rco`.

**Words Introduced:** `array`, `map`, `var`, `get`, `set`, plus data predicates not covered in Chapter 04.

### Chapter 06: Numbers, Math, And Truth

**Definition:** Teach numeric arithmetic, comparison, boolean logic, conversions, clamping, and assertions.

**Complete Means:**

- Covers checked integer math and finite float behavior.
- Explains mixed numeric promotion.
- Explains conversion words returning `Result`.
- Teaches upper and lower numeric boundary thinking without overloading beginners.
- Covers comparison aliases and boolean helpers.
- Teaches assertion helpers as development tools.

**Guided Example:** `examples/learn/06-numbers-math-and-truth/budget.rco`.

**Words Introduced:** all `math` detail words from `rco words --json`, including readable aliases.

### Chapter 07: Strings, JSON, And Regex

**Definition:** Teach practical text processing.

**Complete Means:**

- Covers trimming, slicing, search, case conversion, concatenation, splitting/joining where available.
- Covers JSON encode/decode and result handling.
- Covers regex creation, matching, captures, replacement where available.
- Includes examples for request/response payloads and logs.

**Guided Example:** `examples/learn/07-strings-json-and-regex/log-cleaner.rco`.

**Words Introduced:** all `string` detail words from `rco words --json`.

### Chapter 08: Collections

**Definition:** Teach lists, maps, sets, ranges, indexing, mutation, and higher-order collection work.

**Complete Means:**

- Covers list and set construction.
- Covers indexing, counting, push/pop or equivalent mutation words.
- Covers map/filter/reduce/search style words.
- Uses first-class blocks only as deeply as needed, then points to Chapter 10.
- Explains shared mutable collection behavior.

**Guided Example:** `examples/learn/08-collections/task-list.rco`.

**Words Introduced:** all `collection` detail words from `rco words --json`.

### Chapter 09: Results And Errors

**Definition:** Teach explicit error values as a normal programming path.

**Complete Means:**

- Explains `ok` and `fail`.
- Teaches `ok?`, `error?`, `value`, `error`, `unwrap_or`.
- Covers `map_result`, `and_then`, `result_envelope`.
- Shows that `Result` values are not conditions.
- Demonstrates API boundary maps with `result_envelope`.

**Guided Example:** `examples/learn/09-results-and-errors/config-loader.rco`.

**Words Introduced:** all `result` detail words from `rco words --json`.

### Chapter 10: Control Flow, Functions, And Blocks

**Definition:** Teach program structure.

**Complete Means:**

- Covers `if`, `else`, `end`, `while`, `break`, `continue`, `return`.
- Covers first-class blocks and `call`.
- Covers `function` declarations with and without args metadata.
- Shows readable factoring in postfix style.
- Explains lexical/static lookup enough for later package and macro chapters.

**Guided Example:** `examples/learn/10-control-flow/gradebook.rco`.

**Words Introduced:** control words other than async task words, which get primary teaching in Chapter 15.

### Chapter 11: OOP And Dispatch

**Definition:** Teach Ricochet dynamic OOP with postfix declarations.

**Complete Means:**

- Covers `Subclass`, `Field`, `Accessor`, `Table`, `Method`, `new`, `self`, `send`.
- Teaches generated accessor selectors such as `email.get` and `email.set`.
- Shows inheritance and method override in a small example.
- Explains how MVC model declarations reuse the same vocabulary.

**Guided Example:** `examples/learn/11-oop/contact-book.rco`.

**Words Introduced:** all `oop` detail words from `rco words --json`.

### Chapter 12: Testing, Linting, And Formatting

**Definition:** Teach the feedback loop before advanced effects.

**Complete Means:**

- Covers `rco test`, assertions, fixture style if available, `rco lint`, and `rco fmt`.
- Shows how to structure small examples with expected outputs.
- Explains reference validation and word inventory checks at a high level for contributors.

**Guided Example:** `examples/learn/12-testing-linting-and-formatting/math_words_test.rco`.

**Primary Coverage:** test command, lint command, fmt command, assertion helpers, `@ricochet/test_helpers` overview.

### Chapter 13: Introspection And Debug Basics

**Definition:** Teach readers how to inspect a running program before full debugger tooling.

**Complete Means:**

- Covers `inspect`, type/class/method inspection, task metadata basics, and `debug`.
- Shows stack, locals, globals, and self inspection in the terminal debugger.
- Introduces trace files and JSON debug output without covering DAP yet.

**Guided Example:** `examples/learn/13-introspection-and-debug-basics/debug-tour.rco`.

**Words Introduced:** all `inspect` detail words from `rco words --json`.

### Chapter 14: Date, Time, And Duration

**Definition:** Teach UTC timestamps, RFC3339 strings, dates, parts maps, and durations.

**Complete Means:**

- Explains timestamp millisecond boundaries.
- Covers parse/format/build/parts words.
- Covers date conversion and date arithmetic.
- Covers duration unit words and `duration_parts`.
- Shows result-based failure handling for invalid dates.

**Guided Example:** `examples/learn/14-date-time-and-duration/reminder.rco`.

**Primary Coverage:** date/time/duration `system` words.

### Chapter 15: Async And Tasks

**Definition:** Teach first-class tasks.

**Complete Means:**

- Covers `[ ... ] spawn`, `await`, `await_all`, retained completed/failed handles, reawait behavior, and `release_task`.
- Covers `tasks`, `id`, `info`, `task_status`, `pending?`, `running?`, `completed?`, `failed?`.
- Includes a failed task example and cleanup example.

**Guided Example:** `examples/learn/15-async/parallel-checks.rco`.

**Words Introduced:** async `control` words and task `inspect` words.

### Chapter 16: Capabilities And Sandboxing

**Definition:** Teach Ricochet's host power model before teaching host effects.

**Complete Means:**

- Explains trusted vs sandboxed profiles.
- Covers capability CLI flags and `runtime_capabilities`.
- Explains allowlists for filesystem, HTTP, sockets, environment, process, PTY, TUI, and webview.
- Teaches that examples requiring host powers show the exact flags.

**Guided Example:** `examples/learn/16-capabilities-and-sandboxing/capability-report.rco`.

**Primary Coverage:** capability values, `runtime_capabilities`, `--allow-*` flags, safety posture.

### Chapter 17: Files, Workspaces, Environment, Config, And Secrets

**Definition:** Teach safe local data access.

**Complete Means:**

- Covers filesystem and workspace read/write/list/create/copy/move/delete words.
- Puts destructive delete examples behind explicit warnings and non-destructive dry examples.
- Covers `workspace_resolve`, `workspace_contains?`, and metadata.
- Covers env get/set, secret references, secret resolution, and nested config lookup.
- Shows a real config-loading pattern with `Result`.

**Guided Example:** `examples/learn/17-files-workspaces-env-and-secrets/settings-loader.rco`.

**Primary Coverage:** filesystem, workspace, env, config, and secret `system` words.

### Chapter 18: HTTP And Streams

**Definition:** Teach outbound HTTP and retained response streams.

**Complete Means:**

- Covers simple GET/POST JSON and structured request maps.
- Covers bearer/JSON/timeout helpers.
- Covers task-returning HTTP requests.
- Covers retained stream reads with offsets, metadata, cancel, release, and bounded reads.
- Explains redirects are disabled.

**Guided Example:** `examples/learn/18-http-streams/api-client.rco`.

**Primary Coverage:** HTTP and HTTP stream `system` words.

### Chapter 19: TCP And WebSocket Sockets

**Definition:** Teach raw TCP and WebSocket clients and listeners as powerful retained resources.

**Complete Means:**

- Covers connect/listen/accept/read/write-or-send/close/release.
- Shows a loopback TCP echo server and client.
- Shows a loopback WebSocket echo server and client.
- Explains `--allow-sockets` and `--socket-allow-host`.
- Teaches cleanup and error paths before any broader network examples.

**Guided Example:** `examples/learn/19-sockets/tcp_echo.rco` and `examples/learn/19-sockets/ws_echo.rco`.

**Primary Coverage:** TCP and WebSocket socket `system` words.

### Chapter 20: Processes And PTYs

**Definition:** Teach child process and terminal session integration.

**Complete Means:**

- Covers blocking process spawn, task spawn, retained process jobs, reads, cancellation, release, and env option maps.
- Covers retained PTY sessions, write/read/resize/stop/release, list, and detail.
- Explains platform differences and security boundaries.
- Uses harmless commands in examples.

**Guided Example:** `examples/learn/20-processes-and-ptys/tool-runner.rco`.

**Primary Coverage:** process and PTY `system` words.

### Chapter 21: Terminal UI

**Definition:** Teach terminal UI words and packaging as a terminal app.

**Complete Means:**

- Covers alternate screen, cursor movement, writes, flush, size, key polling, and key reading.
- Builds a small interactive dashboard.
- Shows `rco package --tui`.
- Explains how TUI capability flags apply.

**Guided Example:** `examples/learn/21-tui/task-dashboard.rco`.

**Primary Coverage:** TUI `system` words, `rco package --tui`.

### Chapter 22: Webview And Desktop GUI

**Definition:** Teach local desktop GUI documents and state/action flows.

**Complete Means:**

- Covers escaped GUI document fragments and state/action document generation.
- Builds a small local GUI using `rco gui`.
- Shows `rco package --gui`.
- Explains when to use GUI, TUI, or MVC.

**Guided Example:** `examples/learn/22-gui/notes_gui.rco`.

**Primary Coverage:** webview `system` words, `rco gui`, `rco package --gui`.

### Chapter 23: MVC First App

**Definition:** Scaffold and run the first web app.

**Complete Means:**

- Covers `rco new`, `rco new --with-sqlite`, `rco serve`, `rco serve --watch`, `rco routes`.
- Explains generated project layout.
- Shows how capabilities are narrower in MVC than broad trusted scripts.
- Keeps the first app focused on a single page and one route.

**Guided Example:** Generated from `examples/learn/23-mvc/first_app` instructions.

**Primary Coverage:** project and MVC command family.

### Chapter 24: Routes, Controllers, And Responses

**Definition:** Teach MVC routing and controller response words.

**Complete Means:**

- Covers `METHOD "path" Controller "action" route`.
- Covers route params and action arg binding order.
- Covers request context: method, path, params, query, form, JSON/body, uploads, files, headers, cookies, session, config, logs, and capabilities.
- Covers `view`, `text`, `json`, `redirect`, `status`, and `header`.

**Guided Example:** `examples/learn/23-mvc/controllers`.

**Primary Coverage:** route verbs and controller response `web` words.

### Chapter 25: Templates, Static Assets, And Uploads

**Definition:** Teach rendering and request body handling.

**Complete Means:**

- Covers scalar interpolation with HTML escaping.
- Covers template script blocks, conditionals, and loops.
- Covers static asset configuration and traversal safety at a high level.
- Covers upload maps and retained temporary upload streams.
- Includes large upload stream read/release example.

**Guided Example:** `examples/learn/23-mvc/templates_uploads`.

**Primary Coverage:** template syntax, static assets, upload stream words.

### Chapter 26: Data, Active Record, And Migrations

**Definition:** Teach database-backed apps.

**Complete Means:**

- Covers `[database.default]` for SQLite first, then PostgreSQL/MySQL/MariaDB notes.
- Covers `Table`, `Accessor`, `all`, `find_record`, page/query words, count/existence, insert, and update.
- Covers `rco migrate new/status/apply/rollback/dump`.
- Covers SQL migrations, Ricochet DSL migrations, schema dumps, and seeds.
- Explains Active Record is not a full schema-definition ORM.

**Guided Example:** `examples/learn/26-data/contacts_app`.

**Primary Coverage:** Active Record `web` words and migration CLI family.

### Chapter 27: Sessions, Forms, Auth, And Passwords

**Definition:** Teach the production auth-adjacent surfaces carefully.

**Complete Means:**

- Covers signed sessions and encrypted v2 cookie option at a user level.
- Covers `@ricochet/forms` field maps, validation maps, schema validation, and multipart file maps.
- Covers `@ricochet/auth` session guards, CSRF helpers, credential normalization, password policy validation, and Argon2id hash/verify wrappers.
- Clearly states scaffold login is local beta scaffolding, not a complete production auth framework.

**Guided Example:** `examples/learn/27-auth-forms/login_flow`.

**Primary Coverage:** sessions, auth package, forms package, password policy words.

### Chapter 28: AI Capabilities And The AI Package

**Definition:** Teach Ricochet AI integration without promising a single provider.

**Complete Means:**

- Covers MVC `ai` capability shape and `Result` return maps.
- Covers `@ricochet/ai` provider/message/request/response/error contracts.
- Covers retry helpers, tool call/result maps, schema validation, OpenAI-compatible, Anthropic-compatible, local/Ollama helpers, SSE/NDJSON parsing, retained stream consumers, and fake-provider tests.
- Includes an offline fake provider example before network provider examples.

**Guided Example:** `examples/learn/28-ai/fake_provider_chat`.

**Primary Coverage:** MVC AI capability, `@ricochet/ai`, retained stream AI patterns.

### Chapter 29: Packages, Imports, And Dependencies

**Definition:** Teach local packages and dependency use.

**Complete Means:**

- Covers `rco add`, `rco install`, `rco verify`, `rco audit`, `ricochet.toml`, and `ricochet.lock`.
- Covers path, GitHub, local registry, and static registry dependency shapes.
- Covers static imports shaped like `package/module`.
- Covers `import_dynamic`, `module_call`, and `module_get`.
- Explains package lock integrity and path containment.

**Guided Example:** `examples/learn/29-packages/local_math_package`.

**Primary Coverage:** dependency CLI family, static imports, dynamic runtime imports.

### Chapter 30: Registries, Publish, Yank, And Mirror

**Definition:** Teach package publishing workflows.

**Complete Means:**

- Covers static registry publish/rebuild/check/search.
- Covers provenance, signatures, `sha256:` archive verification, semver, aliases, scoped names, yanks, and same-version replacement protection.
- Covers hosted registry discovery/search/fetch/publish/yank and bearer token secret handling.
- Covers `rco registry serve` and `rco registry mirror`.
- Includes a local hosted registry reference-server walkthrough.

**Guided Example:** `examples/learn/30-registries/local_registry_lab`.

**Primary Coverage:** registry CLI family and hosted registry protocol guide links.

### Chapter 31: Macros And Expansion

**Definition:** Teach compile-time expression and item-row macros after readers know normal Ricochet.

**Complete Means:**

- Covers string-named `"name" Macro` declarations.
- Covers explicit `"name" macro_call`.
- Covers `quote_ast`, `ast_splice`, and `quote_items`.
- Covers expression-item rows, class-body rows, function rows, and subclass rows.
- Covers local/static-import/package macro lookup.
- Covers `rco expand`, source maps, cache metadata, and canonical package macro IDs.
- Includes macro stabilization limits.

**Guided Example:** `examples/learn/31-macros/route_macro`.

**Primary Coverage:** macro language core and `rco expand`.

### Chapter 32: Debugger, DAP, LSP, And Editor Tools

**Definition:** Teach the professional tooling layer.

**Complete Means:**

- Covers terminal debugger commands: `step`, `next`, `out`, `continue`, `abort`, stack/locals/globals/self/tasks views.
- Covers `rco run --trace-file`, `rco debug --json`, `rco debug-adapter`, `rco debug-tui`, and `rco debug-web`.
- Covers MVC `rco serve --debug` request-fault pause reporting.
- Covers VS Code extension, DAP, stack panel, LSP diagnostics/completion/hover/definition/symbols/tokens/formatting/quick fixes/rename.
- Covers `rco lsp`, `rco lsp-diagnostics`, and `rco words --check`.

**Guided Example:** `examples/learn/32-debugger-editor/debuggable_app.rco`.

**Primary Coverage:** debugger/editor command families and docs.

### Chapter 33: Bytecode, Images, And Source Emission

**Definition:** Teach persistent runtime state and compiled artifacts.

**Complete Means:**

- Covers `rco build`, `rco run-bytecode`, and `.rcob`.
- Covers `rco repl --image`, `:save`, `:load`, and `:bindings`.
- Covers `rco image` inspection where available.
- Covers `rco emit-source` and explains it is readable source-like output, not byte-for-byte reconstruction.

**Guided Example:** `examples/learn/33-bytecode-images-and-source-emission/image_lab.rco`.

**Primary Coverage:** runtime command family for bytecode, images, source emission.

### Chapter 34: Packaging, Release, And Updates

**Definition:** Teach application packaging and release artifacts at user level.

**Complete Means:**

- Covers `rco package`, `--tui`, `--gui`, `--gui --mvc`, and Linux `--linux-package tar|deb`.
- Explains standalone launcher embedding for bytecode and MVC bundles.
- Covers release artifacts, checksums, manifests, signing status, and update channel documents as consumer/operator concepts.
- Links to store packaging and updater workflow guides.

**Guided Example:** package the Chapter 21 TUI and Chapter 22 GUI examples.

**Primary Coverage:** packaging CLI family, release docs, updater docs.

### Chapter 35: Capstone CLI Tool

**Definition:** Build a complete command-line utility.

**Complete Means:**

- Uses strings, collections, results, files/workspace, config, tests, linting, and packaging.
- Has exercises for adding one feature and one test.
- Ends with a cleanup-free validation command.

**Guided Example:** `examples/learn/35-capstone-cli/worklog`.

### Chapter 36: Capstone TUI Dashboard

**Definition:** Build a terminal UI app.

**Complete Means:**

- Uses TUI words, async tasks, HTTP or local file data, key handling, and packaging.
- Teaches graceful exit and terminal restoration.

**Guided Example:** `examples/learn/36-capstone-tui/service_dashboard`.

### Chapter 37: Capstone MVC App

**Definition:** Build a database-backed web app.

**Complete Means:**

- Uses routes, controllers, templates, static assets, uploads, sessions, forms, auth helpers, migrations, seeds, and tests.
- Uses SQLite locally with notes for PostgreSQL/MySQL deployment.
- Includes `rco serve --watch`, `rco routes`, and `rco serve --debug`.

**Guided Example:** `examples/learn/37-capstone-mvc/project_journal`.

### Chapter 38: Capstone Packaged GUI App

**Definition:** Build and package a desktop GUI app backed by local MVC or state/action documents.

**Complete Means:**

- Uses webview GUI words or `rco package --gui --mvc`.
- Uses local data, settings, result handling, and release packaging metadata.
- Ends with platform-specific packaging commands and validation notes.

**Guided Example:** `examples/learn/38-capstone-gui/personal_ledger`.

---

## Appendices

### Appendix A: Word Catalog

**Complete Means:**

- Generated or validated from `rco words --json`.
- Groups the current 346 words by `detail`: collection, control, data, inspect, math, oop, result, stack, string, system, and web.
- For every word, lists primary teaching chapter, stack signature summary, and reference anchor.
- Fails validation if live inventory adds, removes, or renames a word without a coverage update.

### Appendix B: CLI Command Catalog

**Complete Means:**

- Covers project/MVC, runtime, packaging, dependencies, registries, editor/diagnostics, docs/quality, and release-adjacent commands.
- Includes one-line purpose, first command to try, and link to teaching chapter.

### Appendix C: Capability Flags

**Complete Means:**

- Lists trusted/sandboxed behavior and all relevant `--allow-*` flags.
- Includes examples for HTTP, sockets, filesystem, workspace, process, PTY, TUI, webview, env, and secrets.

### Appendix D: Syntax Guardrails

**Complete Means:**

- Shows accepted postfix shapes.
- Shows rejected legacy or tempting shapes with correction examples.
- Covers `_` word naming, `-` subtraction, `$name`, dynamic `get`, selectors, blocks, and OOP declarations.

### Appendix E: Troubleshooting

**Complete Means:**

- Covers command-not-found, stale installed `rco`, parse errors, stack underflow, type errors, missing capabilities, socket allowlist failures, MVC config mistakes, database connection errors, package lock mismatch, and docs validation failures.

### Appendix F: Glossary

**Complete Means:**

- Defines stack, word, selector, block, result, capability, retained resource, MVC, controller, template directive, package, registry, static import, dynamic import, macro, bytecode, image, DAP, LSP, TUI, GUI, and update channel.

---

## Implementation Tasks

### Task 1: Establish Manual Skeleton

- [ ] Create `docs/learn/index.md` with the manual title, audience, reading path, and links to all chapters and appendices.
- [ ] Create `docs/learn/manual-map.md` with part/chapter organization and status fields.
- [ ] Create empty chapter files under `docs/learn/chapters`.
- [ ] Create empty appendix files under `docs/learn/appendices`.
- [ ] Add `docs/reference/learn/index.html` using the existing reference topbar, styles, and footer.
- [ ] Add a "Learn Ricochet" link to `docs/reference/index.html` and `docs/reference/guides/index.html`.
- [ ] Add a "Learn Ricochet" entry to `docs/wiki/README.md`.

**Verification:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
rtk git diff --check
```

### Task 2: Add Coverage Data And Validation

- [ ] Generate initial word inventory with `rtk cargo run -q -p ricochet_cli --bin rco -- words --json`.
- [ ] Create `docs/learn/word-coverage.json` mapping each live word to `detail`, `primary_chapter`, and `status`.
- [ ] Create `scripts/validate-learn-manual.ps1`.
- [ ] Make the script fail when a live word is absent from `word-coverage.json`.
- [ ] Make the script fail when `word-coverage.json` contains a stale word.
- [ ] Make the script fail when a chapter listed in coverage does not exist.
- [ ] Make the script fail on placeholder markers such as `TODO`, `TBD`, or `FIXME` in completed chapters.
- [ ] Make the script run every `.rco` example that is marked runnable in an examples manifest.
- [ ] Document intentionally non-runnable examples in a manifest field rather than skipping them silently.

**Verification:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-learn-manual.ps1
rtk cargo run -q -p ricochet_cli --bin rco -- words --check --docs-app docs/reference/app.js --grammar editors/vscode/syntaxes/ricochet.tmLanguage.json
```

### Task 3: Decide And Implement HTML Rendering

- [ ] Inspect the existing reference HTML guide style in `docs/reference/guides`.
- [ ] Implement `scripts/render-learn-manual.ps1` or a small Rust docs helper that renders the Markdown chapter subset into `docs/reference/learn/*.html`.
- [ ] Reuse `docs/reference/styles.css`; add only small manual-specific classes when needed.
- [ ] Generate a manual table of contents, previous/next links, and chapter anchors.
- [ ] Ensure code blocks and shell command blocks are styled consistently with the reference.
- [ ] Ensure generated HTML never links to local machine paths.

**Verification:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\render-learn-manual.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
```

### Task 4: Build The Runnable Example Harness

- [ ] Create `examples/learn/README.md` explaining how examples are organized.
- [ ] Add an example manifest, such as `examples/learn/examples.json`, with chapter, path, command, expected status, and capability flags.
- [ ] Create the first simple runnable examples for Chapters 01, 03, 06, 08, 09, 10, and 11.
- [ ] Keep network, socket, process, PTY, GUI, TUI, MVC, package, registry, and release examples either loopback/local or explicitly marked as manual-run examples.
- [ ] Wire runnable examples into `scripts/validate-learn-manual.ps1`.

**Verification:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-learn-manual.ps1
```

### Task 5: Draft Part I

- [ ] Write Chapter 00.
- [ ] Write Chapter 01.
- [ ] Write Chapter 02.
- [ ] Write Chapter 03.
- [ ] Add all Part I examples.
- [ ] Assign Part I words in `docs/learn/word-coverage.json`.
- [ ] Render HTML.
- [ ] Validate examples and reference docs.

**Verification:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-learn-manual.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
```

### Task 6: Draft Part II Core Words

- [ ] Write Chapter 04.
- [ ] Write Chapter 05.
- [ ] Write Chapter 06.
- [ ] Write Chapter 07.
- [ ] Write Chapter 08.
- [ ] Write Chapter 09.
- [ ] Write Chapter 10.
- [ ] Write Chapter 11.
- [ ] Write Chapter 12.
- [ ] Write Chapter 13.
- [ ] Add examples and exercises for each chapter.
- [ ] Assign data, math, string, collection, result, control, OOP, inspect, and relevant test/debug words.

**Verification:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-learn-manual.ps1
rtk cargo test --workspace
```

### Task 7: Draft Part III Host Capabilities

- [ ] Write Chapter 14.
- [ ] Write Chapter 15.
- [ ] Write Chapter 16.
- [ ] Write Chapter 17.
- [ ] Write Chapter 18.
- [ ] Write Chapter 19.
- [ ] Write Chapter 20.
- [ ] Write Chapter 21.
- [ ] Write Chapter 22.
- [ ] Add local-only examples with explicit capability commands.
- [ ] Ensure retained resources are released in every example.
- [ ] Assign system words for date/time, tasks, capabilities, filesystem, workspace, env, config, secrets, HTTP, streams, TCP, WebSocket, process, PTY, TUI, and webview.

**Verification:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-learn-manual.ps1
rtk cargo test --workspace
```

### Task 8: Draft Part IV MVC, Data, Auth, Forms, And AI

- [ ] Write Chapter 23.
- [ ] Write Chapter 24.
- [ ] Write Chapter 25.
- [ ] Write Chapter 26.
- [ ] Write Chapter 27.
- [ ] Write Chapter 28.
- [ ] Add or adapt examples from `examples/showcase/sqlite_notes`, `examples/showcase/package_auth_forms`, and `examples/showcase/ai_provider_probe`.
- [ ] Keep examples beginner-readable; do not copy showcase complexity blindly.
- [ ] Assign all web words and MVC/data/auth/forms/AI feature coverage.

**Verification:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-learn-manual.ps1
rtk cargo test -p ricochet_web
rtk cargo test -p ricochet_cli --test cli_smoke
```

### Task 9: Draft Part V Toolchain And Distribution

- [ ] Write Chapter 29.
- [ ] Write Chapter 30.
- [ ] Write Chapter 31.
- [ ] Write Chapter 32.
- [ ] Write Chapter 33.
- [ ] Write Chapter 34.
- [ ] Add examples or labs for package imports, local registries, macros, debugger, images, bytecode, and packaging.
- [ ] Link existing wiki guides for deeper protocol and release details.
- [ ] Assign package, registry, macro, debugger, editor, bytecode, image, source emission, packaging, release, and updater coverage.

**Verification:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-learn-manual.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-editor-assets.ps1
```

### Task 10: Build Capstones

- [ ] Write Chapter 35 and its CLI tool.
- [ ] Write Chapter 36 and its TUI dashboard.
- [ ] Write Chapter 37 and its MVC app.
- [ ] Write Chapter 38 and its packaged GUI app.
- [ ] Ensure each capstone points back to the chapters it uses.
- [ ] Keep capstone dependencies local or fake-provider based unless a chapter explicitly requires network credentials.
- [ ] Add capstone validation commands to the example manifest.

**Verification:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-learn-manual.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1
```

### Task 11: Build Appendices

- [ ] Generate or write Appendix A from `docs/learn/word-coverage.json`.
- [ ] Write Appendix B from `rco --help` and command-specific help output.
- [ ] Write Appendix C from current CLI capability flags and `docs/feature-map.md`.
- [ ] Write Appendix D from `AGENTS.md` syntax guardrails and reference syntax.
- [ ] Write Appendix E from common failure cases found in tests and docs.
- [ ] Write Appendix F after all chapters have stabilized.

**Verification:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-learn-manual.ps1
```

### Task 12: Final Site Integration

- [ ] Render all manual HTML pages to `docs/reference/learn`.
- [ ] Add manual links to the reference homepage, guide index, and wiki README.
- [ ] Confirm mobile and desktop layout for the manual landing page and a long chapter.
- [ ] Ensure the GitHub Pages root still resolves through `docs/index.html` to the reference.
- [ ] Ensure no manual page depends on external assets.

**Verification:**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\render-learn-manual.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
rtk git diff --check
```

### Task 13: Full Documentation Release Gate

- [ ] Run formatting and Rust checks if any code/helper changed.
- [ ] Run workspace tests if examples or docs helpers touch runtime behavior.
- [ ] Run learn manual validation.
- [ ] Run reference validation.
- [ ] Run editor validation.
- [ ] Run word inventory check.
- [ ] Run acceptance if examples or packaging commands changed.
- [ ] Update project memory with what the manual covers, validation results, and any stale assumptions discovered.

**Verification:**

```powershell
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo test --workspace
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-learn-manual.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-editor-assets.ps1
rtk cargo run -q -p ricochet_cli --bin rco -- words --check --docs-app docs/reference/app.js --grammar editors/vscode/syntaxes/ricochet.tmLanguage.json
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1
```

---

## Coverage Rules

The first implementation pass should generate `docs/learn/word-coverage.json` with entries shaped like:

```json
{
  "word": "swap",
  "detail": "stack",
  "primary_chapter": "03-postfix-stack-thinking",
  "status": "planned"
}
```

Allowed `status` values:

- `planned`: chapter has not been drafted.
- `drafted`: chapter explains the word but examples are not fully validated.
- `validated`: chapter explanation and examples are validated.
- `appendix`: word is primarily lookup-only and intentionally covered in Appendix A.

Validation must fail when:

- A live word has no coverage row.
- A coverage row points at a missing chapter or appendix.
- A word appears more than once as `validated` primary coverage.
- A completed chapter introduces a word not listed in its "Words Introduced" section.
- A chapter's runnable example command fails.

Initial live inventory observed during plan creation:

| Detail | Count |
| --- | ---: |
| collection | 28 |
| control | 14 |
| data | 10 |
| inspect | 17 |
| math | 52 |
| oop | 9 |
| result | 12 |
| stack | 11 |
| string | 26 |
| system | 144 |
| web | 23 |
| **Total** | **346** |

These counts are not a contract. The validation script must use the live workspace inventory.

---

## Self-Review Checklist

- [ ] The manual starts with Hello World and gradually increases complexity.
- [ ] Every word category is taught before capstone use.
- [ ] Every high-risk host feature includes safety notes and cleanup.
- [ ] Every chapter has a guided example or a clear reason it does not need one.
- [ ] Every example has a validation command.
- [ ] The HTML version is served under `docs/reference/learn`.
- [ ] Reference navigation links to the manual without burying the existing reference.
- [ ] The manual does not claim unsupported roadmap work.
- [ ] The manual avoids internal or agent-facing language.
- [ ] The final validation gate is documented and repeatable.

---

## Execution Handoff

Recommended implementation mode: use subagents for independent drafting after Tasks 1 through 4 are complete. Keep one coordinator responsible for `word-coverage.json`, example validation, HTML rendering, and tone consistency.

Suggested subagent split:

- Agent A: Part I and Part II language basics.
- Agent B: Part III host capabilities, TUI, GUI, sockets, process, PTY.
- Agent C: Part IV MVC, data, auth, forms, AI.
- Agent D: Part V packages, registries, macros, tooling, packaging.
- Agent E: capstones, appendices, validation, and HTML integration.

Do not parallelize final edits to `docs/learn/word-coverage.json` without a coordinator, because it is the single coverage ledger.
