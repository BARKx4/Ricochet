# Updater Workflow

Ricochet's production updater workflow is channel metadata plus platform
verification. The v1 beta does not ship a self-replacing `rco update` command;
installers, package managers, or a future elevated desktop updater can consume
the same metadata without moving risky install privileges into the Ricochet
runtime.

## Channel Metadata

Production tag releases publish `UPDATE-CHANNEL-stable.json` beside the release
artifacts. The file uses schema `ricochet.update-channel` version `1` and is
written by:

```powershell
pwsh -NoProfile -File ./scripts/write-update-channel.ps1 `
  -DistDir dist `
  -Channel stable `
  -Version 0.1.18 `
  -ReleaseTag v0.1.18 `
  -ReleaseUrl https://github.com/BARKx4/Ricochet/releases/tag/v0.1.18
```

The channel document records:

- release version, tag, release URL, generation time, and rollout percentage.
- rollback policy, currently `reject-older-or-equal` by default.
- one platform entry per target: `windows-x64`, `linux-x64`, `macos-arm64`,
  and `macos-x64`.
- each target's `ARTIFACTS-<target>.json` manifest hash and size.
- primary archive, installer/package artifact, checksum file, signing report,
  optional notary report, required verification methods, and per-artifact
  SHA-256 metadata.

Validate channel metadata before publishing it:

```powershell
pwsh -NoProfile -File ./scripts/validate-update-channel.ps1 `
  -DistDir dist `
  -Channel stable `
  -Version 0.1.18 `
  -RequireProduction
```

The release workflow writes and validates the channel document before writing
the combined `SHA256SUMS.txt`, so the channel file is also included in the final
release checksum bundle.

## Update Check Contract

A Ricochet-compatible updater should:

1. Fetch the channel file from an HTTPS release or channel URL.
2. Require `schema = "ricochet.update-channel"` and `schema_version = 1`.
3. Require the expected channel name, such as `stable`.
4. Reject channel versions less than or equal to the installed version unless
   the operator has explicitly chosen a manual rollback.
5. Select the platform target for the current machine.
6. Download the target manifest and all artifacts referenced by the selected
   platform entry.
7. Verify every SHA-256 value against the channel metadata and target manifest.
8. Run platform verification before install:
   - Windows: Authenticode verification on the portable ZIP executables and
     installer.
   - Linux: detached GPG signature verification for the tarball and Debian
     package.
   - macOS: codesign verification for extracted binaries plus
     `NOTARY-<target>.json` with Apple notarytool status `Accepted`.
9. Stage the new install outside the active installation path.
10. Keep a backup of the previous install until the new `rco --help` or a
    stronger product smoke test succeeds.

## Rollback Story

Automatic rollback means reverting from a failed staged install back to the
previous local install. It does not mean silently downgrading from the public
channel.

By default, channel consumers reject older or equal versions. Manual rollback
uses the immutable GitHub release for the desired older version, then repeats
the same manifest, checksum, signature, and notarization checks before
installing.

Updater state should live outside Ricochet VM images, application state, and
project workspaces. A platform installer can keep its own backup directory or
package-manager rollback metadata; Ricochet scripts should not delete an
existing installation unless the updater has already staged and verified the
replacement.
