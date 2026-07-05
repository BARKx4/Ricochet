# MVP Readiness Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden Ricochet's internal platform seams enough to start the cross-platform agent-app MVP without piling more product surface onto fragile gravity wells.

**Architecture:** This is a consolidation gate, not the MVP app itself. The work preserves public behavior while extracting load-bearing boundaries around VM/task runtime state, built-in word metadata, CLI command domains, web serving construction, and strictness diagnostics. The agent MVP begins only after the readiness gates pass.

**Tech Stack:** Rust workspace, Ricochet VM/compiler/CLI/web crates, Wry/Tao/Muda WebView desktop host, existing PowerShell validation scripts, WSL Ubuntu Linux GUI build dependencies.

## Global Constraints

- Always ask the user before deleting anything.
- Preserve existing public Ricochet words and command behavior unless a task explicitly adds a warning-only diagnostic.
- Keep new Ricochet public words RPN/postfix-friendly and use `_` for multiword names.
- Do not resurrect Slint, Avalonia, or WinUI3 experiments for the 1.0 UI path.
- Keep WebView as the primary desktop UI path for 1.0.
- Prefer behavior-preserving extraction over feature expansion.
- Each task should end in a commit.
- Before claiming readiness, run the full verification checklist in Task 8.

---

## Readiness Definition

Ricochet is ready to start the cross-platform agent-app MVP when all of these are true:

- VM task execution no longer depends on hand-copying a long list of duplicated fields between `Task` and `Vm`.
- New or changed built-in words have a structured metadata path that can feed docs/editor/validation instead of being copied manually across surfaces.
- WebView/package GUI behavior is no longer buried directly in the 12k-line CLI root module.
- The most dangerous dynamic-language convenience fallbacks have at least warning-only strict/lint coverage.
- The WebView desktop host has fresh Windows and Linux smoke evidence after the consolidation work.
- The agent-app MVP scope is frozen to mostly consume existing platform capabilities rather than opening another broad platform expansion wave.

---

## File Structure Target

### VM Runtime Boundary

- Create: `crates/ricochet_vm/src/runtime_state.rs`
  - Owns reusable runtime state containers shared by `Vm` and task execution.
  - Starts narrow: language/module/host/task snapshot structs only where they remove manual duplication.
- Modify: `crates/ricochet_vm/src/lib.rs`
  - Adds `mod runtime_state;` and re-exports only types that must be public.
- Modify: `crates/ricochet_vm/src/vm.rs`
  - Replaces manual task-to-VM field copying with a deliberate snapshot/shared-state object.
- Test: `crates/ricochet_vm/src/vm.rs`
  - Adds focused unit tests around task capability inheritance and shared registries.

### Built-In Word Registry

- Create: `crates/ricochet_vm/src/word_registry.rs`
  - Defines `WordMetadata`, `WordCategory`, `CapabilityRequirement`, and registry iteration helpers.
- Modify: `crates/ricochet_vm/src/lib.rs`
  - Exposes registry read APIs if CLI/docs validation need them.
- Modify: `crates/ricochet_vm/src/builtins.rs`
  - Starts registering selected built-in globals and method families from structured metadata.
- Modify: `crates/ricochet_cli/src/lib.rs` or extracted word command module from Task 3
  - Points `rco words --check` at registry metadata where possible.
- Test: `crates/ricochet_vm/src/builtins.rs`
- Test: `crates/ricochet_cli/tests/cli_smoke.rs`

### CLI Command Domains

- Create directory: `crates/ricochet_cli/src/commands/`
- Create: `crates/ricochet_cli/src/commands/mod.rs`
- Create: `crates/ricochet_cli/src/commands/gui.rs`
- Create: `crates/ricochet_cli/src/commands/package.rs`
- Create: `crates/ricochet_cli/src/commands/serve.rs`
- Optional create if touched: `crates/ricochet_cli/src/commands/registry.rs`
- Modify: `crates/ricochet_cli/src/lib.rs`
  - Keeps Clap command definitions and top-level dispatch, delegates behavior to modules.

### Web Serve Builder

- Create: `crates/ricochet_web/src/serve_builder.rs`
- Modify: `crates/ricochet_web/src/lib.rs`
- Modify: `crates/ricochet_web/src/server.rs`
  - Collapses the builder-function matrix behind `ServeBuilder`.
- Test: existing `crates/ricochet_web/src/server.rs` tests, plus focused builder tests.

### Strictness Diagnostics

- Create: `crates/ricochet_vm/src/strictness.rs`
- Modify: `crates/ricochet_vm/src/vm.rs`
- Modify: `crates/ricochet_cli/src/lib.rs` or extracted run/check command module
- Test: `crates/ricochet_vm/src/vm.rs`
- Test: `crates/ricochet_cli/tests/cli_smoke.rs`

### MVP Readiness Docs

- Modify: `docs/feature-map.md`
- Create or modify: `docs/superpowers/plans/2026-07-05-agent-app-mvp.md` after this plan passes.
- Optional modify: `docs/reference/guides/features.html` if public-facing language changes.

---

## Task 1: Baseline Readiness Audit

**Files:**
- Modify: `docs/superpowers/plans/2026-07-05-mvp-readiness-consolidation.md`

**Interfaces:**
- Consumes: current repo state on `codex/webview-desktop-rc`.
- Produces: baseline metrics and verification output attached to the execution notes.

- [ ] **Step 1: Confirm branch and status**

Run:

```powershell
rtk git status --short --branch
```

Expected:

```text
## codex/webview-desktop-rc
```

If there are user changes, stop and inspect them before editing.

- [ ] **Step 2: Record gravity-well file sizes**

Run:

```powershell
Get-ChildItem -LiteralPath crates\ricochet_vm\src\vm.rs,crates\ricochet_vm\src\builtins.rs,crates\ricochet_cli\src\lib.rs,crates\ricochet_web\src\server.rs |
  Select-Object FullName,@{Name='Lines';Expression={(Get-Content -LiteralPath $_.FullName | Measure-Object -Line).Lines}},Length
```

Expected: record current line counts in the task notes. As of planning, the approximate counts are:

```text
crates/ricochet_vm/src/vm.rs        6667
crates/ricochet_vm/src/builtins.rs  9199
crates/ricochet_cli/src/lib.rs     12228
crates/ricochet_web/src/server.rs   3308
```

- [ ] **Step 3: Run baseline focused verification**

Run:

```powershell
rtk cargo fmt --all -- --check
rtk cargo check -p ricochet_cli --bins
rtk cargo test -p ricochet_vm
rtk cargo test -p ricochet_cli --test cli_smoke gui -- --nocapture
```

Expected: all commands exit 0. If not, fix or document pre-existing failures before beginning extraction work.

- [ ] **Step 4: Commit only if the plan file changed**

Run:

```powershell
rtk git add -- docs/superpowers/plans/2026-07-05-mvp-readiness-consolidation.md
rtk git commit -m "Plan MVP readiness consolidation"
```

Expected: one docs-only plan commit.

---

## Task 2: Extract VM Runtime Snapshot Boundary

**Files:**
- Create: `crates/ricochet_vm/src/runtime_state.rs`
- Modify: `crates/ricochet_vm/src/lib.rs`
- Modify: `crates/ricochet_vm/src/vm.rs`
- Test: `crates/ricochet_vm/src/vm.rs`

**Interfaces:**
- Consumes: existing `Vm`, `Task`, `TaskState`, `RunningTaskShared`, registry fields, capability fields.
- Produces: a runtime-state snapshot/shared-state object used by task spawning and `run_task_to_completion`.

- [ ] **Step 1: Write failing tests for task inheritance**

Add tests in `crates/ricochet_vm/src/vm.rs` near existing task tests:

```rust
#[test]
fn spawned_task_inherits_runtime_capability_state() {
    let mut vm = Vm::default();
    vm.enable_filesystem();
    vm.enable_environment();
    vm.set_env_allowlist(["RICOCHET_TASK_TEST".to_string()]);
    vm.execute_source(
        r#"
        [
          runtime_capabilities
        ] task_spawn "capabilities" set
        "capabilities" get task_wait
        "#,
    )
    .expect("task should complete");

    let result = vm.pop().expect("task result should be on stack");
    let Value::Map(capabilities) = result else {
        panic!("expected runtime_capabilities map, got {result:?}");
    };
    assert_eq!(capabilities.get("filesystem.enabled"), Some(Value::Bool(true)));
    assert_eq!(capabilities.get("environment.enabled"), Some(Value::Bool(true)));
}

#[test]
fn spawned_task_shares_approval_registry_with_parent() {
    let mut vm = Vm::default();
    vm.execute_source(
        r#"
        {"kind" "review"} approval_create "created" set
        "created" get "id" at "approval_id" set
        "created" get "token" at "token" set
        [
          "approval_id" get "token" get approval_claim
        ] task_spawn task_wait
        "#,
    )
    .expect("task should claim approval");

    let result = vm.pop().expect("task result should be on stack");
    assert!(matches!(result, Value::Map(_)));
}
```

If exact helper names differ, adapt only to existing public VM APIs. The intent is not the literal map key spelling; the intent is to lock behavior around inherited capability state and shared registries before refactoring.

- [ ] **Step 2: Run tests to confirm current behavior**

Run:

```powershell
rtk cargo test -p ricochet_vm spawned_task_inherits_runtime_capability_state spawned_task_shares_approval_registry_with_parent -- --nocapture
```

Expected: tests either pass before refactor or expose a real inheritance bug. If a test fails because the test syntax is wrong, fix the test before editing runtime code.

- [ ] **Step 3: Create `runtime_state.rs` with narrow containers**

Create:

```rust
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{
    ApprovalRegistry, HttpStreamRegistry, ProcessRegistry, PtyRegistry, TcpListenerRegistry,
    TcpSocketRegistry, UploadStreamRegistry, WebSocketListenerRegistry, WebSocketRegistry,
};

#[derive(Clone)]
pub(crate) struct HostRuntimeState {
    pub(crate) filesystem_enabled: bool,
    pub(crate) filesystem_write_enabled: bool,
    pub(crate) http_enabled: bool,
    pub(crate) sockets_enabled: bool,
    pub(crate) process_enabled: bool,
    pub(crate) pty_enabled: bool,
    pub(crate) tui_enabled: bool,
    pub(crate) webview_enabled: bool,
    pub(crate) environment_enabled: bool,
    pub(crate) sleep_enabled: bool,
    pub(crate) fs_root: Option<PathBuf>,
    pub(crate) process_root: Option<PathBuf>,
    pub(crate) allowed_hosts: Option<Vec<String>>,
    pub(crate) env_allowlist: Option<Vec<String>>,
    pub(crate) sleep_limit: Option<Duration>,
}

#[derive(Clone)]
pub(crate) struct SharedRuntimeState {
    pub(crate) approvals: ApprovalRegistry,
    pub(crate) http_streams: HttpStreamRegistry,
    pub(crate) uploads: UploadStreamRegistry,
    pub(crate) tcp_sockets: Arc<Mutex<TcpSocketRegistry>>,
    pub(crate) tcp_listeners: Arc<Mutex<TcpListenerRegistry>>,
    pub(crate) websockets: Arc<Mutex<WebSocketRegistry>>,
    pub(crate) websocket_listeners: Arc<Mutex<WebSocketListenerRegistry>>,
    pub(crate) processes: Arc<Mutex<ProcessRegistry>>,
    pub(crate) ptys: Arc<Mutex<PtyRegistry>>,
}
```

Keep this initial module private to the VM crate. Do not over-design it into a public embedding API yet.

- [ ] **Step 4: Wire `mod runtime_state`**

Modify `crates/ricochet_vm/src/lib.rs`:

```rust
mod runtime_state;
```

Do not publicly re-export these types unless a compiler error proves another crate needs them.

- [ ] **Step 5: Add `Vm` conversion helpers**

In `crates/ricochet_vm/src/vm.rs`, add private methods:

```rust
impl Vm {
    fn host_runtime_state(&self) -> HostRuntimeState {
        HostRuntimeState {
            filesystem_enabled: self.filesystem_enabled,
            filesystem_write_enabled: self.filesystem_write_enabled,
            http_enabled: self.http_enabled,
            sockets_enabled: self.sockets_enabled,
            process_enabled: self.process_enabled,
            pty_enabled: self.pty_enabled,
            tui_enabled: self.tui_enabled,
            webview_enabled: self.webview_enabled,
            environment_enabled: self.environment_enabled,
            sleep_enabled: self.sleep_enabled,
            fs_root: self.fs_root.clone(),
            process_root: self.process_root.clone(),
            allowed_hosts: self.allowed_hosts.clone(),
            env_allowlist: self.env_allowlist.clone(),
            sleep_limit: self.sleep_limit,
        }
    }

    fn shared_runtime_state(&self) -> SharedRuntimeState {
        SharedRuntimeState {
            approvals: self.approvals.clone(),
            http_streams: self.http_streams.clone(),
            uploads: self.uploads.clone(),
            tcp_sockets: self.tcp_sockets.clone(),
            tcp_listeners: self.tcp_listeners.clone(),
            websockets: self.websockets.clone(),
            websocket_listeners: self.websocket_listeners.clone(),
            processes: self.processes.clone(),
            ptys: self.ptys.clone(),
        }
    }
}
```

Use actual field names from the current file. If a field does not exist or is named differently, update the helper to match the current struct.

- [ ] **Step 6: Replace task manual copying**

Change `Task` so it stores `HostRuntimeState` and `SharedRuntimeState` instead of duplicating each host/runtime field. Change task construction to call `self.host_runtime_state()` and `self.shared_runtime_state()`.

Change `run_task_to_completion` so it creates the task VM from those state objects, then restores only task-owned values such as stack, variables, functions, classes, dynamic modules, debug snapshot, and instruction budget.

- [ ] **Step 7: Verify**

Run:

```powershell
rtk cargo fmt --all -- --check
rtk cargo test -p ricochet_vm spawned_task_inherits_runtime_capability_state spawned_task_shares_approval_registry_with_parent -- --nocapture
rtk cargo test -p ricochet_vm
```

Expected: all pass.

- [ ] **Step 8: Commit**

Run:

```powershell
rtk git add -- crates/ricochet_vm/src/runtime_state.rs crates/ricochet_vm/src/lib.rs crates/ricochet_vm/src/vm.rs
rtk git commit -m "Refactor VM task runtime state snapshot"
```

---

## Task 3: Seed Built-In Word Registry

**Files:**
- Create: `crates/ricochet_vm/src/word_registry.rs`
- Modify: `crates/ricochet_vm/src/lib.rs`
- Modify: `crates/ricochet_vm/src/builtins.rs`
- Modify: `crates/ricochet_cli/src/lib.rs` or extracted word command module if Task 4 has already moved it
- Test: `crates/ricochet_vm/src/builtins.rs`
- Test: `crates/ricochet_cli/tests/cli_smoke.rs`

**Interfaces:**
- Consumes: current built-in word names and docs/editor validation surfaces.
- Produces: structured metadata for at least MVP-critical words and a path for all future words.

- [ ] **Step 1: Create registry types**

Create `crates/ricochet_vm/src/word_registry.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordCategory {
    Core,
    Collection,
    Result,
    Host,
    WebView,
    Agent,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityRequirement {
    None,
    Filesystem,
    FilesystemWrite,
    Http,
    Socket,
    Process,
    Pty,
    Tui,
    WebView,
    Environment,
    Sleep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordMetadata {
    pub name: &'static str,
    pub category: WordCategory,
    pub capability: CapabilityRequirement,
    pub stack: &'static str,
    pub summary: &'static str,
}

pub const BUILTIN_WORDS: &[WordMetadata] = &[
    WordMetadata {
        name: "runtime_capabilities",
        category: WordCategory::Host,
        capability: CapabilityRequirement::None,
        stack: "-- map",
        summary: "Returns the active host capability map.",
    },
    WordMetadata {
        name: "webview_window_app",
        category: WordCategory::WebView,
        capability: CapabilityRequirement::WebView,
        stack: "document -- result",
        summary: "Builds a desktop WebView app document with app-kit metadata.",
    },
    WordMetadata {
        name: "web_command",
        category: WordCategory::WebView,
        capability: CapabilityRequirement::WebView,
        stack: "id label -- command",
        summary: "Creates a WebView/native-menu command descriptor.",
    },
    WordMetadata {
        name: "approval_create",
        category: WordCategory::Agent,
        capability: CapabilityRequirement::None,
        stack: "operation options -- approval",
        summary: "Creates an exactly-once approval record and one-time claim token.",
    },
    WordMetadata {
        name: "process_start",
        category: WordCategory::Agent,
        capability: CapabilityRequirement::Process,
        stack: "command options -- process",
        summary: "Starts a retained host process under the active process policy.",
    },
];

pub fn builtin_words() -> &'static [WordMetadata] {
    BUILTIN_WORDS
}

pub fn builtin_word(name: &str) -> Option<&'static WordMetadata> {
    BUILTIN_WORDS.iter().find(|word| word.name == name)
}
```

This seed is intentionally incomplete. The rule is: any new public word added after this task must enter the registry in the same commit.

- [ ] **Step 2: Wire module exports**

Modify `crates/ricochet_vm/src/lib.rs`:

```rust
pub mod word_registry;
pub use word_registry::{builtin_word, builtin_words, CapabilityRequirement, WordCategory, WordMetadata};
```

- [ ] **Step 3: Add registry tests**

Add tests:

```rust
#[test]
fn seeded_word_registry_has_unique_names() {
    let mut names = std::collections::BTreeSet::new();
    for word in crate::word_registry::builtin_words() {
        assert!(names.insert(word.name), "duplicate word metadata for {}", word.name);
        assert!(!word.stack.trim().is_empty(), "{} missing stack signature", word.name);
        assert!(!word.summary.trim().is_empty(), "{} missing summary", word.name);
    }
}

#[test]
fn mvp_critical_words_are_registered() {
    for name in [
        "runtime_capabilities",
        "webview_window_app",
        "web_command",
        "approval_create",
        "process_start",
    ] {
        assert!(crate::word_registry::builtin_word(name).is_some(), "{name} is not registered");
    }
}
```

- [ ] **Step 4: Add CLI visibility**

Add a small internal helper used by `rco words --check`:

```rust
fn registered_builtin_word_names() -> BTreeSet<&'static str> {
    ricochet_vm::builtin_words()
        .iter()
        .map(|word| word.name)
        .collect()
}
```

Then assert the seed words exist in whatever word inventory validation currently compares against docs/editor assets.

- [ ] **Step 5: Verify**

Run:

```powershell
rtk cargo fmt --all -- --check
rtk cargo test -p ricochet_vm seeded_word_registry_has_unique_names mvp_critical_words_are_registered -- --nocapture
rtk cargo run -p ricochet_cli --bin rco -- words --check --docs-app docs/reference/app.js --grammar editors/vscode/syntaxes/ricochet.tmLanguage.json
```

Expected: all pass.

- [ ] **Step 6: Commit**

Run:

```powershell
rtk git add -- crates/ricochet_vm/src/word_registry.rs crates/ricochet_vm/src/lib.rs crates/ricochet_vm/src/builtins.rs crates/ricochet_cli/src/lib.rs crates/ricochet_cli/tests/cli_smoke.rs
rtk git commit -m "Seed built-in word metadata registry"
```

---

## Task 4: Extract Desktop GUI and Packaging CLI Modules

**Files:**
- Create: `crates/ricochet_cli/src/commands/mod.rs`
- Create: `crates/ricochet_cli/src/commands/gui.rs`
- Create: `crates/ricochet_cli/src/commands/package.rs`
- Modify: `crates/ricochet_cli/src/lib.rs`
- Test: `crates/ricochet_cli/tests/cli_smoke.rs`

**Interfaces:**
- Consumes: existing `run_gui_file`, `run_gui_chunk`, `WebviewSession`, native WebView helpers, `package`, Linux package artifact helpers.
- Produces: behavior-preserving command modules while leaving top-level command definitions in place.

- [ ] **Step 1: Move GUI runtime code without behavior changes**

Create `commands/gui.rs` and move these items from `lib.rs` into it:

```text
run_gui_file
run_gui_chunk
WebviewSession
open_native_webview
NativeGuiEvent
native menu helper functions
HTML export helpers used only by rco gui / rco-gui
```

Expose only:

```rust
pub(crate) fn run_gui_file(path: &str, args: Vec<String>, capabilities: CapabilityOptions) -> anyhow::Result<()>;
pub(crate) fn run_gui_chunk(
    chunk: ricochet_bytecode::Chunk,
    args: Vec<String>,
    capabilities: CapabilityOptions,
    dynamic_import_parent: Option<std::path::PathBuf>,
) -> anyhow::Result<()>;
```

Keep platform-specific `cfg` blocks with the moved functions.

- [ ] **Step 2: Move package code without behavior changes**

Create `commands/package.rs` and move:

```text
PackageOptions
package
package_launcher
append embedded payload helpers if they are package-only
LinuxPackageFormat helpers
create_linux_package_artifacts
linux_package_staging_root
tar/deb/appstream/desktop metadata helpers
```

Expose:

```rust
pub(crate) fn package(path: &str, output: &std::path::Path, options: PackageOptions<'_>) -> anyhow::Result<()>;
```

- [ ] **Step 3: Add module root**

Create `commands/mod.rs`:

```rust
pub(crate) mod gui;
pub(crate) mod package;
```

Modify `lib.rs`:

```rust
mod commands;
```

Update dispatch sites to call `commands::gui::run_gui_file`, `commands::gui::run_gui_chunk`, and `commands::package::package`.

- [ ] **Step 4: Verify focused GUI/package behavior**

Run:

```powershell
rtk cargo fmt --all -- --check
rtk cargo check -p ricochet_cli --bins
rtk cargo test -p ricochet_cli --test cli_smoke gui -- --nocapture
```

Expected: all pass.

- [ ] **Step 5: Verify Linux package build path in WSL**

Run:

```powershell
rtk wsl.exe -d Ubuntu --cd "/mnt/e/LLM Projects/Ricochet" -- bash -lc "source ~/.cargo/env; CARGO_TARGET_DIR=/tmp/ricochet-linux-target cargo test -p ricochet_cli --test cli_smoke gui -- --nocapture"
```

Expected: all Linux GUI/package smoke tests pass.

- [ ] **Step 6: Commit**

Run:

```powershell
rtk git add -- crates/ricochet_cli/src/lib.rs crates/ricochet_cli/src/commands
rtk git commit -m "Extract desktop GUI and package CLI modules"
```

---

## Task 5: Extract ServeBuilder for Web Runtime Construction

**Files:**
- Create: `crates/ricochet_web/src/serve_builder.rs`
- Modify: `crates/ricochet_web/src/lib.rs`
- Modify: `crates/ricochet_web/src/server.rs`
- Test: `crates/ricochet_web/src/server.rs`

**Interfaces:**
- Consumes: `ServeOptions`, runtime creation functions, watch/database/trace/fault-sink combinations.
- Produces: `ServeBuilder` that collapses public builder-function combinations behind one construction path.

- [ ] **Step 1: Add `ServeBuilder` shell**

Create:

```rust
use std::path::{Path, PathBuf};

use anyhow::Result;
use axum::Router;

use crate::server::{self, RequestFaultSink, ServeOptions, WatchTraceSink};

pub struct ServeBuilder {
    project_root: PathBuf,
    options: ServeOptions,
    trace_sink: Option<WatchTraceSink>,
    request_fault_sink: Option<RequestFaultSink>,
    watched: bool,
}

impl ServeBuilder {
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        Self {
            project_root: project_root.as_ref().to_path_buf(),
            options: ServeOptions::default(),
            trace_sink: None,
            request_fault_sink: None,
            watched: false,
        }
    }

    pub fn options(mut self, options: ServeOptions) -> Self {
        self.options = options;
        self
    }

    pub fn watched(mut self, watched: bool) -> Self {
        self.watched = watched;
        self
    }

    pub fn trace_sink(mut self, trace_sink: WatchTraceSink) -> Self {
        self.trace_sink = Some(trace_sink);
        self
    }

    pub fn request_fault_sink(mut self, sink: RequestFaultSink) -> Self {
        self.request_fault_sink = Some(sink);
        self
    }

    pub fn build(self) -> Result<Router> {
        server::build_app_from_serve_builder(self)
    }

    pub(crate) fn into_parts(
        self,
    ) -> (PathBuf, ServeOptions, bool, Option<WatchTraceSink>, Option<RequestFaultSink>) {
        (
            self.project_root,
            self.options,
            self.watched,
            self.trace_sink,
            self.request_fault_sink,
        )
    }
}
```

- [ ] **Step 2: Wire exports**

Modify `crates/ricochet_web/src/lib.rs`:

```rust
pub mod serve_builder;
pub use serve_builder::ServeBuilder;
```

- [ ] **Step 3: Add internal bridge**

In `server.rs`, add:

```rust
pub fn build_app_from_serve_builder(builder: crate::ServeBuilder) -> Result<Router> {
    let (project_root, options, watched, trace_sink, request_fault_sink) = builder.into_parts();
    if watched {
        if let Some(trace_sink) = trace_sink {
            return build_watched_app_from_dir_with_options_and_trace(
                &project_root,
                &options,
                trace_sink,
            );
        }
        return build_watched_app_from_dir_with_options_and_request_fault_sink(
            &project_root,
            &options,
            request_fault_sink,
        );
    }
    build_app_from_dir_internal_with_options_and_request_fault_sink(
        &project_root,
        Some(&options),
        request_fault_sink,
    )
}
```

Then gradually rewrite existing public helper functions to delegate to `ServeBuilder::new(project_root)`.

- [ ] **Step 4: Add focused builder tests**

Add tests:

```rust
#[test]
fn serve_builder_builds_static_router() {
    let root = write_minimal_project_fixture();
    let app = crate::ServeBuilder::new(root.path()).build();
    assert!(app.is_ok());
}

#[test]
fn serve_builder_accepts_watched_options() {
    let root = write_minimal_project_fixture();
    let app = crate::ServeBuilder::new(root.path())
        .watched(true)
        .options(ServeOptions {
            watch: true,
            ..ServeOptions::default()
        })
        .build();
    assert!(app.is_ok());
}
```

Use existing test fixture helpers in `server.rs` instead of creating duplicate fixture writers if such helpers already exist.

- [ ] **Step 5: Verify**

Run:

```powershell
rtk cargo fmt --all -- --check
rtk cargo test -p ricochet_web serve_builder -- --nocapture
rtk cargo test -p ricochet_web
rtk cargo test -p ricochet_cli --test cli_smoke serve -- --nocapture
```

Expected: all pass.

- [ ] **Step 6: Commit**

Run:

```powershell
rtk git add -- crates/ricochet_web/src/serve_builder.rs crates/ricochet_web/src/lib.rs crates/ricochet_web/src/server.rs
rtk git commit -m "Add ServeBuilder for web runtime construction"
```

---

## Task 6: Add Warning-Only Strictness Diagnostics

**Files:**
- Create: `crates/ricochet_vm/src/strictness.rs`
- Modify: `crates/ricochet_vm/src/lib.rs`
- Modify: `crates/ricochet_vm/src/vm.rs`
- Modify: `crates/ricochet_cli/src/lib.rs` or extracted check/run command module
- Test: `crates/ricochet_vm/src/vm.rs`
- Test: `crates/ricochet_cli/tests/cli_smoke.rs`

**Interfaces:**
- Consumes: current dynamic fallback behavior.
- Produces: warning-only diagnostics for strict project checks without changing default execution.

- [ ] **Step 1: Define strictness diagnostics**

Create:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrictnessDiagnosticKind {
    UnknownQuestionWordFallback,
    NilProducingLookup,
    MissingProductionSessionSecret,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrictnessDiagnostic {
    pub kind: StrictnessDiagnosticKind,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct StrictnessConfig {
    pub warn_unknown_question_word_fallback: bool,
    pub warn_nil_producing_lookup: bool,
}
```

- [ ] **Step 2: Wire VM diagnostic collection**

Add to `Vm`:

```rust
strictness: StrictnessConfig,
strictness_diagnostics: Vec<StrictnessDiagnostic>,
```

Add methods:

```rust
pub fn set_strictness(&mut self, strictness: StrictnessConfig) {
    self.strictness = strictness;
}

pub fn strictness_diagnostics(&self) -> &[StrictnessDiagnostic] {
    &self.strictness_diagnostics
}
```

- [ ] **Step 3: Warn on question-word fallback**

In `call_question_word`, when `call_function(word)` returns `UnknownWord` and execution falls back to `call_predicate(word)`, push:

```rust
StrictnessDiagnostic {
    kind: StrictnessDiagnosticKind::UnknownQuestionWordFallback,
    message: format!("{word} fell back to generic predicate dispatch"),
}
```

Only push when the config flag is enabled. Default behavior remains unchanged.

- [ ] **Step 4: Warn on nil-producing lookup**

At the narrowest lookup points for `at`, empty `first`/`last`, and missing fields, push `NilProducingLookup` diagnostics only when the config flag is enabled.

Do not change stack results in this task.

- [ ] **Step 5: Add CLI strict check flag**

Add a warning-only CLI flag to `check`:

```rust
#[arg(long = "strict", help = "Emit strictness warnings for dynamic convenience fallbacks")]
strict: bool,
```

When set, configure the VM with:

```rust
StrictnessConfig {
    warn_unknown_question_word_fallback: true,
    warn_nil_producing_lookup: true,
}
```

Print diagnostics to stderr and keep exit code 0 unless existing check errors occur.

- [ ] **Step 6: Add tests**

Add VM tests:

```rust
#[test]
fn strictness_warns_on_unknown_question_word_fallback() {
    let mut vm = Vm::default();
    vm.set_strictness(StrictnessConfig {
        warn_unknown_question_word_fallback: true,
        ..StrictnessConfig::default()
    });
    vm.execute_source("1 typo?").expect("fallback remains allowed");
    assert!(vm.strictness_diagnostics().iter().any(|diagnostic| {
        diagnostic.kind == StrictnessDiagnosticKind::UnknownQuestionWordFallback
    }));
}

#[test]
fn strictness_warns_on_nil_producing_lookup() {
    let mut vm = Vm::default();
    vm.set_strictness(StrictnessConfig {
        warn_nil_producing_lookup: true,
        ..StrictnessConfig::default()
    });
    vm.execute_source("[] first").expect("nil-producing lookup remains allowed");
    assert!(vm.strictness_diagnostics().iter().any(|diagnostic| {
        diagnostic.kind == StrictnessDiagnosticKind::NilProducingLookup
    }));
}
```

Add CLI smoke coverage for `rco check --strict`.

- [ ] **Step 7: Verify**

Run:

```powershell
rtk cargo fmt --all -- --check
rtk cargo test -p ricochet_vm strictness -- --nocapture
rtk cargo test -p ricochet_cli --test cli_smoke strict -- --nocapture
```

Expected: all pass.

- [ ] **Step 8: Commit**

Run:

```powershell
rtk git add -- crates/ricochet_vm/src/strictness.rs crates/ricochet_vm/src/lib.rs crates/ricochet_vm/src/vm.rs crates/ricochet_cli/src/lib.rs crates/ricochet_cli/tests/cli_smoke.rs
rtk git commit -m "Add warning-only strictness diagnostics"
```

---

## Task 7: Freeze Agent-App MVP Scope

**Files:**
- Modify: `docs/feature-map.md`
- Create: `docs/superpowers/plans/2026-07-05-agent-app-mvp.md`

**Interfaces:**
- Consumes: readiness work from Tasks 1-6.
- Produces: an MVP app plan constrained to consume existing platform capabilities.

- [ ] **Step 1: Add feature-map boundary**

Add a short section to `docs/feature-map.md` under Desktop WebView UI or Remaining Roadmap:

```markdown
## Agent App MVP Readiness Boundary

The first cross-platform agent-app MVP should primarily consume existing
Ricochet capabilities: Desktop WebView UI, approvals, process/process-root,
PTY, filesystem roots, environment allowlists, HTTP where explicitly enabled,
and package GUI output.

New words added for the MVP must be small, postfix-friendly, and registered in
the built-in word registry in the same commit. The MVP should not add another
native UI renderer, a broad plugin system, or a second desktop host stack.
```

- [ ] **Step 2: Create MVP plan stub with fixed scope**

Create `docs/superpowers/plans/2026-07-05-agent-app-mvp.md` with sections:

```markdown
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
```

Do not fill implementation tasks yet unless the user asks to move from readiness consolidation into MVP build planning.

- [ ] **Step 3: Verify docs/reference consistency if feature map is linked**

Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
```

Expected: pass.

- [ ] **Step 4: Commit**

Run:

```powershell
rtk git add -- docs/feature-map.md docs/superpowers/plans/2026-07-05-agent-app-mvp.md
rtk git commit -m "Document agent app MVP readiness boundary"
```

---

## Task 8: Cross-Platform Desktop Smoke Gate

**Files:**
- Modify only if tests reveal gaps:
  - `crates/ricochet_cli/tests/cli_smoke.rs`
  - `examples/webview_ui.rco`
  - `docs/feature-map.md`

**Interfaces:**
- Consumes: WebView desktop runtime after extraction.
- Produces: fresh MVP-readiness evidence for Windows and Linux.

- [ ] **Step 1: Run full Windows verification**

Run:

```powershell
rtk cargo fmt --all -- --check
rtk cargo check -p ricochet_cli --bins
rtk cargo check -p ricochet_vm
rtk cargo clippy -p ricochet_cli --all-targets -- -D warnings
rtk cargo test -p ricochet_vm
rtk cargo test -p ricochet_cli --test cli_smoke
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-editor-assets.ps1
rtk cargo run -p ricochet_cli --bin rco -- words --check --docs-app docs/reference/app.js --grammar editors/vscode/syntaxes/ricochet.tmLanguage.json
```

Expected: all pass.

- [ ] **Step 2: Run Linux WSL build and GUI/package smoke**

Run:

```powershell
rtk wsl.exe -d Ubuntu --cd "/mnt/e/LLM Projects/Ricochet" -- bash -lc "source ~/.cargo/env; CARGO_TARGET_DIR=/tmp/ricochet-linux-target cargo check -p ricochet_cli --bins --locked"
rtk wsl.exe -d Ubuntu --cd "/mnt/e/LLM Projects/Ricochet" -- bash -lc "source ~/.cargo/env; CARGO_TARGET_DIR=/tmp/ricochet-linux-target cargo test -p ricochet_cli --test cli_smoke gui -- --nocapture"
```

Expected: both pass.

- [ ] **Step 3: Export deterministic WebView HTML**

Run:

```powershell
$env:RICOCHET_GUI_EXPORT_HTML='target\webview-ui-smoke.html'
rtk cargo run -p ricochet_cli --bin rco -- gui examples/webview_ui.rco
Remove-Item Env:\RICOCHET_GUI_EXPORT_HTML
Select-String -LiteralPath target\webview-ui-smoke.html -Pattern 'Ricochet Desktop UI','data-rco-action','window.__ricochetApplyDocument'
```

Expected: all patterns found. Do not delete the generated target artifact unless the user approves cleanup.

- [ ] **Step 4: Manual desktop feel test**

Manual checklist:

```text
Windows:
- rco gui examples/webview_ui.rco opens a desktop WebView window.
- Buttons dispatch without visible refresh flash.
- Scroll position does not reset after interaction.
- Native menus open and dispatch.

Linux:
- On a graphical Linux desktop or WSLg-capable environment, rco gui examples/webview_ui.rco opens an embedded WebView window.
- It does not fall back to an external browser unless RICOCHET_GUI_EXTERNAL_BROWSER is explicitly set.
- Buttons dispatch without visible refresh flash.
- Native menus open and dispatch.
```

If Linux cannot be manually opened in the current environment, record the limitation and do not claim full interactive parity.

- [ ] **Step 5: Commit only if smoke tests required code/docs changes**

If changes were required:

```powershell
rtk git add -- <changed-files>
rtk git commit -m "Stabilize desktop smoke gate"
```

If no changes were required, do not make an empty commit.

---

## Task 9: Go/No-Go Review for Agent MVP

**Files:**
- Modify: `docs/superpowers/plans/2026-07-05-agent-app-mvp.md`

**Interfaces:**
- Consumes: completed readiness tasks and verification evidence.
- Produces: explicit go/no-go decision.

- [ ] **Step 1: Confirm no dirty worktree**

Run:

```powershell
rtk git status --short --branch
```

Expected: clean branch status.

- [ ] **Step 2: Fill MVP plan tasks only after readiness passes**

Update `docs/superpowers/plans/2026-07-05-agent-app-mvp.md` with implementation tasks for:

```text
App shell
Workspace selection
Agent task model
Approval workflow
Process/PTY command runner
Result/status persistence
Packaging smoke
Windows/Linux manual comparison
Public demo script
```

Every new public word proposed by that plan must be listed with:

```text
word name
stack effect
capability requirement
registry metadata entry
docs/editor validation update
test path
```

- [ ] **Step 3: Commit MVP plan**

Run:

```powershell
rtk git add -- docs/superpowers/plans/2026-07-05-agent-app-mvp.md
rtk git commit -m "Plan cross-platform agent app MVP"
```

- [ ] **Step 4: Decision**

The MVP can begin if:

```text
Task 2 passed: VM/task state boundary
Task 3 passed: word registry seed
Task 4 passed: GUI/package CLI extraction
Task 6 passed: strictness diagnostics
Task 8 passed: Windows + Linux build/package smoke
No unresolved high-severity test failure remains
```

If any item is incomplete, continue consolidation before starting the app.

---

## Verification Checklist Before Declaring MVP-Ready

Run:

```powershell
rtk cargo fmt --all -- --check
rtk cargo check -p ricochet_cli --bins
rtk cargo check -p ricochet_vm
rtk cargo clippy -p ricochet_cli --all-targets -- -D warnings
rtk cargo test -p ricochet_vm
rtk cargo test -p ricochet_web
rtk cargo test -p ricochet_cli --test cli_smoke
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-editor-assets.ps1
rtk cargo run -p ricochet_cli --bin rco -- words --check --docs-app docs/reference/app.js --grammar editors/vscode/syntaxes/ricochet.tmLanguage.json
rtk wsl.exe -d Ubuntu --cd "/mnt/e/LLM Projects/Ricochet" -- bash -lc "source ~/.cargo/env; CARGO_TARGET_DIR=/tmp/ricochet-linux-target cargo check -p ricochet_cli --bins --locked"
rtk wsl.exe -d Ubuntu --cd "/mnt/e/LLM Projects/Ricochet" -- bash -lc "source ~/.cargo/env; CARGO_TARGET_DIR=/tmp/ricochet-linux-target cargo test -p ricochet_cli --test cli_smoke gui -- --nocapture"
```

Expected: all commands exit 0.

---

## Self-Review

**Spec coverage:** This plan covers the repo-health report's highest-priority items before the agent MVP: VM/task state duplication, built-in registry, CLI gravity well, web builder matrix, strictness story, and cross-platform WebView smoke evidence.

**Intentional deferrals:** This plan does not complete a full migration of every built-in word into structured metadata. It creates the registry and makes it mandatory for new public surface. Full migration can happen incrementally after MVP readiness.

**Risk:** Task 2 can expose real task-state inheritance bugs. If it does, fix the bug before continuing; do not water down the tests.

**Execution recommendation:** Use subagent-driven execution for Tasks 2, 3, 4, and 5, because they touch separate gravity wells and are easier to review independently. Use inline execution for Task 8 so the same session can interpret environment-specific smoke behavior.
