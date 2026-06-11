# Ricochet

Ricochet is a modern, pure-postfix, stack-based programming language descended
in spirit from MUF/MUCK-era Multi-User Forth. The current implementation is a
Rust bytecode VM with dynamic OOP, CLI scripting, MVC web app scaffolding,
template rendering, stack/debug tracing, and early Active Record support for
existing PostgreSQL schemas.

## Current Status

This is an early but runnable vertical slice. You can write `.rco` scripts, run
CLI programs, scaffold an MVC app, list routes, run Ricochet tests, and serve a
local web app. Package management, hot reload, migrations, auth/session helpers,
and the first-party AI package are still planned work.

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
