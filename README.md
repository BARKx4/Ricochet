<p align="center">
  <img src="docs/assets/ricochet-logo.png" alt="Ricochet logo" width="420">
</p>

# Ricochet

Ricochet is a modern, pure-postfix programming language for building real
software in a deliberately different way: stack-first code with a Rust bytecode
VM, dynamic OOP, CLI scripting, MVC web apps, package workflows, desktop WebView
apps, debugger support, and sandboxable host capabilities. It keeps the
directness and composability that make Forth-like languages compelling, while
adding the tools, safety boundaries, documentation, packaging, and editor
support expected from a serious development platform.

Website: [try.ricochet.today](https://try.ricochet.today/)

## Install

Download published builds from the
[GitHub Releases](https://github.com/BARKx4/Ricochet/releases) page.

On Windows, run `ricochet-vX.Y.Z-windows-x64-setup.exe`. The installer adds
Start Menu entries, including a Ricochet Shell that opens a command prompt with
`rco` available. Portable release ZIPs are also available from the same release
page. Extract the ZIP and run `Ricochet Shell.cmd`, or add the extracted folder
to your `PATH`.

On Linux, install the Debian package with:

```bash
sudo apt install ./ricochet_X.Y.Z_amd64.deb
```

The official GitHub asset for Ricochet `1.0.0` is
`ricochet_1.0.0_amd64.deb`. Inside the package, the Debian control metadata
records `Version: 1.0.0`.

The Debian package declares the current Linux launcher runtime packages:
`libgtk-3-0`, `libwebkit2gtk-4.1-0`, and `libxdo3`. Install those packages
manually before using the portable tarball.

Portable Linux tarballs are also available. Extract the tarball and run
`./install.sh`, or add the extracted folder to your `PATH`.

On macOS, choose the stable tarball for your Mac:
`ricochet-vX.Y.Z-macos-arm64.tar.gz` for Apple Silicon or
`ricochet-vX.Y.Z-macos-x64.tar.gz` for Intel. Extract it and run
`./install.sh`, or add the extracted folder to your `PATH`. Official stable
artifacts contain codesigned binaries and include an accepted Apple
notarization report beside the archive.

For an uninstalled source checkout, install the CLI once:

```powershell
cargo install --path crates/ricochet_cli --bin rco --locked
```

Then try a script or scaffold an app:

```powershell
rco run examples/basic-oop.rco
rco new my_app
rco routes my_app
cd my_app
rco serve
```

For native desktop UI previews and packaging, see
[How to Install and Run Ricochet](https://barkx4.github.io/Ricochet/learn/how-to/install-and-run.html#native-desktop-ui).

For a focused example of every case-sensitive language word, use the generated
[per-word example corpus](examples/words/README.md). Its manifest follows the
live `rco words --json` inventory, and the validator compiles every app while
running all examples that are safe and deterministic without an external host.

## Documentation

Start with the published HTML docs:

- [Reference Home](https://barkx4.github.io/Ricochet/reference/)
- [Learn Ricochet](https://barkx4.github.io/Ricochet/learn/)
- [Install And Run](https://barkx4.github.io/Ricochet/learn/how-to/install-and-run.html)
- [Reference Guides](https://barkx4.github.io/Ricochet/reference/guides/)
- [Feature Overview](https://barkx4.github.io/Ricochet/reference/guides/features.html)
- [Getting Started](https://barkx4.github.io/Ricochet/reference/guides/getting-started.html)
- [Language And Runtime](https://barkx4.github.io/Ricochet/reference/guides/language-runtime.html)
- [Web And Data](https://barkx4.github.io/Ricochet/reference/guides/web-and-data.html)
- [Host Capabilities And Safety](https://barkx4.github.io/Ricochet/reference/guides/host-capabilities.html)
- [Packages And Registries](https://barkx4.github.io/Ricochet/reference/guides/packages.html)
- [Editor And Debugging](https://barkx4.github.io/Ricochet/reference/guides/editor-debugging.html)
- [Development And Release](https://barkx4.github.io/Ricochet/reference/guides/development-release.html)

For a source checkout, the same static docs live under `docs/`; open
`docs/index.html` locally to start at the reference site.

## Contributing

Use a current stable Rust toolchain. For implementation changes, install the
formatter, linter, and audit plugin:

```powershell
rustup component add rustfmt clippy
cargo install cargo-audit --locked
```

Run the local verification suite before opening or merging changes:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit --deny warnings
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1
```

For an uninstalled source-tree run, this is equivalent to
`rco run examples/basic-oop.rco`:

```powershell
cargo run -p ricochet_cli --bin rco -- run examples/basic-oop.rco
```

When adding or renaming public words, follow
[docs/adding-words.html](docs/adding-words.html) so VM dispatch, tests, reference
docs, LSP completions, editor grammar, validators, and the per-word corpus stay
in sync.

## License and project policies

Ricochet's first-party source, documentation, and assets are available under the
[Apache License 2.0](LICENSE). Third-party components remain subject to their
own licenses; see the [third-party license report](THIRD_PARTY_LICENSES.html)
and [supplemental notices](THIRD_PARTY_NOTICES.txt).

Report suspected vulnerabilities privately through the
[security policy](https://github.com/BARKx4/Ricochet/security/policy). For usage
questions, bug reports, and supported build information, see the
[support guide](https://github.com/BARKx4/Ricochet/blob/main/SUPPORT.md).
