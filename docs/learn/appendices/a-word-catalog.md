# Appendix A: Word Groups at a Glance

This appendix is a friendly orientation to Ricochet word families. Use the reference site or `rco words` for exact signatures, descriptions, examples, and search.

## First commands

```bash
rco words
rco words --json
```

## Word groups

| Group | What it covers | Learn path |
| --- | --- | --- |
| Stack | Small stack motion and cleanup words. | Chapters 03 and 13 |
| Math | Arithmetic, comparison, boolean helpers, assertions, and numeric conversion. | Chapter 06 |
| Data | Literals, predicates, bindings, and basic value inspection. | Chapters 04 and 05 |
| Collection | Arrays, lists, maps, sets, ranges, lookup, mutation, and block-based traversal. | Chapter 08 |
| String | Cleanup, slicing, search, replacement, JSON, and regex. | Chapter 07 |
| Result | Explicit success and failure values. | Chapter 09 |
| OOP | Classes, accessors, selectors, methods, and dynamic dispatch. | Chapter 11 |
| Control | Functions, blocks, conditionals, loops, tasks, and macros. | Chapters 10, 15, and 31 |
| Inspect | Runtime discovery, debugging, stack visibility, and traces. | Chapters 13 and 32 |
| Web and MVC | Routes, controllers, responses, templates, sessions, forms, and data. | Chapters 23 through 28 and 37 |
| System | Files, workspaces, HTTP, sockets, processes, PTYs, TUI, GUI, packaging, bytecode, and release workflows. | Chapters 14 through 22 and 32 through 38 |

## How to read a word entry

A reference word entry should answer four questions:

1. What stack shape does the word consume and produce?
2. What value family does it belong to?
3. Does it return a `Result` that must be checked or unwrapped?
4. Which capability, if any, must be enabled by the host command?

Examples:

```rco
20 22 +
```

Consumes two numbers and leaves one number.

```rco
"settings.json" fs_read_text value
```

Reads through the filesystem capability and returns a `Result`. Use `value` only after deciding that failure should stop the current run.

```rco
$settings "theme" at
```

Uses container-first access: container, key, then `at`.

## Alias notes

Ricochet has symbolic words such as `+` and readable aliases such as `add`. Prefer the shape that best teaches the surrounding code. In beginner examples, readable aliases can help; in compact arithmetic, symbols are natural.

Case matters. `get` and `GET` are different public words because one is dynamic data access and the other is an HTTP route verb.
