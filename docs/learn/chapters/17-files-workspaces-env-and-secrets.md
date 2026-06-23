# Chapter 17: Files, Workspaces, Environment, Config, And Secrets

> Part III: Host Capabilities and Local App Surfaces

## Why this chapter matters

Useful scripts often need configuration, files, environment variables, and secrets. Ricochet treats those as explicit host interactions that should be scoped and checked.

## What you will build

You will build a settings loader that reads local data, checks workspace
containment, reads process-local environment values, and resolves a dry-run
secret reference.

## Concepts in plain English

Files, workspaces, environment variables, configuration, and secrets are all host boundaries. They should return results and respect explicit roots or allow-lists.

The chapter uses these concepts:

* Filesystem and workspace reads, writes, metadata, and containment checks.
* Environment values, secret references, and nested config lookup.
* Result-returning boundaries around local data.
* Why destructive operations need explicit operator intent.

## Vocabulary and commands

Primary coverage: `env_get`, `env_set`, `env`, `secret_env`,
`secret_literal`, `secret_resolve`, `config_get`, `fs_read_text`,
`fs_write_text`, `fs_exists?`, `fs_list`, `fs_create_dir`, `fs_delete`,
`workspace_resolve`, `workspace_contains?`, `workspace_metadata`,
`workspace_list`, `workspace_read_text`, `workspace_write_text`,
`workspace_mkdir`, `workspace_delete`, `workspace_copy`, and
`workspace_move`.

## Guided example

Open `examples/learn/17-files-workspaces-env-and-secrets/settings-loader.rco`
and run:

```
rco run examples/learn/17-files-workspaces-env-and-secrets/settings-loader.rco
```

The example starts with read-only filesystem access:

```
"README.md" fs_exists? println
"README.md" fs_read_text value length println
```

Workspace words return richer metadata and containment information:

```
resolveOptions map
"docs/learn/index.md" $resolveOptions workspace_resolve value resolved var
$resolved "inside_root" at println
$resolved "exists" at println
```

Bound reads keep examples predictable:

```
readOptions map
$readOptions "max_bytes" 512 put! drop
"examples/learn/01-hello-world/main.rco" $readOptions workspace_read_text value length println
```

Environment writes are process-local. This does not mutate the parent shell:

```
"RICOCHET_LEARN_MODE" "docs" env_set value drop
"RICOCHET_LEARN_MODE" env_get value println
```

Use secret references instead of storing secret values directly in config maps:

```
settings map
provider map
$provider "token" "dry-run-token" secret_literal put! drop
$settings "provider" $provider put! drop

path array
$path "provider" push! drop
$path "token" push! drop

$settings $path config_get value secret_resolve value length println
```

`secret_literal` is for tests, fixtures, and dry-run examples. Real local apps
should usually use `secret_env`.

## How to read the example

Read the command first, then the code. The command grants the host powers the program may use. Inside the program, host calls usually return `Result` values or retained resource handles, so keep the same habit from Chapter 09: check the result, unwrap deliberately, and release or close resources when you are done.

## Try it

Run the example with a sandboxed profile and an explicit filesystem root:

```
rco run --capability-profile sandboxed --fs-root . examples/learn/17-files-workspaces-env-and-secrets/settings-loader.rco
```

Then try a read-only sandbox:

```
rco run --capability-profile sandboxed --fs-root . --fs-readonly examples/learn/17-files-workspaces-env-and-secrets/settings-loader.rco
```

The example still works because it does not write files. If you later adapt it
to write generated output, use `workspace_resolve` and `workspace_contains?`
before writing.

## Check your understanding

- Can you name the host capability this example needs?
- Can you point to the command flag or profile that grants it?
- Can you identify which calls may return `Result` values?
- Can you describe how the example avoids touching more of the host system than necessary?

## Common mistakes

* Skipping workspace containment checks.
* Printing secret material instead of resolving it only where needed.
* Using broad filesystem access for a task that only needs one workspace root.
* Treating `fs_*` paths and `workspace_*` paths as interchangeable. Workspace
  words give structured containment details that are better for app code.

## Safety notes

This chapter documents `fs_delete` and `workspace_delete`, but the runnable
example does not call them. Delete operations should be gated by explicit user
or operator intent, and workspace deletion should resolve the target path
before the operation. `workspace_delete` refuses to delete the configured
filesystem root, but that is a last guard, not a substitute for careful UI and
review.

## Production guidance

Production examples should keep secrets out of logs, prefer `secret_env`, and
preserve path containment. Use `workspace_write_text`, `workspace_mkdir`,
`workspace_copy`, and `workspace_move` only after validating the resolved target
and choosing overwrite behavior deliberately.

## Reference links

* `docs/reference/guides/host-capabilities.html`
* `docs/reference/guides/language-runtime.html`

## What you know now

You know how to access local data with clear boundaries: read and inspect first,
keep workspace paths contained, resolve secrets only when needed, and treat
write/delete/move operations as explicit operator actions.

## Next step

Continue to [Chapter 18: HTTP And Streams](18-http-and-streams.md).
