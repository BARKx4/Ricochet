# Roadmap Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Ricochet from its current v1 developer-beta surface to a roadmap-complete language, web framework, package ecosystem, debugger suite, AI package layer, and production distribution pipeline.

**Architecture:** Complete the roadmap as independent, reviewable epics that preserve the current postfix/RPN language model and existing beta surfaces. Each epic must land behind tests, reference docs, editor inventory updates, and feature-map updates before the next dependent epic builds on it.

**Tech Stack:** Rust workspace crates (`ricochet_syntax`, `ricochet_compiler`, `ricochet_vm`, `ricochet_web`, `ricochet_cli`), Ricochet first-party packages, static reference docs, VS Code extension assets, GitHub Actions release workflows, and PowerShell/Bash release scripts.

## Global Constraints

- Public language syntax must remain postfix/RPN.
- Public multiword Ricochet words use `_` separators.
- Do not introduce leading-dot source syntax or receiver-first pseudo-object host calls.
- Follow `docs/adding-words.md` for every public word, command, or grammar addition.
- Treat `docs/feature-map.md`, live code, tests, and `docs/reference` as the roadmap source of truth.
- Preserve the data-safety policy: ask before deleting anything.
- Every epic must end with updated docs, tests, `docs/feature-map.md`, and a memory checkpoint.
- Every release-facing epic must pass `cargo fmt`, clippy, workspace tests, docs validation, editor validation, word inventory check, audit, and acceptance.
- Stabilize and commit the current credential/password-policy work before starting any later epic.

---

## Current Position

Ricochet already has a broad v1 beta surface: bytecode VM, dynamic OOP, MVC apps, SQLite/PostgreSQL/MySQL support, migrations apply/status, signed/encrypted sessions, first-party auth/forms/AI/test packages, package add/install/verify/audit/publish/static registries, retained process/PTY/HTTP/socket resources, debugger/DAP/VS Code surfaces, native packaging, release scripts, docs validation, and acceptance tests.

The current working tree includes uncommitted credential/password-policy implementation work:

- Core `password_hash` and `password_verify` words using Argon2id.
- `@ricochet/auth` credential normalization, password policy, hash/verify, and credential verification helpers.
- Docs, LSP, grammar, tests, and feature-map updates for that work.

Treat that work as Epic 0. Do not begin the remaining roadmap until it is committed and CI is green.

## Roadmap Dependency Order

1. Stabilization and release hygiene.
2. Web/data foundations: streaming uploads, migrations/seeds/schema, templates.
3. Runtime/language maturity: numeric precision, macros, REPL images, source emission.
4. Packages and loading: dynamic runtime imports, central hosted registry.
5. Debugger/UI polish: suspended-task views, request-fault pause, TUI/browser debugger UI.
6. AI package/platform maturity: richer providers, true streaming, schema validation.
7. Production distribution: signing, notarization, metadata, store packaging, updater.

This order keeps high-risk runtime grammar changes away from distribution work and lets web/data features stabilize before debugger and AI tooling depend on them.

---

## Epic 0: Stabilize Current Credential/Password Policy Work

**Feature Definition:** Turn the current uncommitted credential/password-policy implementation into a clean, reviewed, pushed baseline.

**Complete Means:**

- Current password-policy changes are committed on a topic branch or directly on `main`, depending on release urgency.
- GitHub `CI` and `Push on main` workflows are green.
- `docs/feature-map.md` no longer lists production credential/password policy as remaining work.
- Project memory records the final commit hash and verification results.

**Primary Files:**

- `Cargo.toml`
- `Cargo.lock`
- `crates/ricochet_vm/src/builtins.rs`
- `crates/ricochet_vm/src/vm.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `packages/ricochet_auth/session.rco`
- `packages/ricochet_auth/tests/AuthPackageTest.rco`
- `docs/feature-map.md`
- `docs/reference/app.js`
- `docs/reference/index.html`
- `docs/reference/validate.ps1`
- `editors/vscode/syntaxes/ricochet.tmLanguage.json`

**Implementation Path:**

- [ ] Review the current diff with `rtk git diff --stat` and focused `rtk git diff HEAD -- <path>` commands.
- [ ] Run `rtk cargo fmt --all -- --check`.
- [ ] Run `rtk cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Run `rtk cargo test --workspace`.
- [ ] Run `rtk cargo audit --deny warnings`.
- [ ] Run `rtk powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1`.
- [ ] Run `rtk powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-editor-assets.ps1`.
- [ ] Run `rtk cargo run -p ricochet_cli --bin rco -- words --check --docs-app docs/reference/app.js --grammar editors/vscode/syntaxes/ricochet.tmLanguage.json`.
- [ ] Run `rtk powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1`.
- [ ] Commit with `feat: add production credential policy`.
- [ ] Push and watch GitHub CI to completion.

**Verification Gate:**

All local checks pass, both GitHub workflows pass, and final `git status --short --branch` is clean.

---

## Epic 1: Streaming Upload APIs

**Feature Definition:** Replace the current all-in-memory MVC upload model with bounded streaming upload surfaces that let controllers handle large multipart bodies without loading every file into RAM.

**Complete Means:**

- MVC request parsing supports streamed multipart files with per-file and per-request limits.
- Controllers can access file metadata immediately and consume file content through explicit stream handles.
- Existing in-memory upload behavior remains available for small files and compatibility.
- Upload streams have release/cancel semantics and cannot leak retained resources.
- Acceptance includes a large-file upload smoke test that exceeds the old comfortable in-memory path but stays under configured limits.

**Primary Files:**

- `crates/ricochet_web/src/server.rs`
- `crates/ricochet_web/src/controller.rs`
- `crates/ricochet_web/src/manifest.rs`
- `crates/ricochet_web/tests/web_mvc.rs`
- `crates/ricochet_vm/src/builtins.rs`
- `crates/ricochet_vm/src/vm.rs`
- New runtime module if retained upload streams need shared state, for example `crates/ricochet_vm/src/upload_runtime.rs`
- `docs/reference/app.js`
- `docs/reference/index.html`
- `docs/reference/validate.ps1`
- `docs/wiki/web-and-data.md`
- `docs/reference/guides/web-and-data.html`
- `scripts/acceptance.ps1`

**Public Surface Candidate:**

- `upload_streams -> array`
- `upload_stream id:number -> result(map)`
- `upload_read id:number options:map -> result(map)`
- `upload_release id:number -> result(bool)`
- MVC request upload maps include `stream_id`, `filename`, `content_type`, `size_known`, `size`, and `field`.

**Implementation Path:**

- [ ] Write failing MVC integration tests for a multipart upload whose file is exposed as a stream handle.
- [ ] Add manifest limits under a focused config shape such as `[web.uploads] max_request_bytes`, `max_file_bytes`, `memory_threshold_bytes`, and `max_retained_streams`.
- [ ] Implement temporary-file backed upload storage for streamed files.
- [ ] Add retained upload stream registry with capped reads, release, and cleanup on request completion or explicit release.
- [ ] Expose upload stream words in the VM using `Result` values for user-data failures and VM errors only for stack/type misuse.
- [ ] Preserve current small-upload maps with `text` and `data_base64` for compatibility.
- [ ] Add CLI/MVC smoke coverage for streamed upload read/release behavior.
- [ ] Update docs/reference, wiki, guides, and feature map.
- [ ] Add acceptance coverage that starts `rco serve`, posts multipart data, reads via stream words, and confirms no retained stream remains.
- [ ] Commit as `feat: add streaming upload APIs`.

**Verification Gate:**

Focused `web_mvc` upload tests pass, full workspace tests pass, acceptance passes, and memory/resource release is verified by a test that observes retained stream count returning to zero.

---

## Epic 2: Migration Rollback, Schema Dumps, Seed Command, And Native Migration DSL

**Feature Definition:** Extend the current SQL-only `rco migrate status/apply` workflow into a complete migration lifecycle with reversible migrations, schema export, seed execution, and an optional Ricochet-native migration authoring DSL.

**Complete Means:**

- `rco migrate rollback` rolls back one or more applied migrations safely.
- `rco migrate dump` writes a deterministic schema snapshot for SQLite, PostgreSQL, and MySQL/MariaDB where feasible.
- `rco db seed` or `rco seed` runs ordered seed files idempotently or with explicit non-idempotent warnings.
- Native migration DSL compiles to adapter-specific SQL and can coexist with raw SQL migrations.
- Existing SQL migrations remain supported and unchanged.

**Primary Files:**

- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_web/src/database_capability.rs`
- `crates/ricochet_web/src/manifest.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `crates/ricochet_web/tests/web_mvc.rs`
- New module candidate: `crates/ricochet_web/src/migrations.rs`
- `docs/reference/app.js`
- `docs/wiki/web-and-data.md`
- `docs/reference/guides/web-and-data.html`
- `README.md`

**Command Surface Candidate:**

- `rco migrate rollback [PATH] --steps N`
- `rco migrate dump [PATH] --output db/schema.sql`
- `rco seed [PATH]`
- `rco migrate new NAME --dsl`

**Implementation Path:**

- [ ] Split migration engine logic out of `crates/ricochet_cli/src/lib.rs` into a focused module if current code size makes rollback unsafe to review.
- [ ] Write failing SQLite tests for `apply`, `rollback --steps 1`, and re-apply.
- [ ] Add schema table metadata for migration direction or infer rollback files with a naming convention such as `VERSION_name.up.sql` and `VERSION_name.down.sql`.
- [ ] Implement rollback for SQLite first, then PostgreSQL, then MySQL/MariaDB.
- [ ] Add adapter capability checks that fail loudly when rollback is requested for a migration without a down path.
- [ ] Implement schema dump for SQLite with deterministic object ordering.
- [ ] Implement PostgreSQL/MySQL dump with deterministic table/index extraction or document narrower beta support if full dump fidelity is not yet possible.
- [ ] Add seed command that runs `db/seeds/*.rco` or `db/seeds/*.sql` in deterministic order.
- [ ] Design the Ricochet-native migration DSL with postfix declarations, for example `"users" table_create`, `"email" "text" column`, and explicit adapter SQL generation.
- [ ] Add docs and examples for raw SQL and DSL migration paths.
- [ ] Update `rco doctor` to report migration/seed/schema health.
- [ ] Commit rollout slices separately: rollback, dump, seed, DSL.

**Verification Gate:**

CLI smoke tests cover apply/rollback/dump/seed for SQLite; integration tests cover PostgreSQL and MySQL/MariaDB behind existing test infrastructure or documented fixture gates; acceptance scaffold includes a seed check.

---

## Epic 3: Template Embedded Script Blocks Beyond Interpolation

**Feature Definition:** Expand templates from scalar interpolation only to controlled embedded Ricochet script blocks that can define locals, loop over values, branch, and render repeated escaped output.

**Complete Means:**

- Existing `{ expr }` interpolation remains exactly compatible.
- New block syntax supports loops and conditionals without compromising HTML escaping.
- Template errors include source spans pointing at the template line and embedded Ricochet code.
- Blocks cannot accidentally emit unescaped HTML unless an explicit safe-HTML API is used.
- Docs explain when to use controllers versus template blocks.

**Primary Files:**

- `crates/ricochet_web/src/template.rs`
- `crates/ricochet_web/src/controller.rs`
- `crates/ricochet_web/tests/web_mvc.rs`
- `crates/ricochet_syntax/src/lexer.rs`
- `crates/ricochet_syntax/src/parser.rs`
- `docs/reference/index.html`
- `docs/wiki/web-and-data.md`
- `docs/reference/guides/web-and-data.html`

**Syntax Candidate:**

- Keep `{ expr }` for scalar interpolation.
- Add `{% ... %}` for control/script blocks.
- Add `{{ expr }}` only if needed for compatibility; otherwise avoid introducing a second expression form.
- Require every output expression to use the existing escaping path.

**Implementation Path:**

- [ ] Write template parser tests for literal text, interpolation, block open/close, nested blocks, and malformed blocks.
- [ ] Add a template AST instead of ad hoc string scanning if current parser cannot safely support nesting.
- [ ] Compile embedded blocks into Ricochet bytecode with a controlled output sink.
- [ ] Add output helper inside template runtime, for example `emit` as an internal host function, not necessarily a public VM word.
- [ ] Implement `if` and `while`/collection iteration examples.
- [ ] Preserve the current rule that scalar interpolation must leave exactly one renderable value.
- [ ] Add MVC tests for escaped repeated list rendering and error spans.
- [ ] Add docs showing a small list view and warning against heavy business logic in templates.
- [ ] Commit as `feat: add template script blocks`.

**Verification Gate:**

Existing template tests pass unchanged; new block tests prove escaped output, malformed-block diagnostics, nested block behavior, and no overlap with static assets.

---

## Epic 4: Numeric Precision And Data Type Completion

**Feature Definition:** Finish the remaining numeric/data storage roadmap around exact decimal, money/smallmoney, arbitrary precision integers, and full `u64` unsigned-bigint storage.

**Complete Means:**

- Ricochet can represent exact decimal values without binary float precision loss.
- Database and JSON boundaries preserve exact numeric values according to documented rules.
- SQL `money`/`smallmoney` style values have clear conversion semantics.
- Full `u64` values can round-trip through JSON/session/database paths without coercing to lossy `f64`.
- Numeric display, comparison, truthiness, `type`, `class_of`, templates, and JSON encode/decode are documented and tested.

**Primary Files:**

- `crates/ricochet_vm/src/value.rs`
- `crates/ricochet_vm/src/builtins.rs`
- `crates/ricochet_web/src/server.rs`
- `crates/ricochet_web/src/template.rs`
- `crates/ricochet_web/src/database_capability.rs`
- `crates/ricochet_web/src/active_record.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `crates/ricochet_web/tests/web_mvc.rs`
- `docs/reference/app.js`
- `docs/feature-map.md`

**Public Surface Candidate:**

- `to_decimal`
- `decimal`
- `to_big_integer`
- `to_unsigned_bigint`
- `money`
- `smallmoney`

**Implementation Path:**

- [ ] Choose Rust dependencies for decimal and big integer support after checking license, maintenance, and serde/database fit.
- [ ] Add `Value::Decimal` and possibly `Value::BigInt`/`Value::BigUint`.
- [ ] Define equality/comparison rules across `Number`, `Float`, `Decimal`, and big integer values.
- [ ] Update `json_decode` to preserve large unsigned integers as exact values instead of lossy floats.
- [ ] Update `json_encode`, templates, sessions, database parameter binding, and row conversion.
- [ ] Add conversion words with Result-returning range/precision failures.
- [ ] Add docs and examples for finance-safe values and ID/token-safe unsigned values.
- [ ] Add regression tests for values above `2^53` and above `i64::MAX`.
- [ ] Commit as a staged sequence: value model, JSON/session, database, docs/editor inventory.

**Verification Gate:**

Focused tests prove exact round trips through VM, JSON, session cookies, templates, SQLite, PostgreSQL, and MySQL/MariaDB where adapters support the target type.

---

## Epic 5: Compile-Time Macros

**Feature Definition:** Add a compile-time macro system that lets Ricochet code generate or transform source/AST/bytecode while preserving postfix readability and deterministic builds.

**Complete Means:**

- Macro declarations have RPN-shaped syntax and clear hygiene rules.
- Macros run at compile time, not runtime, and cannot access host capabilities unless explicitly designed as trusted build hooks.
- Errors point to both macro invocation and expansion source.
- LSP can parse macro definitions well enough for diagnostics and formatting.
- The feature is documented as stable enough for package authors.

**Primary Files:**

- `crates/ricochet_syntax/src/lexer.rs`
- `crates/ricochet_syntax/src/parser.rs`
- `crates/ricochet_compiler/src/compiler.rs`
- `crates/ricochet_bytecode/src/chunk.rs`
- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/src/lsp.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `docs/superpowers/specs/2026-06-09-ricochet-design.md`
- `docs/reference/app.js`

**Syntax Candidate:**

```ricochet
"unless" Macro
  ( condition block -> expansion )
  [
    (( produce: condition not if block call end ))
  ]
end
```

The exact syntax must be designed before implementation. It must avoid leading-dot forms and must keep receivers/arguments below declaration operators.

**Implementation Path:**

- [x] Write a dedicated macro design spec before code.
- [x] Decide expansion target: AST expansion before bytecode lowering.
- [x] Add parser support for macro declarations and invocations behind tests.
- [x] Add compiler expansion pass with recursion/depth limits.
- [x] Add hygiene model for local and imported macros, including private
  helper macros expanding in the definition module's scope.
- [x] Add deterministic trace output for debugging.
- [x] Add `rco expand` to show expanded Ricochet source or JSON inspection data.
- [x] Update LSP diagnostics to parse macro declarations and report macro
  declarations without advertising bare macro completions.
- [x] Add `quote_items` item-generation macros for expression-item rows,
  including class-body declaration rows such as `Accessor`, `Field`, `Table`,
  and `Method`.
- [x] Add declaration-item macro output for top-level `function` and
  `Subclass` rows, including class-body declaration rows inside generated
  classes.
- [x] Stabilize the `rco expand --json` schema, source maps, cache hashes, and
  package lockfile canonical module IDs.
- [x] Add public examples and broader package tests.
- [x] Land the macro stabilization follow-up as `test: add macro package examples`.

Epic 5 macro stabilization is now fully covered. Roadmap execution has advanced
through Epics 6 and 7, and Epic 8 now tracks hosted registry work.

**Verification Gate:**

Compiler tests prove deterministic expansion, recursion limits, hygiene, source-map diagnostics, import behavior, and no runtime host capability access.

---

## Epic 6: Persistent REPL Images And Source Emission

**Feature Definition:** Allow interactive sessions and compiled programs to save/load persistent images and emit readable Ricochet source or source-like representations from compiled/runtime state.

**Complete Means:**

- `rco repl` can save an image and later resume bindings/classes/functions.
- Image files are versioned and refuse incompatible runtime formats loudly.
- Source emission can output useful source for bytecode chunks, declarations, or loaded modules.
- Sensitive values such as secrets, session keys, and capability handles are not serialized accidentally.
- Debugger and docs make clear what is and is not preserved.

**Primary Files:**

- `crates/ricochet_vm/src/value.rs`
- `crates/ricochet_vm/src/vm.rs`
- `crates/ricochet_bytecode/src/chunk.rs`
- `crates/ricochet_compiler/src/compiler.rs`
- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `docs/reference/app.js`
- `docs/debugger-integrations.md`

**Command Surface Candidate:**

- `rco repl --image PATH`
- `rco image save PATH`
- `rco image inspect PATH`
- `rco emit-source FILE_OR_BYTECODE`

**Implementation Path:**

- [x] Define a versioned image schema for bytecode chunks, globals, classes, functions, and safe serializable values.
- [x] Mark non-serializable values explicitly: tasks, capabilities, regex internals, process/PTY/socket/upload handles, active approvals, native-only methods, and secret references with literal values.
- [x] Add image serialize/deserialize coverage for ordinary values, classes, functions, and restored REPL sessions; imports do not carry extra runtime image state after compilation.
- [x] Add REPL commands for save/load/list bindings.
- [x] Add source emission for bytecode chunks using stored bytecode/debug metadata as a readable source-like view.
- [x] Add safety tests proving retained host handles fail with clear errors.
- [x] Add docs and examples.
- [ ] Commit as `feat: add persistent repl images`.

**Verification Gate:**

CLI tests save an image, restart a REPL or script from it, call preserved functions/classes, and verify non-serializable host handles fail with clear errors.

---

## Epic 7: Dynamic Runtime Imports

**Feature Definition:** Add runtime import/loading APIs that can load Ricochet modules dynamically from bounded local/package sources without undermining deterministic package locks or path containment.

**Complete Means:**

- Static imports remain preferred and unchanged.
- Dynamic imports can load modules by string/path/package alias at runtime under explicit policy.
- Loaded modules respect package lock integrity and workspace/path containment.
- Runtime import failures return structured `Result` values.
- Hot-reload and MVC request snapshots behave predictably with dynamic imports.

**Primary Files:**

- `crates/ricochet_vm/src/vm.rs`
- `crates/ricochet_compiler/src/compiler.rs`
- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- Package resolver code in `crates/ricochet_cli/src/lib.rs` or a new module if split first.
- `crates/ricochet_web/src/server.rs`
- `docs/reference/app.js`
- `docs/wiki/packages.md`

**Public Surface Candidate:**

- `import_dynamic path:string -> result(map)`
- `module_call module:map name:string args:array -> result(value)`
- `module_get module:map name:string -> result(value)`

**Implementation Path:**

- [x] Split reusable package/module resolution into a library path callable by CLI, VM, and web runtime.
- [x] Add tests proving dynamic import cannot escape workspace/package roots.
- [x] Add lock verification before loading package modules.
- [x] Add VM module registry with explicit loaded-module metadata.
- [x] Define import caching and reload behavior.
- [x] Add MVC tests for dynamic import inside request handlers.
- [x] Add docs that distinguish static imports from dynamic imports.
- [ ] Commit as `feat: add dynamic runtime imports`.

**Verification Gate:**

Tests prove dynamic imports respect lockfile integrity, aliases, local path containment, static registry packages, MVC snapshots, and error contracts.

---

## Epic 8: Central Hosted Package Registry

**Feature Definition:** Move beyond local/static registries by defining and implementing a central hosted registry protocol and operational model.

**Complete Means:**

- Registry API supports package publish, search, metadata fetch, yanking, provenance/signature metadata, immutable version integrity, and alias/scoped package lookup.
- CLI can use hosted registry URLs securely.
- Same-version replacement fails closed.
- Registry metadata is mirrorable and can degrade to static index behavior.
- Authentication and publisher authorization are documented.

**Primary Files:**

- `crates/ricochet_cli/src/lib.rs`
- New registry protocol modules if split from CLI.
- `.github/workflows/*` if publishing release packages changes.
- `packages/README.md`
- `docs/wiki/packages.md`
- `docs/reference/guides/packages.html`
- New registry service repo or `crates/ricochet_registry_server` only if hosting lives in this workspace.

**Protocol Requirements:**

- Version metadata is immutable once published.
- Yanking marks a version unavailable without deleting historical artifacts.
- Package tarballs and package trees keep `sha256:` integrity values.
- Provenance/signature fields remain first-class.
- Client can verify archive integrity before extraction and package integrity after extraction.

**Implementation Path:**

- [x] Write registry protocol spec before code.
- [x] Split CLI static registry code into reusable client/index/metadata modules.
- [x] Add hosted registry client read operations first: search, metadata fetch, install.
- [x] Add publish flow with auth token references through `secret_env`.
- [x] Add server or reference implementation only after protocol/client is stable.
- [x] Add same-version replacement tests against hosted metadata.
- [x] Add mirror command to export hosted registry metadata into static index format.
- [x] Add docs for publisher auth, yanking, provenance, and mirrors.
- [x] Commit in slices: client read, publish, server/reference implementation, docs.

**Verification Gate:**

CLI smoke tests use a local fake hosted registry server, prove install/search/publish/yank, and prove same-version replacement is rejected.

---

## Epic 9: Richer Suspended-Task Debugger Views And MVC Request-Fault Pause

**Feature Definition:** Extend the debugger from current stack/locals/task snapshots to richer suspended task inspection and automatic pause before MVC HTTP 500 responses.

**Complete Means:**

- Debugger can inspect suspended/running task call stacks where safe.
- Task views show parent/child relationships, await points, captured environment summaries, status, result/error, and retained/released state.
- MVC request failures can pause before returning HTTP 500 when debug mode is active.
- DAP and VS Code surfaces expose the new task/request-fault data.

**Primary Files:**

- `crates/ricochet_vm/src/vm.rs`
- `crates/ricochet_bytecode/src/chunk.rs`
- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `crates/ricochet_web/src/server.rs`
- `crates/ricochet_web/tests/web_mvc.rs`
- `editors/vscode/extension.js`
- `docs/debugger-integrations.md`

**Implementation Path:**

- [x] Add VM task snapshot data structures that can report suspended frames without racing worker threads.
- [x] Add debugger JSON event fields for task call stacks and await points.
- [x] Add terminal debugger commands such as `task <id> stack`, `task <id> locals`, and `tasks --tree`.
- [x] Extend DAP custom variables/scopes for task snapshots.
- [x] Add MVC debug flag to pause before error response generation.
- [x] Add tests for task stack inspection, failed task inspection, released task behavior, and MVC request-fault pause.
- [x] Update VS Code stack panel to display task tree and request-fault status.
- [x] Update docs and feature map.
- [x] Commit task snapshot slice as `feat: enrich debugger task snapshots`; land request-fault/editor follow-up as the Epic 9 closure slice.

Epic 9 is now functionally complete in the roadmap branch: task snapshots landed
through terminal, JSON, DAP, and VS Code surfaces, and MVC debug-mode request
faults now report pause events before HTTP 500 responses. Dedicated TUI/browser
debugger UIs are tracked separately in Epic 10.

**Verification Gate:**

CLI debug tests, DAP tests, MVC fault tests, and VS Code asset validation all pass. A manual VS Code smoke should confirm the panel renders task details.

---

## Epic 10: Dedicated TUI And Browser Debugger UI

**Feature Definition:** Build first-class debugger interfaces beyond terminal commands, DAP, and VS Code: a TUI debugger and a browser debugger UI.

**Complete Means:**

- `rco debug-tui` opens an interactive terminal UI for stack, source, locals, globals, tasks, breakpoints, and stepping.
- `rco debug-web` or equivalent serves a local browser UI with the same debugging surface.
- Both UIs use the same debugger protocol/events as CLI/DAP.
- UIs are responsive, keyboard-friendly, and safe for local-only use by default.

**Primary Files:**

- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `editors/vscode/extension.js` only if shared protocol helpers move.
- New TUI module candidate: `crates/ricochet_cli/src/debug_tui.rs`
- New browser module/static assets candidate: `crates/ricochet_cli/src/debug_web.rs` plus `crates/ricochet_cli/assets/debug_web/*`
- `docs/debugger-integrations.md`

**Implementation Path:**

- [x] Extract debugger session protocol into a reusable internal module.
- [x] Build a read-only TUI prototype that attaches to a debug session and renders current source, stack, locals, globals, and tasks.
- [x] Add stepping controls to the TUI command loop.
- [x] Add runtime breakpoint editing controls to the TUI.
- [x] Build browser UI server bound to `127.0.0.1` by default.
- [x] Add websocket or SSE event stream for browser UI updates.
- [x] Add smoke tests for snapshot rendering and loopback bind safety.
- [x] Add manual verification checklist for keyboard navigation and source display.
- [x] Document screenshot deferral until stable assets exist.
- [x] Commit TUI and browser UI as separate slices.

**Verification Gate:**

Automated smoke tests cover startup and protocol behavior; manual verification confirms stepping, breakpoints, variable inspection, and task view in both UIs.

**Progress Note (2026-06-21):** First preview slice adds `rco debug-tui --smoke`
and `rco debug-web --smoke` first-pause snapshot renderers. `rco debug-web`
can serve that read-only snapshot from a loopback-only local server. At the
time, runtime breakpoint editing and protocol extraction had not yet landed.

**Progress Note (2026-06-21):** The TUI control slice adds non-smoke
`rco debug-tui` sessions that render a text snapshot at each pause and accept
`step`, `next`, `out`, `continue`, or `abort` from stdin, plus repeatable
`--command ACTION` flags for deterministic scripted sessions. At the time,
protocol extraction had not yet landed, and richer full-screen layout plus
browser event/control streaming were still ahead.

**Progress Note (2026-06-21):** The browser live-control slice upgrades
non-smoke `rco debug-web` from a static snapshot server to a loopback-only
browser debugger shell. `GET /events` streams debugger events over SSE and
replays the latest pause for late subscribers; `POST /control` accepts `step`,
`next`, `out`, `continue`, and `abort` JSON actions with optional `pause_id`
stale-control protection. At the time, shared protocol extraction had not yet
landed, and richer full-screen TUI layout, browser UI polish, and manual
verification checklist were still ahead.

**Progress Note (2026-06-21):** The runtime breakpoint editing slice adds a VM
debug-control hook plus terminal, TUI, and browser controls for adding,
removing, clearing, and listing line breakpoints while paused. Browser control
actions include `breakpoint_add`, `breakpoint_remove`, `breakpoint_clear`, and
`breakpoints`; TUI and terminal sessions accept `break <line>`, `clear <line>`,
`clear_breakpoints`, and `breakpoints`.

**Progress Note (2026-06-21):** The shared protocol extraction slice moves
debugger command parsing, web control request parsing, runtime breakpoint edit
events, debug event JSON serialization, and debug value labels into
`crates/ricochet_cli/src/debug_protocol.rs`. TUI, browser, JSON trace, and
terminal debug surfaces now share that internal module; remaining Epic 10
polish at that point was richer full-screen TUI layout, browser UI polish, and
manual keyboard/source-display verification.

**Progress Note (2026-06-21):** The browser UI polish/manual verification
slice keeps `debug-web` loopback-only and adds grouped live panes for source,
current instruction, stack, locals, globals, `self`, tasks, output, event log,
and runtime breakpoint state, plus keyboard shortcuts for step, next, out,
continue, and abort. `docs/debugger-integrations.md` now carries the manual
Epic 10 checklist for keyboard navigation, source display, stepping,
breakpoints, variable inspection, and task view in both `debug-tui` and
`debug-web`. No screenshots were added because stable assets are still not
available; richer full-screen TUI layout and screenshot-quality visual polish
remain future work.

**Progress Note (2026-06-22):** Epic 10 is complete for the roadmap. The
feature map no longer lists dedicated debugger UI as remaining; screenshot
assets, richer full-screen TUI layout, and screenshot-quality browser styling
are future polish rather than blockers for roadmap closure.

---

## Epic 11: Richer AI Provider Packages, Streaming Integration, And Structured Schema Validation

**Feature Definition:** Mature Ricochet AI support from basic OpenAI-compatible request helpers and bounded SSE parsing into a provider package ecosystem with true streaming integration, retries/tool execution, and structured schema validation.

**Complete Means:**

- `@ricochet/ai` supports provider-neutral request/response contracts.
- Provider packages can implement OpenAI-compatible, Anthropic-compatible, local/Ollama-compatible, and custom HTTP providers without changing core MVC.
- True incremental AI streaming integrates with retained HTTP streams or a dedicated stream abstraction.
- Structured schema validation can validate model output and tool arguments.
- Retry/backoff and provider error normalization are documented and tested.

**Primary Files:**

- `packages/ricochet_ai/openai.rco`
- `packages/ricochet_ai/tests/AiPackageTest.rco`
- `packages/ricochet_ai/README.md`
- `crates/ricochet_vm/src/http_stream_runtime.rs`
- `crates/ricochet_vm/src/builtins.rs`
- `crates/ricochet_web/src/ai_capability.rs`
- `crates/ricochet_web/tests/web_mvc.rs`
- `docs/wiki/language-runtime.md`
- `docs/reference/guides/language-runtime.html`

**Public Surface Candidate:**

- Package words: `ai_provider`, `ai_chat_request`, `ai_chat_stream`, `ai_retry_policy`, `ai_retryable_error?`, `ai_retry_delay_ms`, `ai_execute_with_retry`, `ai_openai_execute_chat`, `ai_openai_stream_events`, `ai_openai_stream_state`, `ai_openai_stream_read_events`, `ai_anthropic_stream_state`, `ai_anthropic_stream_read_events`, `ai_ollama_stream_state`, `ai_ollama_stream_read_events`, `ai_stream_read_options`, `ai_error_result`, `ai_schema`, `ai_validate_schema`, `ai_tool_call`, `ai_tool_result`, `ai_tool_handlers`, `ai_tool_handler_put`, `ai_execute_tool_call`, `ai_execute_tool_calls`.
- Core changes only if current HTTP stream words cannot support incremental consumption ergonomically.

**Implementation Path:**

- [x] Define provider-neutral message, request, response, error, stream-event, and tool-call maps in `@ricochet/ai`.
- [x] Add package-local schema validation helpers in `@ricochet/ai` using the same field/rule shape as `@ricochet/forms`.
- [x] Add retry policy maps and normalized error maps in `@ricochet/ai`.
- [x] Add package-level retry/backoff and tool execution helpers.
- [x] Add true incremental stream consumption ergonomics over
  `http_stream_read`, including bounded offset reads and done events.
- [x] Add package-level fake provider tests for streaming chunks, tool calls, malformed events, and schema failures.
- [x] Add MVC fake-provider integration tests for provider package flows.
- [x] Add provider examples for OpenAI-compatible and local model endpoints.
- [x] Wire the package retry/tool helpers into OpenAI-compatible provider runtime flows at the package executor boundary.
- [x] Add docs that keep secret values in env-backed references.
- [x] Commit package-only changes first, then core stream ergonomics if needed.

**Progress Note (2026-06-21):** The package-only contract/schema slice adds
provider-neutral provider, message, chat request/stream, retry policy, response,
error, stream-event, tool-call, tool-result, and schema validation map helpers
in `@ricochet/ai`. It deliberately does not change core HTTP streaming,
MVC `[ai.default]`, or provider runtime execution.

**Progress Note (2026-06-21):** The core HTTP stream-read ergonomics slice adds
`http_stream_read` `max_bytes`, `from_offset`, `next_offset`,
backward-compatible `offset`, `bytes_len`, and `done` metadata. Provider
runtime execution, fake-provider integration tests, provider examples, and
package-level retry/tool execution were still open at that point.

**Progress Note (2026-06-21):** The AI execution helper slice adds package-level
retry classification, deterministic retry-delay calculation, retry execution,
tool handler registration, single tool-call execution, and ordered multi-tool
execution in `@ricochet/ai`. Provider-runtime wiring beyond package-local
helpers, MVC fake-provider integration tests, and provider examples were still
open at that point.

**Progress Note (2026-06-21):** The OpenAI-compatible package runtime slice adds
`ai_openai_execute_chat`, rich HTTP status/error normalization, OpenAI
tool-call extraction, safe `ai_openai_stream_events`, and package-level
fake-provider tests for retry, tool calls, malformed stream events, and schema
validation failures. MVC fake-provider coverage, provider examples, and richer
provider packages were still open at that point.

**Progress Note (2026-06-21):** The MVC/provider examples slice adds a
controller-level fake-provider integration test that imports `ai/openai` from a
vendored `@ricochet/ai` dependency, retries a fake 503, normalizes a fake
OpenAI-compatible success response, and returns MVC JSON. The showcase now has
offline fake-provider and local OpenAI-compatible request examples. Richer
provider packages and deeper provider-level streaming integration remained open
at that point.

**Progress Note (2026-06-21):** The local/Ollama package helper slice adds
native Ollama `/api/chat` request builders, fake-executor chat normalization,
and NDJSON stream event parsing in `@ricochet/ai`, plus a native Ollama
dry-run showcase. Anthropic-compatible helpers and deeper provider-level
streaming integration remained open at that point.

**Progress Note (2026-06-21):** The Anthropic-compatible package helper slice
adds Messages API request builders with top-level system-message lifting,
`x-api-key` and `anthropic-version` headers, fake-executor chat normalization,
`tool_use` extraction, and Anthropic SSE parsing for text deltas, tool-input
JSON deltas, done events, and stream error events. Provider-level runtime and
streaming integration beyond package executor boundaries remained open after
that slice.

**Progress Note (2026-06-22):** The retained AI stream-state slice adds
provider-specific state helpers and `http_stream_read` chunk consumers for
OpenAI SSE, Anthropic SSE, and Ollama NDJSON. The helpers carry parser buffers
across reads, expose the next HTTP `offset`, keep monotonic `event_offset`
values, flush final unterminated frames when the retained stream reports
`done`, and preserve malformed completed events on the existing failed
`Result` path. Package tests now cover split frames/lines, done markers,
malformed completed events, and Anthropic tool deltas; MVC fake-provider
coverage now imports the package and runs a split retained-stream flow inside a
controller.

**Verification Gate:**

Epic 11 closes when package tests and MVC fake-provider tests cover
non-streaming, streaming, retry, schema validation, and error normalization
without requiring real external API calls.

---

## Epic 12: Production App Distribution Polish

**Feature Definition:** Turn existing native packaging and release artifacts into production-grade distribution with signing, notarization, metadata, store packaging options, and updater workflows.

**Complete Means:**

- Windows artifacts can be Authenticode-signed.
- macOS artifacts can be signed and notarized.
- Linux packages include correct desktop metadata, icons, appstream metadata, and package maintainer fields.
- GUI/MVC packages include static assets and correct platform metadata.
- Updater workflow is documented and optionally implemented for supported channels.
- CI/release workflows can run signing conditionally without exposing secrets.

**Primary Files:**

- `scripts/package-release.ps1`
- `scripts/package-release-linux.sh`
- `scripts/package-release-macos.sh`
- `.github/workflows/release.yml`
- `.github/workflows/ci.yml`
- `crates/ricochet_cli/src/lib.rs`
- `docs/releases/*`
- `README.md`
- `docs/wiki/development-release.md`
- `docs/reference/guides/development-release.html`

**Implementation Path:**

- [x] Define signing secret names and local dry-run behavior for Windows, macOS, and Linux.
- [x] Add Windows signing step with dry-run detection and clear missing-secret output.
- [x] Add macOS signing/notarization step with dry-run and unsigned fallback only for non-release builds.
- [x] Add Linux `.desktop`, icon, appstream/metainfo, changelog, and package metadata support.
- [x] Ensure `rco package --gui --mvc` embeds or bundles static assets for production desktop apps.
- [ ] Add checksums and signature/provenance metadata for every artifact.
- [ ] Add updater design: channel metadata, version checks, signature verification, rollback story.
- [ ] Implement updater only after signing/signature verification is stable.
- [ ] Add release workflow tests or dry-run jobs for package metadata.
- [ ] Update release docs with exact local and CI commands.

**Verification Gate:**

Release workflow dry-run passes on branch builds, tag release passes with signed artifacts when secrets are configured, and unsigned beta fallback is only allowed for explicitly marked non-production/nightly artifacts.

**Progress Note (2026-06-22):** The first production distribution slice adds
explicit signing modes to the release scripts: Windows
`-SigningMode auto|require|skip|dry-run`, macOS `--signing-mode` and
`--notarization-mode` with the same modes, CI tag builds using `require`, and
nightly/manual dry-run fallback with signing-status reports. Linux release
packages now include `--signature-mode auto|require|skip|dry-run` for GPG
detached-signature reports or `.asc` artifacts, plus REPL desktop metadata, SVG
icon, AppStream metainfo, changelog, maintainer metadata, docs, examples, and
packages. Generated Linux GUI app packages now include per-app `.desktop`, SVG
icon, AppStream metainfo, and changelog metadata, while the existing packaged
MVC GUI tests continue to prove static assets are bundled into production
desktop apps. Remaining Epic 12 work is certificate/keychain/GPG import setup,
signed/notarized tag verification, detached signature/provenance metadata,
updater design/implementation, store packaging, and release dry-run workflow
hardening.

---

## Epic 13: Central Roadmap Closure Audit

**Feature Definition:** Perform a final source-backed audit proving every roadmap item is complete, documented, and tested.

**Complete Means:**

- `docs/feature-map.md` has no remaining roadmap items except explicitly documented non-goals.
- README and HTML reference are production-facing and free of stale local-agent language.
- All feature claims link to live evidence.
- CI and release workflows are green.
- A release candidate is tagged and published only after the audit passes.

**Primary Files:**

- `docs/feature-map.md`
- `README.md`
- `docs/reference/index.html`
- `docs/reference/guides/*.html`
- `docs/wiki/*.md`
- `docs/releases/*.md`
- `.github/workflows/*.yml`

**Implementation Path:**

- [ ] Re-read `AGENTS.md` and `docs/feature-map.md`.
- [ ] Search for `future work`, `remaining`, `not implemented`, `planned`, and stale version claims.
- [ ] Run `rco words --json` and compare against `docs/reference/app.js`, LSP, and grammar.
- [ ] Run the full local gate: fmt, clippy, workspace tests, audit, docs validation, editor validation, words check, acceptance.
- [ ] Run release dry-run workflow if available.
- [ ] Update `docs/feature-map.md` to mark roadmap complete and move non-goals to a separate section.
- [ ] Publish a release candidate.
- [ ] Watch GitHub CI and release workflow to completion.

**Verification Gate:**

No stale roadmap claims remain; release candidate CI is green; feature-map evidence matches live code, tests, and docs.

---

## Explicit Non-Goals Unless Re-Scoped

These appear in the feature map as boundaries or cautions and should not be silently promoted into roadmap goals:

- HTTP redirects remain disabled by policy; `follow_redirects=true` is explicitly unsupported unless a future security design reopens it.
- Browser automation and screenshot capture are not Ricochet core features.
- Approval words are primitives, not a full authorization policy.
- Active Record targets existing schemas and should not be described as a full schema-definition ORM.
- The SQLite scaffold login loop is not full production auth.
- DAP and VS Code debugger surfaces are not missing; remaining debugger work is richer views and dedicated UIs.

---

## Standard Verification Matrix

Run after each epic:

```powershell
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo test --workspace
rtk cargo audit --deny warnings
rtk powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
rtk powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-editor-assets.ps1
rtk cargo run -p ricochet_cli --bin rco -- words --check --docs-app docs/reference/app.js --grammar editors/vscode/syntaxes/ricochet.tmLanguage.json
rtk powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1
rtk git diff --check
```

For public releases, also run or watch:

```powershell
rtk gh run list --repo BARKx4/Ricochet --branch main --limit 10
rtk gh run watch <run-id> --repo BARKx4/Ricochet --exit-status --interval 10
```

## Self-Review

- Spec coverage: Every remaining item from `docs/feature-map.md` is represented by an epic or listed as an explicit non-goal.
- Placeholder scan: This plan uses concrete files, public surfaces, implementation paths, and verification gates instead of vague implementation instructions.
- Type and surface consistency: Public word candidates use postfix/RPN order and `_` separators. Commands are shaped as `rco <family> <action>` where existing CLI patterns already use that style.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-20-roadmap-completion.md`.

Two execution options:

1. Subagent-Driven (recommended) - dispatch a fresh subagent per epic or per task slice, then review between tasks.
2. Inline Execution - execute tasks in this session using the executing-plans skill, with checkpoints after each epic slice.

Recommended first execution step: finish Epic 0 by committing and pushing the current credential/password-policy implementation before opening new feature work.
