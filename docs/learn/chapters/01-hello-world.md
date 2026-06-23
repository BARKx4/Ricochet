# Chapter 01: Hello World

## What You Will Build

You will build the first Ricochet script: a one-line program that prints a
greeting.

## Concepts

- Source files with the `.rco` extension.
- Running a file with `rco run`.
- The smallest useful stack diagram.
- Comments and simple string output.

## Words Introduced

- `println`: consume a value and write it as a line of output.
- `print`: consume a value and write it without adding a newline.
- `(( ... ))`: comment text for notes that should not execute.

## Guided Example

Open `examples/learn/01-hello-world/main.rco`:

```ricochet
(( Run from the repo root with: cargo run -q -p ricochet_cli --bin rco -- run examples/learn/01-hello-world/main.rco ))

"Hello, Ricochet!" println
```

Run it from the repo root:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/01-hello-world/main.rco
```

Expected output:

```text
Hello, Ricochet!
[]
```

The final `[]` is the remaining stack after the program finishes. `println`
consumes the string, so the stack is empty.

| Step | Stack Before | Word Or Value | Stack After |
| --- | --- | --- | --- |
| 1 | `[]` | `"Hello, Ricochet!"` | `["Hello, Ricochet!"]` |
| 2 | `["Hello, Ricochet!"]` | `println` | `[]` |

## Try It

Change the string and run the file again. Then try the same expression in the
REPL:

```powershell
cargo run -q -p ricochet_cli --bin rco -- repl
```

At the prompt, enter:

```ricochet
"stack first, word second" println
```

## Common Mistakes

- Writing `println "Hello"` instead of `"Hello" println`.
- Expecting a semicolon. Ricochet reads whitespace-separated tokens.
- Forgetting that output words consume the value they print.

## What You Know Now

Ricochet words consume values already on the stack. You can read simple code
from left to right: values appear, then words use them.
