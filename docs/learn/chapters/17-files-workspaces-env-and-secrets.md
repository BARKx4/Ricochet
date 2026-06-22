# Chapter 17: Files, Workspaces, Environment, Config, And Secrets

## What You Will Build

This chapter will build a settings loader that uses local files and configuration safely.

## Concepts

- Filesystem and workspace reads, writes, metadata, and containment checks.
- Environment values, secret references, and nested config lookup.
- Result-returning boundaries around local data.

## Words Introduced

Primary coverage: filesystem, workspace, environment, config, and secret system words.

## Guided Example

Planned example: `examples/learn/17-files-workspaces-env-and-secrets/settings-loader.rco`.

## Try It

Readers will read a settings file and validate paths without destructive operations.

## Common Mistakes

- Skipping workspace containment checks.
- Printing secret material instead of resolving it only where needed.

## Safety Notes

Destructive file operations will be documented with explicit warnings and non-destructive dry examples first.

## Production Notes

Production examples should keep secrets out of logs and preserve path containment.

## Reference Links

Links will point to filesystem, workspace, environment, config, and secret references when drafted.

## What You Know Now

Readers will know how to access local data with clear boundaries.
