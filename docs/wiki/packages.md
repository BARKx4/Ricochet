# Packages And Registries

Add a package dependency from a Ricochet project:

```powershell
rco add ./packages/ricochet_forms --as forms
rco add ./packages/ricochet_forms --as forms --version "^0.1.0"
rco publish ./packages/ricochet_forms --registry ../ricochet-registry
rco publish ./packages/ricochet_forms --registry ../ricochet-registry `
  --provenance-file provenance.json --signature-file forms.sig --signature-kind minisign
rco registry rebuild ../ricochet-registry
rco registry check ../ricochet-registry
rco search forms --registry-url file:///E:/path/to/ricochet-registry/index.toml
rco add registry:@ricochet/forms --registry ../ricochet-registry --as forms --version "^0.1.0"
rco add registry:@ricochet/forms --registry-url file:///E:/path/to/ricochet-registry/index.toml --as forms --version "^0.1.0"
rco add github:BARKx4/ricochet_auth@v0.1.0 --no-fetch
rco install
rco verify
rco audit --json
```

`ricochet.lock` records each resolved package source, cache path, optional Git
commit, optional local registry, optional semantic version requirement, resolved
package version, and `sha256:` content integrity hash.

`rco publish --registry` creates a file-backed local registry entry for
packages with `[package] name` and `version`; optional `--provenance-file`,
`--signature-file`, and `--signature-kind` flags attach registry metadata and
lockfile-visible `sha256:` digests for external attestation/signing workflows.

`rco registry rebuild PATH` writes a static `index.toml`, package metadata, and
release-style archives, while `rco registry check PATH` verifies the generated
hashes. `rco add registry:name --registry PATH` installs from the local
registry layout, or uses `RICOCHET_REGISTRY` when `--registry` is omitted.

`rco add registry:@scope/name --registry-url URL --as alias` installs from a
static HTTP/file registry index with required `sha256:` archive verification.
`rco install` and `rco verify` reject packages whose `[package] version` does
not satisfy the manifest requirement, and `rco verify` recomputes the package
tree hash while ignoring VCS metadata, so it catches local path changes,
registry cache drift, or cached Git package drift without fetching or rewriting
anything. Locked static-registry installs also reject same-version registry
metadata changes instead of accepting replacement archives during ordinary
`rco install`.

Use `rco search QUERY --registry-url URL` to discover static registry packages,
and use `rco audit` or `rco audit --json` for dependency reports in CI and
release tooling.

Static imports load dependencies at compile time and remain the preferred
default for ordinary module sharing:

```forth
"forms/validation" import
"email" "ada@example.com" form_field
"value" at
```

Dynamic imports load a module from a runtime string while using the same
resolver, path containment, and lock integrity checks:

```forth
"forms/validation" import_dynamic value forms var
args array
args get "email" push! drop
args get "ada@example.com" push! drop
forms get "form_field" args get module_call value
"value" at
```

`import_dynamic` returns a `Result` containing a module map with `id`,
`specifier`, `path`, `variables`, `functions`, and `classes`. Use
`module_call` for functions and `module_get` for exported variables or classes.
Import failures, missing functions, and lock/containment failures are returned
as `Result` errors so request handlers and scripts can report them explicitly.
Loaded modules are cached for the life of the VM; restart the VM or request
snapshot to reload changed module code.
