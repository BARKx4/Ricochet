# Chapter 02: Running Ricochet

## What You Will Build

You will build confidence with running files, using the REPL, reading help,
and recognizing diagnostics.

## Concepts

- `rco run`, `rco repl`, and command help.
- Source layout and example paths.
- Comments, string escapes, integers, and floats.
- Parse errors, stack errors, and type errors.

## Words Introduced

This chapter reinforces comments, strings, integer literals, and float literals.
No new runtime word gets primary coverage here.

## Guided Example

Use the Chapter 01 file as the smoke test:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/01-hello-world/main.rco
```

Ask for command help:

```powershell
cargo run -q -p ricochet_cli --bin rco -- --help
cargo run -q -p ricochet_cli --bin rco -- run --help
```

Start the REPL:

```powershell
cargo run -q -p ricochet_cli --bin rco -- repl
```

In the REPL, try:

```ricochet
1 2 + println
"line one\nline two" println
3.5 2 + println
```

Plain integer literals are integer `Number` values. Decimal or exponent
literals are `Float` values, and mixed numeric math promotes to `Float`.

## Try It

Create a temporary scratch file outside the manual examples and intentionally
try a broken expression:

```ricochet
"missing closer
```

Then try a stack error:

```ricochet
+ println
```

The important habit is not memorizing every error message. The habit is reading
where the diagnostic points, then reducing the program to the smallest
expression that still fails.

## Common Mistakes

- Running examples from the wrong working directory. The manual commands assume
  the repo root unless they say otherwise.
- Reading the final stack as output text. Program output appears before the
  final stack display.
- Reading a runtime stack error as a parser problem.
- Assuming the REPL and file runner use different language rules.

## What You Know Now

You can run scripts, enter expressions interactively, and ask the CLI for help.
That feedback loop is enough to start learning stack behavior.
