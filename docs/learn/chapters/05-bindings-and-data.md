# Chapter 05: Bindings And Data

## What You Will Build

You will build a small profile-card data shape with named bindings and a map.

## Concepts

- Static bindings with `var` and `$name`.
- Dynamic `get` and `set` after static reads are clear.
- Arrays, maps, and choosing data shapes.

## Words Introduced

Primary coverage: `array`, `map`, `var`, `get`, `set`, `empty?`, and `nil?`.

## Guided Example

Open `examples/learn/05-bindings-and-data/profile-card.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/05-bindings-and-data/profile-card.rco
```

The first bindings are ordinary named values:

```ricochet
"Ada Lovelace" name var
"language designer" role var
```

Read them with `$name` and `$role`:

```ricochet
$profile "name" $name put! drop
$profile "role" $role put! drop
```

Use `$name` for normal reads. Use `"name" get` only when the name is data:

```ricochet
"dynamic read:" print
"name" get println
```

Update an existing binding with `set`:

```ricochet
"Grace Hopper" name set
$name println
```

## Try It

Add another field:

```ricochet
$profile "city" "Chicago" put! drop
$profile "city" at println
```

Then add an array:

```ricochet
tags array
$tags "beginner" push! drop
$tags "manual" push! drop
$tags ", " join println
```

## Common Mistakes

- Reaching for dynamic lookup before a static binding would be clearer.
- Forgetting that collection mutation words return the collection; use `drop`
  when you do not need that returned value.
- Confusing map data with class or model declarations. Maps are flexible data;
  classes and models add behavior and structure.

## What You Know Now

You can name values, read them statically, use dynamic lookup when the name is
data, and shape small records with maps and arrays.
