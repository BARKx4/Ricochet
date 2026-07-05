# Ricochet Feature Map

This is the agent-facing map of what Ricochet currently does, where to verify
it, and what is intentionally outside the current roadmap. Use it before making
roadmap claims or adding a new feature. If this file conflicts with code,
tests, or `docs/reference`, trust the live code and update this map.

Status labels:

- `implemented`: shipped in code, docs, and tests or examples.
- `beta`: usable, but intentionally scoped for the v1 developer beta.
- `polish`: the foundation exists; additional work is ergonomics,
  completeness, or production hardening.
- `future`: not currently implemented as a Ricochet surface.

## Orientation

```mermaid
graph TD
  Syntax["Syntax and parser"] --> Compiler["Compiler and bytecode"]
  Compiler --> VM["VM and value model"]
  VM --> CoreWords["Core word catalog"]
  VM --> HostCaps["Host capabilities"]
  VM --> WebWords["MVC and Active Record words"]
  CLI["rco CLI"] --> VM
  CLI --> Web["MVC apps"]
  CLI --> DesktopUI["Desktop WebView UI"]
  CLI --> Packages["Packages and registries"]
  CLI --> Debugger["Debugger, traces, DAP"]
  CLI --> Packaging["Native packaging"]
  Editors["VS Code extension"] --> LSP["LSP"]
  Editors --> Debugger
  Web --> Data["SQLite/PostgreSQL/MySQL"]
  Web --> FirstParty["First-party packages"]
```

## Start Here

- Public language syntax must stay postfix/RPN. The repo-root `AGENTS.md`
  syntax guardrail is binding for new feature work.
- The static reference site is `docs/reference/index.html`; its word catalog is
  `docs/reference/app.js`.
- Run `rco words --check` from a source checkout when changing public words.
- Follow `docs/adding-words.md` when adding, renaming, or removing public words.
- Do not infer missing features from older design text without checking this
  file, `README.md`, `docs/reference/index.html`, and focused tests.

## Command Families

| Family | Status | Surfaces | Evidence |
| --- | --- | --- | --- |
| Project and MVC | implemented beta | `rco new`, `routes`, `serve`, `migrate`, `doctor` | `crates/ricochet_cli/src/lib.rs`, `crates/ricochet_web`, `README.md`, `docs/reference/index.html` |
| Runtime | implemented beta | `repl`, `run`, `debug`, `run-bytecode`, `build`, `test`, `image`, `emit-source` | `crates/ricochet_cli/src/lib.rs`, `crates/ricochet_vm/src/vm.rs`, `crates/ricochet_cli/tests/cli_smoke.rs` |
| Packaging | implemented beta | `package`, `gui`, `tui`, `--gui --mvc`, `--linux-package tar|deb` | `crates/ricochet_cli/src/lib.rs`, `crates/ricochet_cli/tests/cli_smoke.rs`, `scripts/package-release*.sh`, `scripts/package-release.ps1` |
| Desktop WebView UI | implemented beta, 1.0 focus | `webview_*`, `rco gui`, `rco package --gui`, `rco package --gui --mvc`, `rco-gui` | `crates/ricochet_cli/src/lib.rs`, `crates/ricochet_cli/tests/cli_smoke.rs`, `examples/webview_ui.rco`, `examples/learn/22-gui`, `examples/learn/38-capstone-gui` |
| Dependencies | implemented beta | `add`, `install`, `verify`, `audit` | `crates/ricochet_cli/src/lib.rs`, `crates/ricochet_cli/tests/cli_smoke.rs` |
| Registries | implemented beta | `publish`, `registry rebuild`, `registry check`, `registry yank`, `search` | `crates/ricochet_cli/src/lib.rs`, `README.md`, `docs/reference/index.html` |
| Editor and diagnostics | implemented beta | `lsp`, `lsp-diagnostics`, `lint`, `fmt`, `words` | `crates/ricochet_cli/src/lsp.rs`, `editors/vscode`, `scripts/validate-editor-assets.ps1` |
| Docs and quality | implemented beta | `doc`, `bench`, docs validation, acceptance suite | `docs/reference/validate.ps1`, `scripts/acceptance.ps1`, `.github/workflows` |

## Word Catalog Groups

The live source of truth is `rco words --json` and `docs/reference/app.js`.
Current top-level groups are:

- `stack`: `swap`, `dup`, `drop`, `over`, `rot`, `nip`, `tuck`, `pick`,
  `roll`, `depth`, `clear`.
- `math`: checked integer and finite float math, comparison, numeric conversion
  words, boolean words, assertion helpers, and readable aliases such as `add`,
  `subtract`, `multiply`, `divide`.
- `data`: `nil`, booleans, `array`, `map`, `var`, dynamic `get`, `set`,
  `empty?`, `nil?`.
- `collection`: list/set construction, range, collection mutators, indexing,
  counting, filtering, mapping, reducing, searching, and joining.
- `string`: trim/slice/search/case/concat, JSON encode/decode, and regex
  helpers.
- `oop`: `Subclass`, `Field`, `Accessor`, `Table`, `Method`, `new`, `self`,
  `send`, generated `field.get` / `field.set`.
- `control`: `function`, `return`, blocks, `if`/`else`/`end`, `while`,
  `break`, `continue`, `spawn`, `await`, `await_all`, `release_task`.
- `result`: `ok?`, `value`, `error`, `ok`, `fail`, `error?`, `unwrap_or`,
  `map_result`, `and_then`, `result_envelope`, result assertions.
- `inspect`: type/class/method inspection, task metadata and predicates,
  `inspect`, `debug`.
- `web`: route verbs, controller response words, and Active Record words.
- `system`: host effects, capabilities, date/time, environment/config/secrets,
  process/PTY, HTTP, TCP/WebSocket sockets, TUI, webview, approvals.

## Language Core

Status: implemented beta.

Implemented:

- Pure postfix/RPN execution model with no infix parser or operator precedence.
- `$name` for ordinary static variable reads; dynamic by-name reads use
  `"name" get` or `fieldName get`.
- Declaration words include `Subclass`, `Field`, `Accessor`, `Table`, `Method`,
  `function`, `var`, `array`, `list`, and `map`.
- Blocks use `[ ... ]`; optional args metadata uses `( in -> out )`.
- Control flow includes `if`/`else`/`end`, `while`, `break`, `continue`,
  `return`, first-class blocks, and `call`.
- Compile-time expression macros use string-named `"name" Macro` declarations,
  explicit `"name" macro_call` invocation, AST quoting/splicing through
  `quote_ast` and `ast_splice`, whole-item row generation through
  `quote_items`, local/static-import/package lookup, and `rco expand`
  inspection with a stable v1 JSON schema, source inventory, source maps,
  cache metadata, and canonical package macro IDs.
- `quote_items` supports ordinary expression-item rows, class-body rows such as
  `Accessor`, `Field`, `Table`, and `Method`, and top-level declaration rows
  encoded as `[ body ] "name" function`,
  `( args -> outputs ) [ body ] "name" function`, or
  `[ body-items ] Name Superclass Subclass`.
- Lexer/parser/compiler support comments, doc comments, strings with validated
  escapes, signed integer literals, finite float literals with decimals or
  exponents, `$` references, dot selectors, and diagnostics for legacy
  leading-dot syntax.
- Dash-prefixed public words are rejected; `-` is reserved for subtraction and
  negative literals.

Evidence:

- `AGENTS.md`
- `docs/superpowers/specs/2026-06-09-ricochet-design.md`
- `crates/ricochet_syntax/src/lexer.rs`
- `crates/ricochet_syntax/src/parser.rs`
- `crates/ricochet_compiler/src/compiler.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `examples/macro_release_scorecard.rco`
- `examples/showcase/package_macro_queue_report`

Do not assume:

- Do not introduce leading-dot source syntax, namespace-dot host APIs, or
  receiver-first pseudo-object host calls such as `http .request`.
- Do not use `name get` in new docs/examples for ordinary variable reads.
- Do not expect `rco fmt` to migrate unsupported syntax; diagnostics and LSP
  quick fixes point users at replacements.

## VM And Runtime Values

Status: implemented beta.

Implemented:

- Rust bytecode VM with chunks, nested block chunks, source spans, frames, and
  debug events.
- Runtime values include nil, bool, signed i64 `Number`, finite f64 `Float`,
  string, array, list, map, set, class, instance, member selector, block, task,
  result, regex, and capability.
- Versioned VM images preserve safe language state for stacks, globals,
  functions, classes, instances, blocks, and results. `rco repl --image PATH`
  resumes interactive bindings/classes/functions; `:save`, `:load`, and
  `:bindings` inspect or move image state during a REPL session.
- Plain integer literals stay `Number`; decimal or exponent literals produce
  `Float`; mixed numeric arithmetic promotes to `Float`.
- Numeric conversion words such as `to_integer`, `to_tinyint`,
  `to_unsigned_int`, `to_float32`, and `to_float64` return `Result` values for
  parse, range, and precision-boundary failures.
- Dynamic OOP supports classes, inheritance, fields/accessors, methods,
  generated accessor selectors, `self`, and dynamic `send`.
- Mutable collections are shared values; mutators return the same collection.
- Results are explicit stack values and are not truthy in conditions.

Evidence:

- `crates/ricochet_bytecode/src/op.rs`
- `crates/ricochet_bytecode/src/chunk.rs`
- `crates/ricochet_vm/src/value.rs`
- `crates/ricochet_vm/src/image.rs`
- `crates/ricochet_vm/src/vm.rs`
- `crates/ricochet_vm/src/builtins.rs`
- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`

Limit:

- Source emission is a readable source-like bytecode view; do not treat it as a
  stable byte-for-byte source reconstruction contract.

Do not assume:

- Awaited task handles disappear. Completed and failed handles are retained and
  can be inspected or reawaited until `release_task`.
- Result values can be used directly as conditions. Use `ok?`, then `value` or
  `error`.

## Async And Tasks

Status: implemented beta.

Implemented:

- `[ ... ] spawn` creates first-class task values and captures the spawn-time VM
  environment.
- `await` and `await_all` resolve tasks; completed handles reuse cached values.
- Failed handles retain failed status and rethrow when awaited.
- `tasks`, `id`, `info`, `task_status`, `pending?`, `running?`, `completed?`,
  and `failed?` expose task metadata.
- `release_task` cleans up completed or failed retained task handles.
- HTTP request task helpers exist for GET, POST JSON, and mapped requests.

Evidence:

- `crates/ricochet_vm/src/vm.rs`
- `crates/ricochet_vm/src/builtins.rs`
- `docs/reference/app.js`
- `crates/ricochet_cli/tests/cli_smoke.rs`

## Results, Date, And Time

Status: implemented beta.

Implemented:

- Result words: `ok?`, `value`, `error`, `ok`, `fail`, `error?`, `unwrap_or`,
  `map_result`, `and_then`, `result_envelope`, `assert_ok`, `assert_error`.
- `result_envelope` produces `{ ok, data, error, meta }` maps for app/API
  boundaries.
- Date/time words use UTC Unix epoch milliseconds as the timestamp boundary.
- Timestamp words parse/format RFC3339, expose parts maps, build timestamps from
  parts, and perform timestamp arithmetic.
- Date words parse/format calendar dates, convert to/from timestamps, and do day
  arithmetic.
- Duration unit words produce millisecond durations; `duration_parts` breaks
  durations down.
- Date/time user-data failures return Ricochet `Result` errors.

Evidence:

- `crates/ricochet_vm/src/builtins.rs`
- `crates/ricochet_vm/src/vm.rs`
- `docs/reference/app.js`
- `README.md`
- `crates/ricochet_cli/tests/cli_smoke.rs`

Future extensions (not current roadmap):

- Exact decimal, money/smallmoney, arbitrary precision integers, and full u64
  unsigned-bigint storage are outside the v1 beta storage model.

## Host Capabilities

Status: implemented beta.

Implemented:

- Capability profiles: `trusted` and `sandboxed`.
- CLI flags narrow filesystem, HTTP, TCP/WebSocket sockets,
  environment, sleep, TUI, webview, process, and PTY capabilities.
- `runtime_capabilities` reports the active powers available to the VM.
- Filesystem words: `fs_read_text`, `fs_write_text`, `fs_exists?`, `fs_list`,
  `fs_create_dir`, `fs_delete`.
- Workspace words: `workspace_resolve`, `workspace_contains?`,
  `workspace_metadata`, `workspace_list`, `workspace_read_text`,
  `workspace_write_text`, `workspace_mkdir`, `workspace_delete`,
  `workspace_copy`, `workspace_move`.
- HTTP words cover simple calls, structured request maps, bearer/JSON/timeout
  helpers, task-returning requests, and retained streams with bounded offset
  reads, read metadata, done signaling, cancel, and release.
- Socket words cover retained raw TCP and WebSocket clients/listeners with
  listen/accept/connect/read/write-or-send/close/release, explicit
  `--allow-sockets`, and optional `--socket-allow-host` bind/destination
  allowlists.
- MVC upload stream words cover retained temporary upload files with
  `upload_streams`, `upload_stream`, `upload_read`, and `upload_release`.
- Process words cover blocking spawn, task spawn, retained process jobs, reads,
  cancellation, release, and env option maps.
- PTY words cover retained terminal sessions, write/read/resize/stop/release,
  list, and detail.
- TUI words cover alternate screen, cursor movement, writes, flush, size, and key
  polling/reading.
- Webview words build escaped GUI document fragments, app-kit layouts, native
  menu metadata, and state/action documents for `rco gui` and
  `rco package --gui`; Windows, macOS, and Linux hosts use embedded Wry WebView
  windows, with Tao providing the desktop event loop and Muda providing native
  menu bars. The Linux external-browser path is diagnostic fallback only.
- Approval words provide runtime-local records with exactly-once token claiming.
- Environment/config/secret words cover env get/set, secret references,
  secret resolution, and nested config lookup.

Evidence:

- `crates/ricochet_vm/src/builtins.rs`
- `crates/ricochet_vm/src/process_runtime.rs`
- `crates/ricochet_vm/src/pty_runtime.rs`
- `crates/ricochet_vm/src/http_stream_runtime.rs`
- `crates/ricochet_vm/src/upload_runtime.rs`
- `crates/ricochet_vm/src/socket_runtime.rs`
- `crates/ricochet_vm/src/approval_runtime.rs`
- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `crates/ricochet_web/tests/web_mvc.rs`
- `docs/reference/app.js`

Explicit boundaries (not current roadmap):

- HTTP redirects remain disabled; `follow_redirects=true` is explicitly not
  supported.
- Approval words are primitives, not a full authorization policy.
- Browser automation and screenshot capture are not Ricochet core features.

Safety:

- `fs_delete` and `workspace_delete` are destructive. The project data-safety
  policy still requires asking the user before deleting anything.
- Do not assume `rco serve` uses the same broad trusted defaults as local CLI
  scripts. MVC capabilities are narrower and flag/manifest-driven.
- `docs/reference/app.js` alias metadata is not proof of VM dispatch. Verify
  aliases against `crates/ricochet_vm/src/vm.rs` before relying on them.

## Desktop WebView UI

Status: implemented beta, 1.0 focus.

Implemented:

- WebView words build escaped GUI document fragments, app-kit layouts, native
  menu metadata, and state/action documents for desktop app hosts.
- `rco gui PATH` opens a `webview_window`, `webview_window_state`, or
  `webview_window_app` document.
- `rco package PATH --gui --output APP` embeds WebView bytecode into the
  `rco-gui` launcher.
- `rco package PATH --gui --mvc --output APP` embeds an MVC project directory as
  a local-server desktop GUI app.
- `RICOCHET_GUI_EXPORT_HTML` exports deterministic HTML for WebView smoke tests.
- `RICOCHET_GUI_EVENT` replays a single WebView action event for state/action
  regression tests.
- Windows, macOS, and Linux use embedded Wry WebView windows.
- Native menu metadata dispatches through the same WebView action callback path.
- First-party app-kit words cover command buttons, toolbars, sidebars, tabs,
  split panes, tables, form rows, status bars, and menu descriptors.
- Host-backed shell words cover file/folder dialogs, clipboard access, and
  external URL launch.

Evidence:

- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `examples/webview_ui.rco`
- `examples/showcase/gui_task_monitor.rco`
- `examples/learn/22-gui/notes_gui.rco`
- `examples/learn/38-capstone-gui`
- `docs/reference/app.js`

Current boundaries:

- Desktop UI for 1.0 is native shell plus WebView body plus Ricochet state.
- The withdrawn native-control renderer experiment is not part of the current
  release surface.
- The first-party app-kit is deliberately small for 1.0; command-palette and
  richer data-grid workflows are future polish, not current release blockers.
- Ordinary Ricochet app code should not need raw platform control handles.
## Web, MVC, Templates, And Static Assets

Status: implemented beta.

Implemented:

- `rco new` scaffolds an MVC app; `rco new --with-sqlite` creates a zero-service
  SQLite beta app with seeded data, migrations, `/users`, `/login`, `/me`, and
  `/logout`.
- `rco routes` lists routes.
- `rco serve` runs the current directory as an MVC app.
- `rco serve --watch` reloads routes, controllers, models, views, and manifest
  configuration between requests while active requests keep their snapshot.
- Routes use `METHOD "path" Controller "action" route`.
- Controller responses use `view`, `text`, `json`, `redirect`, `status`, and
  `header`.
- Request context includes method, path, params, query, form, body/json, uploads,
  files, headers, cookies, session, config, logs, and capabilities such as `db`
  and optional `ai`.
- Declared action args bind route params first, then form fields, JSON fields,
  upload fields, query params, and context values.
- Templates run Ricochet scalar interpolation with HTML escaping by default.
  Embedded template directives support non-rendering script blocks
  (`{% ... do %}`), postfix conditionals (`{% condition if %}` /
  `{% else %}` / `{% end %}`), and collection loops
  (`{% collection "item" each %}` / `{% end %}`).
- Static assets serve from `[web.static]`, defaulting to `public` mounted at
  `/assets`, with traversal/canonicalization guards.

Evidence:

- `crates/ricochet_web/src/server.rs`
- `crates/ricochet_web/src/router.rs`
- `crates/ricochet_web/src/controller.rs`
- `crates/ricochet_web/src/template.rs`
- `crates/ricochet_web/tests/web_mvc.rs`
- `examples/showcase/sqlite_notes`
- `README.md`
- `docs/reference/index.html`

Implemented upload behavior:

- MVC multipart uploads preserve small-file `text`/`data_base64` compatibility
  and expose large files through bounded temporary upload streams with explicit
  read/release words.
- Upload limits are configured through `[web.uploads] max_request_bytes`,
  `max_file_bytes`, `memory_threshold_bytes`, and `max_retained_streams`.

Do not assume:

- Static assets are missing.
- Sessions are unsigned JSON cookies.
- Scaffold auth is production auth.

## Data, Active Record, And Migrations

Status: implemented beta.

Implemented:

- `[database.default]` supports SQLite, PostgreSQL, MySQL, and MariaDB.
- Active Record maps models to existing tables through `Table` and `Accessor`.
- Read/query words include `all`, `find_record`, `default_page`, `where`,
  `limit`, `page`, `order_page`, `where_limit`, `where_page`,
  `where_order_page`, `count_records`, `first_record`, and `exists?`.
- Write words include `insert` and `update`.
- `default_page` provides a bounded list page and orders by `id asc` when the
  model maps an `id`.
- `rco migrate new NAME [--dsl]` creates timestamped raw SQL migrations or
  paired Ricochet migration DSL `.up.rco` / `.down.rco` files.
- `rco migrate status` and `rco migrate apply` read ordered SQL and Ricochet
  migration DSL files from `db/migrations` and record applied versions in
  `schema_migrations`.
- `rco migrate rollback` supports reversible SQLite, PostgreSQL, and
  MySQL/MariaDB migrations through paired `VERSION_name.up.sql` /
  `VERSION_name.down.sql` files or
  `VERSION_name.up.rco` / `VERSION_name.down.rco` files, while existing
  one-file `VERSION_name.sql` migrations remain apply-compatible.
- The native migration DSL is scoped to migration files and supports postfix
  table creation/drop, column creation/add/drop/rename, index create/drop,
  unique index creation, and column modifiers `primary_key`, `not_null`,
  `unique`, and `default`, compiling to SQLite, PostgreSQL, or MySQL/MariaDB
  SQL.
- `rco migrate dump` writes deterministic beta DDL schema snapshots for
  SQLite, PostgreSQL, and MySQL/MariaDB, excluding `schema_migrations` and
  adapter-internal objects where applicable.
- `rco seed` runs ordered SQLite, PostgreSQL, and MySQL/MariaDB
  `db/seeds/*.sql` and `db/seeds/*.rco` files, with an explicit
  non-idempotent seed warning.
- The SQLite scaffold creates an initial migration and seeds a local database.

Evidence:

- `crates/ricochet_web/src/active_record.rs`
- `crates/ricochet_web/src/database_capability.rs`
- `crates/ricochet_web/src/manifest.rs`
- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `crates/ricochet_web/tests/web_mvc.rs`
- `docs/reference/app.js`

Roadmap closure:

- None for the v1 beta database lifecycle.

Do not assume:

- Active Record is a full schema-definition ORM; it targets existing schemas.
- Migrations are missing. SQL `status` and `apply` exist.
- PostgreSQL/MySQL rollback, schema dump, and seed commands are missing.
- `rco migrate dump` is a replacement for `pg_dump` or `mysqldump`; it is a
  deterministic Ricochet beta DDL snapshot.

## Sessions, Auth, Forms, And AI

Status: implemented beta.

Implemented:

- MVC sessions use the `ricochet_session` cookie.
- Sessions are HMAC-signed by default with a per-process beta key or a
  manifest-provided secret.
- Authenticated encrypted v2 cookies are available through manifest-provided
  env-backed secrets.
- Secure cookie attributes are emitted for non-local requests.
- SQLite scaffold includes a copyable local beta form/session login loop.
- `@ricochet/auth` provides session guard predicates, route guard result maps,
  fail-closed CSRF helpers, session construction, cookie option helpers,
  credential normalization, password policy validation, and Argon2id password
  hash/verify wrappers.
- `@ricochet/forms` provides field maps, validation maps, schema validation, and
  multipart file maps.
- MVC `[ai.default]` injects an `ai` controller capability whose `chat` method
  calls an OpenAI-compatible `/chat/completions` endpoint and returns `Result`
  maps.
- `@ricochet/ai` provides provider-neutral provider/message/request/response/
  error contracts, retry policy maps, retry classification/delay/execution
  helpers, tool call/result maps, tool handler execution helpers, schema
  validation, OpenAI-compatible request builders, secret reference helpers,
  OpenAI-compatible response/error/tool-call normalization helpers,
  OpenAI-compatible fake-provider-testable chat execution,
  Anthropic-compatible Messages API request/execution/tool-use helpers, native
  local/Ollama request/execution helpers, OpenAI/Anthropic SSE and Ollama
  NDJSON parsing, retained-stream state/read-event consumers, stream event
  maps, stream text extraction, MVC fake-provider integration coverage, MVC
  stream-state import coverage, and offline/local provider examples.
- Retained HTTP stream reads expose bounded `max_bytes` consumption,
  `from_offset`, `next_offset`, backward-compatible `offset`, `bytes_len`, and
  `done` metadata for incremental consumers.
- `@ricochet/test_helpers` provides assertion, fixture, HTTP response, and
  temporary workspace helpers.

Evidence:

- `crates/ricochet_vm/src/builtins.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `crates/ricochet_web/src/server.rs`
- `crates/ricochet_web/src/manifest.rs`
- `crates/ricochet_web/src/ai_capability.rs`
- `packages/README.md`
- `packages/ricochet_auth`
- `packages/ricochet_forms`
- `packages/ricochet_ai`
- `packages/ricochet_test_helpers`
- `examples/showcase/package_auth_forms`
- `examples/showcase/ai_provider_probe`

Roadmap closure: none currently tracked for the beta AI package layer.

Do not assume:

- The scaffold login loop is full production auth.
- MVC AI or first-party AI helpers are missing.

## Packages And Registries

Status: implemented beta for local/static registries, hosted registry client
read/publish/yank operations, and the local hosted registry reference server;
hosted mirror export to static registry format is implemented.

Implemented:

- `rco add` records path, GitHub, local registry, and static registry
  dependencies.
- `rco install` installs manifest dependencies and writes `ricochet.lock`.
- `rco verify` checks dependency manifest/lock consistency and package-content
  integrity without rewriting.
- `rco audit` reports dependency status and supports JSON output.
- Static registry workflows include publish, rebuild, check, search, provenance
  and signature metadata, `sha256:` archive verification, semver requirements,
  aliases, scoped package names, and lock hardening against same-version
  replacement.
- Hosted registry client workflows include discovery, search, package metadata
  fetch, archive fetch, HTTPS enforcement with loopback HTTP tests,
  semver/yank-aware install resolution, archive and package tree verification,
  hosted publish with `--token-env RICOCHET_REGISTRY_TOKEN`,
  `rco registry yank`, and same-version lock hardening against hosted metadata
  or archive replacement.
- `rco registry serve PATH` runs the local hosted registry reference server with
  discovery, search, package metadata, version metadata, artifact, publish, and
  yank endpoints backed by an on-disk `registry.json` and `artifacts/` store.
  It supports all-package `--token-env NAME` and package/scope
  `--publisher PACKAGE=ENV` publisher policies, validates uploaded archive and
  package tree integrity, preserves provenance/signature metadata, rejects
  duplicate same-version publish attempts, and keeps literal bearer tokens out
  of client metadata and output.
- `rco registry mirror REGISTRY_URL PATH` exports hosted registry metadata and
  artifacts into `ricochet-static-registry-v1` format, preserving yanked
  records, registry-relative archive paths, archive/package integrity, and
  provenance/signature digests so existing static registry search/install flows
  can use the mirror.
- The hosted registry protocol spec defines immutable hosted version metadata,
  publish/search/fetch/yank endpoints, bearer-token handling by secret
  reference, registry-relative artifact paths, verification order, and static
  mirror fallback.
- Static imports shaped like `package/module` resolve through
  `[dependencies.package]` when no relative file exists.
- Dynamic runtime imports are available through `import_dynamic`,
  `module_call`, and `module_get`. Host loaders reuse the compiler resolver,
  enforce path containment, and verify package lock integrity before loading.
- Release packages include the first-party package catalog.

Evidence:

- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `README.md`
- `docs/wiki/hosted-registry-protocol.md`
- `docs/reference/index.html`
- `docs/reference/guides/hosted-registry-protocol.html`
- `packages/README.md`
- `scripts/package-release.ps1`
- `scripts/package-release-linux.sh`
- `scripts/package-release-macos.sh`

Roadmap closure:

- None for the v1 beta package and registry workflow.

Do not assume:

- Package support is design-only.
- Release archives omit packages.

## Debugger, LSP, And Editor Tooling

Status: implemented beta.

Implemented:

- Terminal debugger supports `step`, `next`, `out`, `continue`, `abort`,
  `stack`, `locals`, `globals`, `self`, `tasks`, `tasks --tree`,
  `task <id> stack`, and `task <id> locals`.
- `rco run --trace-file` writes JSON runtime traces.
- `rco debug --json` streams JSON Lines events.
- `rco debug-adapter` speaks Debug Adapter Protocol over stdio.
- `rco debug-tui --smoke` renders a read-only first-pause debugger snapshot for
  terminal UI prototyping, and non-smoke `rco debug-tui` runs a
  command-driven text session with per-pause snapshots plus `step`, `next`,
  `out`, `continue`, `abort`, and runtime line-breakpoint edit controls.
- `rco debug-web --smoke` renders the same snapshot as standalone HTML, and
  non-smoke `rco debug-web` serves a loopback-only browser debugger shell with
  `GET /events` Server-Sent Events and `POST /control` step/next/out/continue/
  abort plus runtime line-breakpoint edit actions. The browser shell groups
  current source/instruction, stack, locals, globals, `self`, tasks, output,
  event log, and breakpoint state into separate panes, with click controls and
  keyboard shortcuts for stepping/resume/abort actions.
- DAP supports launch, source breakpoints, continue, pause, step into/over/out,
  stack frames, scopes, variables, output events, and termination.
- Debug task snapshots include operation labels, status predicates, optional
  fault text, and worker-published frame/source/opcode/stack/locals snapshots;
  DAP task variables expand into those task and frame details.
- VS Code extension includes TextMate grammar, LSP client, stack trace
  visualizer, debug configuration, and a live debugger stack panel that expands
  nested DAP task variables into task detail and worker-frame trees.
- `rco serve --debug` installs MVC request-fault pause reporting before HTTP 500
  responses for controller action, view render, and response metadata failures.
- `rco lsp` provides diagnostics, completion, hover, go-to-definition, document
  symbols, semantic tokens, formatting, quick fixes, prepare-rename, and
  single-document rename.
- LSP diagnostics and quick fixes cover `$name` migration and leading-dot legacy
  syntax.
- `rco words --check` validates docs/reference and TextMate grammar against the
  embedded LSP inventory.

Evidence:

- `docs/debugger-integrations.md`
- `crates/ricochet_cli/src/debug_protocol.rs`
- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/src/lsp.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `editors/vscode/package.json`
- `editors/vscode/extension.js`
- `editors/vscode/syntaxes/ricochet.tmLanguage.json`
- `scripts/validate-editor-assets.ps1`

Future polish (not roadmap blockers):

- Richer full-screen TUI layout and screenshot-quality browser styling are
  deferred polish.
- Source breakpoints are line-based until stable instruction IDs exist.

Do not assume:

- Debugger UI is broadly missing; first-pause TUI/browser previews, a
  command-driven text TUI control loop, and a loopback browser SSE/control
  shell with grouped live panes and runtime line-breakpoint editing exist.
- DAP or VS Code debugger surfaces are missing.

## Build, Packaging, Release, And CI

Status: implemented beta.

Implemented:

- `rco build` writes bytecode to `build/app.rcob`.
- `rco run-bytecode` executes `.rcob`.
- `rco package` creates standalone launcher executables by embedding bytecode
  or MVC bundles.
- `rco package --tui`, `--gui`, and `--gui --mvc` cover console, embedded
  WebView GUI launchers, and local-server desktop apps.
- Linux `rco package` can also emit tarballs and Debian packages.
- Release scripts build Windows ZIP/NSIS installer, Linux tar/deb, and macOS
  tarballs with explicit signing or detached-signature
  dry-run/auto/required modes, signing-status reports, checksum files, and
  per-target JSON artifact/provenance manifests.
- Linux release packages include a terminal desktop launcher, SVG icon,
  AppStream metainfo, changelog, maintainer metadata, bundled docs, bundled
  examples, and the first-party package catalog. User-built
  `rco package --gui --linux-package tar|deb` apps include per-app desktop,
  icon, AppStream, and changelog metadata.
- Release workflow uploads Windows, Linux, macOS, checksum, signing-status,
  per-target manifest, and installer artifacts and runs nightly builds. Tag
  builds require Windows signing, Linux detached-signature, and macOS
  signing/notarization prerequisites instead of silently producing unsigned
  production artifacts.
- Tag release package jobs import production signing credentials into
  runner-local stores before packaging: Windows PFX to `Cert:\CurrentUser\My`,
  Linux GPG private key to an ephemeral `GNUPGHOME`, and macOS P12/notarytool
  credentials to an ephemeral keychain. Branch/manual dry-runs and scheduled
  nightlies still run without secrets.
- Release workflow package jobs validate each per-target artifact manifest
  after package smoke tests and before upload, including required installer/deb
  artifacts in CI and Linux detached-signature relationships when signatures
  exist.
- Release workflow package jobs also validate store-ready packaging before
  upload: Windows ZIP/installer shape, Linux tarball and Debian desktop/AppStream
  metadata, and macOS tarball plus production notarization readiness.
- Production tag package jobs verify release signatures before upload: Windows
  checks Authenticode signatures on the portable ZIP executables and installer,
  Linux verifies detached GPG signatures against the selected signing key, and
  macOS verifies codesign signatures plus Apple notarytool's accepted submission
  report.
- Tag publish jobs write and validate a v1 updater channel document:
  `UPDATE-CHANNEL-stable.json` for stable tags or
  `UPDATE-CHANNEL-candidate.json` for semver prerelease tags. The document
  records release version, platform manifests, required verification methods,
  artifact hashes, rollout percentage, and the default reject-older-or-equal
  rollback policy for external installers or future elevated updaters. Stable
  channels require production signature/notarization verification; candidate
  channels record SHA-256 verification metadata for dry-run signed artifacts.
- CI validates formatting, clippy, tests, audit policy, benchmarks, reference
  docs, editor assets, and acceptance.

Evidence:

- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `scripts/package-release.ps1`
- `scripts/package-release-linux.sh`
- `scripts/package-release-macos.sh`
- `scripts/verify-release-signatures.ps1`
- `scripts/validate-store-packaging.ps1`
- `scripts/write-update-channel.ps1`
- `scripts/validate-update-channel.ps1`
- `scripts/setup-windows-signing-certificate.ps1`
- `scripts/setup-linux-gpg-key.sh`
- `scripts/setup-macos-signing-keychain.sh`
- `scripts/cleanup-macos-signing-keychain.sh`
- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `README.md`
- `docs/releases`
- `docs/wiki/store-packaging.md`

External boundaries (not current roadmap):

- No production distribution polish remains in the current roadmap. External
  marketplace uploads still require operator-owned publisher accounts and review
  flows, starting from the validated release artifacts.

Do not assume:

- Native packaging is missing.

## Roadmap Closure And Non-Goals

Implemented foundations that should not be listed as missing:

- SQL and native DSL migrations (`rco migrate new/status/apply/rollback/dump`)
  plus ordered SQL/Ricochet seed files.
- Desktop app packaging (`rco package --gui`, release scripts, `.deb`,
  installers).
- Debugger, DAP, and VS Code debug surfaces.
- Static assets.
- Signed and optionally encrypted sessions.
- MVC AI capability and first-party AI helpers.
- Package add/install/verify/audit/publish/static registry/search workflows.
- Retained process, PTY, HTTP stream cleanup/release, and HTTP stream read
  ergonomics.
- Date/time/duration words.
- Beta compile-time expression macros, including local/static-import/package
  path lookup and `rco expand` inspection.
- Persistent REPL images, image inspection, and source-like bytecode emission.
- Dynamic runtime imports with module metadata and lock/path enforcement.
- Desktop WebView words, `rco gui`, `rco package --gui`, and `rco-gui`
  launcher packaging.

Roadmap closure status:

- No current roadmap implementation items are tracked. External
  marketplace submissions, deeper numeric domains, policy-auth frameworks, and
  richer debugger visuals are explicit future extensions rather than current
  roadmap blockers.

## Maintenance Rules

Update this map when a feature family changes status, gains a new public command
or word family, or when a stale assumption is discovered.

For public word changes, also follow `docs/adding-words.md`.

Recommended verification after feature-map-only edits:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-editor-assets.ps1
rtk cargo run -p ricochet_cli --bin rco -- words --check --docs-app docs/reference/app.js --grammar editors/vscode/syntaxes/ricochet.tmLanguage.json
rtk git diff --check
```

For release-facing feature changes, also run:

```powershell
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo test --workspace
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1
```
