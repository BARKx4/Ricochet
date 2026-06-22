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
when NSIS `makensis.exe` is installed. GitHub Actions installs NSIS
automatically in the release workflow.

Windows signing is controlled with `-SigningMode auto|require|skip|dry-run`.
Local and branch dry-runs can use `-SigningMode dry-run` to write a
`SIGNING-windows-x64.txt` report without requiring a certificate. Production
tag releases use `require` in CI so missing `signtool.exe`,
`RICOCHET_WINDOWS_CERT_SHA1`, or a certificate installed in
`Cert:\CurrentUser\My` fails loudly instead of publishing unsigned artifacts.
Set `RICOCHET_WINDOWS_TIMESTAMP_URL` to override the default timestamp server.

Linux release packages are built on Linux with:

```bash
bash scripts/package-release-linux.sh
```

The script builds `rco`, `rco-gui`, and `ricochet`, creates a portable tarball
with an `install.sh` helper, writes `SHA256SUMS-linux-x64.txt`, and creates a
Debian `.deb` package with `dpkg-deb`.

Linux release packages include a terminal desktop launcher for `rco repl`, an
SVG icon, AppStream metainfo, a changelog, maintainer metadata, and the bundled
reference docs/packages/examples. GUI applications produced by
`rco package --gui --linux-package tar|deb` include their own `.desktop`,
AppStream, icon, and changelog metadata.

Linux detached artifact signatures are controlled with
`--signature-mode auto|require|skip|dry-run`. Dry-run mode writes
`SIGNING-linux-x64.txt`; production tag releases use `require` in CI and expect
`RICOCHET_LINUX_GPG_KEY` to name an imported GPG key.

macOS release tarballs are built on macOS with:

```bash
bash scripts/package-release-macos.sh --target macos-arm64
bash scripts/package-release-macos.sh --target macos-x64
```

The script builds `rco`, `rco-gui`, and `ricochet`, creates a portable tarball
with an `install.sh` helper, and writes a target-specific checksum file. GitHub
Actions builds Apple Silicon and Intel tarballs on separate macOS runners.

macOS signing and notarization are controlled with `--signing-mode` and
`--notarization-mode`, each accepting `auto`, `require`, `skip`, or `dry-run`.
Dry-run mode writes a `SIGNING-<target>.txt` report. Production tag releases
use `require` in CI; scheduled nightlies use `auto` so unsigned beta artifacts
are allowed only with an explicit report. Configure
`RICOCHET_MACOS_SIGN_IDENTITY` and `RICOCHET_MACOS_NOTARY_PROFILE` when signing
and notarization credentials are available on the runner.

To publish a GitHub release, push a version tag:

```powershell
git tag vX.Y.Z
git push origin vX.Y.Z
```

The release workflow packages the Windows, Linux, and macOS artifacts, writes a
combined `SHA256SUMS.txt`, and attaches the ZIP, Windows installer, Linux
tarball, Debian package, macOS tarballs, checksums, and signing-status reports
to the GitHub release.

The same workflow also runs nightly from `main`. Nightly builds use a version
like `X.Y.Z-nightly.N`, build the same Windows, Linux, and macOS packages, and
upload them as GitHub Actions artifacts for 30 days. Nightlies do not create
public GitHub releases.

## Reference Docs

The documentation website is static and lives at `docs/reference/index.html`.
Open it directly in a browser; there is no build step.
