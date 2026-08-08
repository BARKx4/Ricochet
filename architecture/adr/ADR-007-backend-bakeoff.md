# ADR-007: Reference VM and backend bakeoff

Status: Open

Opened: 2026-08-08

Target decision: initial bakeoff in Phase 1; production-mode selection in
Phase 8

Depends on: ADR-002, ADR-003, ADR-004, and ADR-006

## Context

Ricochet needs fast iteration, deterministic semantics, useful diagnostics,
debugging/profiling, native packaging, and a credible performance path. A
versioned bytecode VM is the reference and bootstrap route. A native optimized
backend is valuable only if it improves real applications enough to justify
compile time, platform, ABI, GC, and maintenance cost. WebAssembly is a later
target program, not a hosted-GA prerequisite.

This record is open. It does not select Cranelift, LLVM, a custom backend, or a
specific JIT/AOT split.

## Decision scope

The proposal must define:

- the backend-neutral verified IR contract and semantic ownership by the
  reference VM;
- bytecode magic, schema, verifier, source maps, stack/root maps, and loader;
- interpreter dispatch and optional baseline/JIT strategies;
- native AOT/JIT candidate backends and supported target/toolchain matrix;
- GC safepoints, deoptimization if needed, exceptions/panics, async lowering,
  and FFI callbacks in every backend;
- debug information, coverage, profiler symbols/events, stack traces, and
  reproducibility;
- optimization levels, cache keys, build modes, and fallback rules; and
- the performance/deployment budgets that would make native codegen a GA
  blocker rather than a post-GA optimization.

All backends must produce the same observable result for postfix source:

```ricochet
20 22 +
[ 2 * ] $numbers transform
$task await
```

Backend choice cannot reorder visible effects, cleanup, channel operations, or
FFI calls.

## Diagnostics and tooling pressure

Users must be able to ask which backend, optimization mode, target, and cache
artifact ran. A verifier failure identifies schema, function, instruction,
expected stack/root state, and producer version without executing the payload.

Debugger, coverage, profiler, and crash reports map optimized code to stable
source IDs and inlined frames. Backend fallback is explicit in build output and
machine-readable metadata, never silent release-mode behavior.

## Alternatives already rejected

- Freezing a native backend by preference before the bakeoff is rejected.
- Making Rust ABI a public Ricochet ABI is rejected.
- Letting optimized code define semantics differently from the reference VM is
  rejected.
- WebAssembly, SIMD, `no_std`, embedded, and kernel work cannot delay the hosted
  application GA unless an explicit later owner decision changes the charter.

## Prototype evidence

No Ricochet 2 backend prototype exists yet. The bakeoff uses versioned language
and application workloads covering startup, compiler throughput, numeric and
allocation-heavy code, objects/traits/generics, async I/O, channels, GC, FFI,
debugging, profiling, coverage, packaging, and corrupted artifacts.

Results include correctness, build and incremental time, runtime distributions,
memory, binary size, platform coverage, debug quality, reproducibility,
implementation complexity, dependency/security burden, and maintenance cost.
Raw data and reproduction commands are preserved even for rejected backends.
