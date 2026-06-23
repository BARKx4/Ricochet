# Chapter 30: Registries, Publish, Yank, And Mirror

## What You Will Build

You will build a local registry lab. The checked-in example contains a tiny
scoped package, a local file-backed registry produced with `rco publish`, a
static registry index produced with `rco registry rebuild`, and package metadata
that includes content and provenance hashes. The runnable validation command is
read-only: `rco registry check`.

## Concepts

- Static and hosted registry workflows.
- Publish, rebuild, check, search, install, yank, serve, and mirror.
- Provenance, signatures, hashes, semver, aliases, scopes, and bearer tokens.
- Why duplicate same-version publish is not an update strategy.
- Why hosted publish/yank tokens stay in environment variables.

## Words Introduced

Primary coverage: `rco publish`, `rco registry rebuild`,
`rco registry check`, `rco search`, registry dependencies, static registry
metadata, hosted `rco registry serve`, hosted `rco registry yank`, and hosted
`rco registry mirror`.

## Guided Example

Open `examples/learn/30-registries/local_registry_lab`. The package source
lives in `package_source/greeter_pkg`:

```toml
[package]
name = "@learn/greeter"
version = "0.1.0"
description = "Tiny scoped package used by Learn Ricochet Chapter 30."
```

The package exports one variable and one function:

```ricochet
"learn-registry-greeter" registry_package_label var

( name -> String ) registry_greeting function
  name var
  "hello, " $name concat " from registry" concat
end
```

The local registry in this example was created with:

```powershell
cargo run -q -p ricochet_cli --bin rco -- publish examples/learn/30-registries/local_registry_lab/package_source/greeter_pkg --registry examples/learn/30-registries/local_registry_lab/registry --provenance-file examples/learn/30-registries/local_registry_lab/package_source/provenance.json
```

That command wrote the file-backed package record and reported package and
provenance digests:

```text
published @learn/greeter 0.1.0 ... with integrity sha256:82ac1084ccd4cf404678f3728ef268d74c0398463020fc770cc4687914326450
attached provenance sha256:9b23922733c71d2cbd6c5546de1b0330976ca779e9422e2ac3f410a85147d9bd
```

`publish --registry` stores the package source under the registry directory.
Then `registry rebuild` creates the static, mirrorable registry view:

```powershell
cargo run -q -p ricochet_cli --bin rco -- registry rebuild examples/learn/30-registries/local_registry_lab/registry
```

The static index is intentionally small:

```toml
[registry]
format = "ricochet-static-registry-v1"

[packages]
"@learn/greeter" = "packages/@learn/greeter.toml"
```

The package metadata points at an archive artifact and records the package tree,
archive, and provenance hashes:

```toml
[package]
name = "@learn/greeter"

[[versions]]
version = "0.1.0"
archive = "artifacts/@learn/greeter/0.1.0/greeter-0.1.0.tar.gz"
archive_integrity = "sha256:7436da403955618c38241e33392b061e813a7826e4a865e445ec3432c0675462"
package_integrity = "sha256:82ac1084ccd4cf404678f3728ef268d74c0398463020fc770cc4687914326450"
yanked = false
provenance = "sha256:9b23922733c71d2cbd6c5546de1b0330976ca779e9422e2ac3f410a85147d9bd"
```

Run the read-only registry check:

```powershell
cargo run -q -p ricochet_cli --bin rco -- registry check examples/learn/30-registries/local_registry_lab/registry
```

Expected output:

```text
checked 1 static registry versions
```

Search the local static registry:

```powershell
cargo run -q -p ricochet_cli --bin rco -- search greeter --registry examples/learn/30-registries/local_registry_lab/registry
```

Expected output:

```text
@learn/greeter 0.1.0
```

To consume a registry package, install it into an app with a local alias:

```powershell
cargo run -q -p ricochet_cli --bin rco -- add registry:@learn/greeter --registry examples/learn/30-registries/local_registry_lab/registry --as greeter --version "^0.1.0"
```

The app imports by alias, not by the package's scoped registry name:

```ricochet
"greeter/greeting" import
"Ada" registry_greeting println
```

`rco add` writes the dependency table, installs the package into
`.ricochet/packages/greeter`, and records integrity in `ricochet.lock`.

## Try It

In a scratch copy of the lab, change the package version to `0.1.1`, publish it
to a scratch registry, rebuild, check, and search:

```powershell
cargo run -q -p ricochet_cli --bin rco -- publish package_source/greeter_pkg --registry registry
cargo run -q -p ricochet_cli --bin rco -- registry rebuild registry
cargo run -q -p ricochet_cli --bin rco -- registry check registry
cargo run -q -p ricochet_cli --bin rco -- search greeter --registry registry
```

Use `--dry-run` before writing a real registry:

```powershell
cargo run -q -p ricochet_cli --bin rco -- publish package_source/greeter_pkg --registry registry --dry-run
```

Hosted registry publish and yank use a registry URL plus an environment-backed
bearer token:

```powershell
cargo run -q -p ricochet_cli --bin rco -- registry serve ./hosted-registry --publisher "@learn/*=RICOCHET_REGISTRY_TOKEN"
cargo run -q -p ricochet_cli --bin rco -- publish package_source/greeter_pkg --registry-url http://127.0.0.1:3001 --token-env RICOCHET_REGISTRY_TOKEN
cargo run -q -p ricochet_cli --bin rco -- registry yank @learn/greeter 0.1.0 --registry-url http://127.0.0.1:3001 --token-env RICOCHET_REGISTRY_TOKEN
```

Do not run hosted commands with placeholder tokens. Set the token in the
environment and keep it out of source files, manifests, and lockfiles.

## Common Mistakes

- Replacing the same version without understanding registry rules.
- Putting bearer tokens directly in source examples.
- Expecting `registry yank` to delete artifacts. Yank marks a hosted version
  unavailable while preserving historical metadata.
- Forgetting to run `registry rebuild` after publishing to a file-backed local
  registry.
- Checking in generated registry changes without reviewing archive,
  provenance, and package integrity diffs.
- Importing by registry scope instead of the local dependency alias.

## Safety Notes

The checked-in example is local and read-only during validation. The publish
command shown above has already been run to create the fixture. Do not rerun it
against the checked-in registry unless you are intentionally changing the
fixture; publishing the same package version again should fail or be treated as
a registry-state mistake.

Never store bearer tokens in source examples. Hosted publish, yank, and serve
commands use `--token-env NAME` so token values stay in the process
environment.

## Production Notes

Production registry workflows should document package identity ownership,
version policy, provenance policy, detached signature policy, token handling,
publisher authorization, duplicate-version behavior, yanking policy, static
mirror cadence, and recovery when a hosted registry is unavailable.

`rco registry mirror REGISTRY_URL PATH` exports hosted metadata and artifacts
into the same static registry format used by this chapter. That gives clients a
file/HTTP static fallback path while preserving yanked records, archive
integrity, package integrity, provenance, and signature metadata.

## Reference Links

- `docs/wiki/packages.md`
- `docs/wiki/hosted-registry-protocol.md`
- `docs/reference/guides/packages.html`
- `docs/reference/guides/hosted-registry-protocol.html`

## What You Know Now

You know the package distribution workflow at a user level: publish a package,
rebuild and check a static registry, search package metadata, install by local
alias, keep provenance and integrity visible, and reserve hosted yank/mirror
operations for token-backed registry workflows.
