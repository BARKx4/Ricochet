# Chapter 09: Results And Errors

## What You Will Build

You will build a config-loader flow that treats errors as explicit values.

## Concepts

- `ok` and `fail` values.
- Unwrapping, mapping, chaining, and envelopes.
- API boundaries that return structured results.
- The rule that `Result` values are not conditions by themselves.

## Words Introduced

Primary coverage: `assert_ok`, `assert_error`, `ok?`, `value`, `error`,
`ok`, `fail`, `error?`, `unwrap_or`, `map_result`, `and_then`, and
`result_envelope`.

## Guided Example

Open `examples/learn/09-results-and-errors/config-loader.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/09-results-and-errors/config-loader.rco
```

The success path is explicit:

```ricochet
"loaded" ok success var
$success ok? println
$success value println
```

The error path is explicit too:

```ricochet
"ConfigMissing" "config file was not found" fail missing var
$missing error? println
$missing error "kind" at println
```

Use `unwrap_or` when fallback is the policy:

```ricochet
"default profile" $missing unwrap_or println
```

Use `map_result` and `and_then` when you want to preserve the result boundary:

```ricochet
[ 2 * ] 21 ok map_result value println
[ uppercase ok ] "ada" ok and_then value println
```

At app and API boundaries, `result_envelope` turns a result into a stable map
with `ok`, `data`, `error`, and `meta` fields.

## Try It

Change the example so the final envelope wraps `$success` instead of
`$missing`. Then inspect:

```ricochet
$envelope "ok" at println
$envelope "data" at println
```

In tests, use `assert_ok` or `assert_error` when the branch itself is the
behavior you want to prove.

## Common Mistakes

- Treating `Result` values as conditions. Use `ok?` or `error?`.
- Unwrapping before deciding what failure should mean.
- Collapsing every error to a string too early. The error map carries kind and
  message fields that later code can use.

## What You Know Now

You are ready to work with host APIs, conversions, HTTP, filesystem calls, and
database operations that can fail without turning every failure into a crash.
