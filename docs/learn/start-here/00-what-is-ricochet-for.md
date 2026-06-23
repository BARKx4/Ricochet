# What Ricochet Is For

Ricochet is a programming language for building real programs with a deliberately different reading order. Most languages ask you to read an operation first and then its arguments: `print("hello")`, `add(1, 2)`, or `1 + 2`. Ricochet usually asks you to read the values first and the word that uses them second:

```rco
"hello" println
1 2 +
```

That style is called **postfix**. In Ricochet, postfix is not a gimmick. It is the common shape used for scripts, data processing, classes, MVC controllers, packages, terminal UIs, desktop webviews, and host capabilities. Once the shape clicks, you can read a Ricochet program from left to right as a series of small transformations.

## A useful first mental model

Think of Ricochet as a language with a visible workbench:

1. A value arrives.
2. It sits on the workbench.
3. A word uses one or more values from the workbench.
4. The word leaves its result behind for the next word.

The workbench is called the **stack**. You do not need to master every stack trick before you can write useful code. In fact, good Ricochet code often uses names and data structures so the stack stays small and readable.

## What you can build

The learning path starts with tiny scripts and grows toward practical application shapes:

- command-line scripts and tools;
- data cleanup with strings, JSON, regex, maps, and arrays;
- tests, linting, formatting, debugging, and editor support;
- files, HTTP, sockets, processes, terminal UIs, and desktop webviews;
- MVC web apps with routes, controllers, templates, data, sessions, and forms;
- packages, registries, macros, bytecode, release artifacts, and complete capstone apps.

That breadth can be intimidating. The guide is organized so you do not have to care about all of it at once. Start with the first program, learn how postfix reads, and use the later chapters when the current project needs them.

## Where to go next

Read these short Start Here pages before Chapter 00 if postfix languages are new to you:

1. [How postfix reads](01-how-postfix-reads.md)
2. [The stack as a workbench](02-the-stack-as-a-workbench.md)
3. [How to use this guide](03-how-to-use-this-guide.md)

Then continue to [Chapter 00: Welcome to Ricochet](../chapters/00-orientation.md).
