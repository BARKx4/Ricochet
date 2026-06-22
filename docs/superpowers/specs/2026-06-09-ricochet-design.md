# Ricochet Language Design

Date: 2026-06-09
Status: Historical product draft, refreshed for current syntax

> Syntax note, 2026-06-18: Ricochet had a pre-launch postfix syntax reset after
> the first version of this draft. The examples below have been updated to the
> canonical surface from the repo-root `AGENTS.md`, `README.md`, and
> `docs/reference/index.html`: capitalized OOP declaration words such as
> `Subclass`, `Accessor`, `Table`, and `Method`; receiver-before-selector calls
> such as `user email.get`; `$name` for ordinary static variable reads;
> container-before-key access such as `request "method" at`; collection mutation
> as `collection value push!`; and underscore-separated public words such as
> `json_encode`, `http_request`, and `fs_read_text`. Hyphen is reserved for
> subtraction and negative number literals, not word naming.

## Purpose

Ricochet is a modern, pure-postfix, stack-based programming language descended in spirit from MUF/MUCK-era Multi-User Forth. It is designed for people who still think in the stack, but it should be useful outside games: full CLI applications, server-side web scripting, MVC web apps, live debugging, and dynamic runtime metaprogramming.

The first serious milestone is a Web MVC vertical slice, not a tiny calculator VM. The language should prove itself by serving a real SQLite, PostgreSQL, or MySQL-backed MVC page with stack-aware debugging.

## Design Goals

- Preserve postfix stack thinking as the primary execution model.
- Use class-based OOP without turning the language into C#/Java with Forth punctuation.
- Make declaration syntax postfix wherever feasible: declaration name first, declaration operator second.
- Support both compile-time declarations and runtime dynamic declarations.
- Make debugging a first-class feature: realtime stack traces, breakpoints, request fault inspection, task visibility, and source-aware bytecode metadata.
- Provide batteries-included CLI/web tooling: `ricochet` plus short alias `rco`.
- Keep side effects capability-oriented so apps, tests, plugins, and hosted scripts can receive explicit powers.
- Prefer readable words over old-school single-character operators, while allowing common math/comparison symbols as aliases.

## Name And Files

- Language name: Ricochet.
- Source extension: `.rco`.
- Bytecode extension: `.rcob`.
- CLI executable: `ricochet`.
- Short CLI alias: `rco`.
- Manifest: `ricochet.toml`.
- Lockfile: `ricochet.lock`.

## Core Execution Model

Ricochet is dynamically typed and purely postfix. There is no infix parser and no operator precedence. Operators are words.

```forth
2 3 +
$count 10 < if
  "small" println
end
```

The implementation target is a Rust bytecode VM. Source compiles to bytecode with source maps, frame metadata, declaration metadata, and optional debug information. Development builds preserve rich debug metadata; release profiles can strip or minimize it.

The VM has two distinct stacks:

- Compile-time stack: used by declaration words such as `Subclass`, `Accessor`,
  `Table`, `Method`, `function`, `var`, and `import`.
- Runtime stack: used by ordinary program execution.

This split is explicit in the language model. Declaration words are real compile-time stack operations, not decorative keywords.

## Syntax Principles

Declaration syntax should follow:

```text
<declaration value/name> <declaration operator>
```

Examples:

```forth
User Model Subclass
"name" Accessor
amount var
[
  self name.get
] "displayName" Method
```

Bare declaration names are static compile-time symbols. Strings and variables can be used for dynamic declarations.

```forth
User Model Subclass         (( static symbol ))
"User" Model Subclass       (( literal string ))
$className Model Subclass   (( runtime string ))
```

Single-character variable operators are not part of canonical v1. Ricochet uses readable postfix access words:

```forth
amount var
100 amount set
$amount println

user email.get
"a@example.com" user email.set
```

Collection mutation uses postfix mutator words. Mutators return the same
collection reference so they can be chained or dropped explicitly:

```forth
$users "a@example.com" push! drop
$config "theme" "dark" put! drop
```

Leading-dot mutators and bang-prefixed declaration aliases are historical draft
syntax, not canonical v1 syntax.

Predicate words conventionally end with `?`:

```forth
empty?
nil?
ok?
active?
```

Math/comparison symbols are allowed as word aliases, but readable names are canonical for equality and identity:

```forth
a b equals
a b identical
a b =
a b ===
```

Core date/time words use UTC Unix epoch milliseconds as the timestamp boundary.
`timestamp_parse` accepts RFC3339 strings with offsets and normalizes them to
UTC, while `timestamp_format` emits RFC3339 UTC output. Calendar-only words use
date maps with `year`, `month`, and `day` fields. Durations are millisecond
counts produced by explicit unit words such as `duration_hours`; timestamp and
date arithmetic return `Result` values for user-data failures rather than
silently coercing invalid input.

Numeric values split exact integer storage from approximate floating-point
storage. Plain integer literals produce signed 64-bit `Number` values; decimal
or exponent literals produce finite f64 `Float` values. Mixed numeric math
promotes to `Float`, while `to_integer`, `to_tinyint`, `to_unsigned_int`,
`to_float32`, `to_float64`, and related conversion words make narrower
database/package boundaries explicit with `Result` failures. Exact decimal and
money domains remain separate future value types, not compatibility fallbacks.

Comments use `(( ... ))`. A comment immediately preceding a declaration becomes that declaration's docstring.

```forth
(( Represents a user account. ))
User Model Subclass
end
```

## Blocks, Functions, Methods, And Args

`end` closes declaration bodies and control-flow blocks. Methods are declared
from a block plus a method name.

```forth
[
  self name.get empty? if
    self email.get
  else
    self name.get
  end
] "displayName" Method
```

Functions and methods may include an optional `Args` object. Parentheses build an `Args` object on the compile-time stack.

```forth
( amount target -> Result ) [
  amount var
  target var

  target set
  amount set

  $amount $target self transferTo
] "transfer" Method
```

Args follow Forth stack-effect convention: left-to-right is stack bottom-to-top, so the rightmost input is at the top of the stack. Args are metadata only. They do not automatically bind variables; code must explicitly capture stack values into variables.

Anonymous executable blocks use square brackets and are first-class closures from v1.

```forth
[
  $ctx
  "home/index" swap view
] "index" Method
```

Blocks capture surrounding variables and carry debug metadata.

## Variables And Values

Variables are declared with postfix `var`.

```forth
amount var
100 amount set
$amount
```

Ricochet uses canonical `nil` for absence. JSON `null` and SQL `NULL` map to/from Ricochet `nil`.

Truthiness is dynamic-language style: `false`, `nil`, numeric zero, empty strings, and empty collections are falsey. `Result` values do not participate in truthiness; callers must use `ok?` explicitly.

```forth
user save
dup ok? if
  value
else
  error "message" at println
end
```

Collections are mutable by default. Core collection declarations follow the
same name-first declaration pattern as classes, fields, methods, and variables:

```forth
users array
settings map
queue list
tags Set
```

The lowercase `array`, `map`, and `list` words declare a named collection when
a string/name is on the stack; otherwise they push an anonymous empty
collection. `Set` is capitalized for name-first set declarations because
lowercase `set` is the variable/member setter. Anonymous collection values are
available through built-in classes:

```forth
Array new
Map new
List new
Set new
```

`array` and `list` are separate types. v1 does not need collection literal
syntax.

Ordinary static variable reads use the `$name` reference prefix. The lower-level
`get` word is reserved for dynamic by-name reads such as `"name" get` or
`fieldName get`. Declaration words still use bare names or strings, so
`users array` declares a static array, `"users" array` declares dynamically from
a string literal, and `$name array` reads `name` and declares an array using the
runtime string stored there.

## OOP And Metaprogramming

Ricochet uses class-based OOP with open classes. Normal classes can be declared at compile time:

```forth
User Model Subclass
  "users" Table
  "id" Accessor
  "email" Accessor
  "name" Accessor

  [
    self name.get empty? if
      self email.get
    else
      self name.get
    end
  ] "displayName" Method
end
```

Dynamic class creation is available from v1 and is central to the language's identity.

```forth
$className "Model" Subclass
$className "dynamic_table" Table
$className "name" Accessor
```

The runtime `Subclass` operator consumes a string class name and a superclass
class value or string. It creates or reopens the class without requiring a
compile-time declaration. Outside a lexical class body, `Table` and `Accessor`
accept an explicit class target before the declaration name.

Open classes can add or freely replace methods by reopening the class in source
and declaring the method again.

```forth
User Model Subclass
  [
    self email.get
  ] "displayName" Method
end
```

Dynamic class and accessor names are available as pure stack operations without
a lexical class body. Runtime method mutation remains a design direction until
it has the same postfix shape and validation as static `Method`.

```forth
"User" className var

$className "Object" Subclass
$className "email" Accessor
$className new email.get
```

The target may be a class value such as `User` or a class-name string. Runtime
declaration faults preserve their operands on the stack for debugger inspection.

Subclass instances include fields declared by their ancestors. Method lookup
walks from the concrete class toward its ancestors, so the nearest declaration
wins even when an override changes between bytecode and a native method.
Class-level native methods are inherited as well, while retaining the concrete
child class as their receiver. Inheritance cycles are VM faults.

Method replacement is allowed without warnings, but the VM records replacement metadata for debugging and hot reload traces.

Method calls keep the receiver first and use ordinary selector words:

```forth
user save
user displayName
user "displayName" send
```

Loaded class names are also runtime values. This gives class-level APIs the same
postfix dispatch shape as instance APIs:

```forth
User all
42 User find_record
```

Global functions take precedence when a function and class share a name.

Inside methods, the receiver remains on the runtime stack and is also bound to implicit `self`.

Object field storage is hybrid:

- Declared fields use slots when possible.
- Runtime-added fields use shape revisions and/or extension dictionaries.
- The VM can optimize common field access while preserving open-class behavior.

## Errors And Results

Expected application failures use a single `Result` object on the stack.

```forth
42 User find_record
dup ok? if
  value
else
  error "message" at println
end
```

Runtime faults are separate from expected failures. Stack underflow, invalid bytecode, undefined dispatch, illegal capability access, or template stack mismatch are VM faults.

Fault policy:

- CLI main fault: crash process.
- Spawned background task fault: crash process by default.
- Ordinary web request fault: fail that request with HTTP 500 and log the fault.
- Planned debugger work: pause at request fault before returning 500.

## Control Flow And Async

Control flow uses modern block words and universal `end`.

```forth
condition if
  ...
else
  ...
end

condition while
  ...
end

items each
  ...
end
```

`return`, `break`, and `continue` are v1 features.

The expression sequence immediately before `while` is the loop condition and is
re-executed before every iteration. `continue` jumps to that condition and
`break` exits the nearest enclosing loop. Both are compile errors outside a
loop. Core numeric words include checked integer `add`/`+` and `subtract`/`-`
plus finite float promotion, which, together with mutable variables and
conditionals, provide a counter-machine execution model.

Ricochet supports async from v1. Futures/tasks are real stack values, and `await` is explicit.

```forth
User all-async
await
dup ok? if
  value
end
```

`spawn` is available from v1 and returns a task/future.

```forth
[ sendWelcomeEmail ] spawn
await
```

Multiple task handles can be awaited as a batch. Results are returned in input
order, and completed handles can be awaited again from their cached result.

```forth
handles array
[ sendWelcomeEmail ] spawn $handles push! drop
[ updateSearchIndex ] spawn $handles push! drop
$handles await_all
```

Current implementation note: `[ ... ] spawn` creates a first-class task value,
captures the spawn-time VM environment, and starts the task on a background
worker. `await` waits for one handle when needed, and `await_all` resolves an
array/list of handles. Task handles retain completed/failed status, expose
`id`, `task_status`, `pending?`, `running?`, `completed?`, and `failed?`, and
completed handles can be awaited again for the cached result. The `tasks` word
returns active running task metadata for debugger-style inspection. HTTP task
helpers should use underscore-separated public words and resolve through `await`
to the same `Result` maps as their synchronous counterparts. Richer suspended
task debugger views remain future work.

The debugger can inspect running, suspended, and failed tasks.

## Modules And Packages

Ricochet uses class-first project organization with explicit imports.

```forth
Web.Controller import
"Web.Controller" import
moduleName get import
```

Static symbol imports are normal. Strings and variables enable dynamic imports.

Package management is built into the CLI from v1, but dependencies start with Git/path sources rather than a central registry.

```bash
rco add github:owner/package@v0.1.0
rco add ./packages/ricochet_auth --as auth
rco install
```

`ricochet.toml` records dependency declarations. `ricochet.lock` pins exact commits/paths. A central registry can be added later as a resolver over the same package format.

Current implementation note: `rco add` supports local path dependencies and
GitHub shorthand, and `rco install` writes `ricochet.lock` entries for manifest
dependencies while fetching missing GitHub package caches when needed. Static
imports such as `"greeter/greeting" import` resolve through
`[dependencies.greeter]` when no relative file exists. Central registry
resolution and dynamic runtime imports remain future work.

## CLI And Project Tooling

The CLI should include:

```bash
rco new blog
rco new --with-sqlite beta_blog
rco run app.rco
rco repl
rco serve
rco serve --watch
rco test
rco build
rco doc
rco add github:owner/package@v0.1.0
rco install
```

`rco new PATH` creates a minimal MVC skeleton, not a large opinionated application.
`rco new --with-sqlite PATH` creates the same skeleton with a seeded
`db/development.sqlite3` and a `/users` controller path that exercises Active
Record for zero-service beta testing. It also adds `/login`, `/me`, and
`/logout` routes that demonstrate form params plus the session cookie.

The manifest decides the entry model:

```toml
[cli]
mode = "mvc"
default_controller = "MainController"
default_action = "index"

[web]
mode = "mvc"
routes = "config/routes.rco"
```

CLI apps can use an MVC-like command/controller/action framework.

## Web MVC

Web is part of the first serious milestone. Ricochet v1 is defined as a usable
web app beta target for other developers to test. It should let developers
scaffold, run, iterate on, and exercise a real MVC app locally with clear
failure modes; it is not a production hosting or security-hardening promise.
The v1 vertical slice is scoped as:

- Standalone HTTP serving via `rco serve`.
- MVC routing, controllers, models, and views.
- SQLite, PostgreSQL, or MySQL-backed Active Record against an existing schema.
- Plain HTML templates with full Ricochet interpolation.
- Capability-first request context.

CGI/FastCGI deployment adapters are deferred beyond the local v1 beta target.

Routes are Ricochet code in `config/routes.rco`.

```forth
GET "/users" UserController "index" route
POST "/users" UserController "create" route
GET "/users/:id" UserController "show" route
```

Route params live in the request context. With explicit controller Args, bind the
request map and use container-before-key access such as
`$request "params" at "id" at`. If a controller action declares Args, route
dispatch may also push matching params onto the stack in declared order.

Controller context binding is framework-configurable. The recommended default is both stack and variable binding:

```toml
[web.controllers]
bind_ctx = true
push_ctx = true
```

Views are plain HTML templates by default, with interpolation that runs Ricochet code. Each interpolation must leave exactly one renderable value.

```html
<h1>{ $title }</h1>
<p>{ $user displayName }</p>
<small>{ 20 22 + }</small>
```

Controller variables retain their Ricochet values when passed to a view, so
template expressions can navigate maps and objects rather than receiving only
pre-rendered strings. Interpolation is compiled to ordinary bytecode, and VM
faults or extra stack results fail the request loudly.

Escaping is configurable per project. Generated web apps should default to safe HTML escaping, with an explicit raw-output word for trusted HTML.

```toml
[web.views]
escape = "html"
```

Authentication is not built into core v1. Core web provides capability primitives such as request, response, cookies, session, db, logger, view, and config. Higher-level auth should live in packages.

Current implementation note: controllers receive `request`, `cookies`,
`session`, `logger`, and `config` values through `ctx` and through declared
Args. `request` includes method, path, params, query, form, JSON/body values,
multipart uploads, files, headers, cookies, and the session map. For `POST`,
`PUT`, `PATCH`, and `DELETE`, MVC parses
`application/x-www-form-urlencoded`, `application/json`, and
`multipart/form-data` bodies. Declared Args bind route params first, then form
fields, JSON object fields, upload fields, query params, and context values.
Upload maps include `name`, `filename`, `content_type`, `size`, UTF-8 `text`
when available, and `data_base64` for arbitrary bytes. Request body parsing is
currently in-memory with a 16 MiB beta limit; streaming upload APIs remain
future work. `logger` supports `debug`, `info`, `warn`, `error`, and `entries`
for per-request log inspection. `config` is derived from safe manifest metadata
such as package name, web mode/routes/views, and database adapter; database URLs
and session signing/encryption secrets are intentionally not exposed. The built-in session map is stored in a cookie named
`ricochet_session`; default beta sessions are HMAC-signed with a per-process
ephemeral key, `[web.session] signing_secret_env` enables stable HMAC signing
with a secret from the environment, and `[web.session] encryption_secret_env`
emits authenticated encrypted v2 cookies. Session cookies use `Secure`
automatically for non-local requests unless the manifest explicitly opts out for
local development.
The SQLite beta scaffold includes a small form/session login loop so developers
can exercise ordinary session flow locally; production auth remains package
work.

## Active Record

SQLite, PostgreSQL, and MySQL/MariaDB are the v1 beta database targets. SQLite
gives beta testers a zero-service local app path, PostgreSQL keeps the
production-style relational target visible, and MySQL/MariaDB keeps the beta
credible for developers bringing established web stacks or hosted MySQL apps.
Remote PostgreSQL connections require TLS by default; `sslmode=disable` is
accepted only for `localhost` or loopback development databases.

SQL migrations are part of the v1 beta. `rco migrate status` and
`rco migrate apply` read ordered `.sql` files from `db/migrations`, apply them
to SQLite, PostgreSQL, or MySQL/MariaDB projects, and record applied versions in
`schema_migrations`. Active Record still maps model declarations to existing
tables; migration rollback, schema dumps, seed commands, and a Ricochet-native
migration DSL remain future polish.

```forth
User Model Subclass
  "users" Table
  "id" Accessor
  "email" Accessor
  "name" Accessor
end
```

Table mapping follows the static/dynamic declaration pattern:

```forth
"users" Table
$tableName Table
```

Active Record operations are class methods with ordinary postfix arguments:

```forth
User all
User default_page
42 User find_record
"email" "ada@example.com" User where
10 User limit
10 20 User page
"email" "asc" 10 20 User order_page
"email" "ada@example.com" 10 User where_limit
"email" "ada@example.com" 10 20 User where_page
"email" "ada@example.com" "id" "desc" 10 20 User where_order_page
attributes map
$attributes "email" "ada@example.com" put! drop
$attributes User insert
updates map
$updates "email" "grace@example.com" put! drop
42 $updates User update
```

Active Record v1 supports connection configuration, basic table mapping, `find_record`,
`all`, `where`, a first-page list default with `default_page`, bounded reads
with `limit`/`page`/`where_limit`/`where_page`, deterministic ordered reads
with `order_page`/`where_order_page`, `count_records`, `first_record`,
`exists?`, `insert`, and `update`. `default_page` returns up to 50 rows and orders by `id asc` when the
model maps an `id` field; models without a mapped `id` field use the first
bounded page. Every operation returns one
`Result` object so expected database failures stay in the normal stack flow.

## Capabilities

Ricochet uses capability-first side effects. Code receives external powers as objects through context or explicit injection rather than magical global IO.

Typical web context capabilities:

```text
request
response
cookies
session
db
logger
view
config
```

Current implementation includes `request`, `cookies`, `session`, `logger`,
`config`, `db` when a database backend is configured, and response construction
through `view`, `text`, `json`, `redirect`, `status`, and `header` words.
CLI script execution enables filesystem, HTTP, and webview host capabilities for
trusted local scripts by default, and `rco repl`, `rco run`, `rco run-bytecode`,
and `rco test` accept `--capability-profile trusted|sandboxed`. `trusted`
preserves the local-script default; `sandboxed` starts with filesystem, HTTP,
and webview disabled unless `--fs-root <path>`, `--http-allow-host <host>`, or
`--allow-webview` opens specific access. The same commands accept `--no-fs`,
`--no-http`, and `--no-webview` to deny capabilities explicitly, and
`--fs-readonly` to allow reads while denying writes and directory creation. V1
beta keeps `trusted` as the local-development default and uses `sandboxed` as
the policy for untrusted examples, package review, bug repros, and third-party
code.

Desktop UI starts as a webview capability. The current core words build escaped
HTML fragments and a document map that a desktop webview host can consume:

```forth
"Counter" 1 webview_heading heading var
"Increment" "increment" webview_button button var
$heading $button concat body var
"Counter" $body webview_window value document var
```

`webview_window` and `webview_window_state` return a `Result` whose ok value contains
`type = "webview"`, `title`, raw `body`, full `html`, and default `width` and
`height` fields. `rco gui` can preview these documents through the platform
system browser, and `rco package --gui` embeds the app into the dedicated
`rco-gui` launcher. The launcher host targets Windows, Linux, and macOS without
pulling in Wry/Tao or the GTK/WebKitGTK Rust binding stack. The VM surface stays
portable and testable while GUI host details live in the CLI layer.

The standard library can include broad pure/common utilities, but dangerous or environment-dependent effects should flow through capability objects where practical.

## AI Package Direction

AI support is not core language v1, but it should be a first-party package direction. The core should provide the pieces AI packages need: `Result`, JSON, schemas/validation, HTTP capability/client, configuration, env support, and capability injection.

The first-party AI package should expose provider-agnostic stack words through an `ai` capability. The default adapter should support the OpenAI/OpenAI-compatible API shape.

```toml
[ai.default]
provider = "openai"
model = "gpt-4.1-mini"
api_key = "${OPENAI_API_KEY}"
```

Current implementation note: MVC apps can configure `[ai.default]` and receive
an `ai` controller capability. `$ai chat` posts to an OpenAI-compatible
`/chat/completions` endpoint and returns a `Result` whose ok value is a map with
`provider`, `model`, and `text`.

Example Ricochet shape:

```forth
ai var
"Extract name, email, and priority" $ai chat result var
$result ok? if
  $result value "text" at
else
  $result error "message" at
end
```

AI calls return `Result` objects today. The first-party AI package includes
OpenAI-compatible request builders, bounded SSE response-body parsing, and can
pair with retained core `http_stream_start` / `http_stream_read` jobs for
long-running streams. Richer provider packages remain future work.

## Debugger

Debugging is a central product feature.

CLI surfaces:

```bash
rco run app.rco --debug
rco test --debug
rco serve --debug
```

Debugger features from v1:

- Terminal step, next, out, continue, abort, and pause controls.
- Stack inspection.
- Frame inspection.
- `self`, `ctx`, variables, objects, classes, words.
- Source line, method, and word breakpoints.
- Fault events in debug traces; pausing MVC requests before returning HTTP 500
  remains future debugger polish.
- Configurable stack display: concise diff/top-N by default, full stack on request.
- Basic task inspection for async/spawned work is implemented, including
  running/completed/failed handle status; richer suspended task debugger views
  remain future work.
- JSON trace files, `rco debug --json`, `rco debug-adapter` over stdio DAP,
  and VS Code trace/live-stack panels.

The terminal debugger, JSON event stream, DAP bridge, and VS Code integration
are implemented. Dedicated TUI or browser debugger UIs can consume the same VM
events later without replacing the shared protocol surface.

## Hot Reload And REPL

`rco serve --watch` is a v1 feature. Web requests use stable revision snapshots:

- Active requests keep running on the code revision they started with.
- New requests use the newest successfully reloaded revision.
- Reload events are visible in debug traces.

Current implementation note: `rco serve --watch` reloads the MVC runtime between
requests when `ricochet.toml`, `app/**/*.rco`, `app/**/*.html`, or
`config/**/*.rco` changes. Routes, controllers, models, views, and manifest
view settings are rebuilt together. If a reload fails, the request returns a
clear MVC error and the next request retries after the source is fixed.
`rco serve --debug --watch` prints reload trace lines with the new revision and
changed files, and embedders can capture `WatchTraceEvent` reload events from
watched app builders.

The REPL is a live metaprogramming workspace from v1. It supports redefining classes/methods, open classes, live stack inspection, multiline declarations, and optional debug tracing.

REPL state is ephemeral by default. Future versions may support saving bytecode images and emitting source stubs from live definitions, but that is not v1.

## Tests And Docs

Tests are Ricochet classes/methods integrated with project layout.

```forth
UserTest TestCase Subclass
  [
    User new user var
    "a@example.com" user email.set
    user displayName
    "a@example.com" assert_equals
  ] "testDisplayName" Method
end
```

`rco test --debug` runs tests in the same bytecode VM with stack-aware debugging.

`rco doc` is a v1 feature. It generates documentation from classes, fields, methods, functions/words, Args metadata, table mappings, package metadata, and preceding `(( ... ))` doc comments.

Current implementation note: `rco doc [path]` emits Markdown for `.rco` files,
directories, or projects. It includes class inheritance, table mappings, fields,
methods, functions, Args metadata, and preceding doc comments. Full package
metadata and fully generated built-in reference pages remain future work;
`rco words [--json] [--check]` now exposes the embedded editor/LSP inventory and
validates the checked-in reference catalog against the TextMate grammar.

## First Implementation Milestone

The first milestone is a thin but complete Web MVC slice in Rust:

1. Lex/parse enough `.rco` for declarations, functions/methods, variables, stack words, comments, strings, Args, blocks, and control flow.
2. Compile to bytecode with debug metadata.
3. Run bytecode in a stack VM with objects, classes, open classes, fields, methods, `Result`, and `nil`.
4. Provide `rco run`, `rco repl`, `rco build`, `rco serve`, and `rco test` at a usable early level.
5. Serve one MVC app end-to-end: route, controller, SQLite/PostgreSQL/MySQL Active Record model against existing schema, HTML template interpolation, and HTTP response.
6. Support `--debug`, breakpoints, stack traces, JSON trace/debug streams, DAP,
   and editor debugger integration for that vertical slice; request-fault
   pausing before HTTP 500 remains future debugger polish.
7. Support hot reload with stable request revision snapshots.

## Deferred Features

- Central package registry.
- Migration rollback, schema dump/seed workflows, and a Ricochet migration DSL.
- Template embedded script blocks beyond interpolation.
- General compile-time macros.
- Persistent REPL images and source-like bytecode emission are implemented
  beta runtime surfaces.
- Dedicated TUI/browser debugger UI beyond the terminal, DAP, and VS Code
  integration.
- First-party AI package implementation, unless it proves small enough to include after the main MVC slice.
- Production-grade app distribution polish on top of `rco package`, such as
  signing, notarization, app metadata, and platform store/update workflows.

## References

- Fuzzball MUCK docs: https://fuzzball-muck.github.io/fuzzball/
- Fuzzball MUF manual: https://fuzzball-muck.github.io/fuzzball/mufman.html
- MUCK manual, programming overview: https://www.rdwarf.com/users/mink/muckman/programming.html
