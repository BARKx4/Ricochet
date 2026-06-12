# Ricochet Language Design

Date: 2026-06-09
Status: Draft for review

## Purpose

Ricochet is a modern, pure-postfix, stack-based programming language descended in spirit from MUF/MUCK-era Multi-User Forth. It is designed for people who still think in the stack, but it should be useful outside games: full CLI applications, server-side web scripting, MVC web apps, live debugging, and dynamic runtime metaprogramming.

The first serious milestone is a Web MVC vertical slice, not a tiny calculator VM. The language should prove itself by serving a real PostgreSQL-backed MVC page with stack-aware debugging.

## Design Goals

- Preserve postfix stack thinking as the primary execution model.
- Use class-based OOP without turning the language into C#/Java with Forth punctuation.
- Make declaration syntax postfix wherever feasible: declaration name first, declaration operator second.
- Support both compile-time declarations and runtime dynamic declarations.
- Make debugging a first-class feature: realtime stack trace, breakpoints, break-on-fault, task visibility, and source-aware bytecode metadata.
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
count get 10 < if
  "small" println
end
```

The implementation target is a Rust bytecode VM. Source compiles to bytecode with source maps, frame metadata, declaration metadata, and optional debug information. Development builds preserve rich debug metadata; release profiles can strip or minimize it.

The VM has two distinct stacks:

- Compile-time stack: used by declaration words such as `subclass`, `field`, `method`, `function`, `var`, and `import`.
- Runtime stack: used by ordinary program execution.

This split is explicit in the language model. Declaration words are real compile-time stack operations, not decorative keywords.

## Syntax Principles

Declaration syntax should follow:

```text
<declaration value/name> <declaration operator>
```

Examples:

```forth
User Model subclass
name field
amount var
displayName method
  self .name get
end
```

Bare declaration names are static compile-time symbols. Strings and variables can be used for dynamic declarations.

```forth
User Model subclass        (( static symbol ))
"User" Model subclass      (( literal string ))
className get Model subclass
```

Single-character variable operators are not part of canonical v1. Ricochet uses readable postfix access words:

```forth
amount var
100 amount set
amount get println

user .email get
"a@example.com" user .email set
```

Collection mutation is marked with receiver-style bang methods. Mutators return
the same collection reference so they can be chained or dropped explicitly:

```forth
"a@example.com" users get .push! drop
"theme" "dark" config get .put! drop
```

The older stack words `!push` and `!put` remain compatibility aliases, but
`.push!` and `.put!` are canonical for new code.

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

Comments use `(( ... ))`. A comment immediately preceding a declaration becomes that declaration's docstring.

```forth
(( Represents a user account. ))
User Model subclass
end
```

## Blocks, Functions, Methods, And Args

`end` closes declaration bodies and control-flow blocks.

```forth
displayName method
  self .name get empty? if
    self .email get
  else
    self .name get
  end
end
```

Functions and methods may include an optional `Args` object. Parentheses build an `Args` object on the compile-time stack.

```forth
( amount target -> Result ) transfer method
  amount var
  target var

  target set
  amount set

  self amount get target get .transferTo
end
```

Args follow Forth stack-effect convention: left-to-right is stack bottom-to-top, so the rightmost input is at the top of the stack. Args are metadata only. They do not automatically bind variables; code must explicitly capture stack values into variables.

Anonymous executable blocks use square brackets and are first-class closures from v1.

```forth
"index" [
  ctx get
  "home/index" swap view
] !method
```

Blocks capture surrounding variables and carry debug metadata.

## Variables And Values

Variables are declared with postfix `var`.

```forth
amount var
100 amount set
amount get
```

Ricochet uses canonical `nil` for absence. JSON `null` and PostgreSQL `NULL` map to/from Ricochet `nil`.

Truthiness is dynamic-language style: `false`, `nil`, numeric zero, empty strings, and empty collections are falsey. `Result` values do not participate in truthiness; callers must use `ok?` explicitly.

```forth
user save
dup ok? if
  value
else
  error .message println
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

## OOP And Metaprogramming

Ricochet uses class-based OOP with open classes. Normal classes can be declared at compile time:

```forth
User Model subclass
  users table
  id field
  email field
  name field

  displayName method
    self .name get empty? if
      self .email get
    else
      self .name get
    end
  end
end
```

Dynamic class creation is available from v1 and is central to the language's identity.

```forth
className get "Model" subclass
className get "dynamic_table" table
className get "name" field
className get "displayName" [ self .name get ] !method
```

The runtime `subclass` operator consumes a string class name and a superclass
class value or string. It creates or reopens the class without requiring a
compile-time declaration. Outside a lexical class body, `table` and `field`
accept an explicit class target before the declaration name.

Open classes can add or freely replace methods at runtime.

```forth
User open-class
  displayName method
    self .email get
  end
end
```

The same mutation is available as a pure stack operation without a lexical
class body. This is the dynamic form that enables names from variables:

```forth
"User" className var
"displayName" methodName var

className get methodName get [ self .email get ] !method
className get new .displayName
```

The target may be a class value such as `User` or a class-name string. Runtime
declaration faults preserve their operands on the stack for debugger inspection.

Subclass instances include fields declared by their ancestors. Method lookup
walks from the concrete class toward its ancestors, so the nearest declaration
wins even when an override changes between bytecode and a native method.
Class-level native methods are inherited as well, while retaining the concrete
child class as their receiver. Inheritance cycles are VM faults.

Method replacement is allowed without warnings, but the VM records replacement metadata for debugging and hot reload traces.

Method calls use an explicit dot form to avoid ambiguity with global words:

```forth
user .save
user .displayName
user "displayName" send
```

Loaded class names are also runtime values. This gives class-level APIs the same
postfix dispatch shape as instance APIs:

```forth
User .all
42 User .find
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
42 User .find
dup ok? if
  value
else
  error .message println
end
```

Runtime faults are separate from expected failures. Stack underflow, invalid bytecode, undefined dispatch, illegal capability access, or template stack mismatch are VM faults.

Fault policy:

- CLI main fault: crash process.
- Spawned background task fault: crash process by default.
- Ordinary web request fault: fail that request with HTTP 500 and log the fault.
- `rco serve --debug --break-on-fault`: pause at request fault before returning 500.

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
loop. Core numeric words include checked `add`/`+` and `subtract`/`-`, which,
together with mutable variables and conditionals, provide a counter-machine
execution model.

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

Current implementation note: `[ ... ] spawn` creates a first-class task value
that captures the spawn-time VM environment; `await` resolves it explicitly.
Task handles retain completed/failed status after `await`, expose `.id`,
`.status`, `.pending?`, `.completed?`, and `.failed?`, and completed handles
can be awaited again for the cached result. The `tasks` word returns pending
task metadata for debugger-style inspection. Scheduler-level concurrency and
async IO words remain future work.

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
rco add ./packages/local-auth
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
rco new web blog
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

`rco new web` creates a minimal MVC skeleton, not a large opinionated application.

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

Web is part of the first serious milestone. The v1 vertical slice should support:

- Standalone HTTP serving via `rco serve`.
- CGI/FastCGI deployment path.
- MVC routing, controllers, models, and views.
- PostgreSQL-backed Active Record against an existing schema.
- Plain HTML templates with full Ricochet interpolation.
- Capability-first request context.

Routes are Ricochet code in `config/routes.rco`.

```forth
GET "/users" UserController "index" route
POST "/users" UserController "create" route
GET "/users/:id" UserController "show" route
```

Route params live in `ctx request params`. In the current implementation this is
available as `ctx get .request get .params get`, with compatibility shortcuts
such as `ctx get .params get`. If a controller action declares Args, route
dispatch may also push matching params onto the stack in declared order.

Controller context binding is framework-configurable. The recommended default is both stack and variable binding:

```toml
[web.controllers]
bind_ctx = true
push_ctx = true
```

Views are plain HTML templates by default, with interpolation that runs Ricochet code. Each interpolation must leave exactly one renderable value.

```html
<h1>{ title get }</h1>
<p>{ user get .displayName }</p>
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
Args. `request` includes method, path, params, query, form, headers, cookies,
and the session map. `logger` supports `debug`, `info`, `warn`, `error`, and
`entries` for per-request log inspection. `config` is derived from safe
manifest metadata such as package name, web mode/routes/views, and database
adapter; database URLs are intentionally not exposed. The built-in session map
is stored in an unsigned JSON cookie named `ricochet_session`; signing,
encryption, and higher-level auth remain package work.

## Active Record

PostgreSQL is the first database target. SQLite can come later as a convenience adapter, but Ricochet's first web framework should target real PostgreSQL semantics.

Migrations are not v1. v1 maps models to an existing schema.

```forth
User Model subclass
  users table
  id field
  email field
  name field
end
```

Table mapping follows the static/dynamic declaration pattern:

```forth
users table
"users" table
tableName get table
```

Active Record operations are class methods with ordinary postfix arguments:

```forth
User .all
42 User .find
"email" "ada@example.com" User .where
attributes map
"email" "ada@example.com" attributes get .put! drop
attributes get User .insert
updates map
"email" "grace@example.com" updates get .put! drop
42 updates get User .update
```

Active Record v1 supports connection configuration, basic table mapping, `find`,
`all`, `where`, `insert`, and `update`. Every operation returns one `Result`
object so expected database failures stay in the normal stack flow.

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
CLI script execution enables filesystem and HTTP host capabilities for trusted
local scripts by default, and `rco repl`, `rco run`, `rco run-bytecode`, and
`rco test` accept `--no-fs` and `--no-http` to deny those capabilities for a
specific execution. The same commands accept `--fs-root <path>` to bound
filesystem access to a directory and `--fs-readonly` to allow reads while
denying writes and directory creation. `--http-allow-host <host>` can be
repeated to restrict HTTP requests to specific hosts. Default-off capability
policy and named host-policy profiles remain future work.

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

Example Ricochet shape:

```forth
ctx get ai
"Extract name, email, and priority" prompt
TicketSchema schema
complete-json
```

AI calls should return `Result` objects and support structured JSON/schema validation.

## Debugger

Debugging is a central product feature.

CLI surfaces:

```bash
rco run app.rco --debug
rco test --debug
rco serve --debug
rco serve --debug --break-on-fault
rco serve --debug --full-stack
```

Debugger features from v1:

- Step, next, continue, abort.
- Stack inspection.
- Frame inspection.
- `self`, `ctx`, variables, objects, classes, words.
- Source line, method, and word breakpoints.
- Break-on-fault.
- Configurable stack display: concise diff/top-N by default, full stack on request.
- Basic task inspection for async/spawned work is implemented, including
  retained completed/failed handle status; richer running and suspended task
  debugger views remain future work.

The terminal debugger is first, but the debugger should be protocol/event-stream based so a TUI or browser debugger can consume the same VM events later.

## Hot Reload And REPL

`rco serve --watch` is a v1 feature. Web requests use stable revision snapshots:

- Active requests keep running on the code revision they started with.
- New requests use the newest successfully reloaded revision.
- Reload events are visible in debug traces.

Current implementation note: `rco serve --watch` reloads the MVC runtime between
requests when `ricochet.toml`, `app/**/*.rco`, `app/**/*.html`, or
`config/**/*.rco` changes. Routes, controllers, models, views, and manifest
view settings are rebuilt together. If a reload fails, the request returns a
clear MVC error and the next request retries after the source is fixed; debug
trace events for reloads remain future work.

The REPL is a live metaprogramming workspace from v1. It supports redefining classes/methods, open classes, live stack inspection, multiline declarations, and optional debug tracing.

REPL state is ephemeral by default. Future versions may support saving bytecode images and emitting source stubs from live definitions, but that is not v1.

## Tests And Docs

Tests are Ricochet classes/methods integrated with project layout.

```forth
UserTest TestCase subclass
  testDisplayName method
    user var
    User new
    user set
    "a@example.com" user get .email set
    user get .displayName
    "a@example.com" assert-equals
  end
end
```

`rco test --debug` runs tests in the same bytecode VM with stack-aware debugging.

`rco doc` is a v1 feature. It generates documentation from classes, fields, methods, functions/words, Args metadata, table mappings, package metadata, and preceding `(( ... ))` doc comments.

Current implementation note: `rco doc [path]` emits Markdown for `.rco` files,
directories, or projects. It includes class inheritance, table mappings, fields,
methods, functions, Args metadata, and preceding doc comments. Full package
metadata and built-in word reference generation remain future work.

## First Implementation Milestone

The first milestone is a thin but complete Web MVC slice in Rust:

1. Lex/parse enough `.rco` for declarations, functions/methods, variables, stack words, comments, strings, Args, blocks, and control flow.
2. Compile to bytecode with debug metadata.
3. Run bytecode in a stack VM with objects, classes, open classes, fields, methods, `Result`, and `nil`.
4. Provide `rco run`, `rco repl`, `rco build`, `rco serve`, and `rco test` at a usable early level.
5. Serve one MVC app end-to-end: route, controller, PostgreSQL Active Record model against existing schema, HTML template interpolation, and HTTP response.
6. Support `--debug`, breakpoints, stack trace, and break-on-fault for that vertical slice.
7. Support hot reload with stable request revision snapshots.

## Deferred Features

- Central package registry.
- Migrations in SQL and Ricochet DSL.
- SQLite adapter.
- Template embedded script blocks beyond interpolation.
- General compile-time macros.
- Persistent REPL images and source emission.
- TUI/browser debugger UI.
- First-party AI package implementation, unless it proves small enough to include after the main MVC slice.
- Native executable packaging that bundles VM plus `.rcob`.

## References

- Fuzzball MUCK docs: https://fuzzball-muck.github.io/fuzzball/
- Fuzzball MUF manual: https://fuzzball-muck.github.io/fuzzball/mufman.html
- MUCK manual, programming overview: https://www.rdwarf.com/users/mink/muckman/programming.html
