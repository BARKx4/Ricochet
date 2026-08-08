# ADR-010: Compatibility and release policy

Status: Accepted

Accepted: 2026-08-08

Decision owner: Ricochet project owner

Applies to: Ricochet 1.x LTS and the incompatible Ricochet 2 product line

## Context

Ricochet 1.0 is a shipped language, runtime, toolchain, package format, and
installer. Ricochet 2 needs freedom to redesign all of those layers without
turning the maintained 1.x line into a permanent compatibility subsystem.
Conversely, calling 1.0 "LTS" is meaningful only if users can tell which
contracts are stable, how long they are supported, and which executable owns
which product line.

The existing 1.0 installation complicates the command handoff. Its canonical
tool is `rco`, but its published packages also contain a secondary `ricochet`
launcher. Ricochet 2 needs the canonical `ricochet` command. Reusing that name
must not authorize an installer to delete or overwrite a user's 1.x files.

This ADR turns those owner decisions into branch, tag, artifact, installer,
stability, and support rules. It is accepted now because implementation work
needs a fixed product boundary. Individual installer and package mechanisms
remain subject to ADR-008 and their proving tests.

## Decision

### 1. Two product lines share ancestry, not compatibility

Ricochet 1.x and Ricochet 2 are independent product lines:

| Contract | Ricochet 1.x LTS | Ricochet 2 |
| --- | --- | --- |
| Long-lived branch | `main` | `ricochet-2` |
| Canonical command | `rco` | `ricochet` |
| Source contract | Existing 1.x syntax and extensions | `.ricochet` and the v2 specification |
| Project files | Existing 1.x files | `ricochet.toml` and `ricochet.lock` |
| Local environment | Existing 1.x package behavior | `.rvenv` |
| Release tags | `v1.0.x` | `ricochet-v2.<semver>` |
| Package namespace | Existing 1.x ecosystem | `@ricochet2/*` |
| Environment prefix | Existing 1.x variables | `RICOCHET2_` |
| Stability promise | Patch-compatible LTS | Pre-GA policy below, then SemVer |

Ricochet 2 has no source, bytecode, image, manifest, lockfile, package,
registry, native-plugin, embedding-ABI, configuration, update-channel, or
runtime compatibility obligation to 1.x. A v2 implementation must reject a
1.x artifact before interpreting its payload. A 1.x implementation must never
silently treat a v2 artifact as 1.x.

There is no in-process `--compat-v1` mode and no required `migrate-v1`
command. A future standalone conversion assistant may be useful, but it is a
separate product with separately tested transformations. It cannot weaken the
v2 parser or runtime boundary.

The ordinary command lines are intentionally distinct:

```text
rco run legacy-app.ric
ricochet run src/main.ricochet
```

This is command-line notation rather than Ricochet source, so it does not
participate in the postfix grammar.

### 2. Branch governance

`main` remains the Ricochet 1.x LTS branch until an explicit owner decision at
Ricochet beta or GA changes GitHub's default branch. `ricochet-2` is the v2
integration branch. Neither branch is merged wholesale into the other.

Both long-lived branches are protected against force-push and deletion,
including administrator actions. Normal maintainer pushes are allowed. This
keeps a lightweight solo-maintainer workflow while preventing accidental
history replacement.

A correction needed on both lines is reviewed and validated independently in
each architecture. A cherry-pick is evidence of ancestry, not evidence that a
change is safe on the other line. Changes are labelled with exactly one
primary product line:

- `line: 1.x LTS` for 1.x maintenance;
- `line: 2.x` for Ricochet 2 work; and
- `roadmap: 2.0` in addition to `line: 2.x` when work closes a 2.0 gate.

### 3. CI and release lanes

The branches retain separate visible automation identities:

| Lane | Ricochet 1.x LTS | Ricochet 2 |
| --- | --- | --- |
| Build and test | `CI` | `Ricochet 2 CI` |
| Security analysis | `CodeQL Advanced` | `Ricochet 2 CodeQL` |
| Release | `Release` | `Ricochet 2 Release` |

During Phase 0, the v2 release lane is represented by `Ricochet 2 Release
Contract`. It validates names and policy but publishes nothing. Publishing is
fail-closed until Phase 1 produces the v2 executable, manifest, artifact
schemas, and installer identity. Renaming that workflow to `Ricochet 2
Release` and granting write permissions requires a reviewed release-enablement
change with package tests.

The v1 release workflow never publishes from a v2 tag. The v2 release workflow
never accepts an unprefixed v1 tag. Scheduled v1 nightlies continue to build
from `main`; v2 nightlies receive their own schedule and update document only
after the v2 lane is enabled.

### 4. Tags, versions, channels, and artifact names

Ricochet 1.x keeps the existing `v1.0.x` tags. Ricochet 2 uses a product-line
prefix so a tag is unambiguous without inspecting its commit:

- alpha: `ricochet-v2.0.0-alpha.N`;
- beta: `ricochet-v2.0.0-beta.N`;
- release candidate: `ricochet-v2.0.0-rc.N`;
- GA: `ricochet-v2.0.0`; and
- later 2.x releases: `ricochet-v2.Y.Z` with ordinary SemVer meaning.

Versioned payloads begin with the complete tag version and target, for example:

```text
ricochet-v2.0.0-alpha.1-windows-x64.zip
ricochet-v2.0.0-alpha.1-linux-x64.tar.gz
ricochet-v2.0.0-windows-x64-setup.exe
```

Cross-release metadata uses an explicit v2 namespace:

```text
RICOCHET2-ARTIFACTS-windows-x64.json
RICOCHET2-SHA256SUMS.txt
RICOCHET2-UPDATE-CHANNEL-candidate.json
```

Platform package identifiers and installer application IDs must also be
version-separated; ADR-008 will choose their exact spelling. Integrity,
signing, provenance, and update documents identify the product line, exact
version, target, schema version, and producing commit. An update client cannot
cross product lines.

### 5. Pre-GA stability

Pre-GA versions are usable evidence releases, not compatibility promises:

- **Nightly** may change any v2 source or artifact contract and is retained for
  diagnosis, not production dependency.
- **Alpha** publishes end-to-end vertical slices. Breaking changes are allowed
  when accompanied by an ADR/spec update and clear release notes.
- **Beta** freezes the intended 2.0 source grammar, standard-library naming,
  project manifest, and public package surface. A break after beta requires an
  explicit owner exception and a new beta.
- **Release candidate** accepts only release blockers: correctness, security,
  portability, performance-budget, documentation, or packaging defects. Any
  source-level design change returns the release to beta.
- **GA** begins the post-GA SemVer contract.

Pre-GA bytecode, image, interface, package, cache, and plugin schemas always
carry versions and may be rejected by a later toolchain. "Pre-GA may break"
does not permit a crash, ambiguous interpretation, or silent recompilation
against a different dependency graph. Incompatibility must fail with a useful
diagnostic and a reconstruction command where one is valid.

### 6. Post-GA stability and deprecation

Starting with `ricochet-v2.0.0`:

- patch releases preserve documented source and public API behavior while
  fixing defects;
- minor releases are backward-compatible additions and may deprecate APIs;
- source, package, or semantic removals require the next major product line;
- serialized compiler artifacts may use a narrower compatibility window, but
  the toolchain must identify the mismatch and rebuild from source when the
  lockfile permits it; and
- security fixes may disable an unsafe behavior immediately, with a security
  advisory explaining why the normal deprecation window was impossible.

A public API deprecated in 2.x remains available for the rest of the 2.x line
unless retaining it would violate a security boundary. The formatter and
automatic fixer may offer an explicit previewable migration. They do not
rewrite source merely because a compiler was upgraded.

The source specification, standard library, CLI, manifest, lockfile, registry
protocol, bytecode, package archive, plugin ABI, embedding ABI, and update
protocol each publish their own compatibility identifier. Sharing the number
`2` does not make those schemas interchangeable.

### 7. Ricochet 1.x LTS

The accepted 1.x scope is deliberately narrow:

- correctness fixes for documented behavior;
- security and necessary dependency fixes;
- crash, data-loss, packaging, installer, CI, and supported-platform repairs;
- documentation corrections for behavior that already exists; and
- deterministic test repairs that do not change the public contract.

New syntax, words, commands, packages, targets, language features, or semantic
redesigns do not enter 1.x. All maintenance releases are `1.0.x`; no `1.1.0`
is planned.

Ricochet 1.x is supported through Ricochet 2.0.0 GA and for at least twelve
months afterward. End of life is announced at least ninety days in advance.
This is a maintenance-scope promise, not a paid response-time SLA.

### 8. Non-destructive command handoff

The v2 installer follows these rules before the first alpha:

1. It installs into a v2-specific application directory with a distinct
   installer/application identity.
2. It never deletes, overwrites, renames, or takes ownership of a 1.x file,
   including the legacy 1.x `ricochet` alias.
3. It detects every `rco` and `ricochet` command visible on the prospective
   `PATH` and reports the resolved executable and version.
4. With the user's selected PATH integration, a new shell resolves `rco` to
   1.x and `ricochet` to 2.x. Resolution is achieved through isolated install
   directories and managed PATH ordering, not by altering the 1.x directory.
5. Uninstalling v2 removes only v2-owned files and PATH entries. The original
   1.x installation remains byte-for-byte intact.
6. Portable packages make no machine-wide PATH changes.

The acceptance matrix covers fresh install, v1-then-v2, v2-then-v1, upgrades
of each line, uninstall of either line, per-user and system PATH scopes,
portable packages, and paths containing spaces. Each case verifies binary
hashes and command resolution in a new shell.

## Diagnostics and tooling effects

`rco --version` and `ricochet --version` must make the product line obvious.
The v2 form eventually reports at least toolchain SemVer, commit, host target,
release channel, and compiler-artifact compatibility identifiers in human and
machine-readable forms.

Wrong-line failures name both sides. For example, a loader says that a file is
"Ricochet 1 bytecode" and that "Ricochet 2 accepts `.ricbc` schema N" rather
than reporting a generic parse failure. The package manager, LSP, debugger,
profiler, update client, and IDE discovery use the same identity service; none
independently guesses from a filename.

Release tooling validates tag, compiled version, artifact prefix, manifest,
checksums, signatures, update channel, and commit before publication. A
mismatch fails before a draft can be promoted.

## Postfix-vibe review

This ADR does not add executable language syntax. Its only source-like example
retains the language's value-before-operation reading:

```ricochet
"config/app.json" fs_read_text
```

The command split (`rco ...` versus `ricochet ...`) is shell syntax. Artifact
and tag punctuation never leaks into ordinary Ricochet words. Multiword public
words continue to use `_`, and no compatibility selector or leading-dot API is
introduced.

## Alternatives rejected

### Keep one branch and use SemVer alone

Rejected because every v2 compiler and runtime edit would share the maintenance
surface with 1.x, making accidental feature leakage and release confusion much
more likely.

### Make Ricochet 2 parse and run Ricochet 1 source

Rejected because it would constrain the new grammar, type system, stack
verifier, object model, runtime, and standard library before they are designed.
It would also make "no compatibility intended" false in practice.

### Rename the language or keep `rco` for v2

Rejected by the owner. The product remains Ricochet. The concise `rco` command
belongs to the maintained 1.x toolchain; the full `ricochet` command belongs to
2.x.

### Remove the legacy 1.x `ricochet` launcher

Rejected because it is a published user file and removal is unnecessary. The
v2 installer can establish command precedence without destructive mutation.

### Reuse unprefixed `v2.0.0` tags

Rejected because the repository's established unprefixed tag lane belongs to
1.x. A product-prefixed v2 tag is unmistakable in automation, mirrors, and
support reports.

### Promise compatibility between every compiler artifact

Rejected because bytecode, incremental caches, interfaces, packages, plugins,
and source have different security and evolution constraints. Each needs an
explicit schema and compatibility policy.

## Consequences

- Ricochet 2 can redesign the language without carrying a 1.x parser or VM.
- Users can install both lines and know which command they invoked.
- Release automation and support reports have unambiguous product identity.
- Fixes that affect both lines cost two reviews and two validations.
- Pre-GA users must expect source changes and environment reconstruction.
- The project must maintain 1.x for the stated overlap after 2.0 GA.
- Moving GitHub's default branch remains a later explicit decision; this ADR
  does not authorize it.

## Prototype evidence and operational proof

Evidence available when accepted:

- published baseline tag `v1.0.0` at
  `7a38423f8b02cdd63363f37ce9a524cf919426f7`;
- audited LTS-transition commit
  `e7d72ff925d6920dc8d44cc1cd84dd07c5205253`;
- the 1.x validation record in `RICOCHET_2_PLAN.md`, including 1,129 passing
  tests, repeated async-flake regressions, audit, documentation, editor, and
  acceptance checks;
- protected remote `main` and `ricochet-2` branches with force-push and
  deletion disabled; and
- distinct product-line and roadmap labels.

Evidence still required before the first alpha:

- the complete side-by-side installer matrix described above;
- v2 artifact magic/schema rejection tests in both directions;
- a release dry run proving tag, artifact, checksum, signature, provenance,
  and update-channel separation;
- clean-machine CLI, LSP, debugger, and environment discovery tests; and
- an independent proof user following only public installation and versioning
  documentation.

If those tests show that PATH ordering cannot provide a reliable
non-destructive handoff on a supported platform, release stops. The project
must choose a platform-specific shim or installer design in ADR-008; it may not
silently remove the 1.x alias.
