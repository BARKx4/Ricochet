# Ricochet Tooling Bundle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the next practical v1 tooling layer: formatter, source imports, bytecode execution, and packaged executables.

**Architecture:** Keep the first pass in the CLI/tooling boundary. Reuse the existing parser/compiler/VM where possible, add a syntax formatter module, resolve static imports before compilation, load `.rcob` bytecode through the existing `Chunk` serializer, and package executable apps by appending bytecode to the current launcher.

**Tech Stack:** Rust workspace, Clap CLI, existing Ricochet syntax/compiler/bytecode/VM crates, Cargo integration tests.

---

### Task 1: Formatter CLI

**Files:**
- Create: `crates/ricochet_syntax/src/formatter.rs`
- Modify: `crates/ricochet_syntax/src/lib.rs`
- Modify: `crates/ricochet_cli/src/lib.rs`
- Test: `crates/ricochet_cli/tests/cli_smoke.rs`

- [ ] **Step 1: Write the failing CLI test**

```rust
#[test]
fn fmt_check_reports_unformatted_source() {
    let source_path = write_source("User   Model subclass\nemail field\nend\n");
    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("fmt")
        .arg("--check")
        .arg(&source_path)
        .output()
        .expect("rco fmt should launch");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("would reformat"));
}
```

- [ ] **Step 2: Run the test and verify it fails because `fmt` is missing**

Run: `cargo test -p ricochet_cli fmt_check_reports_unformatted_source`

- [ ] **Step 3: Add `format_source(source: &str) -> Result<String>`**

The formatter parses with `parse_module`, normalizes indentation for class/function/method/block bodies, keeps pure postfix token order intact, emits declaration lines as `<name> <operator>`, and always ends formatted output with one newline.

- [ ] **Step 4: Wire `rco fmt [--check] [path]`**

`--check` returns a non-zero status if the formatted text differs. Without `--check`, the command overwrites the file with formatted source.

- [ ] **Step 5: Verify focused and full CLI tests**

Run: `cargo test -p ricochet_cli fmt`

### Task 2: Static Source Imports

**Files:**
- Modify: `crates/ricochet_cli/src/lib.rs`
- Test: `crates/ricochet_cli/tests/cli_smoke.rs`

- [ ] **Step 1: Write the failing run test**

```rust
#[test]
fn run_loads_static_string_imports_before_main_source() {
    let main_path = write_named_source("main.rco", "\"lib/math\" import\ntriple");
    write_named_source("lib/math.rco", "\"triple\" function\n3 *\nend\n");
    let output = Command::new(env!("CARGO_BIN_EXE_rco"))
        .arg("run")
        .arg(&main_path)
        .output()
        .expect("rco run should launch");

    assert_run_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("Number(3)"));
}
```

- [ ] **Step 2: Run and verify failure from unsupported/unknown `import`**

Run: `cargo test -p ricochet_cli run_loads_static_string_imports_before_main_source`

- [ ] **Step 3: Add CLI source loading with recursive static imports**

Recognize lines shaped as `"path" import`, resolve relative to the importing file, add `.rco` when no extension is present, compile imports before the current file, deduplicate canonical paths, and fail loudly on cycles or missing files.

- [ ] **Step 4: Use import-aware compilation in `run`, `check`, `build`, and tests**

Compiled chunks are merged by running imported chunks first in the same VM or by emitting a combined root chunk for bytecode build.

### Task 3: Bytecode Run And Executable Packaging

**Files:**
- Modify: `crates/ricochet_cli/src/lib.rs`
- Test: `crates/ricochet_cli/tests/cli_smoke.rs`

- [ ] **Step 1: Write failing tests for `run-bytecode` and `package`**

`rco build main.rco`, `rco run-bytecode build/app.rcob`, and `rco package main.rco --output hello-app.exe` should all execute the same final stack.

- [ ] **Step 2: Implement shared VM execution helper**

Move source and bytecode execution through one helper that enables CLI capabilities, args, stdin, debug, breakpoints, stdout/stderr flushing, exit codes, and final stack printing.

- [ ] **Step 3: Add embedded-bytecode launcher support**

On startup, read `std::env::current_exe()`, detect a trailer marker plus length, decode the appended `Chunk`, and run it before Clap parsing.

- [ ] **Step 4: Add `rco package <path> --output <exe>`**

Compile import-aware source, copy the current executable to the output path, append the trailer marker and encoded chunk, and refuse to overwrite directories.

### Task 4: Reference Docs

**Files:**
- Modify: `README.md`
- Modify: `docs/reference/app.js`
- Modify: `docs/reference/index.html`

- [ ] **Step 1: Document the new CLI commands**

Add `rco fmt`, `rco run-bytecode`, and `rco package`.

- [ ] **Step 2: Document static imports**

Show `"lib/math" import` and explain that dynamic import syntax remains a future runtime/package-loader feature.

### Task 5: Verification

**Files:**
- No source files unless verification reveals a bug.

- [ ] **Step 1: Focused verification**

Run: `cargo test -p ricochet_cli fmt import bytecode package`

- [ ] **Step 2: Workspace gates**

Run: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `scripts/acceptance.ps1`.
