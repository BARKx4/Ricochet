# Ricochet 2 architecture decisions

This directory is the decision record for the incompatible Ricochet 2 product
line. The product charter and phase gates live in
[`RICOCHET_2_PLAN.md`](../RICOCHET_2_PLAN.md). These records turn that charter
into reviewable language, compiler, runtime, tooling, and release contracts.

| ADR | Status | Decision |
| --- | --- | --- |
| [ADR-001](adr/ADR-001-typed-postfix-surface.md) | Proposed | Typed postfix source surface |
| [ADR-002](adr/ADR-002-type-and-stack-solver.md) | Proposed | Type and stack solver |
| [ADR-003](adr/ADR-003-managed-heap-and-resources.md) | Proposed | Managed heap and deterministic resources |
| [ADR-004](adr/ADR-004-object-and-value-representation.md) | Open | Object and value representation |
| [ADR-005](adr/ADR-005-effects-and-capability-authority.md) | Open | Effects and capability authority |
| [ADR-006](adr/ADR-006-async-runtime.md) | Open | Async runtime and structured concurrency |
| [ADR-007](adr/ADR-007-backend-bakeoff.md) | Open | Reference VM and backend bakeoff |
| [ADR-008](adr/ADR-008-modules-environments-packages-and-trust.md) | Open | Modules, `.rvenv`, packages, and trust |
| [ADR-009](adr/ADR-009-application-platform-boundaries.md) | Open | Application platform boundaries |
| [ADR-010](adr/ADR-010-compatibility-and-release-policy.md) | Accepted | Compatibility and release policy |

`Open` records define a decision's scope but do not choose a design. `Proposed`
records are concrete designs to prototype, not permission to ship a language
contract. They become `Accepted` only after their evidence gates pass and the
owner approves the resulting tradeoffs. If evidence invalidates a proposal,
preserve the record and supersede it; do not rewrite history to make the first
idea look inevitable.

Every technical ADR must include:

- realistic postfix examples and a postfix-vibe review;
- consequences for modules, generics, async, FFI, debugging, and packaging;
- stable diagnostic and tooling implications;
- rejected alternatives;
- prototype evidence already collected and evidence still required; and
- explicit acceptance and rollback criteria.

These Markdown files are source-controlled architecture data. Public Ricochet
documentation remains the rendered HTML surface under `docs/`.
