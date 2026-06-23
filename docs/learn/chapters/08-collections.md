# Chapter 08: Collections

## What You Will Build

You will build a task-list data workflow with arrays, maps, mutation, lookup,
and summary operations.

## Concepts

- Lists, maps, sets, ranges, and indexing.
- Mutation words and shared mutable collection behavior.
- Lookup, counting, slicing, and joining.
- Higher-order collection work with blocks.

## Words Introduced

Primary coverage: `list`, `Set`, `range`, `push!`, `put!`, `insert!`,
`remove!`, `remove_at!`, `clear!`, `at`, `count`, `first`, `last`, `take`,
`skip`, `reverse`, `has?`, `keys`, `values`, `each`, `transform`, `select`,
`reduce`, `find`, `any?`, `all?`, `join`, and `length`.

## Guided Example

Open `examples/learn/08-collections/main.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/08-collections/main.rco
```

The example uses an array as an ordered list:

```ricochet
tasks array
$tasks "read the chapter" push! drop
$tasks "run the example" push! drop
$tasks "change one line" push! drop
```

It uses a map for named fields:

```ricochet
details map
$details "owner" "Ada" put! drop
$details "status" "open" put! drop
```

The container comes before the key:

```ricochet
$details "owner" at println
```

That same shape applies to mutation:

```ricochet
$details "status" "done" put! drop
```

## Try It

Try these variations in the example:

```ricochet
$tasks first println
$tasks last println
$tasks 2 take ", " join println
$tasks reverse ", " join println
$details keys ", " join println
```

Then try a block-based operation:

```ricochet
$tasks [ uppercase ] transform ", " join println
```

## Common Mistakes

- Forgetting that collection mutation can affect shared values.
- Leaving the returned collection from `push!` or `put!` on the stack.
- Using higher-order words before the block shape is clear.
- Reversing the map order. Use `container key at`, not `key container at`.

## What You Know Now

You know the core collection vocabulary used throughout later chapters. MVC
requests, package metadata, JSON payloads, and GUI state documents are all
ordinary Ricochet data once you can read arrays and maps.
