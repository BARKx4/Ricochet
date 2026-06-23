# Chapter 03: Postfix Stack Thinking

## What You Will Build

You will build a receipt-style calculation that makes stack movement visible.

## Concepts

- Before-and-after stack diagrams.
- Stack manipulation as a tool, not a goal.
- When named bindings are clearer than deeper stack juggling.

## Words Introduced

Primary stack words: `swap`, `dup`, `drop`, `over`, `rot`, `nip`, `tuck`,
`pick`, `roll`, `depth`, and `clear`.

## Guided Example

Open `examples/learn/03-stack/main.rco`:

```ricochet
"Stack receipt" println

4 price var
3 quantity var

$price $quantity * subtotal var
$subtotal 2 + total var

"items:" print
$quantity println
```

Run it:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/03-stack/main.rco
```

The calculation `$price $quantity * subtotal var` has this shape:

| Step | Stack Before | Word Or Value | Stack After |
| --- | --- | --- | --- |
| 1 | `[]` | `$price` | `[4]` |
| 2 | `[4]` | `$quantity` | `[4, 3]` |
| 3 | `[4, 3]` | `*` | `[12]` |
| 4 | `[12]` | `subtotal var` | `[]` |

Naming `subtotal` is clearer than leaving `12` on the stack for many lines.
Stack fluency includes knowing when to stop juggling.

Small stack motion examples:

```ricochet
1 2 swap      (( leaves 2 1 ))
7 dup *       (( squares 7 ))
1 2 over      (( leaves 1 2 1 ))
1 2 3 rot     (( leaves 2 3 1 ))
```

## Try It

Before running each expression, predict the final stack:

```ricochet
1 2 3 drop
1 2 3 nip
1 2 3 tuck
10 20 30 depth
```

Then run them in the REPL one at a time. Use `clear` between experiments if
the stack gets noisy.

## Common Mistakes

- Keeping values on the stack longer than needed.
- Using stack motion where a named binding would be easier to read.
- Forgetting that `print`, `println`, `var`, and mutators such as `push!`
  consume values.

## What You Know Now

You can read simple postfix expressions from left to right and explain how
each word changes the stack. That is the mental model every later Ricochet
surface uses.
