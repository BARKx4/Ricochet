<p align="center">
  <img src="docs/assets/ricochet-logo.png" alt="Ricochet logo" width="420">
</p>

# Ricochet

Ricochet is a modern, pure-postfix programming language for building real
software in a deliberately different way: stack-first code with a Rust bytecode
VM, dynamic OOP, CLI scripting, MVC web apps, package workflows, debugger
support, and sandboxable host capabilities. It keeps the directness and
composability that make Forth-like languages compelling, while adding the
tools, safety boundaries, documentation, packaging, and editor support expected
from a serious development platform.

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

## Documentation

Start with the wiki-style docs index:

- [Docs Wiki](docs/wiki/README.md)
- [Feature Overview](docs/wiki/features.md)
- [Getting Started](docs/wiki/getting-started.md)
- [Language And Runtime](docs/wiki/language-runtime.md)
- [Web And Data](docs/wiki/web-and-data.md)
- [Host Capabilities And Safety](docs/wiki/host-capabilities.md)
- [Packages And Registries](docs/wiki/packages.md)
- [Editor And Debugging](docs/wiki/editor-debugging.md)
- [Development And Release](docs/wiki/development-release.md)

The static reference site lives at `docs/reference/index.html` and can be opened
directly in a browser.

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
[docs/adding-words.md](docs/adding-words.md) so VM dispatch, tests, reference
docs, LSP completions, editor grammar, validators, and examples stay in sync.
