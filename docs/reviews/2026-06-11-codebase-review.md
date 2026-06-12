# Ricochet Codebase Review - 2026-06-11

## Scope

This review covers the current Rust workspace, CLI, bytecode VM, MVC web slice,
reference docs, examples, and acceptance scripts. It focuses on critical flaws,
missing features, bugs, security risks, and testing gaps that matter before real
user-script testing expands.

## Current State

Ricochet now has a coherent first vertical slice:

- `ricochet_syntax`: lexer/parser for postfix declarations, blocks, comments,
  args objects, `if else end`, and `while end`.
- `ricochet_bytecode`: serializable bytecode chunks with debug/source spans.
- `ricochet_compiler`: source-to-bytecode compiler with class, method,
  function, block, branch, and loop support.
- `ricochet_vm`: stack VM, dynamic classes/open classes, first-class blocks,
  collection words, results, debug events, CLI capabilities, and OOP dispatch.
- `ricochet_web`: MVC manifest/routes/controllers/views, template expressions,
  Active Record mapping, PostgreSQL adapter, and request revision skeleton.
- `ricochet_cli`: `rco new`, `check`, `run`, `build`, `serve`, `routes`, `test`,
  and REPL entry points.

The core language shape is already testable. The biggest remaining risks are
around the host boundary: CLI capabilities are still powerful when enabled,
filesystem roots are opt-in rather than default, and higher-level production
policies are not yet in place.

## Verification Performed

- Passed: `rtk "C:\Users\lotti\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe" test --workspace`
  - 220 total Rust tests passed across unit, integration, and doc-test targets.
- Passed: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1`
  - Reference docs validation passed.
  - Examples ran.
  - Scaffold generation, route listing, `check`, and scaffold tests passed.
- Passed: `cargo tree -d`
  - Only observed duplicate was `getrandom`, pulled by `ring`/`rustls` and
    `rand`/`tokio-postgres`.
- Initially blocked, then fixed: `cargo clippy --workspace --all-targets -- -D warnings`
  - Rustup was repaired, Clippy/Rustfmt components were installed, and the
    existing Clippy test warnings were cleaned up.
- Initially blocked, then fixed: `cargo audit`
  - `cargo-audit v0.22.2` was installed locally and now runs against
    `Cargo.lock`.

## Fix Pass Status

Implemented after this review:

- Fresh `rco new` manifests no longer include `[database.default]`, so the
  scaffold no longer requires `DATABASE_URL` before local serving.
- `rco serve` now accepts `--host` and `--port` through `ServeOptions`.
- `rco serve --watch` now reloads routes, controllers, models, views, and the
  manifest between requests.
- `scripts/acceptance.ps1` now invokes `scripts/live-server-smoke.ps1`, which
  starts a real `rco serve` process for the generated no-database scaffold and
  verifies the home page over HTTP.
- Web controllers now receive request metadata, headers, cookies, a lightweight
  session map, per-request logger, and safe manifest config through `ctx` and
  declared Args.
- The web router supports `DELETE`, `PUT`, and `PATCH` in addition to `GET` and
  `POST`.
- The VM has an instruction limit API, and web controllers/templates use
  request-time instruction budgets.
- Async task values now support `.id`, `.status`, `.pending?`, and the `tasks`
  word for basic pending/consumed inspection.
- MVC 500 responses include the full error chain, so budget faults are visible
  to testers.
- HTTP capability calls use a 10 second timeout and cap response bodies at
  1 MiB.
- `rco repl`, `rco run`, `rco run-bytecode`, and `rco test` now accept
  `--no-fs` and `--no-http` so trusted-local defaults can be denied per
  execution.
- Those same commands now accept `--fs-root <path>` to bound filesystem access
  to a directory.
- They also accept `--fs-readonly` to allow filesystem reads while denying
  writes and directory creation.
- They also accept repeatable `--http-allow-host <host>` flags to restrict
  HTTP capability calls to specific hosts.
- `rco test --filter` now pre-skips test files that cannot match the filter
  before running top-level code.
- Active Record now has parameterized `.limit` plus `.count`, `.first`, and
  `.exists?` class methods for bounded/basic reads.
- A root `README.md` and GitHub Actions CI workflow were added for repeatable
  verification. The repo-level `rust-toolchain.toml` was later removed in favor
  of README/CI toolchain instructions.

Still open:

- Default-off capability policy and named host-policy profiles remain larger
  trust-model decisions beyond the per-execution deny/root/read-only/allowlist
  flags.
- Session signing/encryption and auth helpers remain package/framework work.
- PostgreSQL TLS configuration and richer Active Record pagination/querying
  beyond `.limit`/`.count`/`.first`/`.exists?` are still backlog.
- Scheduler-level task concurrency and async IO words remain future async work.
- Watch-mode debug trace events are still planned; request-time hot reload is
  implemented.

## Critical / High Findings

### 1. Fresh MVC scaffolds are not actually server-ready by default

Status: resolved after this review. Fresh scaffolds no longer include
`[database.default]`, and acceptance now starts `rco serve` for the generated
no-database scaffold and hits `/`.

Evidence:

- `rco new` writes a default PostgreSQL config with `url = "${DATABASE_URL}"`
  in `crates/ricochet_cli/src/lib.rs:379` and
  `crates/ricochet_cli/src/lib.rs:381`.
- `rco serve` eagerly attempts to resolve and connect the default database when
  `[database.default]` exists in `crates/ricochet_web/src/server.rs:372`.

Impact:

A brand-new scaffold passes `rco check`, `rco routes`, and `rco test`, but
`rco serve` can fail immediately for a user without `DATABASE_URL` or a running
PostgreSQL instance. That blocks the most important first web-app loop.

Recommendation:

- For v1 ergonomics, scaffold without `[database.default]` unless the user passes
  a database option, or make the default database lazy until a model/database
  capability is actually used.
- Keep the live-server smoke in acceptance so regressions are caught before
  pushing.
- Keep PostgreSQL as the blessed database, but do not make "hello MVC" require
  it.

### 2. Request-time VM execution has no fuel, timeout, or resource budget

Evidence:

- The VM interpreter loop runs while `ip < chunk.instructions.len()` with no
  instruction budget in `crates/ricochet_vm/src/vm.rs:453`.
- MVC controllers create a VM and run controller bytecode on the request path in
  `crates/ricochet_web/src/controller.rs:66` and
  `crates/ricochet_web/src/controller.rs:71`.
- Templates compile and execute Ricochet expressions during rendering in
  `crates/ricochet_web/src/template.rs:66` and
  `crates/ricochet_web/src/template.rs:72`.

Impact:

A controller or template expression with an infinite loop can pin request
handling indefinitely. This is especially important because Ricochet is meant to
be usable as a server-side scripting language.

Recommendation:

- Add VM fuel: a configurable max instruction count per run.
- Add host-side request deadlines for controller and template execution.
- Add max stack depth and max collection size limits for hosted/server mode.
- Surface the limit fault cleanly through the existing debug/error trace model.

### 3. CLI filesystem and HTTP capabilities are powerful but unsandboxed

Status: partially resolved after this review. CLI execution still defaults to
trusted-local filesystem and HTTP powers for compatibility, but `rco repl`,
`rco run`, `rco run-bytecode`, and `rco test` now accept `--no-fs` and
`--no-http` to deny those powers for a specific execution. They also accept
`--fs-root <path>` to bound filesystem access to a directory,
`--fs-readonly` to deny writes and directory creation, and repeatable
`--http-allow-host <host>` flags to restrict HTTP requests to specific hosts.
Default-off policy and named host-policy profiles remain future work.

Evidence:

- CLI capability flags are threaded through `crates/ricochet_cli/src/lib.rs`.
- VM host capability toggles live in `crates/ricochet_vm/src/vm.rs`.
- VM filesystem root resolution bounds read/write/list/create calls when
  `--fs-root` is set.
- VM filesystem write policy returns `PermissionError` for `.write-text!` and
  `.create-dir!` when `--fs-readonly` is set.
- HTTP methods now have timeout/body limits and optional host allowlists.

Impact:

This is acceptable for trusted local scripts, but unsafe for untrusted scripts,
package install hooks, shared examples, hosted jobs, or future web-exposed code.
HTTP also has SSRF and hang risk if these capabilities are ever exposed in a
server context.

Recommendation:

- Keep the capability model, but introduce host policy objects:
  timeout, max response bytes, and named profiles for default-off or read-only
  execution.
- Consider default-off execution, a manifest-based trust prompt, or a named
  policy profile before enabling dangerous effects for arbitrary script files.
- Keep documenting clearly that CLI commands execute trusted code with host
  access unless denied by flags or a future policy.

## Medium Findings

### 4. Only GET and POST routes are wired

Evidence:

- The server imports only `get` and `post` in
  `crates/ricochet_web/src/server.rs:11`.
- Route registration matches only `"GET"` and `"POST"` before failing for other
  methods in `crates/ricochet_web/src/server.rs:98`.

Impact:

The route parser accepts method text, but any `PUT`, `PATCH`, or `DELETE` route
will make app startup fail. This limits normal MVC/API CRUD testing.

Recommendation:

Add `DELETE` next, then `PUT` and `PATCH`, with route-listing and request tests.

### 5. `rco test --filter` still executes top-level code in nonmatching files

Status: mitigated. The CLI now skips files whose source text cannot match the
requested filter before compiling and loading the test file.

Evidence:

- Test files are compiled and loaded with `vm.run_chunk(&chunk)` before methods
  are filtered in `crates/ricochet_cli/src/lib.rs:497` and
  `crates/ricochet_cli/src/lib.rs:500`.

Impact:

The filter prevents nonmatching test methods from running, but it does not
prevent top-level file side effects. That is surprising for a CLI test filter,
especially now that test VMs also have CLI capabilities enabled.

Recommendation:

- Either document that test files should only declare classes/functions at top
  level, or implement a discovery/load mode that can avoid executing arbitrary
  top-level effects from filtered-out files.

### 6. PostgreSQL uses `NoTls` only

Evidence:

- `PostgresDatabase::connect` calls `tokio_postgres::connect(url, NoTls)` in
  `crates/ricochet_web/src/active_record.rs:256`.

Impact:

Local development is fine, but production remote PostgreSQL should have a TLS
path. This matters because the project goal includes server-side web use.

Recommendation:

- Add manifest options for TLS mode, or document that v1 PostgreSQL is local/dev
  only until TLS support lands.

### 7. Active Record reads are unbounded

Status: partially fixed. `.limit`, `.count`, `.first`, and `.exists?` are now
available as parameterized/basic model class methods; offset/order/default
pagination remain backlog.

Evidence:

- `select_all_sql` emits a full `select` in
  `crates/ricochet_web/src/active_record.rs:141`.
- `all` executes it directly in `crates/ricochet_web/src/active_record.rs:299`.
- `where_eq` has no limit/pagination in `crates/ricochet_web/src/active_record.rs:308`.

Impact:

Small examples work, but production tables can be loaded entirely into memory.

Recommendation:

- Add `.limit`, `.offset`, and/or default pagination helpers before encouraging
  real apps to use `.all`.

### 8. Resolved: `--watch` was advertised before implementation

Status: resolved after this review. `rco serve --watch` now rebuilds the MVC
runtime between requests when watched project files change.

Evidence:

- CLI accepts `rco serve --watch` in `crates/ricochet_cli/src/lib.rs:43`.
- Server only prints the watch value in `crates/ricochet_web/src/server.rs:385`.
- Reference docs advertise `rco serve [--debug] [--watch]` in
  `docs/reference/index.html:510`.
- The design spec calls `rco serve --watch` a v1 feature in
  `docs/superpowers/specs/2026-06-09-ricochet-design.md:601`.

Impact:

This is a feature gap, not a runtime bug. It can mislead testers into assuming
hot reload is active.

Original recommendation:

- Either label `--watch` as future work in CLI output/docs, or wire reloading
  into the existing revision snapshot model.

### 9. Fixed serve address and port

Evidence:

- `rco serve` binds `127.0.0.1:3000` directly in
  `crates/ricochet_web/src/server.rs:383`.
- CLI has no host/port options in `crates/ricochet_cli/src/lib.rs:43`.

Impact:

Port conflicts are common during web testing, and server-side scripting use will
need predictable host/port control.

Recommendation:

Add `--host` and `--port`, with `RICOCHET_HOST` / `RICOCHET_PORT` optional env
fallbacks.

### 10. Database calls block inside the async server runtime

Evidence:

- Database capability calls bridge async PostgreSQL through
  `tokio::task::block_in_place` in
  `crates/ricochet_web/src/database_capability.rs:276`.

Impact:

This is understandable for the current synchronous VM, but it is a scalability
limit. Under heavier web traffic, database-heavy requests can tie up runtime
workers.

Recommendation:

- Accept for v1 with documentation, or isolate VM/controller execution into a
  blocking worker pool with clear concurrency limits.
- Longer term: async VM host calls or an effect scheduler.

## Low / Quality Findings

### 11. No root README

Evidence:

- `README.md` does not exist at the repository root.
- The reference website exists under `docs/reference/`, but GitHub visitors do
  not get a quickstart at the repo landing page.

Recommendation:

Add a short root README with:

- What Ricochet is.
- Current status.
- Install/build/test commands.
- `rco new`, `rco run`, `rco test`, `rco routes`.
- Link to `docs/reference/index.html`.

### 12. Template escaping is context-insensitive

Evidence:

- HTML escaping is a single helper in `crates/ricochet_web/src/template.rs:103`.

Impact:

This is fine for text nodes, but not enough for attributes, URLs, CSS, or
JavaScript contexts if users start writing richer views.

Recommendation:

- Document that `{ ... }` escaping is HTML text escaping.
- Add explicit helpers later for attribute/url/json/script contexts.

### 13. Random numbers are not security random

Evidence:

- `random` uses a simple xorshift/time-seeded implementation in
  `crates/ricochet_vm/src/builtins.rs:1456`.

Impact:

Useful for examples, not suitable for tokens, passwords, or session IDs.

Recommendation:

Document it as non-cryptographic and add a separate capability-backed secure
random source later.

## Missing Feature Inventory

These are not defects by themselves, but they are the biggest gaps between the
design docs and the current implementation:

- First-party AI package/provider capability.
- Signed/encrypted auth/session/cookie helpers.
- Migrations/schema management beyond existing-schema Active Record.
- Filesystem capability sandboxing and broader capability policy.
- Production PostgreSQL TLS configuration.
- Scheduler-level task concurrency, async IO words, and richer debugger task
  views.

## What Looks Solid

- The bytecode VM/debugger foundation is real and well covered.
- OOP basics are functioning: dynamic classes, open classes, inheritance,
  native/bytecode methods, first-class blocks, `send`, fields, and model
  metadata all have tests.
- Stack-result error handling is coherent and tested.
- MVC dispatch works end to end in integration tests.
- Active Record SQL generation validates identifiers and parameterizes values,
  which is the right injection boundary.
- Reference docs and examples are now aligned with name-first collection
  declarations and `.push!` chaining.

## Suggested Next Sprint Order

1. Add watch-mode debug trace events.
2. Decide whether default CLI filesystem/HTTP access should become opt-in.
3. Expand Active Record pagination/querying beyond `.limit`, `.count`,
   `.first`, and `.exists?`.
4. Add signed/encrypted session/auth package helpers.
5. Build the first-party AI package/provider capability.
