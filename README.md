# Ricochet

Ricochet is a modern, pure-postfix, stack-based programming language descended
in spirit from MUF/MUCK-era Multi-User Forth. The current implementation is a
Rust bytecode VM with dynamic OOP, CLI scripting, MVC web app scaffolding,
template rendering, stack/debug tracing, and early Active Record support for
existing SQLite, PostgreSQL, and MySQL/MariaDB schemas.

## Current Status

This is an early but runnable vertical slice. The v1 bar is a usable web app
beta target for other developers to test, not a production deployment promise.
You can write `.rco` scripts, run CLI programs, format source, build and run
bytecode, package standalone executables, scaffold an MVC app, list routes,
generate Markdown docs, run Ricochet tests, and serve a local web app. The VM
has first-class task values with `spawn`, explicit `await`/`await-all`,
retained completed/failed task status, eager background task execution, basic
task inspection, and task-returning HTTP helpers. The CLI can record and
install path/GitHub package dependencies, and static imports can load
local package sources. Hot reload is available for MVC apps during local
development, and controllers receive request, header, cookie, lightweight
session, logger, and safe manifest config context. Session cookies can be
HMAC-signed with a manifest secret or a per-process beta key, and can be
emitted as authenticated encrypted v2 cookies from environment-backed manifest
secrets. Active Record has basic reads plus a
bounded `default-page` list helper and explicit `limit`, `page`, `order-page`,
`where-limit`, `where-page`, and `where-order-page` helpers. MVC apps can opt
into an OpenAI-compatible `[ai.default]` provider and receive an `ai` controller
capability whose `.chat` method returns `Result` maps. The SQLite beta scaffold
includes a copyable
form/session login loop for local testing.
Migrations, production auth packages, and structured AI/schema package helpers
are still planned work.

## Quickstart

Until packaged releases exist, install the CLI from this checkout once:

```powershell
cargo install --path crates/ricochet_cli --bin rco --locked
```

Then use `rco` as the Ricochet toolchain:

```powershell
rco new my_app
rco new --with-sqlite my_beta_app
rco routes my_app
rco doc my_app
rco test my_app
```

Run a script:

```powershell
rco run examples/basic-oop.rco
```

Format and package a script:

```powershell
rco fmt examples/basic-oop.rco
rco build examples/basic-oop.rco
rco run-bytecode build/app.rcob
rco package examples/basic-oop.rco --output basic-oop.exe
```

Add a package dependency from a Ricochet project:

```powershell
rco add ./packages/greeter
rco add github:BARKx4/ricochet_auth@v0.1.0 --no-fetch
rco install
```

Local package dependencies can be imported by package name:

```forth
"greeter/greeting" import
packageHello
```

Spawn a task and await its result:

```forth
[ 40 2 + ] spawn answer var
answer get .status
answer get .running?
tasks .count
answer get await
answer get .status
answer get await
handles array
[ 20 2 + ] spawn handles get .push! drop
[ 30 4 + ] spawn handles get .push! drop
handles get await-all
```

HTTP capability calls can also be launched as tasks:

```forth
"https://example.com" http .get-task request var
request get await value response var
"status" response get .at println
```

Serve an MVC app from its project directory:

```powershell
rco serve --host 127.0.0.1 --port 3000
rco serve --watch
```

`--watch` reloads Ricochet MVC routes, controllers, models, views, and the
manifest between requests. If a reload fails, the request returns a clear MVC
error and the next request retries after you fix the source. Combine `--watch`
with `--debug` to print reload trace lines with the new revision and changed
files.

For a zero-service local beta app, `rco new --with-sqlite my_beta_app`
creates `db/development.sqlite3`, seeds `users`, configures Active Record, and
adds `/login`, `/me`, and `/logout` routes that exercise form params and the
session cookie. The manifest shape is:

```toml
[database.default]
adapter = "sqlite"
url = "db/development.sqlite3"
```

For a Postgres-backed app, use the same manifest shape with a Postgres URL:

```toml
[database.default]
adapter = "postgres"
url = "${DATABASE_URL}" # use sslmode=require for remote databases
```

Ricochet requires TLS for remote Postgres connections. `sslmode=disable` is
accepted only for `localhost` or loopback development databases.

For a MySQL or MariaDB-backed app, use the MySQL adapter with a `mysql://` URL:

```toml
[database.default]
adapter = "mysql"
url = "${MYSQL_URL}"
```

Active Record maps model declarations to existing tables; schema migrations are
still planned work.

## Developing Ricochet

Use Cargo when changing the Rust implementation itself. For an uninstalled
source-tree run, this is equivalent to `rco run examples/basic-oop.rco`:

```powershell
cargo run -p ricochet_cli --bin rco -- run examples/basic-oop.rco
```

## Verification

For contributor verification, use a current stable Rust toolchain and install
the formatter, linter, and audit plugin explicitly:

```powershell
rustup component add rustfmt clippy
cargo install cargo-audit --locked
```

Then run:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1
```

The acceptance suite validates the static reference docs, examples, scaffolded
project checks/tests, and a live `rco serve` smoke request against the generated
no-database scaffold.

## Reference Docs

The documentation website is static and lives at
`docs/reference/index.html`. Open it directly in a browser; there is no build
step.

## Safety Notes

The CLI uses the `trusted` capability profile by default for local scripts.
Pass `--capability-profile sandboxed` with `rco run`, `rco run-bytecode`,
`rco repl`, or `rco test` to start with filesystem and HTTP disabled. In the
sandboxed profile, `--fs-root <path>` enables filesystem access only under that
directory, `--fs-readonly` denies writes, and `--http-allow-host <host>` enables
HTTP only for named hosts. `--no-fs` and `--no-http` still deny those host powers
explicitly in either profile. `--no-env` denies environment/current-directory
reads, and `--no-sleep` denies script sleeps. Embedded hosts can leave
capabilities disabled. HTTP calls do not follow redirects, use a timeout and
response body cap, and filesystem access remains powerful CLI behavior unless
you deny or bound it with these flags.

For v1 beta testing, keep `trusted` for your own local scripts and generated
apps. Use `sandboxed` for untrusted examples, bug reports, package reviews, or
third-party code, opening only the filesystem root or HTTP hosts the test
actually needs.
