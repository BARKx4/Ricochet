# Ricochet V1 Beta Readiness Review - 2026-06-12

## Scope

This review covers the current Rust workspace, CLI, VM, MVC web stack,
reference docs, generated app scaffold, acceptance scripts, and v1 beta
positioning. It treats v1 as a developer beta target for usable local MVC web
apps, not a production hosting/security release.

CodeRabbit was attempted before this local review, but the local CLI was not
authenticated and `coderabbit auth login --agent` timed out. This document is
therefore a direct local codebase review backed by repository tests and scripts,
not an external CodeRabbit report.

## Verification Evidence

Passed current-state gates:

- `cargo fmt --all -- --check`
- `cargo test --workspace --quiet`
  - 325 Rust tests passed.
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo audit`
  - Scanned 314 locked crate dependencies with no vulnerability failure.
- `powershell.exe -NoProfile -ExecutionPolicy Bypass -File docs\reference\validate.ps1`
- `powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts\acceptance.ps1`
  - Ran examples.
  - Generated the default no-database scaffold.
  - Served the default scaffold over HTTP.
  - Generated the SQLite beta scaffold.
  - Served `/users` from the SQLite beta scaffold.
  - Exercised the SQLite scaffold form/session flow through `/login`, `/me`,
    and `/logout`.
- Static review searches covered stale `CGI`/`FastCGI` claims,
  `default-page` docs, capability-profile policy wording, and source
  `TODO`/`FIXME`/panic markers.

## Findings Fixed In This Pass

1. The SQLite beta scaffold now uses `User .default-page` for the generated
   `/users` controller instead of hand-rolled ordered pagination.

2. Active Record now exposes `.default-page` both as a model class method and as
   a `DatabaseCapability` method. The helper returns up to 50 rows, orders by
   `id asc` when the mapped model includes `id`, and falls back to a bounded
   first page for mappings without an `id` field.

3. The `.default-page` routing behavior is covered by direct unit tests for the
   mapped-`id` and no-`id` paths, plus VM-level tests for model-class and
   database-capability call forms.

4. The reference docs and README now document `.default-page` as the v1 beta
   list-page default and state the explicit beta capability policy:
   `trusted` for developer-owned local apps, `sandboxed` for untrusted examples,
   package review, bug repros, and third-party code.

5. The design spec no longer presents CGI/FastCGI as part of the local v1 beta
   vertical slice. It is now called out as future deployment adapter work.

6. The older 2026-06-11 review was updated where it had become stale about the
   current web adapter set and remaining beta risks.

## V1 Beta Readiness

Current evidence supports treating the repo as at a v1 beta target for local
developer testing:

- Developers can create a usable MVC app with `rco new`.
- Developers can create a zero-service database-backed beta app with
  `rco new --with-sqlite`.
- The generated SQLite app includes routes, controllers, views, model mapping,
  seeded data, a `/users` Active Record page, and a copyable form/session login
  loop.
- The web stack supports local HTTP serving, route/controller/view dispatch,
  request params/query/form data, cookies, session state, logger/config
  capabilities, instruction budgets, and watch-mode reloads.
- Active Record supports SQLite, PostgreSQL, and MySQL/MariaDB targets against
  existing schemas, with bounded/default read helpers for everyday list pages.
- CLI host capabilities have a documented beta policy and sandboxed profile for
  untrusted code review.
- The reference docs, README, and acceptance suite all reflect the beta target
  rather than a production promise.

## Remaining Non-Beta Blockers

These remain important but should not block the local v1 beta definition:

- Reusable auth packages beyond the scaffolded form/session example.
- Schema migration tooling.
- PostgreSQL TLS/deployment hardening.
- Relation-style Active Record chaining if beta feedback shows it is needed.
- Richer AI provider packages, streaming, and structured/schema helpers.
- Richer suspended-task debugger views.
- CGI/FastCGI or other production deployment adapters.
- External CodeRabbit review once local CodeRabbit authentication is repaired.
