# Chapter 29: Packages, Imports, And Dependencies

> Part V: Packages, Registries, Macros, Tooling, and Release

## Why this chapter matters

Packages let Ricochet projects reuse code. Imports, manifests, locks, verification, and audit commands keep reuse understandable and reproducible.

## What you will build

You will build and consume a local math package. The app will import package
code statically, load the same module dynamically, call exported package
functions through a module map, read an exported variable, and verify the
generated dependency lockfile.

## Concepts in plain English

A package is reusable code with metadata. An import loads code. A lockfile records the resolved dependency graph.

The chapter uses these concepts:

* Dependencies, package manifests, and lockfiles.
* Static imports for ordinary compile-time module sharing.
* Dynamic runtime imports with `import_dynamic`, `module_call`, and
  `module_get`.
* Integrity and path containment.
* Local path dependencies versus registry dependencies.

## Vocabulary and commands

Primary coverage: dependency CLI family, package manifests, `ricochet.lock`,
static package imports, `import_dynamic`, `module_call`, and `module_get`.

## Guided example

Open `examples/learn/29-packages/local_math_package`. The app manifest declares
a local dependency by alias:

```
[package]
name = "learn_local_math_package"
version = "0.1.0"

[dependencies.math_tools]
path = "./packages/math_tools"
version = "^0.1.0"
```

The package has its own manifest:

```
[package]
name = "@learn/math_tools"
version = "0.1.0"
description = "Tiny local package used by Learn Ricochet Chapter 29."
```

Its module exports one variable and three functions:

```
"learn-math-tools" math_package_name var

( values -> Number ) math_sum function
  values var
  0 total var
  [
    value var
    $total $value + total set
  ] $values each drop
  $total
end

( values -> Number ) math_average function
  values var
  $values empty? if
    0
  else
    $values math_sum $values count /
  end
end

( subtotal taxRate -> Number ) math_add_tax function
  taxRate var
  subtotal var
  $subtotal $subtotal $taxRate * +
end
```

Static imports are the default when you know the module at authoring time:

```
"math_tools/stats" import

prices array
$prices 12 push! drop
$prices 18 push! drop
$prices 30 push! drop

$prices math_sum println
$prices math_average println
120 0.08 math_add_tax println
```

Dynamic imports are for runtime module selection and plugin-like flows. They
return a `Result`; the ok value is a module map:

```
"math_tools/stats" import_dynamic value stats var

args array
$args 120 push! drop
$args 0.08 push! drop

$stats "math_add_tax" $args module_call value println
$stats "math_package_name" module_get value println
```

Use `module_call` for functions and `module_get` for exported variables or
classes. If you try to read a function with `module_get`, Ricochet returns a
clear error:

```
$stats "math_sum" module_get error "message" at println
```

Run the example from a directory that contains the example path:

```
rco run examples/learn/29-packages/local_math_package/main.rco
```

Expected output:

```
Local math package
static sum:60
static average:20
static taxed:129.6
dynamic taxed:129.6
module label:learn-math-tools
module function read error:module binding "math_sum" is a function; use module_call
[]
```

The example includes a generated `ricochet.lock`. Verify it:

```
rco verify examples/learn/29-packages/local_math_package
```

The lockfile records the dependency source, version requirement, resolved
version, and package-content integrity hash. It is intentionally machine
checked rather than hand edited.

## How to read the example

Read package and registry examples as reproducibility workflows. A manifest says what the project needs, a lock or registry records what was resolved, and verification checks that the artifacts still match expectations.

## Try it

Create a scratch copy of the example, remove the dependency table from
`ricochet.toml`, then add it with the CLI:

```
Push-Location examples/learn/29-packages/local_math_package
rco add ./packages/math_tools --as math_tools --version "^0.1.0"
rco install
rco verify
Pop-Location
```

Now edit `packages/math_tools/stats.rco` and run `rco verify` again. Verification
should fail with a package integrity mismatch. When the package change is
intentional, run `rco install` from the app directory to refresh the lock.

## Check your understanding

- Which file or command makes the workflow reproducible?
- What would change if a dependency version changed?
- Which command checks the project before you publish or consume it?

## Common mistakes

* Editing lockfiles by hand.
* Treating dynamic import as a replacement for clear static dependencies.
* Using `..` in dependency paths. Dependency paths are kept inside the package
  project and parent traversal is rejected.
* Calling `module_get` for functions instead of `module_call`.
* Forgetting that dynamic imports still use resolver, containment, and lock
  integrity checks.
* Teaching registry publish/yank before local path dependencies are solid.

## Safety notes

This example stays entirely local and does not fetch remote code. Do not run
dependency examples against unreviewed registries or Git URLs until you are
ready to inspect provenance, integrity, and lockfile changes.

## Production guidance

Production dependency workflows should pin versions, review provenance, commit
lockfiles, run `rco verify` in CI, and keep dynamic import specifiers narrow.
Prefer static imports for ordinary app code. Use dynamic imports when the module
name is genuinely runtime-selected and the caller is prepared to handle a
`Result` error.

Registry publishing, hosted registry yanks, static mirrors, and publish
tokens are Chapter 30 topics. Chapter 29’s job is to make one local dependency
boringly clear.

## Reference links

* `docs/wiki/packages.md`
* `docs/feature-map.md`
* `packages/README.md`
* `examples/showcase/package_macro_queue_report`

## What you know now

You know how Ricochet projects share local code through manifests, dependency
aliases, static package imports, dynamic runtime imports, module maps, and
lockfile verification.

## Next step

Continue to [Chapter 30: Registries, Publish, Yank, And Mirror](30-registries-publish-yank-and-mirror.md).
