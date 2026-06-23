# Chapter 13: Introspection And Debug Basics

## What You Will Build

You will build a debug tour program for inspecting runtime state before moving
to the full debugger and editor integrations.

## Concepts

- `inspect`, type inspection, class inspection, and method inspection.
- Callable checks and object field/method lists.
- Terminal debugger basics.
- Trace files and JSON debug output at an introductory level.

## Words Introduced

Primary coverage: `inspect`, `debug`, `type`, `class_of`, `instance_of?`,
`responds_to?`, `fields`, `methods`, and `callable?`.

Task inspection words are taught in Chapter 15 with async tasks.

## Guided Example

Open `examples/learn/13-introspection-and-debug-basics/debug-tour.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/13-introspection-and-debug-basics/debug-tour.rco
```

The first inspection pattern is the same one you saw in Chapter 04:

```ricochet
$settings inspect println drop
```

Use `type` when you only need the runtime kind:

```ricochet
$settings type println
```

Use class and method inspection when behavior matters:

```ricochet
$contact class_of inspect println drop
"label" $contact responds_to? println
Contact fields "," join println
Contact methods inspect println drop
```

`debug` prints an inspection string without changing the stack:

```ricochet
$settings debug
```

That makes it useful when you want a quick look without rewriting the next
stack operation.

## Try It

Run the example through the debugger:

```powershell
cargo run -q -p ricochet_cli --bin rco -- debug --step examples/learn/13-introspection-and-debug-basics/debug-tour.rco
```

The terminal debugger can show stack, locals, globals, `self`, and task
snapshots at pauses. For machine-readable traces, use:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run --trace-file debug-tour.trace.json examples/learn/13-introspection-and-debug-basics/debug-tour.rco
cargo run -q -p ricochet_cli --bin rco -- debug --json examples/learn/13-introspection-and-debug-basics/debug-tour.rco
```

## Common Mistakes

- Printing values when structural inspection would answer the question faster.
- Jumping to full debugger tooling before learning simple inspection.
- Forgetting that `inspect` leaves the original value on the stack.
- Treating method names as selectors when they are data. Use `send` or
  `responds_to?` for dynamic method names.

## What You Know Now

You know how to look inside a running program before advanced tooling: inspect
values, ask for runtime types, check object capabilities, and move to the
debugger when single-line inspection is no longer enough.
