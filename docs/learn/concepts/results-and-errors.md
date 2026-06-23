# Concept: Results and Errors

Ricochet uses explicit `Result` values for operations that can fail. A result is either success, created with `ok`, or failure, created with `fail`.

```rco
"loaded" ok
"ConfigMissing" "config file was not found" fail
```

A result is data. It is not automatically a condition and it is not secretly thrown as an exception.

## The basic pattern

```rco
$config_result ok? if
  $config_result value println
else
  $config_result error "message" at println
end
```

Check the result before unwrapping it. Use `unwrap_or` only when a fallback is truly the policy.

## Why this matters

Filesystem, HTTP, JSON parsing, numeric conversion, dynamic imports, database work, and package workflows all cross boundaries where failure is normal. Explicit results let you decide what each failure means at the correct layer.
