# ADR-002: Type and stack solver

Status: Proposed

Opened: 2026-08-08

Target decision: before Phase 1 type-checker implementation

Depends on: ADR-001 and ADR-010

Coordinates with: ADR-003 through ADR-008

## Context

A statically typed postfix language must answer two questions at every program
point:

1. what type does each value have; and
2. what complete stack shape exists before and after each operation?

Checking only top-of-stack operands is insufficient. Branches can leave
different heights, loops can grow a stack every iteration, early returns can
skip cleanup, generic words can accidentally consume a caller's hidden value,
and dynamic or trait dispatch can conceal an incompatible effect. A traditional
expression type checker added after bytecode generation would discover these
problems too late.

The solver also has to power fast editor feedback, separate compilation,
stable diagnostics, verified bytecode, generic collections, traits, associated
types, `Dynamic` validation, async lowering, affine resources, and FFI. This ADR
proposes one semantic model and a prototype plan. It does not select a specific
Rust solver library or commit to monomorphized versus shared machine-code
representation.

## Proposed decision

### 1. One checked model owns values and stacks

Ricochet uses a unified constraint solver over:

- value types;
- ordered stack rows;
- callable/block signatures;
- generic type parameters and trait goals;
- control-flow refinements;
- bounded effect sets; and
- affine move state for resource-bearing values.

Name resolution builds HIR before solving. The solver produces typed HIR and a
control-flow graph with a proven input/output stack for every node. MIR lowering
then makes moves, cleanup edges, effects, suspension points, and dispatch
explicit. No backend IR or executable bytecode is emitted from a function with
an unresolved error.

The compiler may use recoverable error types internally so one mistake does not
erase all editor feedback. Those recovery values never satisfy a release build
or bytecode verifier.

### 2. The type universe is closed enough to be sound and open enough for apps

The initial semantic categories are:

- fixed-width signed and unsigned integers and floating-point numbers;
- `Bool`, `Char`, Unicode `String`, `Bytes`, and explicit text/byte views;
- `Unit` as an ordinary single value and `Never` for a nonreturning path;
- tuples and nominal records;
- closed enums, including `Option<T>` and `Result<T, E>`;
- managed class references and explicit dynamic trait objects;
- arrays, lists, maps, sets, ranges, iterators, generators, and slices/views;
- callable, closure, task, channel, lock, atomic, and capability types;
- affine resource, guard, pin, and foreign-allocation handles;
- raw pointers and C-layout values restricted by `unsafe`; and
- explicit `Dynamic` values.

There is no ambient nullable reference type. `Option<T>` represents ordinary
absence. A nullable foreign pointer remains a distinct unsafe FFI type and
cannot flow into a managed reference without a checked conversion.

Numeric and string conversions are explicit. Literal constraints may remain
unresolved briefly during local inference, but every compiled literal has a
concrete representable type. Defaulting rules are small, deterministic,
documented, and never used to make an otherwise-invalid public API compile.

An empty output list is different from returning one `Unit` value:

```ricochet
( message: String -> uses console ) log_line public function
( -> Unit ) unit_value public function
```

The distinction matters to stack verification.

### 3. Stack rows are ordered types, not integer heights

The internal notation uses a row variable `ρ` for the caller-owned stack below
a callable's declared inputs. Appending a type is written here as `ρ · T`.
These are documentation symbols, not proposed executable syntax.

Representative primitive signatures are:

```text
drop : forall ρ T.       ρ · T         -> ρ
dup  : forall ρ T:Copy.  ρ · T         -> ρ · T · T
swap : forall ρ A B.     ρ · A · B     -> ρ · B · A
over : forall ρ A:Copy B. ρ · A · B    -> ρ · A · B · A
+    : forall ρ N.       ρ · N · N     -> ρ · N      where N: Add<Output=N>
```

`Copy` excludes affine resources. `drop` is still a consuming operation for
every `T`; when `T` owns a resource, MIR lowers that consumption through the
deterministic release path from ADR-003 rather than discarding the handle.

User signature inputs are listed from deeper to shallower stack position:

```ricochet
( left: Int right: Int -> Int ) add_numbers public function
  $left $right +
end
```

Its elaborated signature is:

```text
forall ρ. ρ · Int(left) · Int(right) -> ρ · Int
```

At function entry the declared inputs become immutable lexical parameters. The
body operates above the preserved `ρ`; it cannot inspect or consume an
undeclared caller value. At return, the declared outputs are appended to the
same row. This isolation gives separate compilation a real function boundary
instead of making every function depend on an unknown ambient stack.

Locally inferred blocks may transform an explicit row because higher-order
stack combinators need that power. Their full row signature is inferred and
stored in HIR/interface metadata:

```text
Block<forall ρ. ρ · Int -> ρ · String uses {}>
```

Escaping closures cannot retain a reference to a transient stack slot. Captures
are copied, moved, or held through a managed reference according to their type.

### 4. Every control-flow edge carries a complete stack state

The solver propagates ordered stack types through the control-flow graph.
Reachable edges may join only when they have:

- the same height above the protected row;
- unifiable types in every position;
- compatible affine ownership state;
- compatible narrowing facts; and
- effects valid for the enclosing callable.

For example, this is invalid because the arms leave different types:

```ricochet
$enabled if
  1
else
  "one"
end
println
```

The compiler does not silently widen both to `Dynamic`. A programmer can
construct an enum, choose an explicit trait object, or validate an intentional
dynamic boundary.

Nominal subclass values may widen to an explicitly expected superclass at a
join. The solver does not search for a surprising common trait merely to make a
branch compile. If more than one sensible join type exists, an annotation or
constructor is required.

Loop entry and every back edge have exactly the same stack and affine state.
This rejects accidental growth:

```ricochet
$keep_running while
  1
end
```

A loop that carries state does so through explicit bindings or a documented
loop combinator signature. `break`, `continue`, early `return`, result
propagation, panic, and cancellation each become explicit CFG exits before
verification.

`Never` paths do not participate in a normal join. Unreachable code is still
parsed and name-resolved for editor recovery, but it cannot hide an invalid
reachable stack effect.

### 5. Inference is local and bidirectional

Ricochet infers:

- local immutable and mutable binding types;
- private callable details when all call sites and the declaration boundary
  provide enough information;
- generic substitutions at call sites;
- closure and block stack effects;
- enum/option/result narrowing after patterns and checked predicates; and
- local bounded effects.

Ricochet requires explicit signatures for:

- exported functions and methods;
- public/package/module fields and constructors;
- public generic types and trait members;
- FFI imports, exports, callbacks, and C-layout records;
- persistent database/config/message schemas; and
- any boundary emitted into a separately compiled interface artifact.

Inference never crosses a package boundary by loading dependency source.
Dependents type-check against the published interface metadata.

Expected types flow inward from annotations, returns, constructor fields, and
generic parameters. Synthesized types flow outward from literals, words, calls,
and patterns. Constraints meet at a local boundary; the compiler does not infer
one giant whole-program type graph for ordinary development.

Mutable cells are never implicitly generalized. Immutable local values may be
generalized only when the value/effect rule proves that doing so cannot hide a
mutable, resource, async, or capability-bearing computation. Public generic
parameters remain explicit even when local calls make them obvious.

### 6. Generics are first-order and parametric

Ricochet 2 supports generic functions, records, enums, classes where valid,
traits, and standard collections. Generic parameters may have ordinary trait
bounds and associated-type equality constraints.

The semantic interface is parametric: code cannot inspect `T` or branch on its
representation unless a reflection or trait capability explicitly permits it.
Whether a backend monomorphizes, shares code, or uses a hybrid representation
is an ABI/performance decision for ADR-004 and ADR-007. It cannot change source
behavior.

The initial system is rank-1. Higher-rank forms may be added only where a
specific standard abstraction requires them and the solver prototype remains
decidable. Higher-kinded types, open type functions, and user-defined type
operators are not 2.0 requirements.

Variance is explicit in compiler/interface metadata and conservative by
default. Mutable containers are invariant. Read-only views may be covariant
when the representation and lifetime rules prove it safe. FFI and affine
resource types do not receive inferred variance shortcuts.

### 7. Trait solving is coherent and deterministic

Traits provide constraints, shared behavior, associated types/functions,
standard operator protocols, and explicit dynamic dispatch. They are nominal;
matching method names alone does not implement a trait.

An implementation is legal only when its package owns the trait or the
implementing nominal type. Overlapping implementations are rejected. The same
program cannot select different implementations because dependency resolution
or import order changed.

Trait goals are solved from:

- explicit generic bounds;
- visible coherent implementations;
- associated-type equalities;
- compiler-known rules for primitive and structural built-ins; and
- explicit auto-trait rules such as send/share, once ADR-006 defines them.

The solver has documented recursion and complexity limits. Exceeding a limit is
a coded diagnostic that includes the unresolved goal chain, not a compiler
hang. Ambiguous goals fail and list the viable candidates. There is no
"first implementation wins" behavior.

Static dispatch is the default for generic bounds. Runtime trait objects are
explicit in the type and carry only object-safe members defined by ADR-004.
Converting a value to a trait object is an observable checked coercion, not a
fallback performed to repair inference.

Operators map to a fixed set of standard traits. A package cannot invent a new
meaning for parser punctuation or add an implementation that violates
coherence.

### 8. Subtyping is narrow and visible

Nominal classes have at most one superclass. A class reference can widen to a
declared superclass. Records and enums do not acquire structural subtyping just
because their fields or cases happen to match.

Trait bounds are constraints, not implicit subtyping. Dynamic trait objects are
explicit erased types. Generic containers do not automatically inherit the
subtyping relation of their elements; variance rules decide each case.

There is no universal implicit `Any` type that absorbs an error. `Dynamic` is a
real explicit type with restricted operations. `Never` can flow to any expected
output only because it does not return.

### 9. `Dynamic` is a checked boundary, not an escape hatch

Untyped JSON, reflection, plugin payloads, and foreign host values may enter as
`Dynamic`. They remain dynamic until code performs a checked validation or
pattern operation. A conversion to `T` returns a typed result or a narrowing
construct with an explicit failure path:

```ricochet
$payload User validate_dynamic
match
  Ok(user) when
    $user save
  Err(problems) when
    $problems report_validation
end
```

There is no implicit `Dynamic` to `T` conversion, arithmetic coercion, method
lookup, or branch-join widening. Dynamic member/key access returns `Dynamic` or
a typed validation result according to its API; it never pretends that a
runtime string is a statically known field.

Successful checks narrow the value only on the dominated control-flow path.
Mutation or aliasing that could invalidate a fact ends the narrowing. The LSP
shows both the declared and narrowed type.

Crossing `Dynamic` may carry a bounded effect selected in ADR-005 if runtime
type inspection can fail or invoke host behavior. The type solver records that
boundary rather than handling it as a special untracked VM operation.

### 10. Effects, resources, and concurrency are solver inputs

Value/stack checking and effect/resource checking share the CFG but report
separate diagnostics.

- A call's inferred effect set must be a subset of the enclosing callable's
  declared or handled effects.
- An affine value changes from available to moved/closed on the consuming edge.
- All reachable exits run or transfer required cleanup.
- A borrow/view/guard cannot outlive the owner or cross a suspension point
  unless its type explicitly permits that transfer.
- Values sent to another task/thread satisfy the send/share contracts selected
  by ADR-006.
- Unsafe pointer and layout operations occur only under the `unsafe` effect and
  module boundary.

ADR-003 defines the resource-state lattice and cleanup semantics. ADR-005
defines effects. ADR-006 defines concurrency traits. This solver ADR requires
their facts to be represented in the same typed CFG so one analysis cannot
invalidate another after the fact.

### 11. Callables and bytecode carry verifiable contracts

Every callable, closure, block, method, and callback has a complete checked
contract containing:

- ordered input and output stack types;
- lexical parameter types;
- generic parameters and resolved bounds;
- effect set;
- affine/resource transfer behavior;
- visibility and dispatch form; and
- source and interface identity.

Versioned bytecode records canonical type IDs and per-function stack maps
sufficient for a loader to verify instruction operands, branches, calls,
returns, exception/panic boundaries, cleanup edges, and GC roots without
trusting producer assertions. A malformed artifact is rejected before
execution.

Interface and bytecode type IDs are stable within their declared schema, not
globally forever. They are never raw process addresses or hash-table iteration
positions.

### 12. Incremental queries are deterministic

Parsing, name resolution, signature collection, constraint generation, trait
solving, body checking, MIR verification, and interface emission are separate
memoized queries with explicit inputs. Cache keys include compiler/schema
version, target assumptions, enabled features, dependency interface hashes,
and relevant capability/FFI configuration.

The same query implementation serves `ricochet check`, build, test, docs, LSP,
and debugger expression validation. The LSP does not have a permissive shadow
type checker.

Canonicalization and diagnostic ordering are deterministic. Parallel checking
cannot change which trait implementation wins, which numeric type defaults, or
which error appears first.

## Representative checks

### Generic stack preservation

```ricochet
<T: Copy>
( value: T -> T T ) duplicate public function
  $value $value
end
```

Elaboration:

```text
forall ρ T. ρ · T -> ρ · T · T
```

### Exhaustive narrowing

```ricochet
( value: Option<Int> -> Int ) value_or_zero public function
  $value match
    Some(number) when
      $number
    None when
      0
  end
end
```

Both arms leave `ρ · Int`; `number` exists only in the first arm.

### Invalid stack join

```ricochet
$condition if
  1 2
else
  3
end
```

The diagnostic identifies the `else` join and renders:

```text
then: ... · Int · Int
else: ... · Int
                 ^ missing one output value
```

### Invalid generic call

```ricochet
1 "two" add_numbers
```

The diagnostic identifies the second input, shows the required signature, and
does not speculate about string-to-number coercion.

### Explicit dynamic validation

```ricochet
$decoded Order validate_dynamic try order let
$order total.get
```

The field access is typed only after the successful result-propagation edge.

## Postfix-vibe review

The solver does not introduce runtime syntax. Its model reinforces postfix
semantics by treating each word as an ordered stack transformation, preserving
the caller row, and showing errors as before/after stacks. Generic resolution,
coercion, and dynamic validation cannot secretly reorder values.

The source examples keep values before calls, scrutinees before `match`, and
results before `try`. Internal symbols such as `ρ` and `·` appear only in
compiler documentation and diagnostics, not as executable words.

## Diagnostics and tooling effects

Every diagnostic includes:

- a stable code and severity;
- one primary source span and any causal secondary spans;
- the expected and actual value type or ordered stack state;
- the callable/trait/branch path that introduced the constraint;
- relevant generic substitutions and effect/resource facts;
- a concise explanation that does not require solver terminology; and
- a fix only when the edit is mechanically safe.

Core diagnostic families include:

- stack underflow or undeclared caller-row access;
- stack height/type mismatch at branch, loop, match, return, and callback joins;
- ambiguous or unsatisfied generic/trait goals;
- conflicting coherent implementations;
- missing public annotations;
- implicit or failed `Dynamic` conversion;
- nonexhaustive or unreachable patterns;
- use-after-move and cleanup escape (owned by ADR-003); and
- undeclared effect or unsafe boundary (owned by ADR-005).

The LSP can request a partial typed result after an edit, but code generation
and tests see the same unresolved errors as `check`. Hover shows source type,
elaborated stack signature, effects, bounds, and current narrowing. Inlay hints
are optional presentation; they do not mutate source or become required to
understand stack order.

Compiler tracing can explain a selected trait implementation or inference
failure in a machine-readable form. Normal errors remain compact and avoid a
raw constraint dump.

## Alternatives rejected at proposal time

### Check only stack height

Rejected because equal-height stacks can contain incompatible values, GC roots,
resources, and callback contracts.

### Translate to an expression AST and ignore source stack states

Rejected because it loses the language's observable composition model and
makes branch/loop stack errors indirect. HIR may use expression-like nodes for
analysis, but it must preserve and verify source stack transformations.

### Let functions consume an arbitrary caller stack

Rejected for public/ordinary callables because it prevents separate
compilation, makes refactoring unsafe, and produces action-at-a-distance. Row
polymorphism preserves the caller-owned prefix without exposing it.

### Infer public APIs across package boundaries

Rejected because builds would depend on dependency source and inference order,
and small implementation edits could silently change consumers.

### Widen failed joins to `Dynamic` or a universal `Any`

Rejected because it converts static mistakes into runtime failures and defeats
nullability, trait, and stack guarantees.

### Structural traits and overlapping implementations

Rejected because method-name coincidence and import order would control
behavior. Nominal implementation plus package coherence is predictable and
cacheable.

### Pervasive explicit stack-row annotations

Rejected for ordinary source because they add ceremony without improving most
public APIs. Rows are inferred, stored, displayed by tooling, and available in
advanced interfaces only when genuinely needed.

### Make backend representation part of generic semantics

Rejected because monomorphization versus shared representation should be
selected from performance and ABI evidence without changing what programs
mean.

## Prototype evidence

No Ricochet 2 solver exists yet. The 1.x VM and compiler exercise dynamic stack
execution, but they do not establish the static soundness claimed here.

ADR-002 cannot become Accepted until a standalone solver prototype proves:

1. primitive and row-polymorphic words, named callable parameters, multiple
   outputs, closures, and higher-order blocks;
2. branch, loop, match, early-return, result-propagation, and unreachable-path
   verification;
3. local bidirectional inference with explicit public interface emission;
4. generic functions/types, trait bounds, associated types, coherent impls,
   ambiguity reporting, and termination limits;
5. `Option`, `Result`, exhaustive narrowing, and explicit `Dynamic`
   validation;
6. integration facts for affine moves, effects, suspension, send/share, and
   unsafe FFI, using temporary models until their ADRs are accepted;
7. incremental invalidation where a private-body edit does not recheck an
   unchanged dependent interface;
8. deterministic results under randomized declaration and parallel query
   scheduling;
9. invalid-program fuzzing with no hangs or compiler crashes; and
10. diagnostic snapshots that an independent user can act on without reading
    compiler source.

The prototype corpus includes every example in ADR-001 and this ADR, plus the
Phase 1 and Phase 2 proof applications. Results record solver time, memory,
incremental invalidation, diagnostic stability, and unresolved design cases.

## Acceptance criteria

Accept this ADR only when:

- the prototype has no known soundness hole for its implemented feature set;
- callable isolation and row polymorphism support the proposed standard
  collection and control words without hidden stack access;
- trait solving is deterministic, coherent, terminating, and separately
  compilable;
- `Dynamic`, unsafe, resources, and effects remain explicit boundaries;
- typed HIR can lower to verifiable MIR and bytecode metadata;
- the editor can recover from incomplete code without accepting it for build;
- performance fits the approved interactive and clean-build budgets; and
- the owner approves the diagnostics and proof reports.

A prototype that compiles only hand-authored happy paths, requires whole-world
inference, or repairs mismatches with implicit dynamic behavior is rejected.
Its evidence is preserved and a superseding ADR narrows or replaces the model.

## Consequences if accepted

- Stack safety becomes a compile-time property across all reachable control
  flow rather than a VM convention.
- Public packages can be checked from stable interface artifacts.
- Generic and trait features remain predictable enough for incremental builds.
- Backend and object-representation experiments can change without changing
  source semantics.
- The compiler must build real CFG/MIR infrastructure early; a one-pass
  word-to-bytecode compiler is no longer sufficient.
- Error quality and incremental query design are part of the type system's
  acceptance, not cleanup work after feature completion.
