# Concept: Stack Effects

A stack effect documents a word by showing what it consumes and what it produces.

```text
println  ( value -- )
+        ( left right -- sum )
map      ( -- map )
at       ( container key -- value )
```

The left side of `--` is the input shape. The right side is the output shape. Empty space after `--` means the word leaves nothing.

## Why stack effects matter

They make postfix code reviewable. Instead of guessing what a word does, ask whether the current stack matches the effect. If the stack has too few values, you will get a stack underflow. If the values are the wrong kind, you will get a type error.

## Stack comments in examples

When you write teaching code, add comments near new shapes:

```rco
$settings "theme" at  (( container key -- value ))
```

Keep comments short. The goal is to make the stack visible without turning every line into a lecture.
