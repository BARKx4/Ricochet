# Chapter 10: Control Flow, Functions, And Blocks

## What You Will Build

You will build a small gradebook program and learn the control words that turn
straight-line stack code into reusable program structure.

## Concepts

- `if`, `else`, `while`, `break`, `continue`, and `return`.
- First-class blocks and `call`.
- Function declarations and readable factoring in postfix style.
- Why `Result` values still need `ok?` before a conditional.

## Words Introduced

Primary coverage: `function`, `return`, `if`, `call`, `while`, `break`,
`continue`, `else`, and `end`.

`Macro` is listed in the control family, but it is taught in Chapter 31 after
ordinary functions and packages are familiar.

## Guided Example

Open `examples/learn/10-control-flow/main.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/10-control-flow/main.rco
```

The loop keeps its index in a binding:

```ricochet
0 index var

$index $scores count < while
  $scores $index at score set
  $index 1 + index set
end
```

Conditionals are postfix: the condition comes first, then `if` starts the
branch.

```ricochet
$score 90 >= if
  "A" grade set
else
  "Needs practice" grade set
end
```

Blocks are first-class values:

```ricochet
[ 40 2 + ] call println
```

Functions give a name to a block-like body:

```ricochet
( score -> String ) letter_grade function
  $score 90 >= if
    "A" return
  end

  $score 80 >= if
    "B" return
  end

  "Needs practice"
end
```

## Try It

Extract the nested grade logic from the example into `letter_grade`, then call:

```ricochet
$score letter_grade grade set
```

Then add a guard inside the loop:

```ricochet
$score 0 < if
  continue
end
```

## Common Mistakes

- Forgetting that arguments still arrive through postfix stack order.
- Hiding too much behavior in anonymous blocks.
- Using a `Result` directly as a condition. Check it with `ok?` or `error?`.
- Forgetting lowercase `end` for control blocks.

## What You Know Now

You can structure Ricochet programs beyond straight-line scripts with loops,
conditionals, early returns, first-class blocks, and named functions.
