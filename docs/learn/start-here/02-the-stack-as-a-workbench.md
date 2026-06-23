# The Stack as a Workbench

The stack is where values wait while words work on them. You can imagine it as a workbench where the newest value is closest to your hand.

```text
bottom                 top
[ earlier, middle, newest ]
```

Most Ricochet words use the newest values first. In `1 2 +`, the word `+` consumes `1` and `2`, then leaves `3`.

## Stack effects

A **stack effect** is a compact way to describe what a word consumes and produces:

```text
+        ( left right -- sum )
println  ( value -- )
dup      ( value -- value value )
drop     ( value -- )
swap     ( a b -- b a )
```

The left side of `--` is what the word needs. The right side is what it leaves. This guide uses stack diagrams when a shape is new or easy to mix up.

## Four stack words are enough at first

You will eventually see many stack motion words, but beginners only need a small starter set:

```rco
7 dup *       (( duplicate 7, then multiply: 49 ))
1 2 swap      (( swap the top two values: leaves 2 1 ))
1 2 over      (( copy the value under the top: leaves 1 2 1 ))
1 2 3 drop    (( remove the top value: leaves 1 2 ))
```

Treat these as repair tools, not as the goal of the language. If a line needs three shuffles before it makes sense, a binding or a small helper function is usually clearer.

## The stack should end cleanly

Many examples print a final stack, often `[]`. An empty stack means the program consumed its temporary values. This is good for examples because it proves the code did not accidentally leave scratch values behind.

In real programs, you will not stare at the final stack constantly. You will still benefit from the habit: when a function or script is done, it should leave only the values it promised to leave.
