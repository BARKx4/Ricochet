# Chapter 13: Introspection And Debug Basics

> Part II: Language Core and Feedback Loop

## Why this chapter matters

Once programs have names, data, functions, and tests, you need tools for seeing what the runtime sees. Introspection and debugging make stack behavior observable instead of mysterious.

## What you will build

You will build a debug tour program for inspecting runtime state before moving
to the full debugger and editor integrations.

## Concepts in plain English

Introspection means asking a running program about values, words, stack depth, symbols, or documentation. Debugging means controlling execution so you can inspect behavior at the point it changes.

The chapter uses these concepts:

* `inspect`, type inspection, class inspection, and method inspection.
* Callable checks and object field/method lists.
* Terminal debugger basics.
* Trace files and JSON debug output at an introductory level.

## Vocabulary and commands

Primary coverage: `inspect`, `debug`, `type`, `class_of`, `instance_of?`,
`responds_to?`, `fields`, `methods`, and `callable?`.

Task inspection words are taught in Chapter 15 with async tasks.

## Guided example

Open `examples/learn/13-introspection-and-debug-basics/debug-tour.rco` and run:

```
rco run examples/learn/13-introspection-and-debug-basics/debug-tour.rco
```

The first inspection pattern is the same one you saw in Chapter 04:

```
$settings inspect println drop
```

Use `type` when you only need the runtime kind:

```
$settings type println
```

Use class and method inspection when behavior matters:

```
$contact class_of inspect println drop
"label" $contact responds_to? println
Contact fields "," join println
Contact methods inspect println drop
```

`debug` prints an inspection string without changing the stack:

```
$settings debug
```

That makes it useful when you want a quick look without rewriting the next
stack operation.

## How to read the example

Trace one representative line from left to right. Identify the values placed on the stack, the word that consumes them, and the result or binding that carries the work forward.

## Try it

Run the example through the debugger:

```
rco debug --step examples/learn/13-introspection-and-debug-basics/debug-tour.rco
```

The terminal debugger can show stack, locals, globals, `self`, and task
snapshots at pauses. For machine-readable traces, use:

```
rco run --trace-file debug-tour.trace.json examples/learn/13-introspection-and-debug-basics/debug-tour.rco
rco debug --json examples/learn/13-introspection-and-debug-basics/debug-tour.rco
```

## Check your understanding

- What new value shape, command, or host boundary did this chapter introduce?
- Which line is most important to trace with a stack diagram?
- Where would you add a binding to make the example easier to read?

## Common mistakes

* Printing values when structural inspection would answer the question faster.
* Jumping to full debugger tooling before learning simple inspection.
* Forgetting that `inspect` leaves the original value on the stack.
* Treating method names as selectors when they are data. Use `send` or
  `responds_to?` for dynamic method names.

## What you know now

You know how to look inside a running program before advanced tooling: inspect
values, ask for runtime types, check object capabilities, and move to the
debugger when single-line inspection is no longer enough.

## Next step

Continue to [Chapter 14: Date, Time, And Duration](14-date-time-and-duration.md).
