# Ricochet Final Review Fix Report

## Status

Complete final-review fix wave for the atomic `workspace_write_text` branch. All five findings are addressed in one coherent change set. The full Task 5 release/acceptance gate was intentionally not run; the controller will rerun it after review.

## Findings and fixes

1. `workspace_copy` and `workspace_move` now use a parser limited to `overwrite` and `create_parent_dirs`. `expected_sha256` is therefore an unknown option and returns `WorkspaceRequestError`; only `workspace_write_text` accepts and validates it.
2. `WorkspaceWriteIo` now exposes a deterministic `before_persist` seam. After the optional final destination hash, the production path invokes that seam, re-inspects the destination, maps explicit unsafe types to `PermissionError`, maps ordinary inspection I/O to `IoError`, retains the exact staging path on failure, and calls persist immediately after the successful inspection.
3. `docs/reference/app.js`, both host-capability HTML copies, and Learn chapter 17 now state the exact `sha256:` plus 64 lowercase hexadecimal format, `WorkspaceRequestError` validation behavior, `std::fs::Permissions` preservation, and owner/ACL/xattr/alternate-data-stream/timestamp/hard-link exclusions. The established durability/coordination paragraph and app.js durability sentence remain verbatim.
4. The spawned-task registry test now runs an actual child `workspace_write_text`, observes its registry attempt, proves it is blocked by the parent's registry, releases it, and verifies the child result. The non-overwrite race now calls the production create-new result path. Both creator and same-precondition tests use readiness channels plus a three-party barrier; the former 500 ms rendezvous is gone.
5. Cleanup-disabled staging remains same-directory. On Unix, only an initially missing destination asks `tempfile::Builder` for mode `0666`, which the process umask filters exactly as it filters a normal `OpenOptions` create. Existing destinations continue to receive their saved `std::fs::Permissions` before sync/persist.

## TDD RED evidence

The first attempted copy/move RED command used an unqualified `--exact` filter and selected zero tests. It is not counted as evidence. The commands below use fully qualified names and executed the intended tests.

### Copy and move option scoping

- `rtk cargo test -p ricochet_vm builtins::tests::workspace_copy_rejects_expected_sha256_option -- --exact`
  - RED: 0 passed, 1 failed. The VM returned success instead of the asserted `WorkspaceRequestError`.
- `rtk cargo test -p ricochet_vm builtins::tests::workspace_move_rejects_expected_sha256_option -- --exact`
  - RED: 0 passed, 1 failed. The VM returned success instead of the asserted `WorkspaceRequestError`.
- `rtk cargo test -p ricochet_vm rejects_expected_sha256_option`
  - GREEN: 2 passed.

### Post-hash unsafe swap

- `rtk cargo test -p ricochet_vm builtins::tests::workspace_write_text_post_hash_unsafe_swap_retains_exact_staging -- --exact`
  - RED: compile error `E0407`, because the desired `before_persist` post-hash/pre-persist seam did not exist.
- Same command after implementation.
  - GREEN: 1 passed. A deterministic directory swap returns `PermissionError`; the retained path equals the captured staging path, still exists, and contains the attempted replacement bytes. The externally moved original bytes remain intact.

### Spawn propagation truthfulness

- The initial child bytecode test failed before reaching the registry with `StackUnderflow` because lower-case `map` interpreted the preceding string as a declaration. The test was corrected to construct the empty options map before pushing path/content; this harness failure is not counted as product RED evidence.
- With the corrected test, `rtk cargo test -p ricochet_vm vm::tests::workspace_write_registry_is_shared_with_spawned_tasks -- --exact` passed.
- Mutation RED: temporarily changed `run_task_to_completion` to install `WorkspaceWriteRegistry::default()` instead of the propagated registry, then ran the same test.
  - RED: the observer timed out while the child independently returned an OK write result, proving the test fails if propagation is removed.
- Restored the production propagation line and reran the same command.
  - GREEN: 1 passed. The child reaches the shared registry, cannot create the destination while the parent lock is held, then succeeds after release.

### Production create-new truthfulness

- Replaced the test-only duplicate file helper with a direct call to `workspace_write_text_result`, coordinated by two readiness signals and a three-party barrier.
- Mutation RED: temporarily replaced production `create_new(true)` with `create(true).truncate(true)` and ran `rtk cargo test -p ricochet_vm builtins::tests::workspace_write_text_non_overwrite_allows_exactly_one_concurrent_creator -- --exact`.
  - RED: both creators succeeded (`left: 2`, `right: 1`).
- Restored `create_new(true)` and reran the same command.
  - GREEN: 1 passed; exactly one OK result, one `AlreadyExists`, and complete final bytes from the winner.

### Deterministic same-precondition boundary

- Removed the `Condvar::wait_timeout(...500 ms...)` test IO helper.
- `rtk cargo test -p ricochet_vm builtins::tests::workspace_write_text_same_precondition_allows_exactly_one_concurrent_writer -- --exact`
  - GREEN: 1 passed. Both writers signal readiness and block at a three-party barrier immediately before the registry call; after release there is exactly one success and one `PreconditionFailed`.

### Unix missing-destination mode

- Ubuntu WSL toolchain: `cargo 1.96.1`, `rustc 1.96.1`.
- `env CARGO_TARGET_DIR=/tmp/ricochet-final-fix-target cargo test --manifest-path '/mnt/e/LLM Projects/Ricochet-atomic-workspace-replace/Cargo.toml' -p ricochet_vm builtins::tests::workspace_write_text_atomic_overwrite_missing_destination_uses_normal_creation_mode -- --exact`
  - RED: committed destination mode was decimal 384 (`0600`), while the normal same-process `OpenOptions` control was decimal 420 (`0644` under the active umask).
  - GREEN after Builder mode configuration: 1 passed; destination and control modes match.

## Final verification

All commands ran from `E:\LLM Projects\Ricochet-atomic-workspace-replace` unless the command explicitly invokes Ubuntu WSL.

- `rtk cargo test -p ricochet_vm` — exit 0; 178 passed across unit and doc-test suites.
- `rtk cargo test -p ricochet_vm workspace_write_text` — exit 0; 25 passed.
- `rtk cargo test -p ricochet_vm workspace_write_registry` — exit 0; 2 passed.
- `rtk cargo test -p ricochet_vm rejects_expected_sha256_option` — exit 0; 2 passed.
- `rtk cargo test -p ricochet_web workspace_write_registry` — exit 0; 2 passed.
- `rtk cargo test -p ricochet_cli --test cli_smoke run_workspace_` — exit 0; 4 passed.
- Unix WSL mode command above — exit 0; 1 passed.
- `rtk powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1` — exit 0; reference validation passed.
- `rtk powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-editor-assets.ps1` — exit 0; editor validation passed.
- `rtk powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-learn-manual.ps1 -RequireWordCoverage -RequireJekyllRawBlocks` — exit 0; 367 live words covered.
- `rtk cargo clippy -p ricochet_vm --all-targets -- -D warnings` — exit 0; no issues.
- `rtk cargo clippy --workspace --all-targets -- -D warnings` — exit 0; no issues.
- `rtk cargo fmt --all -- --check` — exit 0.
- `rtk git diff --check` — exit 0.
- Exact durability paragraph search — one unchanged match in each host-capability/Learn HTML surface; the existing app.js durability sentence also has one unchanged match.

## Files changed

- `crates/ricochet_vm/src/builtins.rs`
- `crates/ricochet_vm/src/vm.rs`
- `crates/ricochet_vm/src/workspace_runtime.rs`
- `docs/reference/app.js`
- `docs/reference/guides/host-capabilities.html`
- `docs/wiki/host-capabilities.html`
- `docs/learn/chapters/17-files-workspaces-env-and-secrets.html`
- `.superpowers/sdd/final-fix-report.md`

## Self-review

- Confirmed `workspace_write_text` still uses the extended parser and copy/move use the restricted parser.
- Confirmed the second unsafe-destination inspection is after the final precondition hash and directly before `persist`; hook/inspection I/O remains `IoError`, while explicit directory/nonregular/readonly/symlink/reparse classifications return `PermissionError` with retained staging.
- Confirmed cleanup remains disabled from staging creation and no retained staging or evidence artifact was deleted.
- Confirmed Unix `0666` is requested only for initially missing destinations; existing-file permission preservation remains unchanged.
- Confirmed task and writer tests use actual production paths and deterministic readiness/release coordination.
- Confirmed documentation names every required exclusion and preserves the exact existing durability wording.
- Reviewed the complete diff for unrelated changes; none found.

## Concerns

- No unresolved design concern.
- The full Task 5 release/acceptance/audit/release-build gate was intentionally not run per the controller instruction.

## Follow-up re-review: final ordering and deterministic contention

This section records the narrow follow-up after commit `6740907d9190923d5f64bde3c0803b30b161dd51`. It supersedes the earlier self-review statement about the immediate pre-persist ordering: the replacement-payload hash is now explicitly completed before the final destination check/hash, so no unbounded replacement-payload hashing remains after the final `expected_sha256` comparison.

### Replacement hash ordering RED/GREEN

- Added `workspace_write_text_hashes_replacement_before_final_destination_check` with a deterministic `after_payload_hash` observer whose state must already be visible at `before_final_check`.
- RED command: `rtk cargo test -p ricochet_vm builtins::tests::workspace_write_text_hashes_replacement_before_final_destination_check -- --exact`.
  - Result: compile error `E0407`; `after_payload_hash` was not a `WorkspaceWriteIo` member.
- GREEN after adding the no-op production seam and moving `sha256_after` ahead of `before_final_check` and the final destination hash.
  - Same command: exit 0; 1 passed.
- Preserved regression: `rtk cargo test -p ricochet_vm builtins::tests::workspace_write_text_post_hash_unsafe_swap_retains_exact_staging -- --exact` — exit 0; 1 passed.
- Final ordering is: stage/write/permissions/sync; replacement-payload hash; final-check seam/type inspection; optional final destination hash and comparison; immediate before-persist unsafe-swap seam/type inspection; persist. Taxonomy and exact staging retention remain unchanged.

### Same-precondition held-lock proof and mutation RED

- Replaced writer start readiness/barrier coordination with an externally held `WorkspaceWriteRegistry` lock.
- The holder enters first and waits. Both writer clones then emit `observe_synchronize_attempts` signals while the holder still owns the mutex. The test confirms the destination remains `initial`, releases the holder, and asserts one success plus one `PreconditionFailed`.
- `recv_timeout(Duration::from_secs(5))` is used only as a failure bound for a missing attempt signal; blocking is proven by holder ownership plus unchanged destination bytes, not elapsed time.
- Mutation RED: temporarily bypassed `registry.synchronize` inside `workspace_write_text_synchronized_result`, then ran `rtk cargo test -p ricochet_vm builtins::tests::workspace_write_text_same_precondition_allows_exactly_one_concurrent_writer -- --exact`.
  - Result: 0 passed, 1 failed at `first writer should attempt registry synchronization: Timeout`, proving the test fails when write-path synchronization/attempt observation is removed.
- Restored synchronization and reran the same command: exit 0; 1 passed.

### Follow-up verification

- `rtk cargo test -p ricochet_vm workspace_write_text` — exit 0; 26 passed.
- `rtk cargo test -p ricochet_vm workspace_write_registry` — exit 0; 2 passed.
- Both focused tests above plus the preserved unsafe-swap test — exit 0; 1 passed each.
- `rtk cargo clippy --workspace --all-targets -- -D warnings` — exit 0; no issues.
- `rtk cargo fmt --all -- --check` — exit 0.
- `rtk git diff --check` — exit 0.
- The full Task 5 release/acceptance gate was not run, as instructed.
