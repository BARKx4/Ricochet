# How to Use This Guide

Learn Ricochet is a guided course, not an exhaustive catalog. Read it in order through Chapter 10 if Ricochet is your first postfix language. After that, follow the path that matches what you want to build.

## The lanes

- **Start Here** explains the mental model before the first chapter.
- **Learn** is the linear tutorial.
- **Concepts** gives standalone explanations you can revisit.
- **How-To Guides** are short task recipes.
- **Appendices** are quick lookup pages for terms, commands, capabilities, and troubleshooting.
- **Reference** should remain the exact word and CLI catalog in the main repository.

## How to read a chapter

Each chapter follows the same public-facing structure:

1. **Why this chapter matters** connects the topic to what you already know.
2. **What you will build** gives you a concrete goal.
3. **Concepts in plain English** defines the new ideas.
4. **Guided example** shows runnable code.
5. **How to read the code** explains the stack, bindings, and data movement.
6. **Try it** gives you a small edit.
7. **Check your understanding** tells you what result to expect.

Do not skip the “How to read the code” sections early on. They are where the guide makes postfix visible.

## Command posture

The main guide assumes `rco` is installed:

```bash
rco run examples/learn/01-hello-world/main.rco
rco repl
```

When you are hacking on the Ricochet implementation from a source checkout, the repository README covers source-specific commands. Those belong in contributor docs, not in the beginner path.
