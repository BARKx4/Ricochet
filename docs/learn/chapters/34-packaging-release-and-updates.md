# Chapter 34: Packaging, Release, And Updates

## What You Will Build

You will package the Chapter 21 task dashboard as a standalone TUI launcher.
The example reuses a known-good app, writes the package into a local ignored
`build/` directory, and prints the artifact name and size so you can confirm
that packaging actually produced something.

## Concepts

- `rco package` as the local application packaging command.
- TUI, GUI, MVC GUI, and Linux tar/deb package modes.
- Standalone launcher embedding for bytecode and application bundles.
- Release scripts as the production layer above local packaging.
- Checksums, artifact manifests, signing status, and update channels as
  operator-facing release metadata.

## Words Introduced

Primary coverage: `rco package`, `--output`, `--tui`, `--gui`, `--mvc`,
`--linux-package tar`, `--linux-package deb`, `--package-name`,
`--package-version`, `--package-description`, release scripts, store packaging
validation, signature verification, and updater channel metadata.

## Guided Example

Open `examples/learn/34-packaging-release-and-updates/run-lab.ps1`:

```powershell
$ErrorActionPreference = "Stop"

$exampleRoot = Split-Path -Parent $PSCommandPath
$repoRoot = Resolve-Path (Join-Path $exampleRoot "..\..\..")
$buildDir = Join-Path $exampleRoot "build"
$output = Join-Path $buildDir "learn-task-dashboard.exe"

New-Item -ItemType Directory -Force -Path $buildDir | Out-Null

Push-Location $repoRoot
try {
  cargo run -q -p ricochet_cli --bin rco -- package examples/learn/21-tui/task-dashboard.rco --tui --output $output --package-name learn-task-dashboard --package-version 0.1.0 --package-description "Learn Ricochet task dashboard TUI package"
}
finally {
  Pop-Location
}

$artifact = Get-Item -LiteralPath $output
"packaged artifact: $($artifact.Name)"
"artifact bytes: $($artifact.Length)"
```

Run it from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File examples/learn/34-packaging-release-and-updates/run-lab.ps1
```

The first line comes from `rco package`:

```text
packaged E:\LLM Projects\Ricochet\examples\learn\34-packaging-release-and-updates\build\learn-task-dashboard.exe
```

The wrapper then confirms the local artifact:

```text
packaged artifact: learn-task-dashboard.exe
artifact bytes: 40573898
```

The exact byte count can vary by platform, build profile, and launcher changes,
but the output path and nonzero size are the important checks.

The central command is:

```powershell
cargo run -q -p ricochet_cli --bin rco -- package examples/learn/21-tui/task-dashboard.rco --tui --output examples/learn/34-packaging-release-and-updates/build/learn-task-dashboard.exe
```

`--tui` selects the console launcher behavior used by terminal applications.
For desktop webview apps, use `--gui`. For an MVC project packaged as a local
desktop webview app, combine `--gui --mvc`. On Linux, `--linux-package tar` and
`--linux-package deb` add portable tarball or Debian package outputs.

Package metadata is separate from program behavior:

```powershell
--package-name learn-task-dashboard `
--package-version 0.1.0 `
--package-description "Learn Ricochet task dashboard TUI package"
```

Treat that metadata as release-facing user information. It should match the
artifact you intend to distribute.

## Try It

Inspect the generated executable:

```powershell
Get-Item -LiteralPath examples/learn/34-packaging-release-and-updates/build/learn-task-dashboard.exe |
  Select-Object Name, Length, LastWriteTime
```

Create a checksum for the local artifact:

```powershell
Get-FileHash -Algorithm SHA256 -LiteralPath examples/learn/34-packaging-release-and-updates/build/learn-task-dashboard.exe
```

Package the Chapter 22 GUI document instead:

```powershell
cargo run -q -p ricochet_cli --bin rco -- package examples/learn/22-gui/notes_gui.rco --gui --output examples/learn/34-packaging-release-and-updates/build/learn-notes-gui.exe --package-name learn-notes-gui --package-version 0.1.0
```

If you are on Linux, try a Linux package mode from a Linux checkout:

```powershell
cargo run -q -p ricochet_cli --bin rco -- package examples/learn/21-tui/task-dashboard.rco --tui --output examples/learn/34-packaging-release-and-updates/build/learn-task-dashboard --linux-package tar --linux-package deb --package-name learn-task-dashboard --package-version 0.1.0
```

The Learn lab does not publish anything. It only creates local artifacts under
`examples/learn/34-packaging-release-and-updates/build/`.

## Common Mistakes

- Treating packaging as a substitute for testing.
- Forgetting `--tui` or `--gui`, then wondering why the launcher does not match
  the application surface.
- Packaging the wrong source path because the command was run from a different
  directory than expected.
- Committing reproducible build outputs. This lab ignores `build/` because the
  executable is generated.
- Publishing unsigned or unchecked artifacts without clear signing status.
- Treating update-channel JSON as an updater by itself. It is metadata for
  installers, package managers, or a future elevated updater to consume.

## Safety Notes

Packaging can embed application code, static files, manifests, and configuration
that you point at. Review the source tree and metadata before sharing an
artifact. Do not include private local files just because they happened to be
near the app you packaged.

The production release scripts add checksums, per-target manifests, store
packaging validation, and signature/notarization checks. Those checks are there
so users and operators can tell what was built, how it was verified, and whether
it is safe to install.

## Production Notes

Local `rco package` is the developer packaging loop. Production Ricochet
releases use platform-specific scripts:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-release.ps1
```

```bash
bash scripts/package-release-linux.sh
bash scripts/package-release-macos.sh --target macos-arm64
```

Those scripts write release artifacts, checksums, signing-status reports, and
per-target JSON manifests. The release workflow then validates store-ready
packaging and verifies production signatures when a production tag requires
them.

Update-channel files are release metadata:

```powershell
pwsh -NoProfile -File ./scripts/write-update-channel.ps1 -DistDir dist -Channel candidate -Version 0.1.19-rc.1 -ReleaseTag v0.1.19-rc.1 -ReleaseUrl https://github.com/BARKx4/Ricochet/releases/tag/v0.1.19-rc.1
pwsh -NoProfile -File ./scripts/validate-update-channel.ps1 -DistDir dist -Channel candidate -Version 0.1.19-rc.1 -RequireProduction
```

Stable channel metadata requires production signature or notarization evidence.
Candidate channel metadata can record SHA-256 verification for dry-run signed
candidate artifacts.

## Reference Links

- `docs/reference/index.html`
- `docs/reference/guides/development-release.html`
- `docs/reference/guides/store-packaging.html`
- `docs/reference/guides/updater-workflow.html`
- `docs/wiki/development-release.md`
- `docs/wiki/store-packaging.md`
- `docs/wiki/updater-workflow.md`
- `docs/feature-map.md`

## What You Know Now

You know how to turn a Ricochet program into a local distributable launcher,
choose the right package mode for TUI, GUI, MVC GUI, and Linux package outputs,
attach release metadata, and distinguish local packaging from the fuller
production release workflow with checksums, manifests, signatures, store
validation, and update-channel metadata.
