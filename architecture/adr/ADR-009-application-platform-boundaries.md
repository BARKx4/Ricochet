# ADR-009: Application platform boundaries

Status: Open

Opened: 2026-08-08

Target decision: initial boundary before Phase 6 application-platform work

Depends on: ADR-001 through ADR-008

## Context

Ricochet 2 will not be a realistic Python, Ruby, Node.js, or PHP alternative if
every ordinary application starts by assembling unsupported third-party
plumbing. It also cannot sustainably freeze every database, broker, RPC, and
web framework into the language core.

This record is open. It must define the line between compiler/runtime
semantics, stable standard-library facilities, supported first-party packages,
and replaceable adapters.

## Decision scope

The proposal must classify and version:

- core semantic primitives that the compiler/runtime must understand;
- standard-library collections, text/bytes, time, filesystem, process,
  networking foundations, test support, and observability APIs;
- first-party HTTP client/server, routing, JSON, configuration, secrets,
  database, migrations, pooling, transactions, and ORM relationship packages;
- first-party logging, metrics, tracing, context propagation, coverage, and
  profiling integration;
- replaceable database drivers, transports, serializers, server adapters, and
  deployment integrations;
- optional gRPC, GraphQL, local actors, DI, persistent collections, and broker
  clients; and
- explicit non-goals such as a built-in durable broker, distributed actor
  cluster, OTP-equivalent supervisor, or database server.

The supported application surface must read as ordinary postfix Ricochet:

```ricochet
Request new request let
"https://api.example.test/users" $request url.set
$request $http send await try response let

$response json_body try UserList validate_dynamic try users let
$users $repository save_all try
```

Selector arguments remain below their receiver. Integrations use public words
with `_`, not host-language namespace-dot facades.

## Placement criteria

A facility belongs in compiler/runtime core only when required for language
semantics, safety, verification, or portable execution. It belongs in the
standard library when it has a stable cross-application contract and can ship
with the toolchain without privileged compiler knowledge. It belongs in a
first-party package when Ricochet promises support but implementation/version
cadence should be replaceable. Provider- or protocol-specific integrations are
adapters unless a later ADR proves a stronger need.

Every supported layer declares effects/capabilities, async behavior, resource
ownership, error types, observability hooks, security limits, and version
policy. "Built in" never means ambient authority or hidden blocking I/O.

## Diagnostics and tooling pressure

Application errors preserve typed causes across HTTP, JSON validation,
database, migrations, and async boundaries. Tooling distinguishes compile-time
schema/type failures, runtime validation, capability denial, network protocol
failure, and provider errors.

The profiler and OpenTelemetry-compatible surface share context with tasks,
HTTP, database, and user spans without requiring one exporter. Secrets and
payload bodies are redacted by default. Package docs and examples are compiled
and run against the same public interfaces shipped to users.

## Alternatives already rejected

- A huge compiler-known standard library containing every integration is
  rejected.
- Leaving HTTP/JSON/config/secrets/database/observability entirely to
  unsupported third parties is rejected for 2.0 GA.
- A built-in durable message broker, distributed actor cluster, or
  OTP-equivalent runtime is not a 2.0 requirement.
- gRPC, GraphQL, a DI framework, persistent collections, and monadic syntax are
  not automatic GA blockers.
- APIs that merely mimic a host language's namespace and call order are
  rejected by the postfix surface rules.

## Prototype evidence

No Ricochet 2 application-platform prototype exists yet. The decision needs
vertical proof applications for CLI/config/secrets, HTTP API, background
worker, database migrations/transactions/relationships, observability, testing,
profiling, packaging, and replaceable adapters. Failure injection covers
malformed input, cancellation, timeouts, backpressure, partial transactions,
pool exhaustion, exporter failure, and secret redaction.

The first-party/support boundary is accepted only after independent users can
build and diagnose those applications from public documentation without
private runtime hooks.
