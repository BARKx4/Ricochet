# Chapter 10: Making Decisions and Reusing Code

## Why this chapter matters

So far, most code has been straight-line: values arrive, words transform them, and output appears. Real programs need decisions, loops, reusable functions, and small blocks of behavior that can be passed to other words.

The postfix rule still holds. Conditions appear before `if`. Loop conditions appear before `while`. Function arguments arrive through stack order.

## What you will build

You will build a small gradebook program and learn the control words that turn stack code into reusable program structure.

## Concepts in plain English

- `if ... else ... end` chooses a branch from a boolean condition.
- `while ... end` repeats while a condition is true.
- `break` exits a loop and `continue` skips to the next loop iteration.
- A block is an executable value written inside `[ ... ]`.
- A function names a reusable body of code.

## Words introduced

Primary words: `function`, `return`, `if`, `call`, `while`, `break`, `continue`, `else`, and `end`.

`Macro` belongs to the wider control family, but it is taught later after ordinary functions and packages are familiar.

## Guided example

Run the gradebook example:

```bash
rco run examples/learn/10-control-flow/main.rco
```

The loop keeps its index in a binding:

```rco
0 index var

$index $scores count < while
  $scores $index at score set
  $index 1 + index set
end
```

Conditionals are postfix: the condition comes first, then `if` starts the branch.

```rco
$score 90 >= if
  "A" grade set
else
  "Needs practice" grade set
end
```

Blocks are first-class values:

```rco
[ 40 2 + ] call println
```

Functions give a name to a body:

```rco
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

## How to read the code

Read `$score 90 >= if` as: get the score, put `90`, compare, then branch on the boolean result. Do not read `if` as a function that wraps the condition. In Ricochet, the condition has already been computed by the time `if` appears.

Read `[ 40 2 + ] call println` as: create a block value, call it, print the value it leaves.

## Try it

Extract the nested grade logic from the example into `letter_grade`, then call:

```rco
$score letter_grade grade set
```

Then add a guard inside the loop:

```rco
$score 0 < if
  continue
end
```

## Check your understanding

- Conditions must leave a boolean before `if` or `while` runs.
- A block can be stored, passed, and called.
- A function should leave the values it promises and no extra scratch values.
- A `Result` is still not a condition. Check it with `ok?` or `error?`.

## Common mistakes

- Forgetting that arguments still arrive through postfix stack order.
- Hiding too much behavior in anonymous blocks.
- Using a `Result` directly as a condition.
- Forgetting lowercase `end` for control blocks.

## What you know now

You can structure Ricochet programs beyond straight-line scripts with loops, conditionals, early returns, first-class blocks, and named functions.
