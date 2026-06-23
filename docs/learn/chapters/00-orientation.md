# Chapter 00: Welcome to Ricochet

## Why this chapter matters

Ricochet is easiest to learn when you know what kind of language you are entering. It is a modern postfix language: values appear first, then words act on those values. That order is different from most mainstream languages, so this guide starts by making the reading model visible instead of assuming you already know stack-language vocabulary.

You do not need to be a Forth, Factor, or concatenative-language expert. You only need to be willing to read small examples from left to right and ask what each word consumes and leaves behind.

## What you will build

In this chapter you will not build a full app yet. You will learn the shape of the course, the role of the `rco` tool, and the safest path through the material.

By the end, you should know where to go when you need a tutorial, a concept explanation, a task recipe, or an exact reference lookup.

## Ricochet in one minute

A Ricochet program is a sequence of values and words:

```rco
"Hello, Ricochet!" println
```

The string is a value. `println` is a word. The word consumes the value and prints it.

A slightly more useful line looks like this:

```rco
4 3 * 2 + println
```

Read it as: put down `4`, put down `3`, multiply them, put down `2`, add it, print the result.

## The stack is a workbench

The stack is the temporary place where values wait. If you have never used a stack language, imagine a workbench where the most recent value is closest to your hand. A word takes the values it needs from that workbench and leaves any result there for the next word.

You will see stack diagrams in the early chapters. They are not meant to be decoration. They are how you learn to read Ricochet without guessing.

## How the site is organized

Use the site as a set of lanes:

- **Start Here** explains Ricochet, postfix order, and the stack before the chapter sequence begins.
- **Learn** is the linear course. Read Chapters 00 through 10 in order if Ricochet is new to you.
- **Concepts** gives reusable explanations for ideas that show up in many chapters.
- **How-To Guides** gives short task recipes.
- **Appendices** gives quick lookup tables for commands, capabilities, syntax shapes, troubleshooting, and terms.
- **Reference** belongs beside this guide as the exact word and command catalog.

The Learn chapters are not supposed to be the full reference. They are here to teach the mental model, show realistic examples, and help you build confidence.

## Before you continue

Install Ricochet so the `rco` command is available. Then check it:

```bash
rco --help
```

The rest of the guide uses installed-tool commands such as:

```bash
rco run examples/learn/01-hello-world/main.rco
rco repl
```

If you are changing the Ricochet implementation itself from a source checkout, use the repository contributor instructions to install or run the CLI. The beginner path keeps that machinery out of the way.

## Common mistakes

- Trying to memorize the whole word catalog before writing a script. Start with a tiny program and a few stack diagrams.
- Treating the stack as a place to hide everything forever. Good Ricochet code uses bindings and data structures when names make code clearer.
- Jumping to MVC, packages, or GUI chapters before you understand postfix order. Those features still use the same language model.

## Check your understanding

Before moving on, answer these in your own words:

1. In `"hello" println`, what is the value and what is the word?
2. What does a word do with the stack?
3. Which part of the site would you use for exact word lookup?

## What you know now

You know what kind of language Ricochet is, how this guide is organized, and why the early chapters spend time on postfix order and the stack. Next you will run the smallest useful Ricochet program.
