# Chapter 02: Running Code and Getting Feedback

## Why this chapter matters

Learning a language is much easier when the feedback loop is short. You want to be able to run a file, try a line interactively, ask for help, and recognize the broad category of an error.

This chapter teaches the everyday `rco` commands before adding more language features.

## What you will build

You will run a known-good script, start the REPL, ask the CLI for help, and intentionally trigger a few beginner errors.

## Concepts in plain English

- `rco run FILE` runs a script.
- `rco repl` starts an interactive prompt.
- `rco --help` shows top-level help.
- `rco run --help` shows help for one command.
- Diagnostics are messages that explain why Ricochet could not parse, type-check, or run something.

## Guided example

Run the previous chapter’s example:

```bash
rco run examples/learn/01-hello-world/main.rco
```

Ask for help:

```bash
rco --help
rco run --help
```

Start the REPL:

```bash
rco repl
```

Try these lines one at a time:

```rco
1 2 + println
"line one\nline two" println
3.5 2 + println
```

Plain integer literals are integer `Number` values. Decimal or exponent literals are `Float` values. Mixed numeric math promotes to `Float`.

## How to read the feedback

A diagnostic is not a personal failure. It is a clue about the shape Ricochet expected.

Try a parse error in a scratch file:

```rco
"missing closer
```

Try a stack error:

```rco
+ println
```

The `+` word needs two values. This program gives it none. The fix is not to memorize the exact message; the fix is to ask what stack shape the word expected.

## The smallest useful debugging habit

When a program surprises you, reduce it. Delete everything that is not needed to reproduce the surprise. Then add one inspection line near the value you do not understand:

```rco
$thing inspect println drop
```

`inspect` gives you a debug string. `println` prints that string. `drop` removes the original value if you were only looking at it.

## Try it

Create `scratch.rco` with this line:

```rco
10 5 - println
```

Run it:

```bash
rco run scratch.rco
```

Then change the line to each of these and predict the output before running:

```rco
10 5 + println
10 5 * println
10 5 / println
```

## Check your understanding

- Use the REPL for tiny experiments.
- Use a file when you want to keep or rerun the program.
- When a stack error appears, find the word that did not receive enough values.

## Common mistakes

- Running examples from a directory that does not contain the example path.
- Reading only the last line of a diagnostic instead of the file and line it points to.
- Trying to fix a large program before making a small reproduction.

## What you know now

You can run code, experiment interactively, ask for CLI help, and approach diagnostics as stack and type-shape clues.
