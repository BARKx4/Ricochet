# Chapter 09: Results and Errors

## Why this chapter matters

Real programs fail in ordinary ways: a file is missing, JSON is invalid, a host denies a capability, a network call times out, or a conversion receives the wrong text. Ricochet makes those failures explicit with `Result` values.

A result is not an exception and not a boolean. It is a value that says either “this worked” or “this failed with information.”

## What you will build

You will build a config-loader flow that treats success and failure as explicit data.

## Concepts in plain English

`ok` wraps a success value. `fail` creates a failure with a kind and message. `ok?` and `error?` ask which branch you have. `value` unwraps success. `error` unwraps failure information. `unwrap_or` applies a fallback. `map_result` and `and_then` let you transform successful results without erasing the failure path.

## Words introduced

Primary words: `assert_ok`, `assert_error`, `ok?`, `value`, `error`, `ok`, `fail`, `error?`, `unwrap_or`, `map_result`, `and_then`, and `result_envelope`.

## Guided example

Run the config-loader example:

```bash
rco run examples/learn/09-results-and-errors/config-loader.rco
```

The success path is explicit:

```rco
"loaded" ok success var
$success ok? println
$success value println
```

The error path is explicit too:

```rco
"ConfigMissing" "config file was not found" fail missing var
$missing error? println
$missing error "kind" at println
```

Use `unwrap_or` when fallback is the policy:

```rco
"default profile" $missing unwrap_or println
```

Use `map_result` and `and_then` when you want to preserve the result boundary:

```rco
[ 2 * ] 21 ok map_result value println
[ uppercase ok ] "ada" ok and_then value println
```

## How to read the code

A common pattern is:

```rco
$result ok? if
  $result value println
else
  $result error "message" at println
end
```

The condition is `$result ok?`, not `$result`. The result itself is data. You must ask whether it is success before unwrapping.

At app and API boundaries, `result_envelope` turns a result into a stable map with `ok`, `data`, `error`, and `meta` fields. That makes responses predictable even when the operation failed.

## Try it

Change the example so the final envelope wraps `$success` instead of `$missing`. Then inspect:

```rco
$envelope "ok" at println
$envelope "data" at println
```

In tests, use `assert_ok` or `assert_error` when the branch itself is the behavior you want to prove.

## Check your understanding

- A `Result` must be checked with `ok?` or `error?` before conditional logic.
- Use `value` only when success is guaranteed or failure should stop the run.
- Keep error kind and message as structured data until the UI or log boundary.

## Common mistakes

- Treating `Result` values as conditions.
- Unwrapping before deciding what failure should mean.
- Collapsing every error to a string too early.

## What you know now

You are ready to work with host APIs, conversions, HTTP, filesystem calls, and database operations that can fail without turning every failure into a crash.
