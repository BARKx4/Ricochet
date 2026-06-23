# Chapter 03: How Postfix Reads

## Why this chapter matters

This is the chapter where Ricochet should stop feeling backwards. The goal is not to memorize every stack word. The goal is to build one durable habit: read values first, then words, and keep track of what each word consumes and leaves.

## What you will build

You will build a receipt-style calculation and trace how values move through the stack.

## Concepts in plain English

Postfix code is read left to right. A value pushes itself onto the stack. A word consumes the values it needs from the top of the stack and may leave a result.

A stack diagram shows the values before and after each step:

```text
[]
[4]
[4, 3]
[12]
[]
```

The rightmost item is the top of the stack.

## Words introduced

Start with these stack words:

- `dup` copies the top value.
- `drop` removes the top value.
- `swap` exchanges the top two values.
- `over` copies the value just below the top.
- `depth` reports how many values are on the stack.
- `clear` removes stack leftovers during experiments.

Other stack words such as `rot`, `nip`, `tuck`, `pick`, and `roll` are useful later. Do not force them into beginner code.

## Guided example

Create a receipt script:

```rco
"Stack receipt" println

4 price var
3 quantity var

$price $quantity * subtotal var
$subtotal 2 + total var

"items:" print
$quantity println

"total:" print
$total println
```

Run it:

```bash
rco run receipt.rco
```

Or run the guide example:

```bash
rco run examples/learn/03-stack/main.rco
```

## How to read the code

Focus on this line:

```rco
$price $quantity * subtotal var
```

| Step | Stack before | Token | Stack after |
| --- | --- | --- | --- |
| 1 | `[]` | `$price` | `[4]` |
| 2 | `[4]` | `$quantity` | `[4, 3]` |
| 3 | `[4, 3]` | `*` | `[12]` |
| 4 | `[12]` | `subtotal var` | `[]` |

`$price` and `$quantity` read named values. `*` consumes two numbers and leaves their product. `subtotal var` gives the product a name and consumes it from the stack.

Naming `subtotal` is clearer than leaving `12` on the stack for many lines. Stack fluency includes knowing when to stop juggling.

## Small stack motion examples

Try these in the REPL one at a time:

```rco
1 2 swap      (( leaves 2 1 ))
7 dup *       (( leaves 49 ))
1 2 over      (( leaves 1 2 1 ))
1 2 3 drop    (( leaves 1 2 ))
```

Use `clear` between experiments if the stack gets noisy.

## Check your understanding

Before running each expression, predict the final stack:

```rco
1 2 3 drop
1 2 swap
10 dup +
1 2 over
```

Expected shapes:

- `1 2 3 drop` leaves `1 2`.
- `1 2 swap` leaves `2 1`.
- `10 dup +` leaves `20`.
- `1 2 over` leaves `1 2 1`.

## Common mistakes

- Keeping values on the stack longer than needed.
- Using stack motion where a named binding would be easier to read.
- Forgetting that `print`, `println`, `var`, and mutators such as `push!` consume values.
- Trying to learn every stack word before writing useful code.

## What you know now

You can read simple postfix expressions from left to right and explain how each word changes the stack. That is the mental model every later Ricochet surface uses.
