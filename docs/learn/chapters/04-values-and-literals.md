# Chapter 04: Values And Literals

## What You Will Build

You will build a value tour that prints and inspects representative Ricochet
values.

## Concepts

- Runtime value families.
- Integer, float, string, boolean, nil, collection, block, task, result, regex,
  and capability values.
- Predicates such as `nil?` and `empty?`.
- Inspection as a learning tool.

## Words Introduced

Primary coverage here: `nil`, `true`, and `false`.

This chapter also previews `nil?`, `empty?`, `type`, `inspect`, and
`callable?`; their primary coverage comes in later chapters.

## Guided Example

Open `examples/learn/04-values/value-tour.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/04-values/value-tour.rco
```

The example prints the runtime kind of common values:

```ricochet
"true type:" print
true type println

"integer type:" print
42 type println

"float type:" print
3.5 type println
```

It also shows a useful inspection pattern:

```ricochet
$values inspect println drop
```

`inspect` leaves the original value in place and pushes a debug string above
it. `println` consumes the debug string, then `drop` removes the original value
so the final stack stays clean.

## Try It

Add these lines to the example and compare the printed values:

```ricochet
"text" empty? println
"" empty? println
nil nil? println
false nil? println
```

`nil` is its own value. It is not the same thing as `false`, an empty string,
or an empty collection.

## Common Mistakes

- Assuming every language treats truthiness the same way.
- Treating decimal literals as integers. `3` is an integer `Number`; `3.0` is
  a `Float`.
- Leaving inspection helper values on the stack while experimenting.

## What You Know Now

You know the literal booleans and nil value, and you have a safe way to inspect
other runtime values before learning mutation and control flow.
