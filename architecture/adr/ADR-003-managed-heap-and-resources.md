# ADR-003: Managed heap and deterministic resources

Status: Proposed

Opened: 2026-08-08

Target decision: semantic contract before Phase 1 object implementation;
collector selection before Phase 4

Depends on: ADR-001, ADR-002, and ADR-010

Coordinates with: ADR-004 through ADR-007

## Context

Ricochet 2 targets developers who would otherwise use Python, Ruby, Node.js, or
PHP for applications. Requiring ownership and borrow annotations for every
string, object, list, and closure would work against that goal. At the same
time, garbage collection cannot safely manage files, sockets, transactions,
locks, foreign buffers, pins, or task scopes because their release time is
semantically important.

One mechanism should not pretend to solve both problems:

- ordinary application values need cycle-safe managed memory; and
- scarce or externally visible resources need deterministic ownership and
  cleanup.

The runtime must also support async tasks, multiple threads, native callbacks,
heap inspection, profiling, and eventually more than one execution backend.
Choosing a collector by familiarity before representative prototypes would
freeze object layout and FFI rules prematurely.

This ADR therefore proposes the language-level memory/resource contract and a
collector bakeoff boundary. It intentionally does not accept a production
collector yet.

## Proposed decision

### 1. Managed values and resource handles are different semantic classes

Ordinary strings, collections, closures, records when boxed, class instances,
tasks, exceptions/panics, reflection metadata, and application graphs live in a
cycle-safe managed heap. They may be freely shared within the limits of the
concurrency type rules. Programmers do not write lifetime or borrow parameters
for ordinary application objects.

Files, sockets, database connections/transactions, locks/guards, processes,
foreign allocations, pinned objects, and similar externally visible state are
affine resources. They have one logical owner, cannot be copied, and are
released deterministically on every scope exit unless ownership is explicitly
transferred.

An affine owner may contain other affine resources, allowing deterministic
resource composition. A resource cannot be hidden inside an unrestricted
managed cycle or copied by placing it in a normal collection. Resource-aware
containers and task transfers expose ownership in their types.

This is not a pervasive Rust-style ownership model. Move checking applies to
resource-bearing and explicitly affine values, not to every record, list, or
class reference.

### 2. Managed memory has a stable semantic contract

The language promises:

- cycles are reclaimable;
- collection timing is not observable program behavior;
- collection may move objects;
- managed references remain valid across collection;
- class identity is stable for the object's lifetime;
- identity and identity hashing do not expose a memory address;
- user code cannot depend on allocation order;
- user-defined GC finalizers do not exist in the 2.0 contract; and
- collection never runs arbitrary application code as a hidden callback.

Nominal records are value-oriented; managed classes have reference identity.
ADR-004 decides exact boxing and representation, but it cannot make a raw
address the meaning of identity. Identity comparison, if publicly exposed, is
an ordinary explicit word:

```ricochet
$first $second same_identity?
```

Equality remains a separate trait-governed operation. A moving collector must
not change either result.

The managed-reference abstraction is collector-independent. Bytecode,
debuggers, native backends, and plugins use typed handles or root APIs rather
than assuming that a reference is a permanently stable pointer.

### 3. Weak references are narrow and callback-free

Weak references are not required for the first bootstrap slice. If the
collector bakeoff or application framework demonstrates a real need, the
initial public contract is:

- `Weak<T>` does not keep `T` alive;
- upgrading it produces `Option<T>`;
- collection order is not observable;
- no user callback runs when the target disappears;
- weak identity/hash tables have separately documented iteration and cleanup
  semantics; and
- weak references cannot be used to manage files, sockets, transactions, or
  other resources.

Representative ordering remains postfix:

```ricochet
$weak upgrade
match
  Some(value) when
    $value use
  None when
    cache_miss
end
```

If these semantics cannot be implemented consistently across the selected VM
and native backend, weak references remain internal or move after 2.0 rather
than exposing collector-specific behavior.

### 4. Precise roots and stack maps are required

The runtime identifies managed roots precisely from:

- verified VM stack maps;
- globals and module state;
- managed object fields;
- task, channel, timer, and executor state;
- JIT/native safepoints and deoptimization metadata if applicable;
- registered embedding/native handles; and
- temporary roots created by runtime helpers.

Conservative scanning may be used only inside an isolated prototype for
comparison. It is not the release contract because false roots, moving
constraints, and unverifiable foreign stacks would leak into language
behavior.

Typed MIR marks every safepoint and live root before backend lowering. The
bytecode verifier confirms that stack maps agree with instruction states. A
backend cannot invent a different root model.

### 5. Collector implementation is selected by evidence

The runtime exposes an internal heap interface supporting allocation, tracing,
safepoints, roots, weak handles if approved, pinning, heap snapshots, and GC
telemetry. Object/reference semantics are defined above that interface.

At least these implementation families are compared:

1. a precise nonmoving tracing collector suitable for the simplest reference
   VM bootstrap;
2. a generational moving or mostly-moving collector aimed at application
   throughput and short-lived allocation; and
3. an incremental/concurrent or region-assisted tracing design if pause data
   shows that the simpler choices miss approved server budgets.

Reference counting without cycle collection is not a candidate. A hybrid may
be measured, but it must reclaim cycles and cannot make destructor timing a
language promise.

Phase 1 may use the simplest correct collector behind the abstraction to
unblock end-to-end compiler work. That bootstrap choice is explicitly
replaceable and cannot be called the production selection. Phase 4 accepts the
collector only after the bakeoff measures correctness, pauses, throughput,
memory overhead, compile/runtime complexity, async interaction, FFI pinning,
heap inspection, supported platforms, and maintenance burden.

Approved performance budgets are recorded with the bakeoff. This ADR does not
invent latency numbers without representative workloads and hardware.

### 6. Affine resources have explicit states

The type checker tracks each resource binding through states equivalent to:

```text
uninitialized -> available -> moved | explicitly released | scope-released
```

Branches join only with compatible ownership. Loops return resources to the
same state at every back edge. A consuming operation moves the handle; any
later use is a compile-time error.

The source scope proposed by ADR-001 is:

```ricochet
"data/report.txt" fs_open_read try file with
  $file read_all try
end
```

`with` consumes the acquired handle, creates the lexical binding, and installs
one cleanup edge. The binding cannot escape unless the scope explicitly
transfers ownership to a compatible return value, affine owner, or child scope.

Explicit transfer remains value-before-operation:

```ricochet
$file worker_scope transfer_resource
```

Exact transfer words and task rules belong to ADR-006, but an undocumented
capture is never a transfer.

An explicit close/release consumes the handle:

```ricochet
$file close try
```

The compiler recognizes that no implicit release remains after a successful
consuming close. If close fails while still retaining ownership, its API type
must say so; the solver cannot guess from a runtime flag.

### 7. Cleanup is lowered into explicit MIR edges

Cleanup is not a GC finalizer and not a hidden VM convention. MIR contains the
cleanup stack and edges for:

- normal fallthrough;
- explicit return;
- `Result` propagation;
- loop `break` and `continue`;
- pattern/control-flow exits;
- panic unwinding to the task boundary;
- cancellation;
- failed construction after partial resource acquisition; and
- task-scope join/cancel behavior.

Cleanups run once in last-acquired, first-released order. The optimizer may
remove a provably redundant cleanup but cannot reorder externally visible
release operations.

The proposed resource protocol separates two concerns:

1. **Release** is non-suspending, non-returning, and safe to invoke exactly once
   during unwinding. It relinquishes ownership and cannot be used to commit
   application data.
2. **Finish/commit/shutdown** is an explicit consuming operation that may fail
   or suspend and returns `Result`. Applications call it when success matters;
   the fallback release performs safe abort/rollback/close behavior.

For example, committing a transaction is explicit while scope release rolls an
unfinished transaction back:

```ricochet
$database begin_transaction try transaction with
  $changes $transaction apply_changes try
  $transaction commit try
end
```

This prevents a fallible commit from running invisibly during cancellation or
panic. Standard resource types document the safe fallback. A resource for which
abandonment is never valid may use a stricter must-finish type state, diagnosed
if no explicit terminal operation occurs.

Arbitrary user code does not become a hidden release hook. Application-defined
affine composites derive structural release from their fields. Custom native
release functions are declared in an unsafe module, have a restricted
non-suspending contract, and receive an audit record. The prototype must prove
whether a safe user-defined resource protocol can be admitted without making
unwinding reentrant or effect-unsound.

### 8. `defer` is structured, typed cleanup

A `defer`-equivalent form registers a block in the current deterministic scope.
It is checked when declared, runs once in LIFO order, accepts/leaves no operand
stack values, and captures affine values by move when required.

The exact ADR-001 spelling remains a prototype item. Its semantics must make
effects and possible suspension visible. A synchronous scope cannot hide an
async defer. Fallible work that determines application success stays explicit
in the body rather than being smuggled into a cleanup block.

The compiler can render the resulting cleanup plan in an advanced diagnostic
or MIR dump. There is no runtime list of untyped source strings to evaluate.

### 9. Views, guards, and borrows are local implementation tools

Slices, string/byte views, lock guards, and pinned views may borrow from an
owner without owning it. These are lexically constrained types inferred by the
compiler. They cannot:

- outlive or be returned beyond the owner unless their public type encodes a
  valid owner relationship;
- be stored in an unrestricted managed object;
- cross `await` or task transfer unless explicitly declared suspension-safe;
- coexist with a mutation that invalidates the view; or
- be converted to a raw pointer outside `unsafe`.

Ordinary application signatures do not acquire pervasive lifetime parameters.
When an API cannot be expressed safely with a short lexical view, it returns an
owned value or an explicit managed/resource handle instead.

### 10. FFI pinning and foreign allocation are affine

Foreign code cannot receive an unrooted managed reference or assume a stable
address. The supported mechanisms are:

- a scoped affine pin token that keeps a managed object alive and immobile;
- an embedding handle that roots an object without exposing its address;
- copied C-layout values and byte buffers;
- foreign-owned allocations represented by affine handles; and
- callback registrations that own explicit rooted state and unregister
  deterministically.

Representative source ordering is:

```ricochet
[
  $buffer pin pinned with
    $length $pinned address.get foreign_write
  end
] unsafe
```

The pointer view cannot outlive `$pinned`. A pin cannot cross `await` by
default, move to another task silently, or be stored in a managed graph. Long
pins receive diagnostics/telemetry because they may damage a moving
collector's performance.

Manual allocation is available only in unsafe modules and returns an affine
foreign-allocation handle. A naked pointer alone is not enough information to
free memory correctly. Callback panic and foreign-error containment are
explicit FFI contracts, not collector behavior.

### 11. Async and threads share one memory contract

The collector supports the executor, blocking/CPU pools, and supported native
threads without giving each subsystem an unrelated heap. Safepoints and root
publication have testable shutdown behavior.

Immutable managed values may cross task boundaries when their types satisfy the
send contract. Mutable sharing requires typed locks/atomics and the share
contract. Resource handles transfer only when their type and current state
permit it. Cancellation cannot abandon an owned resource or run release twice.

ADR-006 selects exact send/share traits and executor mechanics. This ADR
requires collector and cleanup prototypes to include cancellation storms,
cross-thread roots, blocked native calls, channels, locks, and task shutdown.

### 12. Heap and resource observability are first-class

The runtime exposes enough structured data for:

- heap size, live bytes, allocation rate, collection count, pause, and cause;
- per-type/object-retainer heap snapshots with privacy/redaction controls;
- task and native-root attribution;
- pin count and duration;
- live affine resource inventory by type, acquisition span, and owner scope;
- leaked embedding handles and unfinished must-finish resources; and
- profiler events that correlate allocation/collection with tasks and traces.

Telemetry cannot expose arbitrary object contents or secrets by default.
Debug builds may retain source acquisition spans; release overhead is measured
and configurable. The profiler and debugger consume the runtime's canonical
identity/root data instead of walking private collector structures directly.

## Postfix-vibe review

The proposed public forms preserve operand order:

- acquired handle, binding name, then `with`;
- resource receiver before `close`, `commit`, or another selector;
- managed reference before `downgrade`/`upgrade`;
- object before `pin`, pin receiver before `address.get`; and
- unsafe block before `unsafe`.

No destructor syntax is attached as a prefix annotation to ordinary calls. No
raw-pointer namespace-dot API is introduced. Multiword operations use `_`.
Resource movement remains visible as a consuming postfix operation.

## Diagnostics and tooling effects

Resource diagnostics use the `R` family reserved in ADR-001 and include:

- use after move/close;
- double release;
- incompatible ownership state at a branch or loop join;
- resource escape from `with` without an explicit transfer;
- resource capture by a non-affine closure or managed container;
- borrowed view outliving or mutating its owner;
- guard/pin/view crossing a forbidden suspension point;
- must-finish resource abandoned without its terminal operation;
- raw pointer or manual allocation outside `unsafe`; and
- invalid callback/root lifetime.

Each error points to acquisition, move/close, attempted use, and relevant scope
exit. The primary message uses application terms (for example, "file was moved
into worker task here") before compiler-state terminology.

The LSP shows ownership state and resource type on hover, highlights the scope
that owns cleanup, and offers only safe edits. It does not suggest `clone` for a
noncopyable file or insert `unsafe` automatically.

Debugger views distinguish managed identity, value equality, raw addresses,
pins, weak handles, and affine resources. A moving collection does not make a
watch expression refer to a different object.

Build artifacts include precise stack/root maps. Heap/resource inspection data
is schema-versioned and excluded or redacted according to the build profile.

## Alternatives rejected at proposal time

### Pervasive ownership and borrowing for every value

Rejected because Ricochet is an application-first managed language. The
complexity is not justified for ordinary strings, records, collections, and
class graphs. Ownership remains targeted at resources and unsafe views.

### Garbage collection for external resources

Rejected because GC timing is intentionally unobservable and nondeterministic.
Files, locks, transactions, and foreign buffers need predictable release on
all exits.

### User-defined finalizers

Rejected because finalizers expose collector timing, resurrect objects, run at
unsafe times, complicate shutdown, and still do not guarantee timely resource
release.

### Reference counting without cycle collection

Rejected because ordinary application/object graphs contain cycles. Leaking a
cycle or requiring users to break it manually violates the managed-memory
contract.

### Make every managed reference a stable raw address

Rejected because it prevents moving/compacting collectors, leaks
implementation detail into equality/FFI, and makes native misuse difficult to
diagnose. Pins and handles provide explicit stable boundaries.

### Run arbitrary fallible or async destructors during unwinding

Rejected because hidden suspension and error replacement make cancellation and
panic behavior unpredictable. Significant finish/commit work is explicit;
fallback release is restricted and deterministic.

### Select a production collector before prototypes

Rejected by the product charter's evidence rule. Collector choice affects
latency, throughput, object layout, FFI, debugger support, and maintenance and
must be measured on Ricochet workloads.

## Prototype evidence

No Ricochet 2 heap or affine-resource prototype exists yet. The 1.x runtime's
Rust ownership and synchronization are implementation experience, not evidence
that user-visible Ricochet 2 semantics or GC behavior work.

The semantic/resource prototype must prove:

1. cycles, class identity, value equality, and stable identity hashes across
   repeated collection and movement simulation;
2. precise roots for VM stacks, globals, closures, tasks, channels, native
   helpers, callbacks, and embedding handles;
3. use-after-move, branch/loop ownership joins, partial construction, explicit
   transfer, structural affine owners, and resource-container rejection;
4. exactly-once LIFO cleanup across every CFG exit, result propagation, panic,
   cancellation, and task-scope shutdown;
5. explicit commit/finish versus fallback rollback/release behavior, including
   failure injection;
6. view, guard, pin, raw-pointer, callback, and foreign-allocation lifetime
   diagnostics;
7. heap snapshots, resource inventories, pin telemetry, and debugger identity;
8. deterministic behavior under randomized GC safepoints and cancellation;
9. bytecode stack/root-map verification rejecting corrupted artifacts; and
10. an independent proof application using files, database transactions,
    async tasks, cancellation, and one small C library from public docs alone.

The collector bakeoff then runs the same versioned workloads against each
candidate family:

- short-lived CLI allocation and startup;
- long-lived HTTP/API service with realistic request graphs;
- large cyclic object graph and cache churn;
- async channels/timers with cancellation storms;
- multithreaded CPU and blocking-pool work;
- FFI callbacks, embedding handles, copied buffers, and bounded pins;
- heap snapshot/profiler capture under load; and
- constrained-memory and sustained-throughput cases on every supported OS.

Results include correctness failures, pause distributions, throughput, peak and
steady memory, fragmentation, pin impact, startup, binary size, implementation
complexity, platform gaps, debugging quality, and maintenance risk. Raw data,
workload source, hardware/OS details, and reproduction commands are committed.

## Acceptance criteria

Accept the semantic portion only when:

- ADR-002 proves affine state and cleanup edges in typed CFG/MIR;
- every abnormal exit releases each owned resource exactly once;
- managed cycles are reclaimed without user intervention;
- object identity does not depend on address or collector choice;
- FFI roots/pins are explicit, bounded, and verifier-visible;
- async cancellation and cross-thread roots have deterministic tests;
- diagnostics explain ownership without imposing annotations on ordinary
  managed code; and
- the owner approves the proof application and source surface.

Accept a production collector only when its bakeoff meets the approved
correctness, latency, throughput, memory, tooling, platform, and maintenance
budgets. A simple bootstrap collector may ship in pre-alpha builds with a clear
artifact/schema warning, but it cannot silently become the GA choice through
inertia.

If no candidate meets the budgets, Phase 4 stops. The project may revise
object-layout or backend assumptions in a superseding ADR; it may not weaken
cycle safety, deterministic resources, or explicit FFI lifetime boundaries
without an owner decision.

## Consequences if accepted

- Ordinary application programming remains managed and relatively low
  ceremony.
- External resources receive compile-time move checking and deterministic
  cleanup.
- The VM/backend must generate precise roots, stack maps, safepoints, and
  cleanup edges from the beginning.
- FFI code uses handles and scoped pins instead of treating managed references
  as stable pointers.
- User-defined finalizer patterns are unavailable; applications use explicit
  scopes and finish operations.
- A bootstrap collector can unblock compiler work without prematurely freezing
  the production runtime.
- Runtime observability must be designed with the collector rather than bolted
  on after object layout becomes inaccessible.
