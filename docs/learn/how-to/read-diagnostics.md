# How to Read Diagnostics

When Ricochet reports an error, your first goal is to make the failing program smaller.

## The loop

1. Read the file and line mentioned by the diagnostic.
2. Find the nearest word that consumes values.
3. Ask what stack shape that word expected.
4. Reduce the program until the same error still happens.
5. Add `type` or `inspect` near the surprising value.

## Common diagnostic families

| Family | What to check |
| --- | --- |
| Parse error | Missing quote, bad token, unmatched block, or infix-shaped code. |
| Stack underflow | A word needed more values than the stack had. |
| Type error | The stack had enough values, but one was the wrong kind. |
| Result unwrap failure | `value` was used on a failure result. Check `ok?` first. |
| Capability denied | The command did not grant the host power the program requested. |

## A useful debugging line

```rco
$thing inspect println drop
```

`inspect` leaves the original value in place and pushes a debug string above it. `println` consumes the debug string. `drop` removes the original if you only wanted to look at it.
