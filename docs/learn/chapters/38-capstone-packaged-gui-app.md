# Chapter 38: Capstone Packaged GUI App

> Part VI: Capstone Applications

## Why this chapter matters

The final capstone combines GUI state, local data, tests, and packaging into a desktop-style application you can hand to someone else.

## What you will build

You will build and package a personal ledger desktop GUI app. The app builds a
webview document from local ledger state, tests the ledger math and document
shape, then packages the GUI as a standalone launcher in an ignored local
`build/` directory.

## Concepts in plain English

This capstone proves a GUI app can be built, tested, and packaged as a user-facing artifact.

The chapter uses these concepts:

* Webview GUI or packaged local MVC.
* Local data, settings, result handling, release metadata, and packaging commands.
* Platform-specific validation notes.
* Escaped GUI fragments, explicit state, action callbacks, and packaging
  metadata in one end-to-end app.

## Vocabulary and commands

This chapter consolidates GUI, local data, results, MVC, and packaging concepts.

## Guided example

Open `examples/learn/38-capstone-gui/personal_ledger`:

```
lib/ledger.rco
ledger_gui.rco
LedgerTest.rco
run-package.ps1
.gitignore
```

The ledger logic creates local state:

```
( -> Map ) ledger_sample_state function
  entries array
  $entries "Client payment" "income" 2400 ledger_entry push! drop
  $entries "Office rent" "expense" 650 ledger_entry push! drop
  $entries "Tool <subscription>" "expense" 220 ledger_entry push! drop
  $entries "Coffee" "expense" 80 ledger_entry push! drop

  state map
  $state "entries" $entries put! drop
  $state "filter" "all" put! drop
  $state "reviewed" false put! drop
  $state
end
```

The balance function treats income and expense rows differently:

```
( entries -> Number ) ledger_balance function
  entries var
  0 balance var
  nil entry var

  [
    entry set
    $entry "kind" at "income" = if
      $balance $entry "amount" at + balance set
    else
      $balance $entry "amount" at - balance set
    end
  ] $entries each drop

  $balance
end
```

`ledger_build_document` turns state into a webview document map. It uses
webview helpers for user-facing text so labels such as `Tool <subscription>`
are escaped before they become HTML:

```
"Personal Ledger" 1 webview_heading heading var
"Visible entries: " $visibleEntries count to_string concat webview_text countLine var
"Balance: " $balance to_string concat webview_text balanceLine var
"filter" $filter webview_input filterInput var
"Mark reviewed" "review" webview_button reviewButton var
```

The callback updates state and returns a fresh document:

```
( state event -> Map ) mark_reviewed function
  event var
  state var
  $state "reviewed" true put! drop
  $state ledger_build_document
end
```

Run the non-interactive self-check:

```
rco run examples/learn/38-capstone-gui/personal_ledger/ledger_gui.rco
```

Expected output:

```
document title:Ricochet Ledger
entries:4
balance:1450
actions:1
reviewed:false
html has escaped item?true
[]
```

Preview it as a GUI document when you want a window:

```
rco gui examples/learn/38-capstone-gui/personal_ledger/ledger_gui.rco
```

Run the tests:

```
rco test examples/learn/38-capstone-gui/personal_ledger/LedgerTest.rco
```

Expected output:

```
PASS LedgerTest.testDocumentShape
PASS LedgerTest.testLedgerMath
PASS LedgerTest.testMarkReviewedAction
3 tests, 0 failed
```

Package the GUI app:

```
powershell -ExecutionPolicy Bypass -File examples/learn/38-capstone-gui/personal_ledger/run-package.ps1
```

Expected output:

```
packaged E:\LLM Projects\Ricochet\examples\learn\38-capstone-gui\personal_ledger\build\personal-ledger.exe
packaged artifact: personal-ledger.exe
artifact bytes: 32235143
```

The exact byte count can vary. The important part is that the package command
creates a nonzero executable under the example’s ignored `build/` directory.

## How to read the example

Read the capstone from the outside in. Start with the user command or app surface, identify the data boundary, follow the core transformation, and then check tests and packaging. The point is integration, not new syntax.

## Try it

Change the starting filter to `"expense"` in `ledger_sample_state`:

```
$state "filter" "expense" put! drop
```

Then update the tests so the visible entry count matches the new default. Run:

```
rco run examples/learn/38-capstone-gui/personal_ledger/ledger_gui.rco
rco test examples/learn/38-capstone-gui/personal_ledger/LedgerTest.rco
powershell -ExecutionPolicy Bypass -File examples/learn/38-capstone-gui/personal_ledger/run-package.ps1
```

Inspect the generated artifact:

```
Get-Item -LiteralPath examples/learn/38-capstone-gui/personal_ledger/build/personal-ledger.exe |
  Select-Object Name, Length, LastWriteTime
```

## Check your understanding

- What earlier chapters does this capstone combine?
- What is the user-facing entry point?
- What data boundary does the app use?
- Which tests or checks prove the capstone still works?

## Common mistakes

* Treating local GUI state as throwaway once packaging begins.
* Skipping artifact validation after packaging.
* Building HTML by string interpolation before escaping user-facing text.
* Forgetting that `rco gui` needs a document on the stack or in a `document`
  binding.
* Letting tests open a window. Use `rco run` and `rco test` for non-interactive checks;
  reserve `rco gui` for visual preview.
* Packaging before run/test/lint have passed.

## Safety notes

This capstone uses in-memory sample data and writes only the generated package
artifact under `examples/learn/38-capstone-gui/personal_ledger/build/`. That
directory is ignored because the executable is reproducible output. If you add
file-backed ledger storage, keep the path explicit, check containment, and
confirm before overwriting or deleting user data.

## Production guidance

Production desktop apps should document settings storage, state migration,
artifact manifests, signing or notarization status, and update policy. Use
`rco package --gui` for local packaging, then graduate to the release scripts
from Chapter 34 when you need cross-platform artifacts, checksums, signing
reports, store validation, and update-channel metadata.

## Reference links

* `docs/learn/chapters/22-webview-and-desktop-gui.md`
* `docs/learn/chapters/34-packaging-release-and-updates.md`
* `docs/learn/chapters/35-capstone-cli-tool.md`
* `docs/learn/chapters/36-capstone-tui-dashboard.md`
* `docs/learn/chapters/37-capstone-mvc-app.md`
* `docs/reference/guides/development-release.html`
* `docs/reference/guides/store-packaging.html`

## What you know now

You know the end-to-end shape of a packaged Ricochet desktop app: model local
state, build escaped webview HTML, expose actions, validate the document without
opening a window, test the business logic, package the GUI, and inspect the
generated artifact before sharing it.

## Next step

Return to the index and choose the next project path: CLI, TUI, MVC, packages, or GUI packaging.
