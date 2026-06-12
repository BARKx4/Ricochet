# Ricochet

Ricochet is a modern, pure-postfix, stack-based programming language descended
in spirit from MUF/MUCK-era Multi-User Forth. The current implementation is a
Rust bytecode VM with dynamic OOP, CLI scripting, MVC web app scaffolding,
template rendering, stack/debug tracing, and early Active Record support for
existing PostgreSQL schemas.

## Current Status

This is an early but runnable vertical slice. You can write `.rco` scripts, run
CLI programs, format source, build and run bytecode, package standalone
executables, scaffold an MVC app, list routes, generate Markdown docs, run
Ricochet tests, and serve a local web app. The VM has first-class task values
with `spawn`, explicit `await`, retained completed/failed task status, and
basic task inspection. The CLI can record and install path/GitHub package
dependencies, and static imports can load local package sources. Hot reload is
available for MVC apps during local development, and controllers receive
request, header, cookie, lightweight session, logger, and safe manifest config
context. Migrations, signed/encrypted auth helpers, and the first-party AI
package are still planned work.

## Quickstart

Until packaged releases exist, install the CLI from this checkout once:

```powershell
cargo install --path crates/ricochet_cli --bin rco --locked
```

Then use `rco` as the Ricochet toolchain:

```powershell
rco new my_app
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
tasks .count
answer get await
answer get .status
answer get await
```

Serve an MVC app from its project directory:

```powershell
rco serve --host 127.0.0.1 --port 3000
rco serve --watch
```

`--watch` reloads Ricochet MVC routes, controllers, models, views, and the
manifest between requests. If a reload fails, the request returns a clear MVC
error and the next request retries after you fix the source.

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

The CLI enables filesystem and HTTP capabilities for trusted local scripts by
default. Use `--no-fs` or `--no-http` with `rco run`, `rco run-bytecode`,
`rco repl`, or `rco test` to deny those host powers for a specific execution;
use `--fs-root <path>` to bound filesystem access to a directory, and
`--fs-readonly` to deny writes while allowing reads. Use
`--http-allow-host <host>` to restrict HTTP requests to one or more hosts.
Embedded hosts can leave those capabilities disabled. HTTP calls use a timeout
and response body cap; filesystem access remains powerful CLI behavior unless
it is bounded with an explicit root or read-only policy.
