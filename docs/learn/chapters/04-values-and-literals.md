# Chapter 04: Values, Literals, and Inspection

## Why this chapter matters

A stack is only useful if you know what kinds of values are sitting on it. This chapter introduces Ricochet’s everyday value families and gives you two tools, `type` and `inspect`, for checking your assumptions.

## What you will build

You will build a value tour that prints and inspects representative Ricochet values.

## Concepts in plain English

A **literal** is a value written directly in source code, such as `42`, `3.5`, `true`, `false`, `nil`, or `"Ada"`. Collections, blocks, results, regexes, capabilities, and tasks are also values, but you will learn those gradually.

`nil` is its own value. It is not the same thing as `false`, an empty string, or an empty collection.

## Words introduced

- `nil`, `true`, and `false` create basic literal values.
- `type` tells you the runtime family of a value.
- `inspect` creates a debug string while keeping the original value available.
- `nil?`, `empty?`, and `callable?` are predicates: words that answer a yes/no question.

## Guided example

Run the value tour:

```bash
rco run examples/learn/04-values/value-tour.rco
```

The example prints the runtime kind of common values:

```rco
"true type:" print
true type println

"integer type:" print
42 type println

"float type:" print
3.5 type println
```

## How to read the code

This line leaves a value, then transforms it into a type name:

```rco
true type println
```

Read it as: put `true` on the stack, ask for its type, print the type string.

This inspection pattern is slightly different:

```rco
$values inspect println drop
```

`inspect` leaves the original value in place and pushes a debug string above it. `println` consumes the debug string. `drop` removes the original value so the final stack stays clean.

## Try it

Add these lines to the example and compare the printed values:

```rco
"text" empty? println
"" empty? println
nil nil? println
false nil? println
```

Then try:

```rco
42 inspect println drop
```

## Check your understanding

- `nil nil?` should print true.
- `false nil?` should print false.
- `"" empty?` asks whether a string is empty, not whether it is `nil`.

## Common mistakes

- Assuming every language treats truthiness the same way.
- Treating decimal literals as integers.
- Forgetting that `inspect` leaves the original value on the stack.

## What you know now

You can recognize basic values, inspect a value without losing it, and separate `nil`, booleans, strings, numbers, and collections in your head.
