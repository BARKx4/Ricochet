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

Portable Linux tarballs are also available. Extract the tarball and run
`./install.sh`, or add the extracted folder to your `PATH`.

On macOS, choose the unsigned tarball for your Mac:
`ricochet-vX.Y.Z-macos-arm64.tar.gz` for Apple Silicon or
`ricochet-vX.Y.Z-macos-x64.tar.gz` for Intel. Extract it and run
`./install.sh`, or add the extracted folder to your `PATH`. These beta tarballs
are not notarized by Apple.

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
docs, LSP completions, editor grammar, validators, and examples stay in sync.
