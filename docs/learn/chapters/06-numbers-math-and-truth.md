# Chapter 06: Numbers, Math, and Truth

## Why this chapter matters

Arithmetic is the simplest place to practice postfix reading because the stack effects are small: two numbers go in, one number comes out. Comparisons build on the same shape and leave booleans that later chapters use for conditionals.

## What you will build

You will build a budget calculator and use it to learn numeric operations, comparisons, boolean helpers, checked conversions, and assertions.

## Concepts in plain English

Ricochet has integer and float numeric values. Symbolic arithmetic words such as `+` and `*` are compact. Readable aliases such as `add` and `multiply` can make teaching code clearer.

Comparison words consume two values and leave `true` or `false`. Conversion words such as `to_integer` can fail, so they return `Result` values instead of pretending every string is a valid number.

## Words introduced

Arithmetic and comparison words: `+`, `add`, `-`, `subtract`, `*`, `multiply`, `/`, `divide`, `%`, `modulo`, `equals`, `=`, `not_equals?`, `!=`, `<`, `less_than?`, `>`, `greater_than?`, `<=`, `less_or_equals?`, `>=`, and `greater_or_equals?`.

Helpers: `negate`, `abs`, `min`, `max`, `clamp`, `not`, `and`, `or`, `assert`, `assert_true`, `assert_false`, and `assert_equals`.

Conversions include `to_number`, `to_integer`, `to_float`, and size-specific integer and float conversions.

## Guided example

Run the budget example:

```bash
rco run examples/learn/06-numbers-math-and-truth/budget.rco
```

The core calculation is ordinary postfix math:

```rco
$rent $food + spent var
$income $spent - remaining var
$remaining 0 > println
```

## How to read the code

`$rent $food + spent var` reads the two named values, adds them, and names the result `spent`.

| Stack before | Token | Stack after |
| --- | --- | --- |
| `[]` | `$rent` | `[rent]` |
| `[rent]` | `$food` | `[rent, food]` |
| `[rent, food]` | `+` | `[sum]` |
| `[sum]` | `spent var` | `[]` |

Checked conversions return `Result` values:

```rco
"42" to_integer dup ok? if
  value println
else
  error "kind" at println
end
```

The `dup ok? if ... else ... end` pattern keeps the original result below the condition so the chosen branch can unwrap either `value` or `error`.

## Try it

Change `income`, `rent`, and `food`. Then add one assertion:

```rco
$remaining 57 assert_equals
```

If the value is wrong, the VM fails loudly. Assertions are useful in examples, tests, and small scripts where a wrong assumption should stop execution.

## Check your understanding

- `1 2 +` leaves `3`.
- `1 2 <` leaves `true`.
- `"42" to_integer` leaves a `Result`, not a bare integer.
- Use `ok?` before unwrapping a result whose input came from outside your control.

## Common mistakes

- Ignoring `Result` values from conversions.
- Assuming mixed numeric operations never promote values. Integer plus float produces a float.
- Using truthy values where a precise comparison would explain intent.
- Forgetting that `/` with two integers stays integer division.

## What you know now

You know how Ricochet handles everyday numeric work, truth checks, assertions, and checked conversions. Later host APIs use the same `Result` habit for recoverable failures.
