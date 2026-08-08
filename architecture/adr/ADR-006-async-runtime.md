# ADR-006: Async runtime and structured concurrency

Status: Open

Opened: 2026-08-08

Target decision: before Phase 5 runtime implementation

Depends on: ADR-001 through ADR-005

## Context

Ricochet 2 requires async callables, structured task scopes, cancellation,
deadlines, bounded channels, select, task locals, locks, atomics, and a memory
model. These cannot be added as unrelated host words: the executor, timers,
network reactor, blocking pool, cleanup semantics, context propagation, and
debugger must agree on task lifetime.

This record is open. It does not yet select an executor/reactor implementation
or promise that every backend uses identical internal scheduling.

## Decision scope

The proposal must define:

- `Task<T>`, async-call, postfix `await`, scope, group, and explicit detached
  task semantics;
- cancellation tokens, deadlines, timeouts, cancellation points, and masking;
- child join/cancel/transfer rules on every scope exit;
- bounded `Channel<T>`, close, sender/receiver ownership, backpressure, and
  fair multi-source select;
- task locals and trace/capability context propagation;
- executor, timer, network-reactor, blocking-pool, and CPU-pool boundaries;
- `Mutex<T>`, `RwLock<T>`, signals/conditions, atomics, poisoning/failure rules,
  and the documented memory model;
- compiler-checked send/share traits; and
- shutdown, panic containment, resource cleanup, and native callback behavior.

Representative ordering remains postfix:

```ricochet
[
  $primary fetch spawn primary_task let
  $backup fetch spawn backup_task let

  $primary_task $backup_task select_first await
] task_scope

100 Channel<Job> bounded jobs let
$job $jobs send await
$jobs receive await
```

The channel capacity and type precede construction; message arguments remain
below the channel receiver before `send`.

## Diagnostics and tooling pressure

The compiler must diagnose a task escaping its scope, non-sendable captures,
guards/pins crossing forbidden `await`, unbounded-channel requests where policy
forbids them, and cleanup that can be cancelled halfway through. Diagnostics
show the spawn, owner scope, suspension, transfer, and exit edges.

Debugger/profiler views need task trees, states, await reasons, deadlines,
channel occupancy/waiters, lock ownership, blocking-pool work, context, and
cancellation history without changing scheduling materially.

## Alternatives already rejected

- One operating-system thread per async task is not acceptable.
- Detached tasks as the default are incompatible with structured concurrency.
- Unbounded queues as the only channel primitive hide backpressure.
- Retrofitting cancellation after the first async API is rejected.
- Distributed actors, durable mailboxes, and OTP-equivalent clustering are
  post-2.0 ecosystem work rather than runtime prerequisites.

## Prototype evidence

No Ricochet 2 async-runtime prototype exists yet. Required evidence includes
deterministic virtual-time tests, cancellation at every suspension point,
bounded-channel pressure, select fairness, nested task failure, shutdown,
blocking native calls, CPU saturation, locks/atomics, send/share rejection,
resource cleanup, debugger inspection, and long-running leak checks.

Candidate executor/reactor designs run identical workloads on every supported
OS and through the reference VM plus native-backend prototype. The decision
records tail latency, throughput, idle cost, memory, wakeups, shutdown time,
debuggability, implementation complexity, and platform gaps.
