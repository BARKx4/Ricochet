# Task 5B Report: Stabilize `rco expand --json` Contract And Macro Package Identity

- Status: `DONE`
- Commit hash(es) created before controller verification amend:
  `0e0e237bdab78a2929585cb4daefcb1a46c9ba26`. The final amended hash is
  intentionally recorded by the controller after this tracked report is
  committed, because embedding the containing commit hash would change it.

## Files changed

- `crates/ricochet_cli/src/lib.rs`
- `crates/ricochet_cli/tests/cli_smoke.rs`
- `crates/ricochet_compiler/src/compiler.rs`
- `crates/ricochet_compiler/src/imports.rs`
- `crates/ricochet_compiler/src/lib.rs`
- `docs/wiki/macros.md`
- `docs/reference/guides/macros.html`
- `docs/feature-map.md`
- `docs/superpowers/plans/2026-06-20-roadmap-completion.md`

## Schema fields added

- Top-level `schema`
- Top-level `sources`
- Top-level `source_map`
- Top-level `cache`
- Top-level `cache_hash`
- Import entries now include `kind`, `source_hash`, and `package`
- Macro-table and trace span objects now include `source_id`

## Tests run and exact results

- `rtk cargo test -p ricochet_cli expand`
  - Passed: `11 passed, 231 filtered out (5 suites, 0.14s)`
- `rtk cargo test -p ricochet_cli --test cli_smoke macro`
  - Passed: `12 passed, 206 filtered out (1 suite, 0.13s)`
- `rtk cargo test -p ricochet_compiler macro`
  - Passed: `26 passed, 26 filtered out (1 suite, 0.00s)`
- `rtk cargo fmt --all -- --check`
  - Passed
- `rtk git diff --check`
  - Passed
- `rtk powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1`
  - Passed: `Ricochet reference docs validation passed.`
- `rtk cargo run -p ricochet_cli --bin rco -- words --check --docs-app docs/reference/app.js --grammar editors/vscode/syntaxes/ricochet.tmLanguage.json`
  - Passed: `word inventory check passed: 333 documented words, 330 TextMate token literals, 346 built-in LSP entries (0 documented token words missing from the embedded LSP inventory, 0 duplicate reference entries)`

## Self-review notes and remaining risks

- The compiler/import layer now carries source hash, source kind, safe package metadata, and invocation source identity through macro table summaries and trace entries, so the CLI no longer has to infer imported-source ownership from file paths.
- Package macro module IDs now use dependency alias plus revision label when lock data is available, which keeps identities stable across temp directories and avoids leaking `.ricochet` cache paths.
- Cache metadata is intentionally additive: existing top-level fields remain, while `cache` and `cache_hash` provide the stable v1 contract for tooling.
- Remaining macro stabilization work is now public examples and broader package tests.
