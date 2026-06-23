# Chapter 06: Numbers, Math, And Truth

## What You Will Build

You will build a budget calculator and use it to learn numeric operations,
comparisons, boolean helpers, checked conversions, and assertions.

## Concepts

- Integer and float arithmetic.
- Comparison words and readable aliases.
- Boolean helpers and assertions.
- Conversion words that return `Result` values.
- Boundary thinking for checked numeric operations.

## Words Introduced

Arithmetic and comparison words: `+`, `add`, `-`, `subtract`, `*`,
`multiply`, `/`, `divide`, `%`, `modulo`, `equals`, `=`, `not_equals?`,
`!=`, `<`, `less_than?`, `>`, `greater_than?`, `<=`, `less_or_equals?`,
`>=`, and `greater_or_equals?`.

Helpers: `negate`, `abs`, `min`, `max`, `clamp`, `not`, `and`, `or`,
`assert`, `assert_true`, `assert_false`, and `assert_equals`.

Conversions: `to_number`, `to_integer`, `to_bigint`, `to_int`,
`to_mediumint`, `to_smallint`, `to_tinyint`, `to_bit`, `to_unsigned_int`,
`to_unsigned_mediumint`, `to_unsigned_smallint`, `to_unsigned_tinyint`,
`to_unsigned_bigint`, `to_float`, `to_float32`, `to_float64`, `to_double`,
and `to_real`.

## Guided Example

Open `examples/learn/06-numbers-math-and-truth/budget.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/06-numbers-math-and-truth/budget.rco
```

The core calculation is ordinary postfix math:

```ricochet
$rent $food + spent var
$income $spent - remaining var
$remaining 0 > println
```

Comparison words consume two numeric values and produce a boolean. Use the
symbolic forms when they read naturally, and the named forms when prose-like
code is clearer.

Checked conversions return `Result` values:

```ricochet
"42" to_integer dup ok? if
  value println
else
  error "kind" at println
end
```

The `dup ok? if ... else ... end` pattern keeps the original result below the
condition so the chosen branch can unwrap either `value` or `error`.

## Try It

Change `income`, `rent`, and `food`. Then add one assertion:

```ricochet
$remaining 57 assert_equals
```

If the value is wrong, the VM fails loudly. Assertions are useful in examples,
tests, and small scripts where a wrong assumption should stop execution.

## Common Mistakes

- Ignoring `Result` values from conversions.
- Assuming mixed numeric operations never promote values. Integer plus float
  produces a float.
- Using truthy values where a precise comparison would explain intent.
- Forgetting that `/` with two integers stays integer division.

## What You Know Now

You know how Ricochet handles everyday numeric work, truth checks, assertions,
and checked conversions. Later host APIs use the same `Result` habit for
recoverable failures.
