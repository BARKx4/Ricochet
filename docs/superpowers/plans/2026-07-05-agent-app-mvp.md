# Cross-Platform Agent App MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a polished Windows/Linux desktop agent app that proves Ricochet can compose UI, local process/PTY workflows, approvals, and packaging with less glue code than a conventional stack.

**Non-Goals:**

- No new native renderer.
- No broad agent framework.
- No cloud dependency for the core demo.
- No unregistered public words.

## MVP Shape

- WebView app-kit shell with sidebar, toolbar, task list, log/output pane, approvals pane, and status bar.
- Local workspace root and process root configured explicitly.
- One agent workflow that reads a repo, proposes a small change, requests approval, runs a command, and reports result.
- Packaged Windows app and Linux embedded WebView app.

## Readiness Gate

Status: go for MVP implementation after the MVP readiness consolidation gates.

Evidence from the readiness branch:

- VM/task runtime state has a snapshot boundary.
- MVP-critical built-in words have seeded registry metadata.
- WebView GUI and package code are extracted from the CLI root.
- Web serving construction has a `ServeBuilder` boundary.
- Strictness diagnostics exist for dynamic convenience fallbacks.
- Windows checks, Linux WSL build, Linux GUI package smoke, and deterministic WebView HTML export passed.
- Human Windows/Linux embedded WebView pass on `examples/webview_ui.rco` matched across platforms: native menu/action dispatch worked, no visible full-window refresh flash was reported, and Linux opened as an embedded WSLg WebView window. The Linux sample used light mode while Windows used dark mode.

Known sample-app wart: pressing `Save Profile` resets the editable name field to `Ada` because the example action only persists `saved = true`; this is app state wiring, not renderer refresh. The MVP app should explicitly preserve input state.

The MVP app itself still needs visible-desktop feel testing for flicker, scroll retention, native menu polish, and theme parity before public demo recording, but that does not block starting implementation.

## Existing Capabilities To Consume

- Desktop UI: `webview_window_app`, `web_command`, `web_menu`, `web_menu_bar`, `web_toolbar`, `web_sidebar`, `web_tabs`, `web_split_pane`, `web_table`, `web_form_row`, `web_status_bar`, `web_command_button`.
- Shell services: WebView file/folder dialogs, clipboard read/write, and external URL launch.
- Workspace access: `workspace_resolve`, `workspace_contains?`, `workspace_metadata`, `workspace_list`, `workspace_read_text`, `workspace_write_text`, `workspace_mkdir`, `workspace_copy`, `workspace_move`.
- Approval flow: `approval_create`, `approval_claim`, `approval_complete`, `approval_reject`, `approval_detail`, `approval_release`.
- Process and PTY: `process_start`, `process_read`, `process_write`, `process_cancel`, `process_release`, `process_spawn`, `process_spawn_task`, `pty_start`, `pty_read`, `pty_write`, `pty_stop`, `pty_release`.
- Packaging: `rco gui`, `rco package --gui`, Linux `--linux-package tar`, and Linux `--linux-package deb`.

## Public Word Change Policy

The initial MVP proposes no new public Ricochet words. If implementation reveals a hard gap, pause and update this plan before coding the word.

Any proposed public word must include:

| Word | Stack effect | Capability requirement | Registry metadata entry | Docs/editor validation update | Test path |
| --- | --- | --- | --- | --- | --- |
| None planned | N/A | N/A | N/A | N/A | N/A |

## Implementation Tasks

- [ ] **Task 1: App Shell**
  - Create a single Ricochet WebView app document with native menu metadata.
  - Layout: sidebar for workspaces/tasks, toolbar for run controls, split pane for task detail and output, approvals pane, status bar.
  - Use first-party app-kit words only; raw HTML is allowed only for small presentational gaps.
  - Verification: export with `RICOCHET_GUI_EXPORT_HTML` and assert shell markers, action IDs, menu metadata, and `window.__ricochetApplyDocument`.

- [ ] **Task 2: Workspace Selection**
  - Add an explicit workspace-root selection flow using existing shell dialogs and workspace words.
  - Resolve and display workspace metadata before enabling process actions.
  - Keep all workspace reads/writes under the configured filesystem root.
  - Verification: CLI export smoke with a fixture workspace; negative test for path outside root.

- [ ] **Task 3: Agent Task Model**
  - Represent tasks as Ricochet maps/lists containing id, title, workspace, status, requested command, approval id, output offsets, and result.
  - Persist enough task state to survive document refreshes inside the WebView session.
  - Keep task state in Ricochet values first; do not add a database dependency for the MVP.
  - Verification: action replay through `RICOCHET_GUI_EVENT` creates, selects, and updates a task without visible document reload.

- [ ] **Task 4: Approval Workflow**
  - Create approval records before any write or process command that changes workspace state.
  - Show approval details in the approvals pane and wire approve/reject actions through WebView callbacks.
  - Use `approval_claim`, `approval_complete`, and `approval_reject` to make state transitions explicit.
  - Verification: event replay covers approve and reject paths; rejected commands do not run.

- [ ] **Task 5: Process And PTY Command Runner**
  - Run a small, deterministic local command under the configured process root.
  - Use retained process jobs for simple command output and PTY only when an interactive transcript is useful.
  - Read output incrementally using offsets so UI refreshes do not duplicate logs.
  - Verification: Windows and Linux smoke run a harmless command, capture stdout/stderr, and release retained jobs.

- [ ] **Task 6: Result And Status Persistence**
  - Store task status, approval outcome, command snapshot, output offsets, and final result in the app state.
  - Render status consistently in the task list, detail pane, and status bar.
  - Surface failures as visible result maps instead of silent UI no-ops.
  - Verification: replayed event sequence reaches success, rejection, and command-failure states.

- [ ] **Task 7: Packaging Smoke**
  - Package the app on Windows with `rco package --gui`.
  - Package on Linux with embedded WebView support and Linux package metadata.
  - Keep external-browser launch as diagnostic fallback only.
  - Verification: packaged app export smoke on Windows; WSL Linux package smoke for tar/deb metadata and embedded WebView build path.

- [ ] **Task 8: Windows/Linux Manual Comparison**
  - Manually compare Windows and Linux embedded WebView windows using the same example workspace.
  - Check native menus, scroll retention, command buttons, approval actions, and output pane behavior.
  - Record any visible parity gaps before public demo capture.
  - Verification: dated notes or screenshots attached to the MVP implementation branch.

- [ ] **Task 9: Public Demo Script**
  - Write a short script that shows workspace selection, task creation, approval, command execution, result review, and packaging.
  - Keep the demo offline and deterministic by default.
  - Include fallback narration for unavailable Linux graphical environments without framing Linux as secondary.
  - Verification: dry-run the script against the packaged Windows app and the Linux embedded WebView path.
