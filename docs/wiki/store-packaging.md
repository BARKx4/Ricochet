# Store Packaging

Ricochet publishes store-ready release inputs, not marketplace account uploads.
The repository-owned boundary is packaging, metadata, signing, notarization,
checksums, manifests, and update-channel metadata. Microsoft Partner Center,
Apple App Store Connect, Flathub, Snapcraft, and distro repository uploads stay
operator-owned because they require external publisher accounts, review flows,
and account-specific legal metadata.

## Supported Release Inputs

Windows releases provide:

- a portable `ricochet-v<version>-windows-x64.zip`.
- an NSIS `ricochet-v<version>-windows-x64-setup.exe` installer.
- Authenticode signing for the ZIP executables and installer on production
  tags.
- `ARTIFACTS-windows-x64.json`, `SIGNING-windows-x64.txt`, and
  `SHA256SUMS.txt`.

Linux releases provide:

- a portable `ricochet-v<version>-linux-x64.tar.gz`.
- a Debian `ricochet_<version>_amd64.deb`.
- `ricochet-repl.desktop`, `ricochet.svg`, AppStream metainfo, changelog,
  maintainer metadata, bundled docs, examples, and first-party packages.
- detached GPG signatures on production tags.
- `ARTIFACTS-linux-x64.json`, `SIGNING-linux-x64.txt`, and
  `SHA256SUMS-linux-x64.txt`.

macOS releases provide:

- portable `ricochet-v<version>-macos-arm64.tar.gz` and
  `ricochet-v<version>-macos-x64.tar.gz` archives.
- codesigned binaries and accepted Apple notarization reports on production
  tags.
- `ARTIFACTS-<target>.json`, `SIGNING-<target>.txt`,
  `NOTARY-<target>.json`, and target checksum files.

## Store Readiness Gate

Run the store packaging validator after artifact manifest validation:

```powershell
pwsh -NoProfile -File ./scripts/validate-store-packaging.ps1 `
  -Target windows-x64 `
  -PackageVersion 0.1.18

pwsh -NoProfile -File ./scripts/validate-store-packaging.ps1 `
  -Target linux-x64 `
  -PackageVersion 0.1.18

pwsh -NoProfile -File ./scripts/validate-store-packaging.ps1 `
  -Target macos-arm64 `
  -PackageVersion 0.1.18
```

Add `-RequireProduction` for production tag artifacts. That rejects dry-run,
skipped, or unsigned fallback signing reports and requires the production
signing/notarization state for the target.

The release workflow runs this validator for every platform package before
uploading artifacts. It inspects:

- Windows ZIP contents and installer presence.
- Linux tarball metadata and Debian package control/filesystem metadata.
- macOS tarball contents and, for production tags, accepted notarization
  reports.

## Marketplace Submission

Marketplace submission starts from the validated GitHub release artifacts:

- Windows: upload the signed installer or use it as the source for a future
  Partner Center/MSIX packaging pass tied to the store publisher identity.
- Linux: use the Debian package and AppStream metadata for apt repositories or
  as source material for Flathub/Snapcraft packaging.
- macOS: distribute the signed and notarized tarballs directly. A Mac App Store
  SKU would need a separate sandboxed bundle, entitlements, product identity,
  and App Store Connect submission flow.

Do not publish marketplace artifacts that failed manifest validation,
store-packaging validation, signature verification, or update-channel
validation.
