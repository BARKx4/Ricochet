# Appendix A: Word Catalog

## Purpose

This appendix is the compact orientation layer for the live Ricochet word
inventory. The full source of truth is the CLI:

```powershell
cargo run -q -p ricochet_cli --bin rco -- words --json
```

Use the static reference site for full descriptions, examples, and search:

```text
docs/reference/index.html
```

## Live Group Summary

The current validated inventory contains 346 words:

| Detail group | Count | Start with |
| --- | ---: | --- |
| `stack` | 11 | Chapters 03 and 13 |
| `math` | 52 | Chapter 06 |
| `data` | 10 | Chapters 04 and 05 |
| `collection` | 28 | Chapter 08 |
| `string` | 26 | Chapter 07 |
| `oop` | 9 | Chapter 11 |
| `control` | 14 | Chapters 10 and 15 |
| `result` | 12 | Chapter 09 |
| `inspect` | 17 | Chapter 13 |
| `web` | 23 | Chapters 23 through 27 and 37 |
| `system` | 144 | Chapters 14 through 22 and 32 through 38 |

## How To Read A Word Entry

A reference word entry answers four questions:

- What stack shape does the word consume and produce?
- What value family does it belong to?
- Does it return a `Result` that must be checked or unwrapped?
- Which capability, if any, must be enabled by the host command?

Examples:

```ricochet
20 22 +
```

Consumes two numbers and leaves one number.

```ricochet
"settings.json" fs_read_text value
```

Reads through the filesystem capability and returns a `Result`; `value` unwraps
only after you have decided that failure should abort the current run.

```ricochet
$settings "theme" at
```

Keeps the container before the key.

## Alias Notes

Ricochet has symbolic words such as `+` and readable aliases such as `add`.
Prefer the shape that best teaches the surrounding code. In beginner examples,
readable aliases can help; in compact arithmetic, symbols are natural.

Case matters. `get` and `GET` are different public words because one is dynamic
data access and the other is an HTTP route verb.

## Verification

When public words, reference docs, or editor grammar change, run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- words --check --docs-app docs/reference/app.js --grammar editors/vscode/syntaxes/ricochet.tmLanguage.json
```

For Learn manual coverage, run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-learn-manual.ps1
```

## Maintenance

Do not hand-edit generated word inventories. Regenerate from
`rco words --json`, then validate with `words --check` and the Learn manual
validator.

Status: drafted from the live `rco words --json` inventory.
