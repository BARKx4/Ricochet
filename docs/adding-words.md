# Adding Ricochet Words

Use this checklist when adding, renaming, or removing any public Ricochet word.
The goal is to keep the VM, CLI docs, LSP completions, editor grammar, examples,
and validation scripts in lockstep.

## Design Check

- Keep the API postfix/RPN: arguments are below the receiver or operation word.
- Use `_` for public multiword names, for example `timestamp_parse`.
- Do not use leading `.` syntax, fake namespace-dot APIs, or leading-dash words.
- Reserve `-` for subtraction and negative number literals.
- Decide whether failures are VM errors or Ricochet `Result` values:
  - Type/stack misuse should usually stay a VM error.
  - User data failures such as parse errors should usually return `Result`.
- Decide whether the word needs a host capability gate. If yes, update
  capability docs and `runtime_capabilities` output as needed.

## Required Code Updates

- Add a failing behavior test first.
  Good default locations are `crates/ricochet_cli/tests/cli_smoke.rs` for
  public language behavior and `crates/ricochet_vm/src/*` unit tests for
  focused runtime internals.
- Implement the VM behavior in `crates/ricochet_vm/src/builtins.rs` or the
  relevant runtime module.
- Add the dispatch arm in `crates/ricochet_vm/src/vm.rs`.
- If the word needs a new dependency, add it to workspace dependencies in
  `Cargo.toml` and to the consuming crate manifest.
- If the change affects syntax rather than an ordinary word, also update the
  lexer/parser/formatter/diagnostic tests and the design spec.
- If the change adds a runtime value kind, update display/JSON conversion,
  template rendering, web/controller bridges, database parameter and row
  conversions, `type`/`class_of`, and diagnostics that print `value_kind`.

## Required Documentation And Tooling Updates

- Add or update the reference catalog entry in `docs/reference/app.js`.
- Add the word to the required-word list in `docs/reference/validate.ps1`.
- Add at least one copyable example to `docs/reference/index.html` when the word
  introduces a new public workflow or concept.
- Add an LSP entry in `crates/ricochet_cli/src/lsp.rs`.
- Add the token to the VS Code grammar in
  `editors/vscode/syntaxes/ricochet.tmLanguage.json`.
- Update `README.md` when the word changes a headline workflow, security model,
  release promise, or common user-facing example.
- Update `docs/superpowers/specs/2026-06-09-ricochet-design.md` when the word
  changes or clarifies the language design contract.
- Update examples or package docs that should demonstrate the new word.

## Verification

From the repository root, run:

```powershell
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo test --workspace
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-editor-assets.ps1
rtk cargo run -p ricochet_cli --bin rco -- words --check --docs-app docs/reference/app.js --grammar editors/vscode/syntaxes/ricochet.tmLanguage.json
```

For release-facing changes, also run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1
```

## Common Drift Symptoms

- `rco words --check` reports a documented word missing from LSP or grammar:
  update `lsp.rs` or `ricochet.tmLanguage.json`.
- `docs/reference/validate.ps1` reports missing example text: update
  `docs/reference/index.html` or `docs/reference/app.js`.
- The editor validator fails after a word rename: update both the reference
  catalog and TextMate grammar.
- A CLI smoke test passes but acceptance fails: check static docs, examples,
  scaffold behavior, and live `rco serve` smoke coverage.
