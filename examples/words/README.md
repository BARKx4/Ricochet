# Ricochet per-word examples

This directory contains one short `.rco` app for every case-sensitive entry in
the live `rco words --json` inventory. Numbered filenames keep symbolic words
and case-distinct words such as `get` and `GET` portable across filesystems;
`manifest.json` maps each exact word to its app.

Run the corpus from the repository root:

```powershell
cargo build -p ricochet_cli --bin rco
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-word-examples.ps1
```

The validator enforces exact one-to-one inventory coverage and compiles every
app. It also executes deterministic examples in bounded modes: sandboxed,
allowlisted environment, read-only filesystem, supplied stdin, short sleep,
and document-only WebView construction. Examples that require MVC/database
state, filesystem mutation, a loopback peer, upload state, an interactive TUI
or native dialog, or an operating-system process are compile-checked and carry
an explicit integration-evidence path in the manifest.

The numbered apps are generated. Update the reference example or the focused
override in `scripts/generate-word-examples.ps1`, rebuild `rco`, then regenerate:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\generate-word-examples.ps1
```

Support files beginning with `_` are small shared fixtures. The generator never
deletes stale numbered files; it fails loudly so removal remains an explicit
maintainer decision.
