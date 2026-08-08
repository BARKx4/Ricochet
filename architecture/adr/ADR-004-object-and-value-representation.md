# ADR-004: Object and value representation

Status: Open

Opened: 2026-08-08

Target decision: before Phase 2 data-layout implementation and Phase 3 object
dispatch

Depends on: ADR-001, ADR-002, and ADR-003

## Context

Ricochet 2 needs primitive values, tuples, records, enums, managed classes,
traits, closures, generics, reflection, FFI layout, debugger views, and more
than one backend. Treating every value as the same boxed runtime object would
be simple initially but could make numeric code, FFI, memory use, and native
code generation needlessly expensive. Exposing backend layout directly would
make later collector and backend work unsafe.

This record is open. It defines what the representation bakeoff must decide; it
does not yet select a tagged-value scheme, object header, dispatch table, or
generic representation.

## Decision scope

The proposal must define:

- canonical representations for primitives, tuples, records, enum tags and
  payloads, managed class references, closures, trait objects, and `Dynamic`;
- value equality, class identity, hashing, copying, moving, and boxing rules;
- generic representation and the boundary between specialization and shared
  code;
- field order, padding, alignment, object headers, and collector metadata;
- static, virtual, final, trait, and dynamic dispatch;
- object-safe trait members and associated-type restrictions;
- standard operator traits and overload resolution;
- typed reflection metadata and what is retained or stripped by build profile;
- debugger/profiler rendering and stable source/type identity; and
- a separate explicit C-layout family that never relies on internal managed
  layout.

The source pressure case must remain postfix and use capitalized OOP meta words:

```ricochet
Point public Record
  x F64 public Field
  y F64 public Field
end

Shape public Trait
  ( -> F64 ) area public required Method
end

Circle public final Class
  center Point private Field
  radius F64 private Field
end

Circle Shape Implements
  ( -> F64 ) area public Method
    self radius.get dup * pi *
  end
end

$circle Shape as_trait shape let
$shape area
```

Exact construction, conversion, and trait-object words remain ADR-001/ADR-004
prototype questions. Arguments and values must precede the receiver/selector.

## Diagnostics and tooling pressure

The accepted design must let tools distinguish value equality from identity,
show enum variants and hidden fields safely, explain boxing/specialization when
requested, and preserve source spans through virtual/trait dispatch. Layout
diagnostics must report size, alignment, padding, target, and the declaration
that prevents a requested C-compatible layout.

Reflection cannot mutate compiler metadata or bypass visibility. Stripped
metadata fails with a typed availability result rather than returning a
partially valid dynamic map.

## Alternatives already rejected

- Multiple class inheritance and C3 linearization are outside the 2.0
  contract; composition and traits are the supported model.
- Rust ABI or the host compiler's private struct layout is never a public ABI.
- Raw addresses cannot define managed object identity.
- Arbitrary operator punctuation and import-order overload resolution are not
  allowed.
- A universal boxed representation is not accepted without benchmark evidence.

## Prototype evidence

No Ricochet 2 representation prototype exists yet. Before this record can move
to Proposed, candidate layouts must run the same semantic, memory, dispatch,
generic, reflection, debugger, GC-movement, and FFI-copy workloads across the
reference VM and at least one native-backend prototype. Results must include
object size, allocation, dispatch cost, code size, compile time, target gaps,
and implementation complexity.

The decision is blocked until ADR-003 establishes the managed-reference and
pinning abstraction. Evidence and rejected prototypes are preserved.
