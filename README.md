<p align="center">
  <img src="docs/assets/ricochet-logo.png" alt="Ricochet logo" width="420">
</p>

# Ricochet

Ricochet is a modern, pure-postfix, stack-based programming language descended
in spirit from Forth. The current implementation is a
Rust bytecode VM with dynamic OOP, CLI scripting, MVC web app scaffolding,
template rendering, stack/debug tracing, and early Active Record support for
existing SQLite, PostgreSQL, and MySQL/MariaDB schemas.

Website: [try.ricochet.today](https://try.ricochet.today/)

## Current Features

Ricochet is currently aimed at a developer-facing v1 beta: a usable web app
foundation that other developers can scaffold, run, inspect, and extend.

- Language runtime: Rust bytecode VM, dynamic OOP, stack/debug tracing, regular
  expressions, collections, and postfix control flow.
- Task model: first-class task values with `spawn`, `await`, `await-all`,
  retained completed/failed status, explicit `release-task` cleanup, eager
  background execution, task inspection, and task-returning HTTP helpers.
- Approval model: local apps can create approval records, claim a generated
  token exactly once, and complete or reject the record with retained audit
  state through `approval_create`, `approval_claim`, `approval_complete`,
  `approval_reject`, and `approval_detail`.
- Host capabilities: filesystem/workspace, HTTP, environment, sleep, TUI,
  webview, opt-in process execution, and opt-in PTY sessions can be inspected
  with `runtime_capabilities`; direct child process execution uses
  `process_spawn`, `process_spawn_task`, or retained `process_start` jobs.
- Desktop GUI beta: trusted local scripts can build escaped `webview` document
  maps, preview them with `rco gui`, and package them as native Windows, Linux,
  or macOS WebView executables with `rco package --gui`. MVC projects can also
  be packaged as local-server desktop WebView apps with
  `rco package --gui --mvc`.
- Terminal UI beta: trusted local scripts can use the `tui` capability for
  alternate-screen terminal apps with drawing, cursor movement, flushing, size
  checks, and key polling/reading through `rco tui` or `rco package --tui`.
- CLI workflow: run `.rco` scripts, format source, build and run bytecode,
  package standalone executables, generate Markdown docs, run Ricochet tests,
  and serve local web apps.
- Package workflow: record, install, and verify path/GitHub dependencies, pin
  Git dependencies to immutable commits in `ricochet.lock`, store deterministic
  `sha256:` package-content integrity hashes, and import local package sources
  by package name.
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
  tables with `find`, `all`, `where`, `count`, `first`, `exists?`, `insert`,
  `update`, `default-page`, `limit`, `page`, `order-page`, `where-limit`,
  `where-page`, and `where-order-page`.
- Database migrations: apply ordered SQL migrations from `db/migrations` to
  SQLite, PostgreSQL, or MySQL/MariaDB projects and track them in
  `schema_migrations`.
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
- Security posture: sandboxable host capabilities, no-follow HTTP redirects,
  import/dependency path containment, signed default sessions, view/template
  traversal guards, and TLS-required remote PostgreSQL connections.

Still planned: GUI event/state callbacks, migrations, production auth packages,
and structured AI/schema package helpers.

## Quickstart

For published releases, download the package for your platform from
the [GitHub Releases](https://github.com/BARKx4/Ricochet/releases) page.

On Windows, run `ricochet-vX.Y.Z-windows-x64-setup.exe`. The installer adds
Start Menu entries, including a Ricochet Shell that opens a command prompt with
`rco` available. Portable release ZIPs are also available from the same release
page. Extract the ZIP and run `Ricochet Shell.cmd`, or add the extracted folder
to your `PATH`.

On Linux, install the Debian package with:

```bash
sudo apt install ./ricochet_X.Y.Z_amd64.deb
```

Portable Linux tarballs are also available. Extract the tarball and run
`./install.sh`, or add the extracted folder to your `PATH`.

On macOS, choose the unsigned tarball for your Mac:
`ricochet-vX.Y.Z-macos-arm64.tar.gz` for Apple Silicon or
`ricochet-vX.Y.Z-macos-x64.tar.gz` for Intel. Extract it and run
`./install.sh`, or add the extracted folder to your `PATH`. These beta tarballs
are not notarized by Apple.

For an uninstalled source checkout, install the CLI once:

```powershell
cargo install --path crates/ricochet_cli --bin rco --locked
```

Then use `rco` as the Ricochet toolchain:

```powershell
rco new my_app
rco new --with-sqlite my_beta_app
rco routes my_app
rco migrate status my_beta_app
rco migrate apply my_beta_app
rco doctor my_app
rco verify my_app
rco lsp-diagnostics my_app/app/Models/User.rco --pretty
rco lsp
rco doc my_app
rco test my_app
```

MVC apps serve static files from `public/` at `/assets` by default. Override
that with `[web.static] dir = "..."` and `mount = "/..."` in `ricochet.toml`.

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

Preview and package a desktop GUI app:

```powershell
rco gui examples/webview_ui.rco
rco package examples/webview_ui.rco --gui --output webview-ui.exe
```

Run and package an interactive terminal UI app:

```powershell
rco tui examples/tui_counter.rco
rco package examples/tui_counter.rco --tui --output tui-counter.exe
```

Package an MVC app directory as a desktop beta build:

```powershell
rco new my_desktop_app
rco package my_desktop_app --gui --mvc --output my-desktop-app.exe
```

Use `--output webview-ui` on Linux and macOS. Linux GUI launchers use
WebKitGTK 4.1; Debian packages generated for GUI apps declare the
`libwebkit2gtk-4.1-0` and `libgtk-3-0` runtime dependencies.

On Linux, the same `package` command can also create portable tarballs or
Debian packages for a `.rco` file:

```bash
rco package examples/basic-oop.rco \
  --output basic-oop \
  --linux-package tar \
  --linux-package deb \
  --package-name basic-oop \
  --package-version 0.1.0
```

Add a package dependency from a Ricochet project:

```powershell
rco add ./packages/greeter
rco add ./packages/greeter --version "^0.2.0"
rco publish ./packages/greeter --registry ../ricochet-registry
rco add registry:greeter --registry ../ricochet-registry --version "^0.2.0"
rco add github:BARKx4/ricochet_auth@v0.1.0 --no-fetch
rco install
rco verify
rco audit --json
```

`ricochet.lock` records each resolved package source, cache path, optional Git
commit, optional local registry, optional semantic version requirement, resolved
package version, and `sha256:` content integrity hash. `rco publish --registry`
creates a file-backed local registry entry for packages with `[package] name`
and `version`; `rco add registry:name --registry PATH` installs from that
registry, or uses `RICOCHET_REGISTRY` when `--registry` is omitted. `rco install`
and `rco verify` reject packages whose `[package] version` does not satisfy the
manifest requirement, and `rco verify` recomputes the package tree hash while
ignoring VCS metadata, so it catches local path changes, registry cache drift,
or cached Git package drift without fetching or rewriting anything.
Use `rco audit` for a human-readable dependency report or `rco audit --json`
for CI and release tooling.

Local package dependencies can be imported by package name:

```forth
"greeter/greeting" import
packageHello
```

Read an existing binding with `$name`. Declaration words still keep the
name-first shape, and `$` is useful when a declaration name is itself stored in
another variable:

```forth
"users" name var
$name array
$users "Ada" push! drop
$users count println
```

Top-level declarations are shared across the VM. Function and method calls get
fresh local declaration scopes, so helper locals declared with `var`, `array`,
`map`, `list`, or `Set` refresh within the active call and do not leak into
later calls.

Spawn a task and await its result:

```forth
[ 40 2 + ] spawn answer var
$answer status
$answer running?
tasks count
$answer await
$answer release-task
$answer status
handles array
$handles [ 20 2 + ] spawn push! drop
$handles [ 30 4 + ] spawn push! drop
$handles await-all
```

HTTP capability calls can also be launched as tasks:

```forth
"https://example.com" http_get_task request var
$request await value response var
$response "status" at println
```

For authenticated provider calls, pass a request map with explicit headers.
`http_request` and `http_request_task` preserve the same runtime host allowlist
and no-follow redirect policy as the simpler HTTP words. Request maps can also
narrow themselves with `allowed_hosts` and `allowed_schemes`, and can set
bounded `timeout_ms` and `max_response_bytes` values:

```forth
headers map
headers get "Authorization" "Bearer token" put! drop
hosts array
hosts get "api.example" push! drop
schemes array
schemes get "https" push! drop
body map
body get "probe" true put! drop
request map
request get "url" "https://api.example/v1/models" put! drop
request get "method" "POST" put! drop
request get "headers" headers get put! drop
request get "json" body get put! drop
request get "allowed_hosts" hosts get put! drop
request get "allowed_schemes" schemes get put! drop
request get "timeout_ms" 30000 put! drop
request get "max_response_bytes" 1048576 put! drop
request get http_request value response var
response get "status" at println
```

Build a webview document for desktop UI hosts:

```forth
"Counter" 1 webview_heading heading var
"Increment" "increment" webview_button button var
"<main>" $heading concat
$button concat
"</main>" concat body var
"Counter" $body webview_window value document var
```

Serve an MVC app from its project directory:

```powershell
rco serve --host 127.0.0.1 --port 3000
rco serve --allow-env --http-allow-host 127.0.0.1
rco serve --env-allow OPENAI_API_KEY --http-allow-host api.openai.com
rco serve --allow-process --fs-root .
rco serve --allow-process --process-root .\scripts
rco serve --allow-pty --fs-root .
rco serve --watch
rco serve --watch --fs-root . --http-allow-host 127.0.0.1
```

`--watch` reloads Ricochet MVC routes, controllers, models, views, and the
manifest between requests. If a reload fails, the request returns a clear MVC
error and the next request retries after you fix the source. Combine `--watch`
with `--debug` to print reload trace lines with the new revision and changed
files. The same filesystem, HTTP, environment, process, and PTY capability
flags used by ordinary `rco serve` are also honored by watched MVC runtimes and
by each hot-reloaded revision.

Use `rco doctor [path]` for a read-only health check of a source file, source
tree, package project, or MVC app. Add `--capabilities` to print the MVC
manifest capability surface that will matter for trusted local beta apps.
Package projects can also run `rco verify [path]` to check dependency
manifest/lock consistency, local path containment, git package cache commit
matches, and locked package-content integrity without fetching or rewriting
anything.

`rco serve` keeps MVC process environment reads disabled unless you pass
`--allow-env` or one or more `--env-allow NAME` entries. Prefer
`--env-allow` for trusted local beta apps that store secret references as
environment variable names. `--no-env` keeps the default disabled behavior
explicit, and conflicts with both env-opening flags.

Workspace helpers provide structured filesystem access for local apps while
preserving the same `--fs-root` and `--fs-readonly` bounds as the lower-level
`fs_*` words:

```forth
options map
writeOptions map
writeOptions get "create_parent_dirs" true put! drop
"README.md" options get workspace_read_text value readme var
"." options get workspace_list value entries var
"generated/out.txt" "hello" writeOptions get workspace_write_text value written var
written get "relative_path" at println
runtime_capabilities "workspace" at "root" at println
```

Process execution is stronger than the ordinary trusted local profile and stays
disabled unless you pass `--allow-process`. Use `process_spawn` with a direct
command string, an argument array, and an options map when you want to run a
process to completion. `--process-root PATH` narrows process and PTY `cwd`
values independently from `--fs-root`; when omitted, process cwd containment
falls back to the filesystem root if one is configured:

```forth
args array
args get "status" push! drop
options map
options get "timeout_ms" 10000 put! drop
"git" args get options get process_spawn value result var
result get "success" at println
```

Use `process_start` for long-running jobs. The runtime keeps a retained job
registry with bounded captured output, and it can be inspected across MVC
requests with `process_jobs`, `process_job`, `process_read`, and
`process_cancel`:

```forth
args array
args get "status" push! drop
options map
options get "stdout_max_bytes" 1048576 put! drop
"git" args get options get process_start value job var
readOptions map
job get "id" at readOptions get process_read value output var
output get "stdout" at println
```

PTY sessions are separate and also opt in. Use `--allow-pty` for trusted
scripts or MVC apps that need a real pseudo-terminal. Command names and shell
newlines are host-specific:

```forth
args array
args get "repl" push! drop
options map
"rco" args get options get pty_start value session var
session get "id" at "1 2 +\r\n" pty_write value drop
readOptions map
session get "id" at readOptions get pty_read value screen var
screen get "output" at println
stopOptions map
session get "id" at stopOptions get pty_stop value drop
```

Approval records are runtime-local and shared across MVC requests. `approval_create`
returns a generated token once; `approval_claim` consumes that token exactly
once before the caller performs the mutating operation:

```forth
operation map
operation get "capability" "workspace.write" put! drop
operation get "summary" "Write generated file" put! drop
options map
operation get options get approval_create value approval var
approval get "id" at approval get "token" at approval_claim value claim var
claim get "claimed" at println
```

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

MVC actions parse `application/x-www-form-urlencoded`, `application/json`, and
`multipart/form-data` request bodies for `POST`, `PUT`, `PATCH`, and `DELETE`.
Declared action Args bind route params first, then form fields, JSON object
fields, upload fields, query params, and finally context values. The same data
is available through `ctx get "request" at`: `form` holds text fields, `json`
and `body` hold parsed JSON values, `uploads` is keyed by multipart file field
name, and `files` contains every uploaded file. Upload values include
`name`, `filename`, `content_type`, `size`, `text` when the bytes are UTF-8, and
`data_base64` for arbitrary file bytes. Request body parsing is in-memory with a
16 MiB beta limit.

## Editor Support

A VS Code-compatible extension for `.rco` files lives in `editors/vscode`. It
registers the `source.ricochet` scope, highlights Ricochet comments, strings,
`$name` binding reads, postfix selectors, declarations, control flow, async
words, route verbs, core built-ins, and collection types, and launches `rco lsp`
for live editor support.

`rco lsp` speaks stdio Language Server Protocol and provides live diagnostics,
completion, hover, go-to-definition, document symbols, semantic tokens,
document formatting, prepare-rename, and single-document rename support. The VS
Code extension exposes `ricochet.server.path` when `rco` is not on `PATH`, plus
`Ricochet: Restart Language Server` for local toolchain rebuilds.

`scripts/validate-editor-assets.ps1` checks the grammar and VS Code wiring
against the reference word catalog, and release archives include this folder
under `editors/vscode`.

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

The acceptance suite validates the static reference docs, editor assets,
examples, scaffolded project checks/tests, and a live `rco serve` smoke request
against the generated no-database scaffold.

## Release Packaging

Windows release packages are built from this repository with:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-release.ps1
```

The script builds `rco.exe`, `rco-gui.exe`, and `ricochet.exe`, creates a
portable ZIP, writes `SHA256SUMS.txt`, and creates a Windows `.exe` installer when NSIS
`makensis.exe` is installed. GitHub Actions installs NSIS automatically in the
release workflow.

Linux release packages are built on Linux with:

```bash
bash scripts/package-release-linux.sh
```

The script builds `rco`, `rco-gui`, and `ricochet`, creates a portable tarball
with an `install.sh` helper, writes `SHA256SUMS-linux-x64.txt`, and creates a
Debian `.deb` package with `dpkg-deb`.

Unsigned macOS release tarballs are built on macOS with:

```bash
bash scripts/package-release-macos.sh --target macos-arm64
bash scripts/package-release-macos.sh --target macos-x64
```

The script builds `rco`, `rco-gui`, and `ricochet`, creates a portable tarball
with an `install.sh` helper, and writes a target-specific checksum file. GitHub
Actions builds Apple Silicon and Intel tarballs on separate macOS runners.

To publish a GitHub release, push a version tag:

```powershell
git tag vX.Y.Z
git push origin vX.Y.Z
```

The release workflow packages the Windows, Linux, and macOS artifacts, writes a
combined `SHA256SUMS.txt`, and attaches the ZIP, Windows installer, Linux
tarball, Debian package, unsigned macOS tarballs, and checksums to the GitHub
release.

The same workflow also runs nightly from `main`. Nightly builds use a version
like `X.Y.Z-nightly.N`, build the same Windows, Linux, and macOS packages, and
upload them as GitHub Actions artifacts for 30 days. Nightlies do not create
public GitHub releases.

## Reference Docs

The documentation website is static and lives at
`docs/reference/index.html`. Open it directly in a browser; there is no build
step.

## Safety Notes

The CLI uses the `trusted` capability profile by default for local scripts.
Pass `--capability-profile sandboxed` with `rco run`, `rco run-bytecode`,
`rco repl`, or `rco test` to start with filesystem, HTTP, TUI, and webview
disabled. Process execution and PTY sessions are disabled unless explicitly
enabled in either profile.
In the sandboxed profile, `--fs-root <path>` enables filesystem access only
under that directory, `--fs-readonly` denies writes,
`--http-allow-host <host>` enables HTTP only for named hosts, and
`--allow-tui` enables terminal UI access, while `--allow-webview` enables
webview document building. `--no-fs`, `--no-http`, `--no-tui`, and
`--no-webview` still deny those host powers explicitly in either profile.
`--allow-process` enables direct child process execution with captured
stdout/stderr, bounded output, blocking `process_spawn`, and long-running
`process_start` jobs. `--process-root <path>` can make process and PTY cwd
resolution narrower than the filesystem workspace. `--allow-pty` enables
retained pseudo-terminal sessions through `pty_start`, `pty_write`, `pty_read`,
`pty_resize`, `pty_stop`, `pty_list`, and `pty_detail`.
`--env-allow <name>` enables or narrows environment variable reads to named
variables, `--no-env` denies environment/current-directory reads, and
`--no-sleep` denies script sleeps. Embedded hosts can leave capabilities
disabled. HTTP calls do not follow redirects, use a timeout and response body
cap, and filesystem access remains powerful CLI behavior unless you deny or
bound it with these flags.
For MVC servers, `rco serve` enables filesystem and HTTP only through
`--fs-root` and `--http-allow-host`, and enables process environment reads only
through `--allow-env` or `--env-allow`. MVC process execution requires
`--allow-process`, MVC PTY sessions require `--allow-pty`, and
`--process-root` can narrow execution cwd values. Watched MVC runtimes use the
same capability setup and share retained approval, process, and PTY registries
across hot reloads.

For v1 beta testing, keep `trusted` for your own local scripts and generated
apps. Use `sandboxed` for untrusted examples, bug reports, package reviews, or
third-party code, opening only the filesystem root or HTTP hosts the test
actually needs.
