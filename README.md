# Ricochet

Ricochet is a modern, pure-postfix, stack-based programming language descended
in spirit from MUF/MUCK-era Multi-User Forth. The current implementation is a
Rust bytecode VM with dynamic OOP, CLI scripting, MVC web app scaffolding,
template rendering, stack/debug tracing, and early Active Record support for
existing PostgreSQL schemas.

## Current Status

This is an early but runnable vertical slice. You can write `.rco` scripts, run
CLI programs, format source, build and run bytecode, package standalone
executables, scaffold an MVC app, list routes, run Ricochet tests, and serve a
local web app. The CLI can record path/GitHub package dependencies and static
imports can load local package sources. Hot reload, migrations, auth/session
helpers, `rco install`, and the first-party AI package are still planned work.

## Quickstart

```powershell
cargo build -p ricochet_cli --bin rco
cargo run -p ricochet_cli --bin rco -- new my_app
cargo run -p ricochet_cli --bin rco -- routes my_app
cargo run -p ricochet_cli --bin rco -- test my_app
```

Run a script:

```powershell
cargo run -p ricochet_cli --bin rco -- run examples/basic-oop.rco
```

Format and package a script:

```powershell
cargo run -p ricochet_cli --bin rco -- fmt examples/basic-oop.rco
cargo run -p ricochet_cli --bin rco -- build examples/basic-oop.rco
cargo run -p ricochet_cli --bin rco -- run-bytecode build/app.rcob
cargo run -p ricochet_cli --bin rco -- package examples/basic-oop.rco --output basic-oop.exe
```

Add a package dependency from a Ricochet project:

```powershell
cargo run -p ricochet_cli --bin rco -- add ./packages/greeter
cargo run -p ricochet_cli --bin rco -- add github:BARKx4/ricochet_auth@v0.1.0 --no-fetch
```

Local package dependencies can be imported by package name:

```forth
"greeter/greeting" import
packageHello
```

Serve an MVC app from its project directory:

```powershell
cargo run -p ricochet_cli --bin rco -- serve --host 127.0.0.1 --port 3000
```

`rco serve --watch` is reserved for hot reload and currently exits with a clear
not-implemented error.

## Verification

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1
```

The repo includes `rust-toolchain.toml` to request the Clippy and Rustfmt
components. `cargo-audit` is a Cargo plugin; install it with:

```powershell
cargo install cargo-audit --locked
```

## Reference Docs

The documentation website is static and lives at
`docs/reference/index.html`. Open it directly in a browser; there is no build
step.

## Safety Notes

The CLI enables filesystem and HTTP capabilities for trusted local scripts.
Embedded hosts can leave those capabilities disabled. HTTP calls use a timeout
and response body cap; filesystem access is intentionally powerful CLI behavior
and should not be given to untrusted scripts without a future host policy layer.
