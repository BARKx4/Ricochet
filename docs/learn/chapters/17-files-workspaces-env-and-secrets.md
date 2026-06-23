# Chapter 17: Files, Workspaces, Environment, Config, And Secrets

## What You Will Build

You will build a settings loader that reads local data, checks workspace
containment, reads process-local environment values, and resolves a dry-run
secret reference.

## Concepts

- Filesystem and workspace reads, writes, metadata, and containment checks.
- Environment values, secret references, and nested config lookup.
- Result-returning boundaries around local data.
- Why destructive operations need explicit operator intent.

## Words Introduced

Primary coverage: `env_get`, `env_set`, `env`, `secret_env`,
`secret_literal`, `secret_resolve`, `config_get`, `fs_read_text`,
`fs_write_text`, `fs_exists?`, `fs_list`, `fs_create_dir`, `fs_delete`,
`workspace_resolve`, `workspace_contains?`, `workspace_metadata`,
`workspace_list`, `workspace_read_text`, `workspace_write_text`,
`workspace_mkdir`, `workspace_delete`, `workspace_copy`, and
`workspace_move`.

## Guided Example

Open `examples/learn/17-files-workspaces-env-and-secrets/settings-loader.rco`
and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/17-files-workspaces-env-and-secrets/settings-loader.rco
```

The example starts with read-only filesystem access:

```ricochet
"README.md" fs_exists? println
"README.md" fs_read_text value length println
```

Workspace words return richer metadata and containment information:

```ricochet
resolveOptions map
"docs/learn/index.md" $resolveOptions workspace_resolve value resolved var
$resolved "inside_root" at println
$resolved "exists" at println
```

Bound reads keep examples predictable:

```ricochet
readOptions map
$readOptions "max_bytes" 512 put! drop
"examples/learn/01-hello-world/main.rco" $readOptions workspace_read_text value length println
```

Environment writes are process-local. This does not mutate the parent shell:

```ricochet
"RICOCHET_LEARN_MODE" "docs" env_set value drop
"RICOCHET_LEARN_MODE" env_get value println
```

Use secret references instead of storing secret values directly in config maps:

```ricochet
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

## Try It

Run the example with a sandboxed profile and an explicit filesystem root:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run --capability-profile sandboxed --fs-root . examples/learn/17-files-workspaces-env-and-secrets/settings-loader.rco
```

Then try a read-only sandbox:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run --capability-profile sandboxed --fs-root . --fs-readonly examples/learn/17-files-workspaces-env-and-secrets/settings-loader.rco
```

The example still works because it does not write files. If you later adapt it
to write generated output, use `workspace_resolve` and `workspace_contains?`
before writing.

## Common Mistakes

- Skipping workspace containment checks.
- Printing secret material instead of resolving it only where needed.
- Using broad filesystem access for a task that only needs one workspace root.
- Treating `fs_*` paths and `workspace_*` paths as interchangeable. Workspace
  words give structured containment details that are better for app code.

## Safety Notes

This chapter documents `fs_delete` and `workspace_delete`, but the runnable
example does not call them. Delete operations should be gated by explicit user
or operator intent, and workspace deletion should resolve the target path
before the operation. `workspace_delete` refuses to delete the configured
filesystem root, but that is a last guard, not a substitute for careful UI and
review.

## Production Notes

Production examples should keep secrets out of logs, prefer `secret_env`, and
preserve path containment. Use `workspace_write_text`, `workspace_mkdir`,
`workspace_copy`, and `workspace_move` only after validating the resolved target
and choosing overwrite behavior deliberately.

## Reference Links

- `docs/reference/guides/host-capabilities.html`
- `docs/reference/guides/language-runtime.html`

## What You Know Now

You know how to access local data with clear boundaries: read and inspect first,
keep workspace paths contained, resolve secrets only when needed, and treat
write/delete/move operations as explicit operator actions.
