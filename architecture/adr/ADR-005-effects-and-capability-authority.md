# ADR-005: Effects and capability authority

Status: Open

Opened: 2026-08-08

Target decision: before Phase 4 effect checking and sandbox enforcement

Depends on: ADR-001, ADR-002, and ADR-003

## Context

Ricochet needs compile-time visibility for async suspension, unsafe operations,
and host actions while retaining runtime authority checks. Static effects say
what code intends to do; they do not prove that a process was granted access to
a file, network endpoint, secret, or child process.

This record is open. It must choose a deliberately bounded effect vocabulary
and capability model, not a general algebraic-effect language.

## Decision scope

The proposal must define:

- the closed 2.0 effect vocabulary, including `async`, `unsafe`, filesystem,
  network, process, environment, database, time, random, and secrets;
- inference inside private code and explicit exported `uses` clauses;
- effect subtyping/sets, generic effect parameters if any, and trait methods;
- capability values, process grants, attenuation, delegation, and revocation;
- package-manifest declarations and build-script authority;
- how tests supply deterministic fake time, random, network, and filesystem;
- effect behavior through callbacks, reflection, `Dynamic`, FFI, panics,
  resources, and async tasks; and
- sandbox threat boundaries and host audit events.

Representative source must keep authority and operands visible:

```ricochet
( path: Path -> Result<String, FsError> uses filesystem ) load_text public function
  $path workspace fs_read_text
end

( url: Url -> Result<Response, HttpError> uses async network ) fetch public function
  $url network_client http_request await
end
```

The exact capability-receiver APIs must pass the selector-order rules: ordinary
arguments sit below the capability/client receiver, which sits before the
selector.

## Diagnostics and tooling pressure

An undeclared effect error must show the call chain that introduced it and the
narrowest public signature that needs review. A denied capability error must
distinguish compile-time declaration from runtime authority and name the grant
source without printing secrets.

The LSP shows inferred and declared effects. Package, docs, test, and audit
tools consume the same effect/capability metadata. An automatic fix may add a
private inferred clause for preview, but it cannot grant runtime authority or
silently widen a public package manifest.

## Alternatives already rejected

- General user-defined effect handlers and monadic syntax are not 2.0
  requirements.
- Static effects alone cannot replace runtime capability enforcement.
- Ambient global authority for filesystem, network, process, or secrets is not
  the safe default.
- Hiding `unsafe` or suspension behind an unannotated public wrapper is not
  allowed.

## Prototype evidence

No Ricochet 2 effect/capability prototype exists yet. Required evidence covers
local inference, exported interfaces, effect-polymorphic higher-order calls,
capability attenuation, sandbox denial, deterministic tests, package build
scripts, async/resource cleanup, FFI callbacks, and adversarial attempts to
smuggle authority through `Dynamic` or reflection.

The prototype must compare usability and diagnostic noise with and without
explicit capability receiver values. It records every required escape hatch.
This record cannot move to Proposed until the vocabulary is small enough to
teach and precise enough to enforce.
