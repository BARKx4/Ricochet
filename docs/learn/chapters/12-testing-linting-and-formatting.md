# Chapter 12: Testing, Linting, and Formatting

## Why this chapter matters

Before you use files, HTTP, processes, UI surfaces, packages, or databases, you need a small feedback loop. Ricochet gives you assertions in code plus CLI commands for tests, linting, and formatting.

Testing answers “does this behavior still work?” Linting answers “does this source shape look suspicious?” Formatting answers “can this code be made consistent without changing behavior?”

## What you will build

You will build the smallest feedback loop: assertions in a runnable file, then commands that scale that habit to tests, linting, formatting, and documentation checks.

## Concepts in plain English

An assertion is a deliberate claim. If the claim is wrong, the run should fail. A linter points out likely mistakes. A formatter changes layout, not meaning. A test suite runs many assertions together.

## Words and commands introduced

Primary coverage: `rco test`, `rco lint`, `rco fmt`, assertion helpers, and a high-level overview of `@ricochet/test_helpers`.

## Guided example

Run the example:

```bash
rco run examples/learn/12-testing-linting-and-formatting/main.rco
```

The file uses assertion words directly:

```rco
40 2 + 42 assert_equals
true assert_true
false assert_false
```

For a single file, run the same feedback loop with lint and format:

```bash
rco lint examples/learn/12-testing-linting-and-formatting/main.rco
rco fmt examples/learn/12-testing-linting-and-formatting/main.rco --check
```

For a project or package, `rco test PATH` runs Ricochet test files under that path.

## How to read the code

`40 2 + 42 assert_equals` computes `42`, then compares it to the expected `42`. The expected value appears just before the assertion word. If the actual and expected values differ, the assertion makes the run fail loudly.

## Try it

Change `42 assert_equals` to `41 assert_equals` and run the example again. Read the failure, then restore the expected value.

Then try:

```bash
rco lint examples/learn/12-testing-linting-and-formatting/main.rco
rco fmt examples/learn/12-testing-linting-and-formatting/main.rco --check
```

## Check your understanding

- Make a test fail once before trusting it.
- Keep one assertion focused on one behavior.
- Treat formatter output as layout, not behavior.
- Use lint warnings as teaching moments about accepted syntax shapes.

## Common mistakes

- Treating formatter output as a behavior change.
- Writing broad tests before the small stack contract is clear.
- Hiding too many behaviors behind one assertion.
- Skipping lint until a project is large.

## What you know now

You know the basic safety loop before using advanced host powers: run a small program, assert the stack contract, lint the source, format separately, and test projects as they grow.
