# Concept: Postfix Evaluation

Postfix evaluation is the core reading rule of Ricochet: values come first, words come after the values they consume.

```rco
10 5 - println
```

The word `-` consumes the two numbers above it and leaves the result. The word `println` consumes that result and writes it as output.

## Read left to right

A Ricochet line is easiest to read as a story:

```rco
"  Ada  " trim uppercase println
```

Start with text. Trim it. Uppercase it. Print it.

## Prefer visible boundaries

Use bindings when a value becomes important enough to name:

```rco
"  Ada  " trim name var
$name uppercase println
```

This is often clearer than keeping many values on the stack while you perform unrelated work.

## Check yourself

For any unfamiliar line, annotate the stack after each token. Stop as soon as the stack shape surprises you. The surprising point is where the misunderstanding lives.
