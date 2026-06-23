# Chapter 12: Testing, Linting, And Formatting

## What You Will Build

You will build the smallest feedback loop: assertions in a runnable file, then
the commands that scale that habit to tests, linting, formatting, and docs
validation.

## Concepts

- `rco test`, `rco lint`, and `rco fmt`.
- Assertion helpers and small fixture style.
- Contributor-facing validation at a high level.
- Why formatting is separate from behavior.

## Words Introduced

Primary coverage: `rco test`, `rco lint`, `rco fmt`, assertion helpers, and a
high-level overview of `@ricochet/test_helpers`.

## Guided Example

Open `examples/learn/12-testing-linting-and-formatting/main.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/12-testing-linting-and-formatting/main.rco
```

The file uses assertion words directly:

```ricochet
40 2 + 42 assert_equals
true assert_true
false assert_false
```

For a single file, run the same feedback loop with lint and format:

```powershell
cargo run -q -p ricochet_cli --bin rco -- lint examples/learn/12-testing-linting-and-formatting/main.rco
cargo run -q -p ricochet_cli --bin rco -- fmt examples/learn/12-testing-linting-and-formatting/main.rco --check
```

For a project or package, `rco test PATH` runs Ricochet test files under that
path. First-party packages such as `@ricochet/test_helpers` provide helpers for
assertions, fixture maps, HTTP response assertions, and temporary workspaces.

## Try It

Change `42 assert_equals` to `41 assert_equals` and run the example again. Read
the failure, then restore the expected value.

For contributor docs checks, the repo also has:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-learn-manual.ps1
```

## Common Mistakes

- Treating formatter output as a behavior change.
- Writing broad tests before the small stack contract is clear.
- Forgetting to make a test fail once before trusting that it proves anything.
- Hiding too many behaviors behind one assertion.

## What You Know Now

You know the basic safety loop before using advanced host powers: run a small
program, assert the stack contract, lint the source, format separately, and use
project validation when documentation or public words change.
