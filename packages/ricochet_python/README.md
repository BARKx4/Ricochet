# @ricochet/python

`@ricochet/python` is a first-party bridge for using existing Python SDKs from
Ricochet without waiting for every provider to publish native Ricochet bindings.

The package starts a retained Python worker with `process_start`, sends JSON-lines
requests through `process_write`, and returns ordinary Ricochet `Result` values.
Primitive Python values come back as Ricochet JSON values. Modules, classes,
functions, and non-JSON objects come back as Python reference maps that can be
used in later calls.

## Modules

- `protocol.rco`: request, reference, and response envelope helpers.
- `worker.rco`: retained worker startup and request exchange.
- `sdk.rco`: generic `py_import`, `py_call`, `py_construct`, `py_call_method`,
  `py_getattr`, `py_setattr`, `py_exports`, and `py_release` helpers.
- `generator.rco`: static Ricochet source generation for checked-in wrapper
  words.
- `worker/ricochet_python_bridge.py`: the Python JSON-lines worker.

## Example

```ricochet
"@ricochet/python/sdk" import

env map
"python" "packages/ricochet_python/worker" "packages/ricochet_python/worker" $env py_worker_start value worker var

args array
$args 20 push! drop
$args 22 push! drop
kwargs map
$worker "fake_sdk.add" $args $kwargs py_call value println

$worker py_worker_shutdown value drop
```

Run scripts that use the bridge with `--allow-process`; add `--process-root` if
you want to bound the Python worker and SDK path.

## Wrapper Generation

Runtime calls are useful for exploration, but package-quality bindings should be
checked in as ordinary Ricochet source. `generator.rco` can inspect a Python
module and emit static wrapper words:

```ricochet
"@ricochet/python/generator" import

env map
"python" "packages/ricochet_python/worker" "packages/ricochet_python/worker" $env py_worker_start value worker var
$worker "fake" "fake_sdk" false py_generate_wrappers value println
```

The generated functions accept `(worker args kwargs -> Result)` and delegate to
`py_call`, so callers keep explicit control over worker lifetime, interpreter
path, environment, and capability policy.
