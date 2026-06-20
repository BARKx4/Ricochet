# Feature Overview

Ricochet is currently aimed at a developer-facing v1 beta: a usable web app
foundation that developers can scaffold, run, inspect, and extend.

## Current Features

- Language runtime: Rust bytecode VM, dynamic OOP, stack/debug tracing, regular
  expressions, collections, and postfix control flow.
- Task model: first-class task values with `spawn`, `await`, `await_all`,
  retained completed/failed status, explicit `release_task` cleanup, eager
  background execution, task inspection, and task-returning HTTP helpers.
- Approval model: local apps can create approval records, claim a generated
  token exactly once, and complete or reject the record with retained audit
  state through `approval_create`, `approval_claim`, `approval_complete`,
  `approval_reject`, and `approval_detail`.
- Host capabilities: filesystem/workspace, HTTP, TCP/WebSocket sockets,
  environment, sleep, TUI, webview, opt-in process execution, and opt-in PTY
  sessions can be inspected with `runtime_capabilities`; direct child process
  execution uses `process_spawn`, `process_spawn_task`, or retained
  `process_start` jobs.
- Desktop GUI beta: trusted local scripts can build escaped `webview` document
  maps, preview them with `rco gui`, and package them as native Windows, Linux,
  or macOS WebView executables with `rco package --gui`. MVC projects can also
  be packaged as local-server desktop WebView apps with `rco package --gui
  --mvc`.
- Terminal UI beta: trusted local scripts can use the `tui` capability for
  alternate-screen terminal apps with drawing, cursor movement, flushing, size
  checks, and key polling/reading through `rco tui` or `rco package --tui`.
- CLI workflow: run `.rco` scripts, format source, build and run bytecode,
  package standalone executables, generate Markdown docs, run Ricochet tests,
  and serve local web apps.
- Debugging workflow: trace instruction execution, pause on source breakpoints,
  step into/over/out of calls, inspect stack/locals/globals at pauses, and
  inspect task snapshots at pauses; write JSON runtime traces with `rco run
  --trace-file`, stream JSON Lines with `rco debug --json`, or serve IDEs
  through `rco debug-adapter` over Debug Adapter Protocol stdio.
- Package workflow: record, install, verify, audit, and locally publish
  path/GitHub/registry dependencies, enforce semver requirements, pin Git
  dependencies to immutable commits in `ricochet.lock`, store deterministic
  `sha256:` package-content integrity hashes, attach provenance/signature
  digests to registry packages, rebuild/check static mirrorable registries,
  search static registry indexes, install scoped registry packages through
  `--registry-url`, and import package sources by local alias.
- First-party beta packages: repo-local `@ricochet/auth`, `@ricochet/ai`,
  `@ricochet/forms`, and `@ricochet/test_helpers` packages provide session
  guards, fail-closed CSRF helpers, credential/password policy helpers,
  Argon2id password hash/verify wrappers, provider HTTP request builders, SSE
  response-body parsing, form/validation helpers, and package test utilities
  without moving app-specific workflows into language core.
- MVC web apps: scaffold projects, list routes, serve apps, use hot reload
  during local development, serve static assets from `public/` under `/assets`,
  and render templates with controller-provided view data.
- Controller context: request params, query, URL-encoded and multipart form
  data, JSON request bodies, in-memory file uploads, headers, cookies,
  lightweight session state, logger, safe manifest config, database capability,
  and optional AI capability are available to controllers.
- Sessions and cookies: sessions are HMAC-signed by default with a per-process
  beta key or a manifest-provided secret, can use authenticated encrypted v2
  cookies, and emit secure cookie attributes for non-local requests.
- Active Record: map models to existing SQLite, PostgreSQL, and MySQL/MariaDB
  tables with `find_record`, `all`, `where`, `count_records`, `first_record`,
  `exists?`, `insert`, `update`, `default_page`, `limit`, `page`,
  `order_page`, `where_limit`, `where_page`, and `where_order_page`.
- Database migrations: generate and apply ordered SQL or native Ricochet DSL
  migrations from `db/migrations` to SQLite, PostgreSQL, or MySQL/MariaDB
  projects and track them in `schema_migrations`.
- Local beta scaffold: `rco new --with-sqlite` creates a zero-service app with
  a seeded SQLite database, `/users` Active Record page, and copyable
  form/session login loop. New apps include `public/app.css` served at
  `/assets/app.css`.
- AI integration: MVC apps can opt into an OpenAI-compatible `[ai.default]`
  provider and receive an `ai` controller capability whose `chat` method
  returns `Result` maps.
- Result contracts: stack `Result` values use `ok?`, `value`, and `error`;
  `result_envelope` converts them to `{ ok, data, error, meta }` maps for
  app/API boundaries that need stable structured responses.
- HTTP streams: retained `http_stream_start`, `http_stream_read`,
  `http_stream`, `http_streams`, `http_stream_cancel`, and
  `http_stream_release` jobs support bounded offset reads for SSE or other
  long-running response bodies.
- Socket clients and listeners: retained `tcp_connect`/`tcp_listen` plus
  `tcp_read`/`tcp_write`, and `ws_connect`/`ws_listen` plus `ws_read`/`ws_send`,
  support bounded raw TCP and WebSocket client/server development behind
  explicit socket capability flags.
- Security posture: sandboxable host capabilities, no-follow HTTP redirects,
  import/dependency path containment, signed default sessions, Argon2id password
  hashing primitives, package-level credential/password policy helpers,
  view/template traversal guards, and TLS-required remote PostgreSQL
  connections.
