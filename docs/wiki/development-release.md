# Development And Release

## Benchmarks

`rco bench` runs a repeatable local benchmark suite for parser/compiler work,
VM arithmetic, function and method dispatch, collection mutation, JSON
encode/decode, template rendering, package verification, and a SQLite MVC
request path. Use `rco bench --smoke` for CI-sized coverage or
`rco bench --iterations 5` for a local baseline. The current website-copy
baseline lives in `docs/benchmarks/2026-06-17-baseline.md`.

## Developing Ricochet

Use Cargo when changing the Rust implementation itself. For an uninstalled
source-tree run, this is equivalent to `rco run examples/basic-oop.rco`:

```powershell
cargo run -p ricochet_cli --bin rco -- run examples/basic-oop.rco
```

## Verification

For contributor verification, use a current stable Rust toolchain and install
the formatter, linter, and audit plugin explicitly:

```powershell
rustup component add rustfmt clippy
cargo install cargo-audit --locked
```

Then run:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit --deny warnings
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1
```

The acceptance suite validates the static reference docs, editor assets, word
inventory drift check, examples, scaffolded project checks/tests, and a live
`rco serve` smoke request against the generated no-database scaffold.

## Release Packaging

See `docs/releases/v0.1.18-beta.md` for the current beta release notes,
hardening checklist, and artifact smoke-test expectations.

Windows release packages are built from this repository with:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-release.ps1
```

The script builds `rco.exe`, `rco-gui.exe`, and `ricochet.exe`, creates a
portable ZIP, writes `SHA256SUMS.txt`, and creates a Windows `.exe` installer
when NSIS `makensis.exe` is installed. It also writes
`ARTIFACTS-windows-x64.json`, a machine-readable release manifest covering the
ZIP, installer when present, checksum file, and signing-status report. GitHub
Actions installs NSIS automatically in the release workflow.

Windows signing is controlled with `-SigningMode auto|require|skip|dry-run`.
Local and branch dry-runs can use `-SigningMode dry-run` to write a
`SIGNING-windows-x64.txt` report without requiring a certificate. Production
tag releases use `require` in CI so missing `signtool.exe`,
`RICOCHET_WINDOWS_CERT_SHA1`, or a certificate installed in
`Cert:\CurrentUser\My` fails loudly instead of publishing unsigned artifacts.
Set `RICOCHET_WINDOWS_TIMESTAMP_URL` to override the default timestamp server.

For production tag builds, GitHub Actions imports the Authenticode certificate
before packaging with `scripts/setup-windows-signing-certificate.ps1`. Configure
these repository secrets:

- `RICOCHET_WINDOWS_CERT_PFX_BASE64`: base64 of the `.pfx` signing certificate.
- `RICOCHET_WINDOWS_CERT_PASSWORD`: password for the `.pfx`.
- `RICOCHET_WINDOWS_CERT_SHA1`: optional expected SHA-1 thumbprint to validate;
  the setup step also writes the imported thumbprint back to
  `RICOCHET_WINDOWS_CERT_SHA1` for the package script.

Local credential setup uses the same environment variables:

```powershell
$env:RICOCHET_WINDOWS_CERT_PFX_BASE64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes("C:\path\ricochet-signing.pfx"))
$env:RICOCHET_WINDOWS_CERT_PASSWORD = "<pfx password>"
$envFile = Join-Path ([IO.Path]::GetTempPath()) ("ricochet-windows-signing-" + [guid]::NewGuid().ToString("N") + ".ps1")
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\setup-windows-signing-certificate.ps1 -Required -EnvFile $envFile
. $envFile
```

Validate a Windows output directory with:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-release-artifacts.ps1 -Target windows-x64 -RequireInstaller
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-store-packaging.ps1 -Target windows-x64
```

Omit `-RequireInstaller` for local dry-runs on machines without NSIS; CI keeps
it enabled because release jobs install NSIS and require the installer artifact.
For production tag outputs, verify the Authenticode signatures on the portable
ZIP contents and installer before upload:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release-signatures.ps1 -Target windows-x64 -RequireProduction -ExpectedWindowsThumbprint $env:RICOCHET_WINDOWS_CERT_SHA1
```

Linux release packages are built on Linux with:

```bash
bash scripts/package-release-linux.sh
```

The script builds `rco`, `rco-gui`, and `ricochet`, creates a portable tarball
with an `install.sh` helper, writes `SHA256SUMS-linux-x64.txt`, and creates a
Debian `.deb` package with `dpkg-deb`. It also writes
`ARTIFACTS-linux-x64.json`, including SHA-256, size, source, signing report, and
detached `.asc` signature relationships when signatures are produced.

Linux release packages include a terminal desktop launcher for `rco repl`, an
SVG icon, AppStream metainfo, a changelog, maintainer metadata, and the bundled
reference docs/packages/examples. GUI applications produced by
`rco package --gui --linux-package tar|deb` include their own `.desktop`,
AppStream, icon, and changelog metadata.

Linux detached artifact signatures are controlled with
`--signature-mode auto|require|skip|dry-run`. Dry-run mode writes
`SIGNING-linux-x64.txt`; production tag releases use `require` in CI and expect
`RICOCHET_LINUX_GPG_KEY` to name an imported GPG key.

For production tag builds, GitHub Actions imports the GPG private key before
packaging with `scripts/setup-linux-gpg-key.sh`. Configure these repository
secrets:

- `RICOCHET_LINUX_GPG_PRIVATE_KEY_BASE64`: base64 of an armored or binary GPG
  secret-key export.
- `RICOCHET_LINUX_GPG_KEY`: optional key id or fingerprint to validate and use;
  when omitted and exactly one secret key is imported, the setup step writes the
  imported fingerprint to `RICOCHET_LINUX_GPG_KEY`.
- `RICOCHET_LINUX_GPG_OWNERTRUST_BASE64`: optional base64 ownertrust export.

Local credential setup uses the same environment variables:

```bash
export RICOCHET_LINUX_GPG_PRIVATE_KEY_BASE64="$(gpg --armor --export-secret-keys KEYID | base64 -w0)"
export RICOCHET_LINUX_GPG_KEY="KEYID"
env_file="$(mktemp)"
bash scripts/setup-linux-gpg-key.sh --required --env-file "$env_file"
set -a
. "$env_file"
set +a
```

Validate a Linux output directory with:

```powershell
pwsh -NoProfile -File ./scripts/validate-release-artifacts.ps1 -Target linux-x64 -RequireDeb
pwsh -NoProfile -File ./scripts/validate-store-packaging.ps1 -Target linux-x64
```

For production tag outputs, verify every detached GPG signature against the
selected signing key before upload:

```powershell
pwsh -NoProfile -File ./scripts/verify-release-signatures.ps1 -Target linux-x64 -RequireProduction -GpgKey $env:RICOCHET_LINUX_GPG_KEY
```

macOS release tarballs are built on macOS with:

```bash
bash scripts/package-release-macos.sh --target macos-arm64
bash scripts/package-release-macos.sh --target macos-x64
```

The script builds `rco`, `rco-gui`, and `ricochet`, creates a portable tarball
with an `install.sh` helper, writes a target-specific checksum file, and writes
`ARTIFACTS-<target>.json` for the tarball, checksum file, and signing-status
report. GitHub Actions builds Apple Silicon and Intel tarballs on separate
macOS runners.

macOS signing and notarization are controlled with `--signing-mode` and
`--notarization-mode`, each accepting `auto`, `require`, `skip`, or `dry-run`.
Dry-run mode writes a `SIGNING-<target>.txt` report. Production tag releases
use `require` in CI; scheduled nightlies use `auto` so unsigned beta artifacts
are allowed only with an explicit report. Configure
`RICOCHET_MACOS_SIGN_IDENTITY` and `RICOCHET_MACOS_NOTARY_PROFILE` when signing
and notarization credentials are available on the runner.

For production tag builds, GitHub Actions creates an ephemeral keychain, imports
the P12 signing certificate, configures codesign access, and stores a
notarytool profile before packaging with
`scripts/setup-macos-signing-keychain.sh`. The package script submits a
temporary ZIP copy to Apple notarization and keeps the tarball as the published
artifact. It preserves Apple notarytool's accepted submission response as
`NOTARY-<target>.json`, includes that report in the checksum file and artifact
manifest, and records the relationship in `SIGNING-<target>.txt`. An `always()`
cleanup step runs
`scripts/cleanup-macos-signing-keychain.sh` after packaging to restore the
previous keychain state and remove the generated keychain. Configure these
repository secrets:

- `RICOCHET_MACOS_CERT_P12_BASE64`: base64 of the signing certificate `.p12`.
- `RICOCHET_MACOS_CERT_PASSWORD`: password for the `.p12`.
- `RICOCHET_MACOS_KEYCHAIN_PASSWORD`: optional password for the ephemeral
  keychain; the setup script generates one when omitted.
- `RICOCHET_MACOS_SIGN_IDENTITY`: optional expected codesign identity to
  validate; when omitted, the setup step selects an imported identity and writes
  it for the package script.
- `RICOCHET_MACOS_NOTARY_PROFILE`: required notarytool profile name to create
  and pass to packaging.
- Apple ID notarization path: `RICOCHET_MACOS_NOTARY_APPLE_ID`,
  `RICOCHET_MACOS_NOTARY_TEAM_ID`, and
  `RICOCHET_MACOS_NOTARY_PASSWORD`.
- App Store Connect API key notarization path:
  `RICOCHET_MACOS_NOTARY_API_KEY_BASE64`, `RICOCHET_MACOS_NOTARY_KEY_ID`, and
  `RICOCHET_MACOS_NOTARY_ISSUER_ID`.

Local credential setup uses the same environment variables:

```bash
export RICOCHET_MACOS_CERT_P12_BASE64="$(base64 -i /path/ricochet-signing.p12 | tr -d '\n')"
export RICOCHET_MACOS_CERT_PASSWORD="<p12 password>"
export RICOCHET_MACOS_NOTARY_PROFILE="ricochet-release"
export RICOCHET_MACOS_NOTARY_APPLE_ID="release@example.com"
export RICOCHET_MACOS_NOTARY_TEAM_ID="TEAMID1234"
export RICOCHET_MACOS_NOTARY_PASSWORD="<app-specific password>"
env_file="$(mktemp)"
bash scripts/setup-macos-signing-keychain.sh --env-file "$env_file"
set -a
. "$env_file"
set +a
trap 'bash scripts/cleanup-macos-signing-keychain.sh' EXIT
```

For App Store Connect API key notarization, use
`RICOCHET_MACOS_NOTARY_API_KEY_BASE64`, `RICOCHET_MACOS_NOTARY_KEY_ID`, and
`RICOCHET_MACOS_NOTARY_ISSUER_ID` instead of the Apple ID variables.

Validate a macOS output directory with:

```bash
pwsh -NoProfile -File ./scripts/validate-release-artifacts.ps1 -Target macos-arm64
pwsh -NoProfile -File ./scripts/validate-store-packaging.ps1 -Target macos-arm64
pwsh -NoProfile -File ./scripts/validate-release-artifacts.ps1 -Target macos-x64
pwsh -NoProfile -File ./scripts/validate-store-packaging.ps1 -Target macos-x64
```

For production tag outputs, verify the extracted binaries' codesign signatures
and the `NOTARY-<target>.json` accepted notarization report before upload:

```bash
pwsh -NoProfile -File ./scripts/verify-release-signatures.ps1 -Target macos-arm64 -RequireProduction -ExpectedMacosIdentity "$RICOCHET_MACOS_SIGN_IDENTITY"
pwsh -NoProfile -File ./scripts/verify-release-signatures.ps1 -Target macos-x64 -RequireProduction -ExpectedMacosIdentity "$RICOCHET_MACOS_SIGN_IDENTITY"
```

## Production Release Commands

Production releases are tag-driven. With the GitHub repository secrets from
this guide configured, the normal release path is:

```powershell
$version = "0.1.18"
$tag = "v$version"

git fetch origin
git checkout main
git pull --ff-only origin main
git status --short

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit --deny warnings
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1

git tag -a $tag -m "Ricochet $tag"
git push origin $tag
gh run list --workflow Release --limit 5
gh run watch <run-id> --exit-status
gh release view $tag --json tagName,name,isDraft,isPrerelease,url
```

The release workflow imports production signing credentials on tag jobs, builds
Windows, Linux, and macOS packages, validates artifact manifests, validates
store-ready packaging, verifies production signatures, writes the update channel
metadata, writes the combined checksum file, and creates the GitHub release.

For a manual production package dry run before tagging, run the platform
packaging commands on their native operating systems with production modes:

```powershell
# Windows, with NSIS and the Authenticode certificate already available.
$version = "0.1.18"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -Version $version -Target windows-x64 -RequireInstaller -SigningMode require
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-release-artifacts.ps1 -Target windows-x64 -PackageVersion $version -RequireInstaller
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-store-packaging.ps1 -Target windows-x64 -PackageVersion $version -RequireProduction
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release-signatures.ps1 -Target windows-x64 -RequireProduction -ExpectedWindowsThumbprint $env:RICOCHET_WINDOWS_CERT_SHA1
```

```bash
# Linux, with GPG credentials imported.
version="0.1.18"
bash scripts/package-release-linux.sh --target linux-x64 --version "$version" --signature-mode require
pwsh -NoProfile -File ./scripts/validate-release-artifacts.ps1 -Target linux-x64 -PackageVersion "$version" -RequireDeb
pwsh -NoProfile -File ./scripts/validate-store-packaging.ps1 -Target linux-x64 -PackageVersion "$version" -RequireProduction
pwsh -NoProfile -File ./scripts/verify-release-signatures.ps1 -Target linux-x64 -RequireProduction -GpgKey "$RICOCHET_LINUX_GPG_KEY"
```

```bash
# macOS, once for each native target with the signing keychain prepared.
version="0.1.18"
bash scripts/package-release-macos.sh --target macos-arm64 --version "$version" --signing-mode require --notarization-mode require
pwsh -NoProfile -File ./scripts/validate-release-artifacts.ps1 -Target macos-arm64 -PackageVersion "$version"
pwsh -NoProfile -File ./scripts/validate-store-packaging.ps1 -Target macos-arm64 -PackageVersion "$version" -RequireProduction
pwsh -NoProfile -File ./scripts/verify-release-signatures.ps1 -Target macos-arm64 -RequireProduction -ExpectedMacosIdentity "$RICOCHET_MACOS_SIGN_IDENTITY"
```

After all platform artifacts are collected in one `dist` directory, validate the
release channel and combined artifact set:

```powershell
pwsh -NoProfile -File ./scripts/write-update-channel.ps1 -DistDir dist -Channel stable -Version 0.1.18 -ReleaseTag v0.1.18 -ReleaseUrl https://github.com/BARKx4/Ricochet/releases/tag/v0.1.18
pwsh -NoProfile -File ./scripts/validate-update-channel.ps1 -DistDir dist -Channel stable -Version 0.1.18 -RequireProduction
```

The release workflow packages the Windows, Linux, and macOS artifacts, writes a
validated `UPDATE-CHANNEL-stable.json`, writes a combined `SHA256SUMS.txt`, and
attaches the ZIP, Windows installer, Linux tarball, Debian package, macOS
tarballs, checksums, signing-status reports, per-target JSON manifests, notary
reports, and update-channel metadata to the GitHub release. The combined
checksum file includes the uploaded JSON manifests, macOS notary reports, and
update-channel metadata. Each package job validates its
`ARTIFACTS-<target>.json` manifest after the package smoke test and before
uploading artifacts, validates store-ready package contents and metadata,
including installer/deb requirements in CI and detached Linux `.asc` signature
relationships when signatures are present. Production tag package jobs also
verify signatures before upload: Windows checks the portable ZIP executables
and installer Authenticode signatures, Linux verifies detached GPG signatures,
and macOS verifies codesign plus the accepted notarytool response. See
[Store Packaging](store-packaging.md) for marketplace handoff rules and
[Updater Workflow](updater-workflow.md) for channel metadata, version-check,
verification, and rollback rules.

The same workflow also runs nightly from `main`. Nightly builds use a version
like `X.Y.Z-nightly.N`, build the same Windows, Linux, and macOS packages, and
upload them as GitHub Actions artifacts for 30 days. Nightlies do not create
public GitHub releases.

## Reference Docs

The documentation website is static and lives at `docs/reference/index.html`.
Open it directly in a browser; there is no build step.
