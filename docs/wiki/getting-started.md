# Getting Started

After installing `rco`, use it as the Ricochet toolchain:

```powershell
rco new my_app
rco new --with-sqlite my_beta_app
rco routes my_app
rco migrate status my_beta_app
rco migrate apply my_beta_app
rco doctor my_app
rco verify my_app
rco lsp-diagnostics my_app/app/Models/User.rco --pretty
rco lsp
rco bench --smoke
rco doc my_app
rco test my_app
```

MVC apps serve static files from `public/` at `/assets` by default. Override
that with `[web.static] dir = "..."` and `mount = "/..."` in `ricochet.toml`.

## Scripts

Run a script:

```powershell
rco run examples/basic-oop.rco
```

Format, lint, and package a script:

```powershell
rco fmt examples/basic-oop.rco
rco lint examples/basic-oop.rco
rco build examples/basic-oop.rco
rco run-bytecode build/app.rcob
rco emit-source build/app.rcob
rco package examples/basic-oop.rco --output basic-oop.exe
```

## Desktop GUI And TUI

Preview and package a desktop GUI app:

```powershell
rco gui examples/webview_ui.rco
rco package examples/webview_ui.rco --gui --output webview-ui.exe
```

Run and package an interactive terminal UI app:

```powershell
rco tui examples/tui_counter.rco
rco package examples/tui_counter.rco --tui --output tui-counter.exe
```

Package an MVC app directory as a desktop beta build:

```powershell
rco new my_desktop_app
rco package my_desktop_app --gui --mvc --output my-desktop-app.exe
```

Use `--output webview-ui` on Linux and macOS. Windows/macOS GUI launchers open
native WebView windows. Linux GUI launchers open through the system browser, and
Debian packages generated for GUI apps declare an `xdg-utils` runtime
dependency.

On Linux, the same `package` command can also create portable tarballs or
Debian packages for a `.rco` file:

```bash
rco package examples/basic-oop.rco \
  --output basic-oop \
  --linux-package tar \
  --linux-package deb \
  --package-name basic-oop \
  --package-version 0.1.0
```

## Showcase Apps

Explore the beta showcase apps:

```powershell
rco check examples/showcase/sqlite_notes
rco run examples/showcase/package_auth_forms/main.rco
rco run examples/showcase/ai_provider_probe/main.rco
rco gui examples/showcase/gui_task_monitor.rco
rco debug --step examples/showcase/debugger_demo.rco
```

See `examples/showcase/README.md` for the SQLite notes MVC app, first-party
package consumer, AI provider probe, GUI v2 task monitor, and debugger demo.
