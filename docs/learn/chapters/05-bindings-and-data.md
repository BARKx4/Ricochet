# Chapter 05: Names, Bindings, and Small Data

## Why this chapter matters

Postfix does not mean “never name anything.” Names are how you keep code readable when a value has a role in the problem. This chapter teaches bindings as the beginner-friendly escape hatch from stack overload.

## What you will build

You will build a small profile-card data shape with named bindings, an array, and a map.

## Concepts in plain English

A **binding** is a name for a value. You create one with `var` and read it with `$name`.

```rco
"Ada Lovelace" name var
$name println
```

Use a binding when a value has meaning beyond the current expression. Use the stack for immediate transformations.

## Words introduced

Primary words: `array`, `map`, `var`, `get`, `set`, `empty?`, and `nil?`.

`$name` is the normal way to read a known binding. The dynamic form `"name" get` is for cases where the name itself is data.

## Guided example

Run the profile-card example:

```bash
rco run examples/learn/05-bindings-and-data/profile-card.rco
```

The first bindings are ordinary named values:

```rco
"Ada Lovelace" name var
"language designer" role var
```

Read them with `$name` and `$role`:

```rco
$name println
$role println
```

Build a map when you need named fields:

```rco
profile map
$profile "name" $name put! drop
$profile "role" $role put! drop
```

## How to read the code

This map update has a consistent order:

```rco
$profile "name" $name put! drop
```

Read it as: put the container on the stack, put the key on the stack, put the value on the stack, then call `put!`. The mutation word returns the collection, so `drop` removes that returned collection when you do not need it.

Use static binding reads for ordinary code:

```rco
$name
```

Use dynamic lookup only when the name is data:

```rco
"name" get
```

## Try it

Add another field:

```rco
$profile "city" "Chicago" put! drop
$profile "city" at println
```

Then add an array:

```rco
tags array
$tags "beginner" push! drop
$tags "manual" push! drop
$tags ", " join println
```

## Check your understanding

- `$name` reads a binding called `name`.
- `"name" get` looks up a binding by a string at runtime.
- `put!` mutates a map and returns the map.
- `drop` is common after mutators when the returned collection is not needed.

## Common mistakes

- Reaching for dynamic lookup before a static binding would be clearer.
- Forgetting that collection mutation words return the collection.
- Reversing map access order. Use `container key at`.
- Using a map where two simple bindings would be easier to read.

## What you know now

You can name values, update bindings, choose between arrays and maps, and keep the stack readable by naming important values.
