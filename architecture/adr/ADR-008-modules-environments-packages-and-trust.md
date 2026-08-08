# ADR-008: Modules, environments, packages, and trust

Status: Open

Opened: 2026-08-08

Target decision: module/interface slice in Phase 3; full `.rvenv` and package
contract by Phase 7

Depends on: ADR-001, ADR-002, ADR-005, ADR-007, and ADR-010

## Context

Ricochet 2 needs separate compilation and a Python-inspired project-local
environment without inheriting Python's package formats or global-state
problems. Environments must be isolated, disposable, reproducible from
`ricochet.lock`, automatically discovered, and usable without shell activation.
Packages may include native code and build scripts, so resolution is also a
trust and capability boundary.

This record is open. The `.rvenv` product contract is accepted in the roadmap;
resolver, interface, registry, signing, native-package, and installer mechanics
still need a proposal and evidence.

## Decision scope

The proposal must define:

- modules, stable identities, imports/exports, visibility, initialization, and
  cycle rejection;
- compiler-readable interface artifacts and separate checking without source;
- `ricochet.toml` and `ricochet.lock` schemas and v2 ecosystem marker;
- nearest-project and `.rvenv` discovery, explicit overrides, activation, and
  editor/LSP parity;
- `env create`, `env sync`, `env info`, `env exec`, reset, offline, and moved
  project behavior;
- deterministic resolution, feature selection, yanking, mirrors, and lockfile
  updates;
- shared content-addressed storage that never makes an undeclared package
  importable;
- registry integrity, provenance, signatures, trust roots, and compromise
  recovery;
- capability-restricted build scripts, generated code, native artifacts, and
  target selection; and
- package archive, plugin ABI, SBOM, audit, publish, and installer boundaries.

Import syntax must remain declaration-shaped postfix metadata. A pressure case
for ADR-001 is:

```ricochet
Http Client import
Json Value import

( client: Client url: Url -> Result<Response, HttpError> uses async network ) fetch public function
  $url $client request await
end
```

The exact module path spelling remains open. It cannot become fake
namespace-dot executable syntax.

## `.rvenv` invariants

- `.rvenv` contains environment state, never project source or secrets.
- It is disposable and non-portable; locked sync reconstructs it.
- Activation is optional for `check`, `run`, `test`, build, LSP, and project
  commands.
- Two adjacent projects may resolve incompatible package and native-plugin
  versions without leakage.
- A shared cache improves efficiency but not visibility or trust.
- Reset is explicit, confirms interactively, and is never implied by sync.
- Moving a project invalidates machine-specific paths safely and permits
  reconstruction.

## Diagnostics and tooling pressure

Resolution errors show the dependency chain, constraints, selected target,
registry/mirror, yanked state, and safe remediation without printing tokens.
Interface mismatches identify compiler/schema/package versions. Discovery tools
explain which manifest, environment, toolchain, lockfile, and cache entry won
and why.

The LSP and CLI use one discovery library. Build-script capability prompts and
audit output are machine-readable. Lockfile changes are reviewable and do not
occur during `--locked` commands.

## Alternatives already rejected

- Global mutable package installation as the normal project workflow is
  rejected.
- Treating `.rvenv` as portable or committing it is rejected.
- Requiring activation for correctness is rejected.
- Loading dependency source to infer public APIs is rejected.
- Unrestricted package build scripts and native plugins are rejected.
- Reusing 1.x manifest, lockfile, package, registry, or plugin schemas is
  rejected by ADR-010.

## Prototype evidence

No Ricochet 2 module/resolver/environment prototype exists yet. Required
evidence includes two adjacent incompatible projects, clean locked recreation,
move/copy behavior, populated-cache offline sync, activation and no-activation,
all supported shells, editor discovery, concurrent sync, interrupted writes,
registry yanks/mirror failure, checksum/signature attack cases, restricted build
scripts, native target selection, SBOM/audit, and side-by-side v1/v2 installs.

The independent proof user receives only public CLI/package documentation and
must reproduce the application without repository access or private registry
state.
