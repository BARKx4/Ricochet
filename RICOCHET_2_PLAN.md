# Ricochet 2 Product Charter and Execution Roadmap

Status: architecture baseline

Owner decision date: 2026-08-08

Target release: Ricochet 2.0.0

This document supersedes the earlier phase-by-feature Ricochet 2 roadmap. It is
the authority for the 1.x LTS boundary, the incompatible 2.x product, the
architecture decisions that must precede implementation, and the gates used to
decide whether Ricochet 2 is ready.

## 1. Product decision

Ricochet 1.x and Ricochet 2.x are separate product lines with shared ancestry,
not two dialects of one language.

| Line | Product | Canonical tool | Branch | Release tags | Compatibility promise |
| --- | --- | --- | --- | --- | --- |
| 1.x | Ricochet | `rco` | `main` | `v1.0.x` | Existing 1.x source and artifact contracts are maintained by patch releases. |
| 2.x | Ricochet | `ricochet` | `ricochet-2` | `ricochet-v2.*` | No source, bytecode, image, manifest, lockfile, package, plugin, or runtime compatibility with 1.x. |

The 1.0 source and published artifacts already contain a secondary `ricochet`
launcher. It is a legacy 1.x alias, not the canonical 1.x tool. It remains
untouched until ADR-010 defines a non-destructive installer handoff. New 1.x
documentation uses `rco`; Ricochet 2 owns `ricochet` and does not add an `rco`
alias. Before the first 2.x alpha, side-by-side installation must prove that
`rco` resolves to 1.x and `ricochet` resolves to 2.x without removing an
existing user's 1.0 files.

The existing 1.x website remains version-pinned while Ricochet 2 is developed.
Ricochet 2 receives version-separated installer metadata, documentation,
package namespace, update channels, configuration, and release artifacts
before its first public alpha.

## 2. Product charter

Ricochet 2 is an application-first, statically safe, pure-postfix language for
people who might otherwise choose Python, Ruby, Node.js, or PHP to build a CLI,
web application, API, background worker, automation, or native integration.

Ricochet succeeds when a developer can:

1. install one toolchain and create a project in minutes;
2. write concise postfix code without pervasive type ceremony;
3. receive compile-time protection for types, stack effects, nullability,
   resources, concurrency boundaries, and declared effects;
4. use a complete local loop: check, run, test, format, lint, debug, profile,
   document, package, and deploy;
5. build ordinary applications with HTTP, JSON, databases, configuration,
   secrets, logging, metrics, and tracing without leaving the supported
   ecosystem;
6. cross native boundaries deliberately through a small auditable unsafe
   surface; and
7. understand the language from public documentation without private
   implementation guidance.

Ricochet is not required to replace Rust or C for kernels, hard real-time code,
or allocation-free embedded firmware. It is also not required to operate a
distributed database, message broker, or orchestration cluster as part of the
language runtime.

## 3. Non-negotiable design principles

### 3.1 Postfix remains the identity

- Runtime evaluation remains pure postfix/RPN.
- Receivers precede selectors.
- Selector arguments remain below the receiver.
- Containers precede keys and values for access and mutation.
- Public multiword names use `_`; `-` remains subtraction and a numeric sign.
- Block structures use lowercase `end`.
- Type syntax may use punctuation inside declarations, but it must not turn
  ordinary executable code into infix or leading-dot syntax.

Every proposed surface must pass a written postfix-vibe review with realistic
examples before implementation.

### 3.2 Safety with inference, not annotation everywhere

- All executable code is statically checked unless it crosses an explicit
  `Dynamic` or `unsafe` boundary.
- Local bindings and private implementation details are inferred when the
  compiler has enough information.
- Public module APIs, exported types, FFI boundaries, and persistent schemas
  have explicit signatures.
- Stack effects are verified for every reachable control-flow path whether or
  not the programmer writes the inferred effect.
- There is no ambient `null`. Absence is `Option<T>`; foreign null pointers are
  confined to unsafe FFI code.

### 3.3 Managed application code by default

- Ordinary objects and collections use managed memory.
- Garbage-collection timing is not observable language behavior.
- Finalizers are not a resource-management mechanism.
- Files, sockets, database transactions, locks, foreign buffers, and similar
  resources use deterministic scopes and move-only or affine handles.
- Ricochet does not impose a pervasive Rust-style borrow checker on ordinary
  application code.
- Raw pointers, manual allocation, volatile access, and foreign layout are
  available only inside an explicit unsafe effect and module boundary.

### 3.4 Structured concurrency by default

- Every ordinary task belongs to a scope.
- Leaving a scope joins, cancels, or explicitly transfers every child task.
- Cancellation, deadlines, bounded queues, and backpressure are designed with
  the first async release, not retrofitted later.
- Immutable values cross task boundaries freely. Mutable sharing requires
  typed synchronization and compiler-checked send/share rules.
- Detached process-lifetime work is explicit and uncommon.

### 3.5 Application completeness is a release criterion

A language feature is incomplete until its compiler behavior, runtime
behavior, diagnostics, formatter, LSP, debugger, tests, examples, and public
documentation agree. A usable language is the entire development loop, not
only a parser and VM.

### 3.6 Evidence selects infrastructure

No native backend, garbage collector, async executor, object representation,
or package protocol is frozen by preference alone. Each receives an ADR and a
representative prototype. The choice is made from correctness, implementation
cost, compile time, runtime performance, debug information, platform coverage,
and maintenance evidence.

## 4. Ricochet 1.x LTS contract

### 4.1 Allowed work

Ricochet 1.x accepts only:

- correctness fixes for documented behavior;
- security fixes and dependency updates required for a supported security
  boundary;
- crash, data-loss, packaging, installer, CI, and supported-platform repairs;
- documentation corrections that describe existing behavior; and
- test repairs that remove nondeterminism without changing the public
  contract.

All 1.x releases are `1.0.x` patch releases. There is no planned `1.1.0`.

### 4.2 Disallowed work

Ricochet 1.x does not accept:

- new syntax, public words, commands, packages, targets, or language features;
- semantic redesigns disguised as fixes;
- Ricochet compatibility modes, parsers, migrations, or artifact loaders;
- changes made only to make 1.x internals resemble Ricochet; or
- broad dependency churn without a maintenance reason.

When the classification is ambiguous, the change does not enter 1.x until it
has a minimal reproduction and a written explanation of the violated 1.x
contract.

### 4.3 Support horizon

Ricochet 1.x remains supported through Ricochet 2.0.0 general availability and
for at least twelve months afterward. Any end-of-life date is announced at
least ninety days in advance. LTS is a maintenance-scope promise, not a paid
response-time SLA; support remains best effort.

### 4.4 Branch and release discipline

- `main` remains the protected Ricochet 1.x LTS branch during Ricochet
  development.
- Ricochet fixes are developed from `main`, verified against the 1.x suite,
  and tagged `v1.0.x`.
- `ricochet-2` is independently protected and never merged wholesale into
  `main`.
- A fix may be applied to both lines only after separate review in each
  architecture. Blind cherry-picks are prohibited.
- GitHub's default branch may move to `ricochet-2` at Ricochet beta or GA, but that
  administrative change does not merge or delete either history and requires
  an explicit owner decision.

## 5. Ricochet identity and artifact boundary

The following names are the initial 2.x contract. Any alteration requires an
ADR before the first public alpha.

| Surface | Ricochet 2 identity |
| --- | --- |
| Toolchain executable | `ricochet` |
| Source extension | `.ricochet` |
| Project manifest | `ricochet.toml` with a required v2 ecosystem marker |
| Lockfile | `ricochet.lock` with a v2-only schema |
| First-party package scope | `@ricochet2/*` in the v2 registry |
| Environment prefix | `RICOCHET2_` |
| Configuration and cache root | platform Ricochet directory followed by a `2` version segment |
| Project environment | `.rvenv`, disposable and reconstructed from `ricochet.lock` |
| Bytecode container | `.ricbc`, with v2 magic and schema; no 1.x loader |
| VM image, if retained | `.ricimg`, with a v2-only schema; no 1.x loader |
| Package archive | `.ricpkg`, with a v2-only schema and integrity record |
| Release artifacts | `ricochet-v2.<semver>-<target>.<format>` and v2-specific metadata names |
| Update channels | v2-specific stable, candidate, and nightly documents |
| Installer/application identity | Ricochet 2, isolated from installed Ricochet 1.x |

Source, bytecode, images, manifests, lockfiles, registry records, package
archives, native plugins, environment variables, configuration directories,
and update channels carry explicit v2 identity. Binary artifact kinds are
distinguishable before deserialization. Ricochet 2 and Ricochet 1.x must be
installable side by side through their canonical `ricochet` and `rco` tools.

There is no `migrate-v1` requirement. A future standalone conversion assistant
may be built if it is useful, but it cannot constrain Ricochet's design, is not
part of the compiler, and is not a 2.0 release gate.

## 6. Required language model

### 6.1 Core types

The GA language includes:

- `Bool`, `Char`, UTF-8 `String`, `Bytes`, unit, and never/bottom;
- fixed-width signed and unsigned integers, pointer-sized integers, `F32`, and
  `F64`;
- library-provided arbitrary-precision integer and decimal types suitable for
  identifiers, finance, and data interchange;
- tuples, nominal records/structs, enums/sum types, and managed classes;
- arrays, lists, maps, sets, slices/views, iterators, ranges, and generators;
- function, closure, task, channel, result, option, and resource types; and
- explicit `Dynamic` for untyped external data.

Numeric and string conversions are explicit. The compiler does not silently
coerce an arbitrary value because an operator happens to accept another type.

### 6.2 Static type checking

- Type checking is sound outside declared `Dynamic` and `unsafe` boundaries.
- Local inference is bidirectional and does not infer public APIs across module
  boundaries.
- Flow analysis narrows enums, options, results, and checked dynamic values.
- Generic functions and types support ordinary first-order type parameters,
  trait bounds, associated types, and variance rules where required.
- Package coherence prevents conflicting trait implementations. An
  implementation is legal only when the defining package owns the trait or the
  implementing nominal type.
- Higher-kinded types are not a 2.0 requirement.

### 6.3 Static stack effects

Every callable has a typed stack effect. The checker supports stack-row
polymorphism so ordinary stack combinators do not require a closed whole-stack
type. It verifies:

- underflow and output arity;
- input and output types;
- branch join equality;
- loop invariants;
- early returns and pattern arms;
- generic calls and trait dispatch;
- closure capture and multiple returns; and
- the boundary around dynamic dispatch.

The compiler may infer private effects, but published module interfaces contain
the complete checked signature. Verified bytecode records enough type and stack
metadata for the loader to reject malformed or incompatible artifacts.

### 6.4 Algebraic data and matching

- Enums carry typed payloads.
- Pattern matching covers enums, records, tuples, literals, options, and
  results.
- Matches over closed types are exhaustive, with unreachable arms diagnosed.
- Destructuring is available in bindings and parameters when it remains
  readable in postfix form.
- `Option<T>` is the only ordinary absence mechanism.
- `Result<T, E>` is the normal recoverable-error mechanism.

Expected failures do not require exceptions. A postfix propagation construct
may return early from a `Result`, while pattern matching remains the explicit
form. Panics represent violated invariants or unrecoverable task failure; they
are not the normal application error channel.

### 6.5 Effects and capabilities

Ricochet uses a deliberately bounded compile-time effect system rather than
general algebraic effect handlers in 2.0.

- Pure code has an empty effect set.
- Effects include async suspension, unsafe operations, and host capabilities
  such as filesystem, network, process, environment, database, time, random,
  and secrets.
- Local effects are inferred; exported effects are explicit in module
  interfaces.
- Calls may use only effects present in the caller or explicitly handled by a
  narrower capability object.
- Runtime capability grants still enforce what a process may do. Static
  effects describe intent; runtime capabilities enforce authority.

Purity analysis is therefore useful and concrete without adding user-defined
effect handlers or monadic syntax to the 2.0 critical path.

### 6.6 Modules and visibility

- Modules have explicit imports and exports, acyclic initialization, stable
  identities, and separate compilation.
- Public, package, module, protected, and private visibility are defined once
  and used consistently by types, fields, methods, and constructors.
- Public interfaces are emitted as compiler-readable metadata so dependents do
  not need dependency source for type checking.
- Cyclic package dependencies are rejected.
- Builds are reproducible from the manifest and lockfile.

### 6.7 Object and protocol model

Ricochet supports multiple styles without building two unrelated languages:

- records are nominal value-oriented data;
- enums model closed alternatives;
- classes provide managed reference identity and encapsulation;
- traits/interfaces provide shared behavior and constraints; and
- composition is preferred over inheritance.

Classes support at most one superclass and multiple traits. The model includes
constructors, explicit visibility, associated/static functions, abstract and
sealed/final forms, properties, and virtual dispatch. Multiple class
inheritance and C3 linearization are not 2.0 requirements.

Operator overloading is limited to named standard traits so operators retain
predictable meaning. Reflection exposes typed metadata for public names,
fields, methods, variants, attributes, and signatures; unrestricted mutation
of compiler metadata is not required.

## 7. Memory, resources, and unsafe code

### 7.1 Managed heap

The language contract specifies tracing or otherwise cycle-safe managed memory,
not a particular collector. The selected implementation must support cycles,
predictable pause goals for server workloads, heap inspection, and safe
interaction with async tasks and native calls.

Weak references, if added, receive explicit semantics. User-defined finalizers
are excluded from the initial contract because they make resource lifetime and
collector behavior observable.

### 7.2 Deterministic resources

- Resource-bearing values are move-only or affine.
- A lexical resource scope and `defer`-equivalent cleanup run on success,
  result propagation, cancellation, and panic unwinding at task boundaries.
- The compiler diagnoses use-after-move and resources that can escape without
  a valid owner.
- Ordinary records, classes, and collections are not burdened with borrow
  annotations.

### 7.3 Native boundary

The stable native surface includes:

- C imports, callbacks, and a documented host embedding API;
- explicit C-compatible layout and alignment;
- fixed-width types, byte buffers, slices, raw and nullable pointers;
- pinned managed references where a foreign API requires a stable address;
- manual allocation and deallocation primitives inside unsafe code;
- callback lifetime rules;
- foreign error and panic containment; and
- a versioned Ricochet plugin ABI built only from documented C-compatible
  types.

Direct compilation of exported Ricochet functions as a native C library is a
2.x backend milestone. It becomes a GA requirement only if the selected
execution backend can support it without destabilizing the application
toolchain; embedding the production VM through the stable C API satisfies the
2.0 interoperability gate.

Rust ABI compatibility is never promised. Unsafe operations remain visible in
signatures and compiler effects.

## 8. Concurrency model

The GA concurrency surface includes:

- language-level async callables and postfix `await` behavior;
- structured task scopes and typed `Task<T>` handles;
- cancellation tokens, deadlines, timeouts, and cancellation-safe cleanup;
- bounded typed channels with backpressure;
- multi-source wait/select;
- task-local values and context propagation;
- explicit blocking-task and CPU-task pools;
- `Mutex<T>`, `RwLock<T>`, condition/signal primitives, and standard atomic
  integer/boolean/reference operations;
- a documented memory model; and
- compiler-checked send/share traits for values that cross threads.

The implementation must not create one operating-system thread per async task.
The executor, timers, network reactor, and blocking pools are one runtime with
testable shutdown semantics.

Local actors may be a first-party package over tasks and channels. Distributed
actors, remote supervision trees, durable mailboxes, cluster membership, and an
OTP-equivalent platform are post-2.0 ecosystem work.

## 9. Compiler and runtime architecture

### 9.1 Required pipeline

```text
source
  -> lossless CST
  -> semantic AST
  -> resolved HIR
  -> typed HIR
  -> control-flow, resource, and effect MIR
  -> verified backend-neutral IR
  -> versioned bytecode and selected native/WASM backends
```

- The CST powers formatting and source-preserving tools.
- HIR owns names, modules, generics, traits, and desugaring.
- MIR owns control flow, stack shapes, moves, cleanup, effects, async lowering,
  and optimization-independent semantics.
- Backend IR cannot be emitted until verification succeeds.
- Stable source IDs and spans survive every stage for diagnostics, debugging,
  profiling, and coverage.

The compiler is organized as an incremental query database with explicit cache
keys. The same analysis services power `ricochet check`, builds, tests, docs, and
the language server; the LSP does not implement a second type checker.

### 9.2 Reference VM and backend selection

A versioned bytecode VM is the executable reference for language semantics and
the fastest bootstrap path. Bytecode files include magic, schema version,
compiler compatibility, target assumptions, feature flags, integrity data,
and source-map identity.

Native AOT/JIT is a product goal, not a preselected library. An early bakeoff
must compare at least the practical Cranelift and LLVM integration paths, plus
the option of retaining an optimized VM for some targets. The decision records:

- implementation and maintenance cost;
- cold and incremental compile time;
- representative application performance;
- Windows, Linux, and macOS support;
- unwind, debugger, profiler, sanitizer, and source-map quality;
- C ABI and dynamic library behavior; and
- a viable WebAssembly route.

The winning architecture may use different backends for fast development and
optimized release builds, but they share one verified MIR and conformance
suite.

### 9.3 Artifact policy

- Artifact schemas version independently from the language version.
- Unknown or incompatible versions fail before execution with a stable
  diagnostic code.
- Deserialization is bounded and fuzzed.
- Release builds are deterministic for identical source, lockfile, compiler,
  target, and declared environment inputs.
- Source maps and build metadata are separable so reproducibility does not leak
  local paths or secrets.

## 10. Application platform required for GA

### 10.1 Standard library

The supported standard library includes:

- strings, bytes, collections, iterators, regex, date/time, duration, UUIDs,
  URLs, and explicit numeric conversion;
- filesystem, paths, environment, process, terminal, TCP, UDP, TLS, HTTP
  client, and WebSocket primitives;
- JSON plus schema-driven serialization, with TOML and common text formats;
- cryptographically secure randomness, hashing, and safe secret references;
- configuration layering with typed decoding;
- structured logging and context propagation; and
- testable clocks, randomness, filesystem, and network boundaries.

### 10.2 Web and data

First-party, release-supported packages include:

- HTTP server, routing, middleware, streaming, uploads, and WebSockets;
- HTML templates or an equally complete server-rendering path;
- sessions, cookies, CSRF primitives, password hashing, and authentication
  building blocks;
- SQLite and PostgreSQL at minimum, with pooling, prepared queries,
  transactions, migrations, and seeds;
- typed row/schema mapping and a query API that never requires stringly typed
  field access in normal code; and
- ORM-style models with one-to-one, one-to-many, many-to-many, eager/lazy load
  policy, validation, and transaction-aware relationship updates.

MySQL support is desirable but does not block GA if its quality would lag the
two primary adapters.

### 10.3 Native project environments

Ricochet 2 includes a Python-inspired virtual-environment workflow as a native
toolchain contract.

- `.rvenv` is the conventional project-local environment directory. The
  distinct name avoids colliding with Python's `.venv` in polyglot projects.
- `ricochet new` creates it by default and writes the appropriate source-control
  ignore entry. `--no-env` supports intentionally global or container-managed
  workflows.
- The environment records its base Ricochet toolchain, target, profile, locked
  dependency graph, generated interfaces, installed command shims, and isolated
  native plugins.
- Package resolution remains sourced from `ricochet.toml` and
  `ricochet.lock`. The environment is never the source of dependency truth.
- Environments are disposable, non-portable, and never contain project source
  or secrets. A clean `ricochet env sync --locked` reconstructs one.
- A global content-addressed download/build store may be shared for efficiency,
  but each environment exposes only its own locked package graph and target
  artifacts. Shared storage cannot make an undeclared package importable.
- `ricochet`, run from a project tree, discovers the nearest manifest and
  matching `.rvenv` automatically. Explicit shell activation is convenient but
  never required for `check`, `run`, `test`, build scripts, or installed project
  commands.
- Activation scripts are generated for PowerShell, `cmd.exe`, POSIX shells,
  and other supported shells. They modify only the current shell's path and
  prompt and provide a reversible `deactivate` action.
- `ricochet env exec` runs an arbitrary declared project command in the
  environment without activation; `ricochet env info` explains the selected
  environment, base toolchain, target, and lock status.
- Resetting an existing environment is explicit, confirms interactively, and
  supports a non-interactive confirmation flag. Synchronization itself is
  incremental and non-destructive.
- IDEs and the LSP use the same discovery rules as the CLI, including the same
  toolchain and locked interface artifacts.

The proving matrix includes two adjacent projects with incompatible dependency
and native-plugin versions, recreation after moving the project, offline
reconstruction from a populated cache, activation and non-activation workflows,
and verification that environment variables or packages do not bleed between
projects.

### 10.4 Packages and supply chain

- A package may contain source, precompiled interfaces, target artifacts,
  generated code, documentation, and tests under a versioned manifest.
- Dependency resolution is deterministic and lockfile based.
- Registry artifacts have integrity hashes, provenance, yanking, audit, and
  offline mirror behavior.
- Native dependencies declare supported targets and unsafe capabilities.
- Build scripts are capability-restricted and visible in the lock/audit view.
- The toolchain can emit an SBOM and a dependency/security report.

### 10.5 Operability

GA includes:

- structured logs with trace and task context;
- stable metrics instruments and an exporter-neutral API;
- distributed trace context and spans;
- an OpenTelemetry-compatible exporter package;
- CPU sampling and instrumented profiling;
- allocation and heap summaries;
- task, channel, lock, and async-wait diagnostics;
- benchmark and regression tooling; and
- production-friendly crash reports with symbol/source mapping and secret
  redaction.

The telemetry API is separate from its SDK/exporter so libraries can instrument
without forcing a backend.

## 11. Toolchain and developer experience

The single `ricochet` executable owns the supported workflow:

| Command family | Required behavior |
| --- | --- |
| `new`, `init` | Create minimal CLI, library, web, and worker projects without hidden generators. |
| `env create`, `env sync`, `env info`, `env exec` | Create, reproduce, inspect, and use isolated `.rvenv` project environments; activation remains optional. |
| `check`, `build`, `run` | Share incremental analysis; offer fast development and reproducible release modes. |
| `test` | Unit, integration, async, snapshot, property, and filtered/watch execution with machine-readable reports. |
| `fmt`, `lint`, `fix` | Deterministic formatting, semantic linting, and previewable safe fixes. |
| `doc` | Build linked API docs from the same typed module interfaces used by the compiler. |
| `repl` | Preserve normal type, effect, module, and async semantics instead of using a toy interpreter. |
| `add`, `remove`, `install`, `update`, `audit`, `publish` | Complete package and registry lifecycle with lockfile and provenance enforcement. |
| `lsp` | Diagnostics, completion, hover, definitions, references, rename, symbols, semantic tokens, inlay hints, formatting, and code actions. |
| `debug` | Source breakpoints, stepping, stack/locals/tasks, exceptions/panics, async causality, and DAP support. |
| `bench`, `profile`, `coverage` | Reproducible performance, CPU/allocation/task profiles, and branch/line coverage. |
| `package` | Self-contained applications, target selection, metadata, checksums, signing hooks, and update-channel output. |
| `doctor` | Diagnose toolchain, target, dependency, certificate, linker, and runtime configuration. |

All diagnostics exposed by these commands have a stable identifier, primary
span or resource, concise explanation, and actionable help when a safe action
exists. Machine-readable output uses versioned schemas.

## 12. Feature disposition

This table prevents a desired feature from silently becoming either a 2.0
blocker or an abandoned idea.

| Capability | 2.0 disposition |
| --- | --- |
| Static typing, inference, generics, traits | GA blocker |
| Enums/ADTs, exhaustive matching, `Option`, `Result` | GA blocker |
| Static stack-effect verification | GA blocker |
| Bounded effect/capability checking | GA blocker |
| Modules, visibility, separate compilation | GA blocker |
| Managed memory and deterministic resources | GA blocker |
| Async syntax, structured tasks, cancellation, channels, select | GA blocker |
| Locks, atomics, send/share rules, memory model | GA blocker |
| Records, classes, single inheritance, traits, associated methods | GA blocker |
| Abstract/sealed/final forms and trait-based operators | GA blocker |
| Typed reflection | GA blocker |
| Fixed-width numbers, bytes, C layout, pointers, C ABI/FFI | GA blocker |
| Native application packaging | GA blocker |
| Native optimized backend | 2.x milestone; a GA blocker only if the reference VM misses approved performance or deployment budgets |
| Native `.rvenv` project environments | GA blocker |
| WebAssembly target | 2.x target program after the hosted GA core; not a GA blocker |
| SIMD | Post-backend optimization program; not a GA blocker |
| HTTP/JSON/config/secrets/database/migrations | GA blocker |
| ORM relationships | GA blocker for the first-party application stack |
| Logging, metrics, traces, profiler, coverage | GA blocker |
| OpenTelemetry exporter | First-party beta by GA |
| Local actors | First-party beta after structured concurrency |
| gRPC and GraphQL | Typed first-party packages during 2.x; not GA blockers |
| Dependency injection framework | Optional package; not a language feature or GA blocker |
| Immutable persistent collections | Standard-library option; not the universal data model |
| Higher-kinded types and monadic syntax | Post-2.0 research, not blockers |
| Multiple class inheritance | Explicit non-goal; use traits and composition |
| Built-in durable broker | Explicit non-goal; provide replaceable client packages |
| Distributed actors and OTP-equivalent clustering | Post-2.0 ecosystem work |
| `no_std`, bare-metal, embedded, and kernel targets | Post-2.0 target program |

## 13. Architecture decisions required before feature implementation

Each ADR must contain representative postfix source, rejected alternatives,
prototype evidence, and an explicit effect on diagnostics and tooling.

1. **ADR-001: Typed postfix surface.** Declaration grammar, stack signatures,
   generics, patterns, modifiers, async/effect spelling, and formatter rules.
2. **ADR-002: Type and stack solver.** Inference boundaries, stack rows,
   trait resolution, associated types, dynamic checks, and diagnostic model.
3. **ADR-003: Managed heap and resources.** Collector candidates, object
   identity, affine handles, cleanup lowering, weak references, and FFI pinning.
4. **ADR-004: Object and value representation.** Records, enums, classes,
   dispatch, layout, reflection metadata, and generic representation.
5. **ADR-005: Effects and capability authority.** Static effect vocabulary,
   propagation, runtime grants, sandbox threat model, and package declarations.
6. **ADR-006: Async runtime.** Executor, reactor, blocking pool, structured
   scopes, cancellation, channel semantics, send/share rules, and shutdown.
7. **ADR-007: Backend bakeoff.** Reference VM, Cranelift, LLVM, WebAssembly,
   debug/profiling evidence, and release-mode selection.
8. **ADR-008: Modules, environments, packages, and trust.** Interface
   artifacts, `.rvenv` discovery, resolver, lockfile, registry, build scripts,
   native packages, signing, and provenance.
9. **ADR-009: Application platform boundaries.** What belongs in core, the
   standard library, first-party packages, and replaceable adapters.
10. **ADR-010: Compatibility and release policy.** Ricochet pre-GA churn,
    post-GA SemVer, artifact compatibility, deprecation, and support windows.

No ADR is approved merely because its first prototype works. It must explain
how the choice survives modules, generics, async, FFI, debugging, packaging,
and independent use.

## 14. Dependency-correct execution roadmap

Milestones are gate-driven, not date-driven. Tooling, documentation, tests,
security review, and benchmarks advance continuously; the phase describing
them is when their complete public contract becomes a gate.

### Phase 0: LTS transition and clean baseline

Deliverables:

- publish the Ricochet 1.x LTS scope in `README.md`, `SUPPORT.md`, and
  `SECURITY.md`;
- keep `main` at the 1.x line and create the protected `ricochet-2` branch;
- establish distinct issue labels, CI lanes, release names, and artifact
  prefixes;
- reproduce and resolve or formally quarantine every flaky 1.x test, beginning
  with the observed timing-sensitive
  `await_all_resolves_tasks_in_order_and_retains_completed_status` case;
- approve ADR-010 and open the remaining architecture ADRs; and
- record the exact v1.0.0 source, toolchain, test, docs, and release baseline.

Gate: `main` is clean, the supported 1.x suite has no accepted flaky tests, the
LTS rules are public, and Ricochet can change without mutating a 1.x contract.

### Phase 1: Ricochet bootstrap and compiler spine

Deliverables:

- the `ricochet` executable, `.ricochet` source, `ricochet.toml`, and `ricochet.lock`;
- lossless CST, AST, name-resolved HIR, compiler database, stable source IDs,
  and coded diagnostics;
- minimal typed primitives, bindings, blocks, callables, control flow, and
  verified stack effects;
- new bytecode header, verifier, loader, source maps, and reference VM;
- `ricochet new`, `env create`, `env sync`, `env info`, `check`, `build`,
  `run`, `test`, `fmt`, and `doc` minimum vertical slices; and
- public language orientation and CLI documentation built from a versioned
  docs snapshot.

Proof application: a small field-equipment readiness CLI with an isolated
`.rvenv`, typed records, validation, tests, deterministic report output, and
one intentionally triggered documented compiler diagnostic. It uses only
Phase 1 features.

Gate: a new user can install the candidate, create and reproduce its
environment, create the application, check it, test it, build versioned
bytecode, run it, and diagnose a failure without source-repository access.

### Phase 2: Safe typed data and control flow

Deliverables:

- full primitive numeric set, strings, bytes, tuples, records, enums,
  collections, `Option`, and `Result`;
- local inference, generic functions/types, trait constraints required by core
  collections, stack-row polymorphism, and typed closures;
- exhaustive pattern matching, destructuring, result propagation, iterators,
  and generators;
- flow narrowing and explicit `Dynamic` validation/casts; and
- complete formatter, LSP, debugger inspection, examples, and diagnostics for
  the phase.

Proof application: a multi-format inventory importer that decodes untrusted
dynamic input, validates it into typed records and enums, uses generic
transformations, and emits a searchable deterministic report. Its negative
tests prove type, stack, nullability, and exhaustiveness failures occur at
compile time.

Gate: ordinary data transformation requires no runtime type guessing after the
explicit input boundary.

### Phase 3: Modules, protocols, and object model

Deliverables:

- modules, imports/exports, visibility, interface artifacts, and separate
  compilation;
- complete trait/interface model, associated types/functions, coherence, and
  explicit dynamic trait objects;
- managed classes, single inheritance, constructors, properties, virtual and
  final dispatch, abstract/sealed forms, and typed reflection;
- trait-based operator protocols; and
- package-local documentation and cross-module LSP/refactoring behavior.

Proof application: an extensible pricing or policy engine whose independent
modules implement shared traits for records and classes. The application must
add a new implementation without modifying the dispatcher and must prove that
visibility and coherence violations are diagnosed.

Gate: the compiler can check consumers from interface artifacts without
loading dependency source, and all dispatch forms remain type safe.

### Phase 4: Effects, resources, capabilities, and native boundary

Deliverables:

- managed heap implementation selected through ADR-003 evidence;
- affine resource checking, deterministic scopes, cleanup lowering, and task
  panic boundaries;
- compile-time effect checking plus runtime capability enforcement;
- C layout, pointers, slices, pinning, allocation, callbacks, imports/exports,
  and unsafe diagnostics;
- versioned plugin ABI; and
- sanitizer, fuzz, leak, and capability-boundary tests.

Proof application: a file-analysis CLI that owns multiple resources and calls a
small documented C library. It must demonstrate deterministic cleanup on
success, error propagation, and cancellation simulation, while unsafe code is
confined to one audited module.

Gate: safe application modules cannot forge authority, leak an affine resource,
use a moved handle, dereference a pointer, or call foreign code without the
declared boundary.

### Phase 5: Structured concurrency and async I/O

Deliverables:

- executor, reactor, timers, blocking/CPU pools, async callables, and `await`;
- task scopes, task groups, cancellation, deadlines, timeouts, and cleanup;
- bounded channels, select, backpressure, task locals, and context propagation;
- mutexes, read/write locks, signals, atomics, send/share traits, and the memory
  model; and
- deterministic async test utilities, task-aware debugger views, and
  concurrency profiling events.

Proof application: a bounded concurrent endpoint monitor and job processor. It
must cap concurrency, apply deadlines, cancel a subtree, preserve output order
where requested, handle backpressure, and shut down without orphan tasks.

Gate: stress, race-model, cancellation, and shutdown suites are green; the
proof application does not depend on sleeps to establish task state.

### Phase 6: Application platform

Deliverables:

- HTTP client/server, TLS, routing, middleware, streaming, uploads, WebSockets,
  JSON/schema serialization, configuration, secrets, and structured logging;
- SQLite and PostgreSQL adapters, pooling, prepared typed queries,
  transactions, migrations, and seeds;
- typed models, validation, and ORM relationship behavior;
- sessions, cookies, CSRF, password hashing, and authentication building
  blocks;
- production process lifecycle, graceful shutdown, health/readiness endpoints,
  and test fixtures; and
- first-party package versioning and compatibility policy.

Proof application: a multi-user service with authenticated sessions, related
database records, a JSON API, a server-rendered administrative view, a
background job, migrations, metrics, and graceful shutdown. Tests use both
SQLite and PostgreSQL where their contracts overlap.

Gate: the application is deployable from a clean machine using documented
commands and survives restart, migration, concurrent requests, invalid input,
database rollback, and secret-redaction checks.

### Phase 7: Complete development loop and ecosystem

Deliverables:

- incremental check/build/watch performance and cache correctness;
- complete formatter, linter/fixer, LSP, DAP debugger, REPL, API docs, test
  runner, mocking/fixtures, coverage, benchmark, and profiler surfaces;
- package add/update/audit/publish/yank/mirror workflows with provenance;
- complete `.rvenv` activation, command-shim, cache, offline, native-plugin,
  target, and IDE integration;
- logs, metrics, traces, profiles, crash reports, and an OpenTelemetry exporter;
- self-contained packaging, signing hooks, checksums, SBOM, update metadata,
  and platform installers; and
- stable, searchable, versioned public documentation and a maintained example
  corpus.

Proof application: an evaluator publishes a reusable package and consumes it
from a separate application repository using only the public registry and docs.
They must exercise edit/check/test/debug/profile/package/install workflows and
report the time and blockers for each.

Gate: no essential workflow requires repository scripts, private notes, or a
Rust toolchain on the application developer's machine.

### Phase 8: Performance qualification and target expansion

Deliverables:

- qualify the production VM against the compiler, startup, throughput, latency,
  and memory budgets approved through ADR-007;
- implement the ADR-007 optimized backend before GA only if the production VM
  misses those budgets or the chosen deployment contract requires it;
- preserve differential VM/backend conformance whenever a second backend is
  present;
- complete release-mode optimization, debug symbols, stack traces, sampling
  profiles, and reproducible artifacts for every GA target;
- C embedding and plugin examples on every stable desktop/server target;
- prototype SIMD intrinsics with scalar fallbacks after the backend shape is
  known; and
- prototype a WebAssembly target with documented host capabilities and
  browser/non-browser boundaries, promoting it during 2.x only when its own
  target gate passes.

Proof application: one compute-heavy data processor and one embedded-library
host compare VM and optimized builds for identical results, diagnostics, FFI
behavior, and profiles. A separate WASM example proves the documented subset
without pretending unsupported host APIs work.

Gate: GA performance targets set from the earlier benchmark baseline are met by
the shipped production backend with no semantic divergence. Experimental SIMD,
WebAssembly, or a second backend does not delay GA unless it was required to
meet that core gate; each retains its own post-GA promotion criteria.

### Phase 9: Beta freeze and ecosystem proving

Deliverables:

- freeze source syntax, type/effect semantics, standard library naming,
  manifest/lockfile schemas, package protocol, and stable tool commands;
- publish the language specification and executable conformance suite;
- complete parser/compiler/bytecode/package fuzzing, dependency audit, threat
  model, unsafe review, and supported-target soak tests;
- classify every useful Ricochet 1 capability as rebuilt, replaced, dropped,
  or deferred, without adding compatibility code; and
- publish application tutorials for CLI, web/API, worker, package, FFI, and
  deployment workflows.

Gate: `ricochet-v2.0.0-beta.1` begins the compatibility promise for the frozen
surface. Later breaking changes require an explicit beta reset and rationale.

### Phase 10: Release candidates and 2.0.0

Deliverables:

- independent capstone repositories for a production-style web application, a
  CLI/background worker, and a native integration;
- signed or explicitly audited artifacts for supported platforms;
- final install, update, rollback, package, docs, and incident runbooks;
- published performance and resource-usage results with reproducible inputs;
- no open release-blocking correctness, security, soundness, data-loss, or
  documentation defects; and
- explicit support and deprecation policy for the 2.x stable line.

Gate: all release criteria in Section 17 pass from immutable candidate
artifacts. Tag `ricochet-v2.0.0` only after the same artifact set is promoted
without rebuilding it.

## 15. Black-box proving protocol

Every phase proof is cumulative over completed phases but may not require a
future feature.

1. Build and checksum an immutable candidate toolchain and matching public
   documentation snapshot.
2. Install them into a clean machine, VM, or contained workspace where the
   Ricochet source repository, internal plans, tests, and implementation notes
   are unavailable.
3. Give the evaluator only a plain user-story brief, installation instructions,
   CLI help, and the public docs snapshot.
4. Require a new git repository containing application source, tests, README,
   exact build/run instructions, and a feature-to-problem map.
5. Capture the complete command transcript, toolchain/docs hashes, environment,
   time spent, diagnostics, and evaluator report.
6. Stop at an implementation blocker, contradictory or missing docs,
   undecipherable diagnostic, postfix inconsistency, hidden dependency, or
   feature that works only through internal helpers.
7. Classify the blocker as design, compiler, runtime, tooling, documentation,
   packaging, or environment. Fix it in the product; do not coach around it.
8. Rebuild a new immutable candidate and rerun from a clean environment.
9. Tag the proving repository with the exact candidate identifier only after it
   completes without an unresolved blocker.

The phase implementer cannot be the sole evaluator. The evaluator may be a
person or an independently briefed agent, but must not receive hidden context
that an ordinary adopter would lack.

## 16. Continuous quality gates

Every implementation phase must maintain:

- formatting, lint, unit, integration, conformance, docs, editor, package, and
  acceptance checks;
- zero accepted flaky tests;
- deterministic negative tests for every static guarantee;
- environment-isolation tests proving conflicting packages, plugins,
  toolchains, targets, and activation state cannot leak across projects;
- fuzz targets for all untrusted parsers and artifact loaders;
- differential VM/backend tests after a second backend exists;
- platform CI for Windows x64, Linux x64, macOS arm64/x64, and every advertised
  additional target;
- dependency, license, secret, provenance, and vulnerability scans;
- benchmark history for compiler latency, runtime, memory, startup, async,
  server, database, and package operations; and
- documentation examples compiled and, where safe, executed by CI.

Performance budgets are established from Phase 1 and representative comparator
implementations, then approved in ADR-007. The project does not invent numbers
after implementation merely to declare success.

## 17. Final 2.0.0 release criteria

Ricochet 2.0.0 is releasable only when all of the following are true:

### Language and soundness

- The public specification matches the compiler and conformance suite.
- Static type, stack, nullability, resource, send/share, and effect guarantees
  have no known soundness holes outside explicit `Dynamic` or `unsafe` code.
- All unsafe standard-library and runtime modules have an audit record.
- Compiler crashes on valid or invalid user input are release blockers.

### Application viability

- The capstone web application, worker/CLI, package, and native integration are
  reproducible from clean repositories.
- HTTP, JSON, configuration, secrets, database transactions/migrations,
  relationships, async shutdown, and observability work through supported
  public APIs.
- A developer can complete the normal loop without editing generated internals
  or invoking repository-only scripts.

### Tooling

- Check, build, test, format, lint, docs, LSP, debugger, coverage, profiler,
  environment, package, audit, and self-contained packaging are release
  quality.
- Diagnostics have stable codes, spans, help, and versioned machine output.
- Profiles and crash reports resolve Ricochet frames and async/task context.

### Security and operations

- No open critical/high vulnerability or known sandbox/capability escape exists.
- Package integrity/provenance, secret redaction, artifact bounds checking, and
  update verification pass adversarial tests.
- Runtime health, graceful shutdown, logs, metrics, and traces are documented
  and proven in the capstone service.

### Release engineering

- Supported-platform artifacts come from one immutable release candidate.
- Checksums, signatures or explicit signing-status reports, SBOMs, provenance,
  update metadata, and installer tests pass.
- Ricochet 2 and Ricochet 1.x install and run side by side without command,
  configuration, registry, or update-channel collisions.
- `.rvenv` environments rebuild from the lockfile, remain isolated from global
  and neighboring projects, and work with or without shell activation.
- Upgrade compatibility within the frozen Ricochet beta contract is tested.

### Independent usability

- All phase proving repositories are complete and tagged.
- Final evaluators receive only public artifacts and docs.
- No unresolved design, documentation, workflow, or postfix-consistency blocker
  remains.

## 18. Scope control

A feature may enter the GA blocker set only when it is necessary to satisfy a
charter outcome and its dependencies fit the roadmap. Attractive features do
not enter merely to make the language inventory longer.

If a phase grows beyond one coherent independently provable vertical slice, it
must be split before implementation. If a design choice would require a future
phase to make the current phase usable, the dependency order is wrong and the
plan must be corrected.

No phase closes through a stub, undocumented host helper, test-only bypass, or
proof application written with private knowledge.

## 19. Key risks and mitigations

| Risk | Mitigation |
| --- | --- |
| Static stack polymorphism becomes the critical compiler bottleneck. | Prototype the solver before broad syntax work; require property tests and small explanatory diagnostics. |
| Managed objects, affine resources, async cancellation, and FFI produce conflicting lifetime rules. | Resolve them together in MIR and ADRs 003, 005, and 006 before exposing any one as stable. |
| The project rebuilds all of 1.x before learning whether 2.x syntax works. | Ship narrow end-to-end alphas and independent proof apps from Phase 1. |
| Native backend work consumes the application roadmap. | Keep the reference VM viable, run the bakeoff early, and implement optimized codegen after language semantics stabilize. |
| A huge standard library becomes unmaintainable. | Keep small semantic primitives in core, stable general facilities in std, and replaceable integrations in first-party packages. |
| The two branches drift operationally or fixes leak across them. | Separate CI/release identities and require architecture-specific review for every cross-line fix. |
| Public docs lag the compiler. | Generate inventories from compiler metadata and compile/run documentation examples in CI. |
| “LTS” turns into unbounded feature pressure. | Enforce the explicit allowed/disallowed 1.x change list and patch-only version policy. |

## 20. Immediate next actions

1. Commit this charter and the explicit 1.x LTS policy on `main`.
2. Reproduce and repair the known 1.x async test flake without changing public
   task semantics.
3. Record a clean v1.0.0 maintenance baseline.
4. Create and protect `ricochet-2` from that audited history.
5. Add the Ricochet 2 identity skeleton without deleting or renaming 1.x assets.
6. Write ADR-001, ADR-002, and ADR-003 before implementing the type checker.
7. Build throwaway syntax/type/resource prototypes and discard none without
   explicit owner approval.
8. Approve the Phase 1 proof brief and immutable proving environment.

## 21. Design references

These sources are comparative inputs, not requirements to copy another
language:

- Python's virtual-environment contract treats environments as isolated,
  disposable, reproducible directories whose activation is optional:
  <https://docs.python.org/3/library/venv.html>
- Python's official typing documentation demonstrates generic protocols and
  typed dictionary shapes: <https://docs.python.org/3/library/typing.html>
- Python documents async/await as the basis for high-level network and server
  frameworks: <https://docs.python.org/3/library/asyncio.html>
- Python ships a standard profiler workflow:
  <https://docs.python.org/3/library/profile.html>
- Node documents an integrated test runner/watch workflow and a runtime
  permission model: <https://nodejs.org/api/test.html> and
  <https://nodejs.org/api/permissions.html>
- PHP documents user-facing type declarations and closed enum values:
  <https://www.php.net/manual/en/language.types.declarations.php> and
  <https://www.php.net/manual/en/language.types.enumerations.php>
- Ruby's Ractor documentation is a useful example of isolation-oriented
  parallelism: <https://docs.ruby-lang.org/en/3.3/Ractor.html>
- OpenTelemetry specifies separable APIs/SDKs for traces, metrics, and logs:
  <https://opentelemetry.io/docs/specs/otel/>
- WebAssembly's published goals establish it as a portable compilation target
  with host interactions layered outside the core format:
  <https://webassembly.org/docs/high-level-goals/>

## 22. Phase 0 baseline record

The transition started from the published `v1.0.0` tag at commit
`7a38423f8b02cdd63363f37ce9a524cf919426f7` with Rust and Cargo 1.96.0.

The first LTS transition validation produced:

- `cargo fmt --all -- --check`: passed;
- `cargo clippy --workspace --all-targets -- -D warnings`: passed;
- `cargo test --workspace`: 1,129 passed and 1 ignored across 40 suites;
- the deterministic running-task and `await_all` unit tests: 100/100 repeated
  passes each;
- the scheduler-independent CLI task lifecycle test: 100/100 repeated passes;
- `cargo audit --deny warnings`: passed with 558 dependencies scanned;
- public reference documentation validation: passed;
- editor asset validation: passed;
- word/example inventory: 372 examples compiled, 270 executed, and 102 linked
  to host-context integration evidence; and
- the full acceptance suite, including packaging contracts, governance,
  notices, docs, examples, scaffolds, web servers, SQLite migrations, and seeds:
  passed.

The previously observed `await_all` failure was a test race, not a runtime
ordering defect: the test assumed two 50 ms sleepers must still be running when
the host inspected them. The maintenance fix uses an explicit channel barrier
for running-state coverage and makes lifecycle integration assertions valid for
either scheduler state before `await`.

The product-line governance established on 2026-08-08 adds:

- remote protected `main` and `ricochet-2` branches with force-push and
  deletion disabled for administrators as well as ordinary contributors;
- the `line: 1.x LTS`, `line: 2.x`, and `roadmap: 2.0` issue labels;
- distinct `CI`/`CodeQL Advanced` and `Ricochet 2 CI`/`Ricochet 2 CodeQL`
  automation identities;
- a fail-closed `Ricochet 2 Release Contract` lane that validates v2 naming but
  cannot publish before the Phase 1 identity skeleton exists;
- accepted compatibility and release policy in ADR-010;
- concrete proposed designs and evidence gates in ADR-001 through ADR-003; and
- open decision briefs for ADR-004 through ADR-009.
