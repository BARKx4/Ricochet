# Chapter 08: Collections

## Why this chapter matters

Collections are how small values become real program state. Scripts use arrays for ordered items, maps for named fields, sets for membership, and ranges for repeated work. Later chapters reuse these same shapes for JSON payloads, MVC data, package manifests, GUI documents, and test fixtures.

## What you will build

You will build a task-list data workflow with arrays, maps, mutation, lookup, and summary operations.

## Concepts in plain English

A list or array is ordered. A map stores values under keys. A set stores unique values. Mutation words ending in `!` change a collection. Higher-order collection words such as `transform`, `select`, and `reduce` run a block against items.

## Words introduced

Primary words: `list`, `Set`, `range`, `push!`, `put!`, `insert!`, `remove!`, `remove_at!`, `clear!`, `at`, `count`, `first`, `last`, `take`, `skip`, `reverse`, `has?`, `keys`, `values`, `each`, `transform`, `select`, `reduce`, `find`, `any?`, `all?`, `join`, and `length`.

## Guided example

Run the collection example:

```bash
rco run examples/learn/08-collections/main.rco
```

The example uses an array as an ordered list:

```rco
tasks array
$tasks "read the chapter" push! drop
$tasks "run the example" push! drop
$tasks "change one line" push! drop
```

It uses a map for named fields:

```rco
details map
$details "owner" "Ada" put! drop
$details "status" "open" put! drop
```

The container comes before the key:

```rco
$details "owner" at println
```

## How to read the code

Mutation words usually return the collection they changed. That makes chaining possible, but beginner examples often use `drop` to keep the final stack clean.

```rco
$tasks "read the chapter" push! drop
```

Read it as: get the tasks array, put the new item on the stack, push the item, drop the returned array.

The same container-first rule appears in maps:

```rco
$details "status" "done" put! drop
$details "status" at println
```

## Try it

Try these variations:

```rco
$tasks first println
$tasks last println
$tasks 2 take ", " join println
$tasks reverse ", " join println
$details keys ", " join println
```

Then try a block-based operation:

```rco
$tasks [ uppercase ] transform ", " join println
```

## Check your understanding

- `at` reads with `container key at`.
- `put!` writes with `container key value put!`.
- `push!` mutates and returns the collection.
- `transform` takes a block and applies it to each item.

## Common mistakes

- Forgetting that collection mutation can affect shared values.
- Leaving the returned collection from `push!` or `put!` on the stack.
- Using higher-order words before the block shape is clear.
- Reversing the map order. Use `container key at`, not `key container at`.

## What you know now

You know the core collection vocabulary used throughout later chapters. MVC requests, package metadata, JSON payloads, and GUI state documents are all ordinary Ricochet data once you can read arrays and maps.
