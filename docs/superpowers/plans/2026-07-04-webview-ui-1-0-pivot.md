# WebView UI 1.0 Pivot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to execute this plan.

**Goal:** Remove the failed native-control UI experiments from Ricochet's current release surface and make WebView the primary, polished, 1.0-ready desktop UI system.

**Architecture:** Retire the `rco app` native-control renderer path and its WinUI3, Avalonia, and Slint package/host experiments. Preserve and mature the existing `rco gui`, `rco package --gui`, and `rco-gui` WebView path into a first-class app system: native shell, WebView body, Ricochet state.

**Tech Stack:** Rust CLI/VM, Wry WebView, existing Ricochet `webview_*` built-ins, first-party Ricochet package words, PowerShell/Bash release scripts, current HTML docs/reference pipeline.

## Constraints

- Ask the user before deleting anything, including generated hosts, package directories, launcher binaries, or uncommitted hardening edits.
- Do not rewrite published tags or historical release artifacts without explicit approval. Prefer adding a superseded note and removing stale links from current navigation.
- Keep `rco gui`, `rco package --gui`, and `rco-gui` alive as the 1.0 desktop UI path.
- Remove the native-control beta surface from the release candidate path: `rco app`, `rco-app`, `@ricochet/ui`, `@ricochet/winui`, `@ricochet/avalonia`, `@ricochet/slint`, and the WinUI/Avalonia host projects.
- Keep UI syntax aligned with the RPN/postfix guardrail in `AGENTS.md`. New public multiword words use underscores.
- Do not introduce a general-purpose frontend package manager or arbitrary framework integration as a 1.0 dependency. Add a curated WebView app kit first, then leave a narrow escape hatch for advanced assets later.

## Current Evidence

Recent commits show the native-control stack was layered on after the current public RC baseline:

- `cad7afe` - `v0.1.19-rc.4` baseline.
- `06b7920` - backend-neutral native UI package core.
- `3e1cdce` - portable UI document contracts.
- `b43d9cc` - WinUI backend package.
- `57aea82` - native app JSON export in the CLI.
- `6a4ad30` - `rco-app` launcher and native payload packaging.
- `e9bfa37` - WinUI host.
- `3daf85d` - native app UI docs.
- `d2191a9` - native UI showcase app.
- `0591694`, `d1787e6`, `0e62a5d` - Slint package, live renderer, then validate-only fallback.
- `cd8b741` - Avalonia backend and host.
- `4ed2c89` - native component gallery app.

The current working tree also contains uncommitted native host hardening changes:

- `hosts/avalonia/Ricochet.Avalonia.Host/MainWindow.cs`
- `hosts/avalonia/Ricochet.Avalonia.Host/UiRenderer.cs`
- `hosts/winui/Ricochet.WinUI.Host/MainWindow.xaml.cs`
- `hosts/winui/Ricochet.WinUI.Host/UiRenderer.cs`

Treat those four files as cleanup residue. Do not discard them until the user approves the cleanup pass.

## Cleanup Target

Remove these direct native-control experiment surfaces after approval:

- `hosts/winui/Ricochet.WinUI.Host/`
- `hosts/avalonia/Ricochet.Avalonia.Host/`
- `packages/ricochet_ui/`
- `packages/ricochet_winui/`
- `packages/ricochet_avalonia/`
- `packages/ricochet_slint/`
- `crates/ricochet_cli/src/bin/rco_app.rs`

Remove references to those surfaces from:

- `Cargo.toml`
- `Cargo.lock`
- `crates/ricochet_cli/Cargo.toml`
- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `packages/README.md`
- `scripts/acceptance.ps1`
- `scripts/package-release.ps1`
- `scripts/package-release-linux.sh`
- `scripts/package-release-macos.sh`
- `scripts/validate-store-packaging.ps1`
- `scripts/verify-release-signatures.ps1`
- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `docs/feature-map.md`
- `docs/reference/app.js`
- `docs/reference/index.html`
- `docs/reference/validate.ps1`
- `docs/learn/how-to/install-and-run.html`
- current release notes and current navigation pages that promote native UI.

Preserve or improve these WebView surfaces:

- `rco gui`
- `rco package --gui`
- `rco-gui`
- existing `webview_*` built-ins
- `examples/webview_ui.rco`
- `examples/showcase/gui_task_monitor.rco`
- `examples/learn/22-gui/notes_gui.rco`
- `examples/learn/38-capstone-gui/`

## Phase 1: Safety Gate And Baseline

1. From `E:\LLM Projects\Ricochet`, inspect the current state.

   ```powershell
   git status --short --untracked-files=all
   git log --oneline --decorate -20
   git diff --stat
   git diff --name-status cad7afe..HEAD
   ```

2. Present the deletion/discard list to the user and ask for explicit approval.

   Include the uncommitted host edits in the request:

   ```text
   Approval request: delete the native-control experiment files and discard the four uncommitted host hardening changes listed above?
   ```

3. If approval is not granted, stop before destructive edits and keep the plan as the handoff artifact.

4. If approval is granted, stop live native demo processes before deleting files.

   ```powershell
   Get-Process | Where-Object {
     $_.ProcessName -match 'Ricochet|rco-app|dotnet'
   } | Select-Object Id,ProcessName,Path
   ```

   Only stop exact processes the user confirms are part of the demo run.

## Phase 2: Remove Native-Control Backend Surface

1. Remove native-control package and host directories after approval.

   ```powershell
   Remove-Item -LiteralPath 'hosts/winui/Ricochet.WinUI.Host' -Recurse
   Remove-Item -LiteralPath 'hosts/avalonia/Ricochet.Avalonia.Host' -Recurse
   Remove-Item -LiteralPath 'packages/ricochet_ui' -Recurse
   Remove-Item -LiteralPath 'packages/ricochet_winui' -Recurse
   Remove-Item -LiteralPath 'packages/ricochet_avalonia' -Recurse
   Remove-Item -LiteralPath 'packages/ricochet_slint' -Recurse
   Remove-Item -LiteralPath 'crates/ricochet_cli/src/bin/rco_app.rs'
   ```

2. Remove the CLI `app` command and `--app --backend winui|avalonia|slint` packaging paths from `crates/ricochet_cli/src/lib.rs`.

3. Remove the `rco-app` bin entry from `crates/ricochet_cli/Cargo.toml`.

4. Remove native-control-only dependencies:

   - `slint-interpreter`
   - `spin_on`
   - Avalonia host build assumptions in docs/scripts.

5. Regenerate lockfile after dependency removal.

   ```powershell
   cargo check -p ricochet_cli
   ```

6. Remove native-control CLI tests from `crates/ricochet_cli/tests/cli_smoke.rs`.

7. Add or retain focused WebView smoke coverage before moving on.

   ```powershell
   cargo test -p ricochet_cli --test cli_smoke gui
   ```

8. Commit this phase separately.

   ```powershell
   git status --short
   git add Cargo.toml Cargo.lock crates/ricochet_cli hosts packages
   git commit -m "Remove native-control UI experiments"
   ```

## Phase 3: Remove Native-Control Release And Docs Surface

1. Update release packaging scripts so release artifacts contain only current supported launchers:

   - `rco`
   - `ricochet`
   - `rco-gui`

2. Remove `rco-app` from:

   - package manifests
   - store validation
   - signature verification
   - CI/release workflow artifact lists
   - release acceptance checks.

3. Update `docs/feature-map.md` before making roadmap claims.

   Desired feature map wording:

   ```text
   Desktop UI: WebView app runtime is the primary app surface for 1.0. It uses a native host for windows, shell commands, dialogs, packaging, and OS integration, while the app body is rendered by the Ricochet WebView document/runtime.
   ```

4. Replace native UI beta docs with a short superseded note.

   The note should say the native-control experiment was withdrawn before 1.0 because the renderers were not stable enough for new users, and WebView is the supported desktop app path.

5. Remove native-control examples from current reference navigation.

6. Update `docs/reference/app.js`, `docs/reference/index.html`, and `docs/reference/validate.ps1`.

7. Update `packages/README.md` to remove the native backend packages and introduce the new WebView app-kit package target.

8. Run docs validation.

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File docs/reference/validate.ps1
   ```

9. Commit this phase separately.

   ```powershell
   git status --short
   git add .github docs packages scripts
   git commit -m "Document WebView as the desktop UI path"
   ```

## Phase 4: Introduce The WebView App Kit

1. Add a first-party package, recommended name:

   ```text
   packages/ricochet_webview/
   ```

   Public package name:

   ```text
   @ricochet/webview
   ```

2. Package goals:

   - Make common app layouts easy without raw HTML.
   - Keep all state represented as Ricochet values.
   - Emit stable WebView document maps consumed by existing `webview_window`.
   - Avoid a frontend build step for 1.0.

3. Initial package files:

   - `packages/ricochet_webview/ricochet.toml`
   - `packages/ricochet_webview/README.md`
   - `packages/ricochet_webview/layout.rco`
   - `packages/ricochet_webview/components.rco`
   - `packages/ricochet_webview/shell.rco`
   - `packages/ricochet_webview/examples/app_gallery.rco`
   - `packages/ricochet_webview/tests/WebViewAppKitTest.rco`

4. Proposed first wave of public words:

   - `web_app`
   - `web_app_title`
   - `web_app_body`
   - `web_menu_bar`
   - `web_menu`
   - `web_menu_item`
   - `web_toolbar`
   - `web_split_pane`
   - `web_sidebar`
   - `web_tabs`
   - `web_tab`
   - `web_data_grid`
   - `web_tree`
   - `web_modal`
   - `web_toast`
   - `web_status_bar`
   - `web_command_palette`
   - `web_command`
   - `web_keybinding`
   - `web_stylesheet`

5. Keep the low-level built-ins as the primitive layer:

   - `webview_text`
   - `webview_heading`
   - `webview_button`
   - `webview_action`
   - `webview_input`
   - `webview_link`
   - `webview_container`
   - `webview_window`
   - `webview_window_state`

6. Add tests proving the app-kit words emit stable document/state/action maps.

   ```powershell
   cargo test -p ricochet_cli --test cli_smoke webview
   rco test packages/ricochet_webview/tests/WebViewAppKitTest.rco
   ```

7. Commit this phase separately.

   ```powershell
   git add packages/ricochet_webview packages/README.md crates/ricochet_cli/tests/cli_smoke.rs
   git commit -m "Add WebView app kit package"
   ```

## Phase 5: Make WebView Interactive Without Refresh Flash

The current WebView path already generates HTML with action metadata and JavaScript that calls `window.ipc.postMessage(...)`, but the native Wry host path must be upgraded into a live app runtime.

1. Extract the GUI runtime out of the large CLI file if needed.

   Suggested module:

   ```text
   crates/ricochet_cli/src/gui_runtime.rs
   ```

2. Add an IPC handler to the Wry WebView builder.

   Runtime responsibilities:

   - Hold the current Ricochet source/chunk/session.
   - Hold current state/action map.
   - Receive `{ "type": "rco-action", "action": "...", "value": ... }`.
   - Dispatch through the existing action path.
   - Re-render the document to HTML fragments or a stable document JSON payload.
   - Patch the existing DOM in place rather than recreating/reloading the window.

3. Add a small embedded bridge script.

   Required behavior:

   - Event delegation from `[data-rco-action]`.
   - Stable element ids or path keys.
   - Scroll position preservation.
   - Focus preservation where possible.
   - Controlled patching of root/app regions.
   - No full-page reload after ordinary component interaction.

4. Preserve deterministic event replay for tests through `RICOCHET_GUI_EVENT`.

5. Add tests around:

   - action serialization
   - action dispatch
   - state update
   - generated bridge script
   - scroll position preservation script hooks
   - exported HTML containing the expected bridge bootstrap.

6. Manually verify on Windows:

   ```powershell
   rco gui examples/webview_ui.rco
   rco gui packages/ricochet_webview/examples/app_gallery.rco
   ```

   Acceptance criteria:

   - Button clicks do not flash the whole UI.
   - Scrollbars stay where the user left them.
   - Text input does not lose focus after unrelated actions.
   - Menus and command palette open reliably.

7. Commit this phase separately.

   ```powershell
   git add crates/ricochet_cli packages/ricochet_webview examples docs
   git commit -m "Make WebView apps interactive without reloads"
   ```

## Phase 6: Add Native Shell Services Through WebView

The 1.0 target should feel native where native matters, without returning to native-control rendering.

1. Add shell command metadata to the WebView document model.

   Suggested shape:

   ```ricochet
   [
     "File" [
       "Open" "file.open" web_command
       "Save" "file.save" web_command
     ] web_menu
   ] web_menu_bar
   ```

2. Implement Web-rendered menus first because they are cross-platform and testable.

   Acceptance criteria:

   - File menus open.
   - Keyboard navigation works.
   - Commands dispatch through the same WebView IPC path.
   - The visual result is polished enough for first-time users.

3. Add native shell hooks where they offer real OS value:

   - open file dialog
   - save file dialog
   - folder picker
   - clipboard
   - open external URL

4. Proposed shell words:

   - `web_file_open_command`
   - `web_file_save_command`
   - `web_folder_pick_command`
   - `web_clipboard_write_command`
   - `web_open_url_command`

5. Add test fakes for dialogs through environment variables so CI can validate command dispatch without opening modal OS UI.

6. Commit this phase separately.

   ```powershell
   git add crates/ricochet_cli packages/ricochet_webview docs examples
   git commit -m "Add WebView shell commands"
   ```

## Phase 7: Replace The Native Gallery With A WebView 1.0 Demo

1. Create a WebView component gallery that replaces the failed native component gallery.

   Suggested file:

   ```text
   packages/ricochet_webview/examples/app_gallery.rco
   ```

2. The gallery should demonstrate:

   - app shell
   - menu bar
   - toolbar
   - sidebar
   - split pane
   - tabs
   - data grid
   - tree/list
   - form inputs
   - modal
   - toast
   - command palette
   - file dialog command stub.

3. Add a stronger vertical demo only after the app kit feels stable.

   The first vertical demo should prove Ricochet accelerates development in the chosen field by making app structure, state, shell commands, and AI-assisted flows concise in Ricochet itself.

4. Update Learn docs:

   - beginner GUI chapter shows `rco gui`.
   - capstone GUI uses the app kit.
   - reference docs list the primitive WebView words and the app-kit words separately.

5. Update release notes:

   - native-control beta withdrawn before 1.0
   - WebView app runtime promoted
   - new app-kit words listed
   - migration advice from `@ricochet/ui` to `@ricochet/webview`.

6. Commit this phase separately.

   ```powershell
   git add docs examples packages/ricochet_webview
   git commit -m "Add WebView app gallery and docs"
   ```

## Phase 8: Final Validation

Run the full validation stack before calling the pivot complete.

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
powershell -NoProfile -ExecutionPolicy Bypass -File docs/reference/validate.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/acceptance.ps1
```

Run release packaging dry checks on every supported platform script available from the current machine.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-release.ps1 -NoArchive
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/validate-store-packaging.ps1
```

If Linux packaging cannot be validated locally from Windows, leave a clear release-blocking note and run it in the Linux release environment before tagging.

## Completion Criteria

- No current docs, reference pages, package docs, release scripts, or acceptance scripts promote `rco app` or native-control backends.
- `rco-app` is no longer built, packaged, signed, or listed as a current binary.
- Slint/Avalonia/WinUI native-control packages and hosts are removed from current source after user approval.
- WebView remains available through `rco gui`, `rco package --gui`, and `rco-gui`.
- WebView app interactions update without visible full refresh flashes.
- Scroll position and focus survive ordinary interaction.
- File/menu-style workflows are available through the WebView app kit and shell bridge.
- A polished WebView gallery exists for new users exploring the language.
- The feature map names WebView as the 1.0 desktop UI path.
- Full validation passes or every remaining failure has a concrete owner and blocker note.

## Recommended Execution Order

1. Ask for cleanup approval.
2. Remove native-control code and packaging.
3. Update docs to stop promoting the withdrawn path.
4. Add the WebView app-kit package.
5. Add live WebView IPC and in-place patching.
6. Add shell services.
7. Build the WebView gallery.
8. Validate, commit each phase, and only then resume vertical demo planning.

