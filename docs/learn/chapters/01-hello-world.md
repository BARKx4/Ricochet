# Chapter 01: Your First Program

## Why this chapter matters

A first program should prove only one thing: you can run Ricochet code and see a result. This chapter keeps the program tiny so you can focus on the reading order.

The most important line is this:

```rco
"Hello, Ricochet!" println
```

The value comes first. The word comes second.

## What you will build

You will write and run a one-line script that prints a greeting.

## Concepts in plain English

A Ricochet source file usually ends in `.rco`. A string is text wrapped in quotes. A word is an operation such as `println`. A comment is text for humans that Ricochet should ignore.

Ricochet comments use this shape:

```rco
(( this is a comment ))
```

## Words introduced

- `println` consumes one value and writes it as a line of output.
- `print` consumes one value and writes it without adding a newline.
- `(( ... ))` marks a comment.

## Guided example

Create a file named `hello.rco`:

```rco
(( My first Ricochet program. ))
"Hello, Ricochet!" println
```

Run it:

```bash
rco run hello.rco
```

You should see the greeting:

```text
Hello, Ricochet!
[]
```

The final `[]` is the remaining stack after the program finishes. `println` consumed the string, so the stack is empty.

You can also run the chapter example from the guide examples:

```bash
rco run examples/learn/01-hello-world/main.rco
```

## How to read the code

Trace the one useful line from left to right:

| Step | Token | What happens | Stack after |
| --- | --- | --- | --- |
| 1 | `"Hello, Ricochet!"` | Put the string on the stack. | `["Hello, Ricochet!"]` |
| 2 | `println` | Consume the string and print it. | `[]` |

Nothing magical happens before `println`. Ricochet does not read this as `println("Hello")`. The value waits first, then the word uses it.

## Try it

Change the string and run the file again:

```rco
"stack first, word second" println
```

Then try the same line in the REPL:

```bash
rco repl
```

At the prompt, enter:

```rco
"stack first, word second" println
```

## Check your understanding

- If you write `println "Hello"`, the order is wrong because `println` has no value to consume yet.
- If you remove `println`, the string will remain on the final stack.
- If you use `print` instead of `println`, the output does not add a newline for you.

## Common mistakes

- Writing the operation first out of habit from function-call languages.
- Expecting semicolons. Ricochet reads whitespace-separated tokens.
- Forgetting that output words consume the values they print.

## What you know now

You can run a Ricochet file, recognize a string, read one postfix expression, and explain why the final stack is empty.
