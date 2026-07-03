# Native App UI WinUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a native app UI stack for Ricochet with `@ricochet/ui` as the backend-neutral package and WinUI3 as the first supported native backend.

**Architecture:** Build the portable app model first as Ricochet package code: document maps, response maps, commands, event helpers, validation, and examples. Add a thin `@ricochet/winui` package for backend identity and scoped WinUI options. Then add CLI/launcher support for `rco app` and `rco package --app --backend winui`, using JSON export/event replay as the stable smoke-test boundary and a Windows WinUI host for live rendering.

**Tech Stack:** Ricochet first-party packages, Ricochet map/array/value APIs, `Result` values, existing `rco test/lint/verify/audit` package flows, `ricochet_cli` packaging markers, JSON serialization through `serde_json`, Windows App SDK/WinUI3 host process for the first live backend, and existing RTK-wrapped Cargo and PowerShell verification commands.

## Global Constraints

- Keep public Ricochet syntax postfix/RPN and follow `AGENTS.md`: receivers before selectors, args below receivers, `_` for multiword public words, no leading-dot syntax, no fake namespace-dot host APIs, and no dash-prefixed public words.
- Ask the user before deleting anything. This plan creates and modifies files only.
- `@ricochet/ui` is the public design center. WinUI-specific behavior stays in `@ricochet/winui`, CLI backend selection, or scoped `native_options` maps.
- Ordinary Ricochet app code must not receive raw WinUI object handles in v1.
- V1 includes tree views, drag/drop, data-grid basic contract, and rich-text basic contract.
- V1 defers spreadsheet-grade grids, full word processing, custom drawing/canvas, multi-window docking, tray icons, system notifications, mobile-specific controls, and retained native handles.
- CLI smoke paths must work without opening a visible native window by exporting UI JSON and replaying event fixtures.
- Live WinUI support may be Windows-only, but unsupported platforms must fail loudly when `--backend winui` is requested for live rendering or packaging.
- New Markdown source docs are ignored by default in this repo; force-add approved plan/spec Markdown files when committing them.

---

## Target File Structure

Create or modify these paths:

```text
packages/
  README.md
  ricochet_ui/
    ricochet.toml
    README.md
    document.rco
    validation.rco
    events.rco
    commands.rco
    rich_text.rco
    examples/
      counter_app.rco
      project_tree_drag_drop.rco
      data_grid_viewer.rco
      rich_text_note.rco
    tests/
      UiDocumentPackageTest.rco
      UiValidationPackageTest.rco
      UiInteractionPackageTest.rco
  ricochet_winui/
    ricochet.toml
    README.md
    backend.rco
    tests/
      WinuiBackendPackageTest.rco

crates/
  ricochet_cli/
    Cargo.toml
    src/
      bin/
        rco_app.rs
      lib.rs
    tests/
      cli_smoke.rs

hosts/
  winui/
    Ricochet.WinUI.Host/
      Ricochet.WinUI.Host.csproj
      Program.cs
      App.xaml
      App.xaml.cs
      MainWindow.xaml
      MainWindow.xaml.cs
      UiDocument.cs
      UiRenderer.cs

docs/
  feature-map.md
  reference/
    app.js
```

The package split is:

- `document.rco`: portable node/document builders.
- `validation.rco`: portable node, document, response, grid, rich-text, and drag/drop validation.
- `events.rco`: event map builders and predicates.
- `commands.rco`: command map builders.
- `rich_text.rco`: rich text document/span/list helpers.
- `backend.rco`: WinUI backend descriptor and scoped native option helpers.
- `ricochet_cli/src/lib.rs`: `rco app`, package `--app --backend winui`, JSON export/replay, embedded app payloads.
- `rco_app.rs`: launcher entrypoint for packaged native app payloads.
- `hosts/winui`: live Windows host used by `rco app --backend winui` and packaged app launch.

---

## Public API Contract

The portable package exposes these v1 words:

```text
ui_node                 ( id type props children events nativeOptions -> Map )
ui_window               ( id title children -> Result<Map> )
ui_response             ( state document commands -> Result<Map> )
ui_native_options       ( node backend options required -> Map )

ui_text                 ( id text -> Map )
ui_heading              ( id text level -> Map )
ui_button               ( id label -> Map )
ui_text_input           ( id label value -> Map )
ui_multiline_text_input ( id label value -> Map )
ui_checkbox             ( id label checked -> Map )
ui_toggle               ( id label checked -> Map )
ui_select               ( id label value options -> Map )

ui_stack                ( id orientation children -> Map )
ui_grid                 ( id columns rows children -> Map )
ui_split_pane           ( id orientation first second -> Map )
ui_scroll_view          ( id child -> Map )
ui_group                ( id title children -> Map )
ui_spacer               ( id -> Map )

ui_list                 ( id items selectedIds -> Map )
ui_tree                 ( id nodes expandedIds selectedIds -> Map )
ui_tree_node            ( id label children -> Map )
ui_data_grid            ( id columns rows selectedRowIds -> Map )
ui_grid_column          ( id title kind width -> Map )
ui_grid_row             ( id cells -> Map )

ui_rich_document        ( blocks -> Map )
ui_rich_paragraph       ( spans -> Map )
ui_rich_span            ( text marks href -> Map )
ui_rich_text            ( id document -> Map )
ui_rich_text_input      ( id label document -> Map )

ui_menu_bar             ( id items -> Map )
ui_command_bar          ( id items -> Map )
ui_context_menu         ( id items -> Map )
ui_command_item         ( id label shortcut -> Map )

ui_event                ( type id value -> Map )
ui_click_event?         ( event -> Bool )
ui_change_event?        ( event -> Bool )
ui_drop_event?          ( event -> Bool )

ui_focus                ( target -> Map )
ui_open_dialog          ( target -> Map )
ui_close_dialog         ( target -> Map )
ui_show_message         ( title message -> Map )
ui_open_file_picker     ( id title -> Map )
ui_save_file_picker     ( id title -> Map )
ui_open_folder_picker   ( id title -> Map )
ui_clipboard_write      ( text -> Map )
ui_scroll_into_view     ( target -> Map )
ui_set_window_state     ( state -> Map )
ui_close_window         ( -> Map )

ui_drag_payload         ( kind value -> Map )
ui_drag_source          ( node payload operations -> Map )
ui_drop_target          ( node accepts operations -> Map )
```

The WinUI package exposes:

```text
winui_backend           ( -> Map )
winui_option            ( key value -> Map )
winui_required_options  ( options -> Map )
winui_advisory_options  ( options -> Map )
```

---

### Task 1: Portable UI Package Scaffold And Core Builders

**Files:**
- Create: `packages/ricochet_ui/ricochet.toml`
- Create: `packages/ricochet_ui/document.rco`
- Create: `packages/ricochet_ui/tests/UiDocumentPackageTest.rco`
- Modify: `packages/README.md`

**Interfaces:**
- Produces: `ui_node`, `ui_window`, `ui_response`, `ui_native_options`, text/input/layout/list builders.
- Consumes: only core Ricochet map/array/result words.

- [ ] **Step 1: Write the package manifest**

Create `packages/ricochet_ui/ricochet.toml`:

```toml
[package]
name = "@ricochet/ui"
version = "0.1.0"
description = "Backend-neutral native app UI document, event, and command helpers for Ricochet."
license = "GPL-3.0"
```

- [ ] **Step 2: Write failing tests for core node builders**

Create `packages/ricochet_ui/tests/UiDocumentPackageTest.rco` with tests that assert:

```ricochet
"../document" import

UiDocumentPackageTest TestCase Subclass
  [
    props map
    children array
    events array
    native map
    "save" "button" $props $children $events $native ui_node node var
    $node "schema_version" at 1 assert_equals
    $node "id" at "save" assert_equals
    $node "type" at "button" assert_equals
  ] "testNodeShape" Method
  [
    children array
    $children "title" "Hello" ui_text push drop
    "main" "Demo" $children ui_window result var
    $result ok? true assert_equals
    $result value "type" at "window" assert_equals
    $result value "props" at "title" at "Demo" assert_equals
  ] "testWindowResult" Method
  [
    "save" "Save" ui_button button var
    $button "events" at "click" has? true assert_equals
    $button "props" at "label" at "Save" assert_equals
  ] "testButtonRegistersClickEvent" Method
  [
    state map
    document map
    commands array
    $state $document $commands ui_response result var
    $result ok? true assert_equals
    $result value "state" at $state assert_equals
    $result value "commands" at count 0 assert_equals
  ] "testResponseShape" Method
end
```

- [ ] **Step 3: Run the failing package test**

Run: `rtk cargo run -q -p ricochet_cli --bin rco -- test packages/ricochet_ui`

Expected: FAIL because `../document` does not exist or `ui_node` is unknown.

- [ ] **Step 4: Implement `document.rco` core helpers**

Create `packages/ricochet_ui/document.rco` with:

```ricochet
(( Backend-neutral native app UI document builders. ))
( id type props children events nativeOptions -> Map ) ui_node function
  nativeOptions var
  events var
  children var
  props var
  type var
  id var
  node map
  $node "schema_version" 1 put drop
  $node "id" $id put drop
  $node "type" $type put drop
  $node "props" $props put drop
  $node "children" $children put drop
  $node "events" $events put drop
  $node "native_options" $nativeOptions put drop
  $node
end

( id text -> Map ) ui_text function
  text var
  id var
  props map
  $props "text" $text put drop
  children array
  events array
  native map
  $id "text" $props $children $events $native ui_node
end

( id text level -> Map ) ui_heading function
  level var
  text var
  id var
  props map
  $props "text" $text put drop
  $props "level" $level put drop
  children array
  events array
  native map
  $id "heading" $props $children $events $native ui_node
end

( id label -> Map ) ui_button function
  label var
  id var
  props map
  $props "label" $label put drop
  children array
  events array
  $events "click" push drop
  native map
  $id "button" $props $children $events $native ui_node
end

( id label value -> Map ) ui_text_input function
  value var
  label var
  id var
  props map
  $props "label" $label put drop
  $props "value" $value put drop
  children array
  events array
  $events "change" push drop
  $events "submit" push drop
  native map
  $id "text_input" $props $children $events $native ui_node
end

( id label value -> Map ) ui_multiline_text_input function
  value var
  label var
  id var
  props map
  $props "label" $label put drop
  $props "value" $value put drop
  children array
  events array
  $events "change" push drop
  $events "submit" push drop
  native map
  $id "multiline_text_input" $props $children $events $native ui_node
end

( id label checked -> Map ) ui_checkbox function
  checked var
  label var
  id var
  props map
  $props "label" $label put drop
  $props "checked" $checked put drop
  children array
  events array
  $events "change" push drop
  native map
  $id "checkbox" $props $children $events $native ui_node
end

( id label checked -> Map ) ui_toggle function
  checked var
  label var
  id var
  props map
  $props "label" $label put drop
  $props "checked" $checked put drop
  children array
  events array
  $events "change" push drop
  native map
  $id "toggle" $props $children $events $native ui_node
end

( id label value options -> Map ) ui_select function
  options var
  value var
  label var
  id var
  props map
  $props "label" $label put drop
  $props "value" $value put drop
  $props "options" $options put drop
  children array
  events array
  $events "change" push drop
  native map
  $id "select" $props $children $events $native ui_node
end

( id orientation children -> Map ) ui_stack function
  children var
  orientation var
  id var
  props map
  $props "orientation" $orientation put drop
  events array
  native map
  $id "stack" $props $children $events $native ui_node
end

( id columns rows children -> Map ) ui_grid function
  children var
  rows var
  columns var
  id var
  props map
  $props "columns" $columns put drop
  $props "rows" $rows put drop
  events array
  native map
  $id "grid" $props $children $events $native ui_node
end

( id title children -> Result ) ui_window function
  children var
  title var
  id var
  props map
  $props "title" $title put drop
  events array
  $events "close" push drop
  native map
  $id "window" $props $children $events $native ui_node ok
end

( state document commands -> Result ) ui_response function
  commands var
  document var
  state var
  response map
  $response "schema_version" 1 put drop
  $response "state" $state put drop
  $response "document" $document put drop
  $response "commands" $commands put drop
  diagnostics array
  $response "diagnostics" $diagnostics put drop
  $response ok
end

( node backend options required -> Map ) ui_native_options function
  required var
  options var
  backend var
  node var
  nativeOptions $node "native_options" at var
  backendOptions map
  $backendOptions "required" $required put drop
  $backendOptions "options" $options put drop
  $nativeOptions $backend $backendOptions put drop
  $node "native_options" $nativeOptions put drop
  $node
end
```

- [ ] **Step 5: Add the package to the first-party catalog**

Modify `packages/README.md` to add `@ricochet/ui` to the list of first-party packages and to the local publish command list.

- [ ] **Step 6: Verify and commit Task 1**

Run:

```powershell
rtk cargo run -q -p ricochet_cli --bin rco -- test packages/ricochet_ui
rtk cargo run -q -p ricochet_cli --bin rco -- lint packages/ricochet_ui
rtk cargo run -q -p ricochet_cli --bin rco -- verify packages/ricochet_ui
rtk git diff --check
```

Expected: all commands pass.

Commit:

```powershell
rtk proxy git add packages/ricochet_ui packages/README.md
rtk proxy git commit -m "feat: add backend-neutral UI package core"
```

---

### Task 2: Portable Validation, Rich Text, Data Grid, Tree, And Drag/Drop

**Files:**
- Modify: `packages/ricochet_ui/document.rco`
- Create: `packages/ricochet_ui/validation.rco`
- Create: `packages/ricochet_ui/rich_text.rco`
- Create: `packages/ricochet_ui/events.rco`
- Create: `packages/ricochet_ui/commands.rco`
- Create: `packages/ricochet_ui/tests/UiValidationPackageTest.rco`
- Create: `packages/ricochet_ui/tests/UiInteractionPackageTest.rco`

**Interfaces:**
- Consumes: `ui_node` from Task 1.
- Produces: validation words, rich text builders, data-grid builders, tree builders, drag/drop wrappers, commands, and event helpers used by examples and CLI smoke fixtures.

- [ ] **Step 1: Add tests for validation and advanced control contracts**

Create tests that import all new modules and verify these exact facts:

```ricochet
"../document" import
"../validation" import
"../rich_text" import
"../events" import
"../commands" import

UiValidationPackageTest TestCase Subclass
  [
    children array
    $children "name" "Name" "" ui_text_input push drop
    "main" "Demo" $children ui_window value document var
    $document ui_validate_document result var
    $result "ok" at true assert_equals
  ] "testValidWindowDocument" Method
  [
    node map
    $node "type" "button" put drop
    $node ui_validate_node result var
    $result "ok" at false assert_equals
    $result "errors" at first "message" at "node id is required" assert_equals
  ] "testValidationRequiresNodeId" Method
  [
    "name" "Name" "string" "2*" ui_grid_column column var
    $column "kind" at "string" assert_equals
    cells map
    $cells "name" "Ada" put drop
    "row-1" $cells ui_grid_row row var
    rows array
    $rows $row push drop
    columns array
    $columns $column push drop
    selected array
    "users" $columns $rows $selected ui_data_grid grid var
    $grid "type" at "data_grid" assert_equals
  ] "testDataGridShape" Method
end

UiInteractionPackageTest TestCase Subclass
  [
    spans array
    $spans "Hello " array nil ui_rich_span push drop
    marks array
    $marks "bold" push drop
    $spans "native UI" $marks nil ui_rich_span push drop
    blocks array
    $blocks $spans ui_rich_paragraph push drop
    $blocks ui_rich_document doc var
    $doc "blocks" at count 1 assert_equals
    "note" $doc ui_rich_text "type" at "rich_text" assert_equals
  ] "testRichTextDocumentShape" Method
  [
    payload "tree_nodes" "node-1" ui_drag_payload var
    node "file-1" "File" array ui_tree_node var
    ops array
    $ops "move" push drop
    $node $payload $ops ui_drag_source source var
    $source "props" at "drag" at "payload" at "kind" at "tree_nodes" assert_equals
  ] "testDragSourceShape" Method
  [
    "click" "save" nil ui_event event var
    $event ui_click_event? true assert_equals
    "name" ui_focus "type" at "focus" assert_equals
  ] "testEventsAndCommands" Method
end
```

- [ ] **Step 2: Implement validation words**

Create `validation.rco` with these functions:

```ricochet
( -> Map ) ui_validation function
  result map
  errors array
  $result "ok" true put drop
  $result "errors" $errors put drop
  $result
end

( validation id message -> Map ) ui_add_validation_error function
  message var
  id var
  validation var
  error map
  $error "id" $id put drop
  $error "message" $message put drop
  $validation "errors" at $error push drop
  $validation "ok" false put drop
  $validation
end

( node -> Map ) ui_validate_node function
  node var
  ui_validation result var
  $node "id" at nil? if
    $result "" "node id is required" ui_add_validation_error result set
  end
  $node "type" at nil? if
    $result $node "id" at "node type is required" ui_add_validation_error result set
  end
  $node "props" at nil? if
    $result $node "id" at "node props map is required" ui_add_validation_error result set
  end
  $result
end

( document -> Map ) ui_validate_document function
  document var
  $document ui_validate_node result var
  $document "type" at "window" = false = if
    $result $document "id" at "document root must be window" ui_add_validation_error result set
  end
  $result
end

( response -> Map ) ui_validate_response function
  response var
  ui_validation result var
  $response "state" at nil? if
    $result "response" "response state is required" ui_add_validation_error result set
  end
  $response "document" at nil? if
    $result "response" "response document is required" ui_add_validation_error result set
  end
  $response "commands" at nil? if
    $result "response" "response commands are required" ui_add_validation_error result set
  end
  $result
end
```

- [ ] **Step 3: Implement rich text helpers**

Create `rich_text.rco` with `ui_rich_span`, `ui_rich_paragraph`, `ui_rich_document`, `ui_rich_text`, and `ui_rich_text_input`. Import `document.rco` and use `ui_node` for renderable controls.

- [ ] **Step 4: Extend `document.rco` for tree and data-grid helpers**

Add `ui_tree_node`, `ui_tree`, `ui_grid_column`, `ui_grid_row`, `ui_data_grid`, `ui_list`, `ui_menu_bar`, `ui_command_bar`, `ui_context_menu`, and `ui_command_item`.

- [ ] **Step 5: Implement event and command helpers**

Create `events.rco` with `ui_event`, `ui_click_event?`, `ui_change_event?`, and `ui_drop_event?`.

Create `commands.rco` with command map helpers listed in the public API contract. Each command map must include `schema_version`, `type`, and `required`.

- [ ] **Step 6: Implement drag/drop wrappers**

Add `ui_drag_payload`, `ui_drag_source`, and `ui_drop_target` to `document.rco`. Drag metadata should live under `props.drag` or `props.drop`, not under backend-specific native options.

- [ ] **Step 7: Verify and commit Task 2**

Run:

```powershell
rtk cargo run -q -p ricochet_cli --bin rco -- test packages/ricochet_ui
rtk cargo run -q -p ricochet_cli --bin rco -- lint packages/ricochet_ui
rtk cargo run -q -p ricochet_cli --bin rco -- verify packages/ricochet_ui
rtk git diff --check
```

Expected: all commands pass.

Commit:

```powershell
rtk proxy git add packages/ricochet_ui
rtk proxy git commit -m "feat: expand portable UI document contracts"
```

---

### Task 3: WinUI Backend Package

**Files:**
- Create: `packages/ricochet_winui/ricochet.toml`
- Create: `packages/ricochet_winui/backend.rco`
- Create: `packages/ricochet_winui/README.md`
- Create: `packages/ricochet_winui/tests/WinuiBackendPackageTest.rco`
- Modify: `packages/README.md`

**Interfaces:**
- Consumes: `ui_native_options` from `@ricochet/ui`.
- Produces: `winui_backend`, `winui_option`, `winui_required_options`, `winui_advisory_options`.

- [ ] **Step 1: Create package manifest and README**

Create `ricochet.toml` for `@ricochet/winui` and a README explaining that WinUI is the first native renderer for `@ricochet/ui`, not the public UI model.

- [ ] **Step 2: Write package tests**

Create `WinuiBackendPackageTest.rco`:

```ricochet
"../backend" import

WinuiBackendPackageTest TestCase Subclass
  [
    winui_backend backend var
    $backend "id" at "winui" assert_equals
    $backend "platform" at "windows" assert_equals
    $backend "native" at true assert_equals
  ] "testBackendDescriptor" Method
  [
    options map
    $options "style" "AccentButtonStyle" put drop
    $options winui_required_options wrapped var
    $wrapped "backend" at "winui" assert_equals
    $wrapped "required" at true assert_equals
    $wrapped "options" at "style" at "AccentButtonStyle" assert_equals
  ] "testRequiredOptions" Method
end
```

- [ ] **Step 3: Implement backend helpers**

Create `backend.rco`:

```ricochet
(( WinUI backend descriptor and native option helpers. ))
( -> Map ) winui_backend function
  backend map
  $backend "id" "winui" put drop
  $backend "name" "WinUI" put drop
  $backend "platform" "windows" put drop
  $backend "native" true put drop
  $backend
end

( key value -> Map ) winui_option function
  value var
  key var
  option map
  $option $key $value put drop
  $option
end

( options -> Map ) winui_required_options function
  options var
  wrapped map
  $wrapped "backend" "winui" put drop
  $wrapped "required" true put drop
  $wrapped "options" $options put drop
  $wrapped
end

( options -> Map ) winui_advisory_options function
  options var
  wrapped map
  $wrapped "backend" "winui" put drop
  $wrapped "required" false put drop
  $wrapped "options" $options put drop
  $wrapped
end
```

- [ ] **Step 4: Update package catalog**

Add `@ricochet/winui` to `packages/README.md` and publish command examples.

- [ ] **Step 5: Verify and commit Task 3**

Run:

```powershell
rtk cargo run -q -p ricochet_cli --bin rco -- test packages/ricochet_winui
rtk cargo run -q -p ricochet_cli --bin rco -- lint packages/ricochet_winui
rtk cargo run -q -p ricochet_cli --bin rco -- verify packages/ricochet_winui
rtk git diff --check
```

Commit:

```powershell
rtk proxy git add packages/ricochet_winui packages/README.md
rtk proxy git commit -m "feat: add WinUI backend package"
```

---

### Task 4: Examples And Package-Level Documentation

**Files:**
- Create: `packages/ricochet_ui/examples/counter_app.rco`
- Create: `packages/ricochet_ui/examples/project_tree_drag_drop.rco`
- Create: `packages/ricochet_ui/examples/data_grid_viewer.rco`
- Create: `packages/ricochet_ui/examples/rich_text_note.rco`
- Modify: `packages/ricochet_ui/README.md`
- Modify: `packages/ricochet_winui/README.md`

**Interfaces:**
- Consumes: `@ricochet/ui` and `@ricochet/winui` package helpers.
- Produces: app entrypoint examples with `app_init`, `app_view`, and `app_update` for later CLI app/export tests.

- [ ] **Step 1: Write counter app example**

`counter_app.rco` must define `app_init`, `app_view`, and `app_update`. `app_view` builds a `ui_window`; `app_update` handles click event ID `increment_button` and returns `ui_response`.

- [ ] **Step 2: Write tree drag/drop example**

`project_tree_drag_drop.rco` must build a `ui_tree` with one draggable node and one drop target. It should print or bind a `document` value for validation.

- [ ] **Step 3: Write data grid example**

`data_grid_viewer.rco` must build a `ui_data_grid` with two columns and two rows. It should keep selected row IDs in state.

- [ ] **Step 4: Write rich text note example**

`rich_text_note.rco` must build a `ui_rich_text_input` with a paragraph containing at least one bold span.

- [ ] **Step 5: Update READMEs**

Document the declarative document/state/event/command model and show the counter app. Document that `@ricochet/winui` is selected by CLI backend name `winui`.

- [ ] **Step 6: Verify and commit Task 4**

Run:

```powershell
rtk cargo run -q -p ricochet_cli --bin rco -- run packages/ricochet_ui/examples/counter_app.rco
rtk cargo run -q -p ricochet_cli --bin rco -- run packages/ricochet_ui/examples/project_tree_drag_drop.rco
rtk cargo run -q -p ricochet_cli --bin rco -- run packages/ricochet_ui/examples/data_grid_viewer.rco
rtk cargo run -q -p ricochet_cli --bin rco -- run packages/ricochet_ui/examples/rich_text_note.rco
rtk cargo run -q -p ricochet_cli --bin rco -- test packages/ricochet_ui packages/ricochet_winui
rtk git diff --check
```

Commit:

```powershell
rtk proxy git add packages/ricochet_ui packages/ricochet_winui
rtk proxy git commit -m "docs: add native UI package examples"
```

---

### Task 5: CLI Native App Export And Event Replay

**Files:**
- Modify: `crates/ricochet_cli/src/lib.rs`
- Modify: `crates/ricochet_cli/tests/cli_smoke.rs`

**Interfaces:**
- Consumes: app source defining `app_init`, `app_view`, and `app_update`.
- Produces: `rco app PATH --backend winui --export-ui-json PATH`, `RICOCHET_APP_EXPORT_UI_JSON`, and `RICOCHET_APP_REPLAY_EVENTS_JSON`.

- [ ] **Step 1: Add failing CLI smoke for UI JSON export**

Add a test fixture that writes a small app importing `packages/ricochet_ui/document.rco`, then runs:

```powershell
rco app app.rco --backend winui --export-ui-json ui.json
```

Expected JSON contains `"type":"window"` and `"backend":"winui"` metadata.

- [ ] **Step 2: Add failing CLI smoke for event replay**

Add a fixture event JSON file:

```json
[
  { "type": "click", "id": "increment_button", "value": null }
]
```

Run:

```powershell
rco app app.rco --backend winui --replay-events events.json --export-ui-json after.json
```

Expected JSON contains `"count":1` in the exported app state or document text.

- [ ] **Step 3: Add CLI arguments**

Add `Command::App` with:

```rust
App {
    #[command(flatten)]
    capabilities: CapabilityOptions,
    path: String,
    #[arg(long, default_value = "winui")]
    backend: String,
    #[arg(long = "export-ui-json")]
    export_ui_json: Option<PathBuf>,
    #[arg(long = "replay-events")]
    replay_events: Option<PathBuf>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}
```

- [ ] **Step 4: Implement app rendering helpers**

Add helpers near the existing GUI helpers:

```rust
fn run_app_file(
    path: &str,
    args: Vec<String>,
    capabilities: CapabilityOptions,
    backend: &str,
    export_ui_json: Option<&Path>,
    replay_events: Option<&Path>,
) -> Result<()>
```

The helper compiles the file, creates a VM, calls `app_init`, calls `app_view`, validates that the returned value is a map with `type = "window"`, replays events by calling `app_update`, then writes JSON export. If no export path is provided and backend is `winui`, it calls the live backend hook added in Task 7.

- [ ] **Step 5: Add value-to-JSON conversion for app export**

Reuse or add a local conversion from `ricochet_vm::Value` to `serde_json::Value` for nil, bool, number, float, string, arrays/lists, and maps. Unsupported values fail loudly with a message naming the unsupported kind.

- [ ] **Step 6: Implement environment export aliases**

If `RICOCHET_APP_EXPORT_UI_JSON` is set, use it as the export path. If `RICOCHET_APP_REPLAY_EVENTS_JSON` is set, use it as the replay path. CLI flags override environment variables.

- [ ] **Step 7: Verify and commit Task 5**

Run:

```powershell
rtk cargo fmt --all -- --check
rtk cargo test -p ricochet_cli --test cli_smoke app -- --nocapture
rtk cargo clippy -p ricochet_cli --all-targets -- -D warnings
rtk git diff --check
```

Commit:

```powershell
rtk proxy git add crates/ricochet_cli/src/lib.rs crates/ricochet_cli/tests/cli_smoke.rs
rtk proxy git commit -m "feat: add native app JSON export"
```

---

### Task 6: Native App Packaging Marker And Launcher

**Files:**
- Modify: `crates/ricochet_cli/Cargo.toml`
- Create: `crates/ricochet_cli/src/bin/rco_app.rs`
- Modify: `crates/ricochet_cli/src/lib.rs`
- Modify: `crates/ricochet_cli/tests/cli_smoke.rs`

**Interfaces:**
- Consumes: app rendering/export helpers from Task 5.
- Produces: `rco package app.rco --app --backend winui --output MyApp.exe` and packaged export smoke.

- [ ] **Step 1: Add launcher binary**

Add to `crates/ricochet_cli/Cargo.toml`:

```toml
[[bin]]
name = "rco-app"
path = "src/bin/rco_app.rs"
```

Create `rco_app.rs`:

```rust
fn main() -> anyhow::Result<()> {
    ricochet_cli::run_app_launcher()
}
```

- [ ] **Step 2: Add package CLI options**

Extend `Command::Package` with:

```rust
#[arg(long, help = "Package as a native app using --backend")]
app: bool,
#[arg(long, default_value = "winui")]
backend: String,
#[arg(long = "app-launcher", value_name = "PATH")]
app_launcher: Option<PathBuf>,
```

Package validation must reject `--app` combined with `--gui`, `--tui`, or `--mvc`.

- [ ] **Step 3: Add embedded app marker**

Add `EmbeddedAppKind::NativeApp` and marker bytes `RICOCHET_EMBEDDED_NATIVE_APP_V1`. Native app payloads are bytecode chunks plus backend metadata stored in a small JSON header before the chunk bytes or in a structured payload enum.

- [ ] **Step 4: Add launcher lookup**

Add `package_app_launcher(app_launcher: Option<&Path>) -> Result<PathBuf>` mirroring `package_launcher` for `rco-gui`.

- [ ] **Step 5: Add packaged export smoke test**

Test packaging a small app, run packaged executable with `RICOCHET_APP_EXPORT_UI_JSON`, and assert the output JSON contains `"backend":"winui"` and `"type":"window"`.

- [ ] **Step 6: Verify and commit Task 6**

Run:

```powershell
rtk cargo build -p ricochet_cli --bins
rtk cargo test -p ricochet_cli --test cli_smoke app -- --nocapture
rtk cargo fmt --all -- --check
rtk cargo clippy -p ricochet_cli --all-targets -- -D warnings
rtk git diff --check
```

Commit:

```powershell
rtk proxy git add crates/ricochet_cli
rtk proxy git commit -m "feat: package native app payloads"
```

---

### Task 7: WinUI Host Process And Live Backend Hook

**Files:**
- Create: `hosts/winui/Ricochet.WinUI.Host/Ricochet.WinUI.Host.csproj`
- Create: `hosts/winui/Ricochet.WinUI.Host/Program.cs`
- Create: `hosts/winui/Ricochet.WinUI.Host/App.xaml`
- Create: `hosts/winui/Ricochet.WinUI.Host/App.xaml.cs`
- Create: `hosts/winui/Ricochet.WinUI.Host/MainWindow.xaml`
- Create: `hosts/winui/Ricochet.WinUI.Host/MainWindow.xaml.cs`
- Create: `hosts/winui/Ricochet.WinUI.Host/UiDocument.cs`
- Create: `hosts/winui/Ricochet.WinUI.Host/UiRenderer.cs`
- Modify: `crates/ricochet_cli/src/lib.rs`
- Modify: `crates/ricochet_cli/tests/cli_smoke.rs`

**Interfaces:**
- Consumes: exported app document JSON and event update protocol.
- Produces: live WinUI rendering for the initial portable controls, with loud failure when Windows App SDK runtime or host executable is unavailable.

- [ ] **Step 1: Create WinUI project**

Create a Windows-only .NET project using `Microsoft.WindowsAppSDK` and `Microsoft.WinUI`. The project reads an initial UI JSON path argument:

```powershell
Ricochet.WinUI.Host.exe --document initial-ui.json --events events-out.jsonl --responses responses-in.jsonl
```

The first live slice may block on a top-level window and render static controls; event roundtrip is added in Step 5.

- [ ] **Step 2: Implement document model**

`UiDocument.cs` defines serializable records for `UiNode`, `UiResponse`, `UiCommand`, and `UiEvent`. It must preserve unknown properties in `JsonElement` or dictionaries so new portable fields do not crash the host.

- [ ] **Step 3: Implement renderer**

`UiRenderer.cs` maps v1 controls to WinUI controls:

```text
window -> MainWindow content root
stack -> StackPanel
grid -> Grid
text/heading -> TextBlock
button -> Button
text_input/multiline_text_input -> TextBox
checkbox -> CheckBox
toggle -> ToggleSwitch
select -> ComboBox
list -> ListView
tree -> TreeView
data_grid -> ListView with column-like row panel for v1
rich_text -> RichTextBlock
rich_text_input -> RichEditBox
```

Controls not yet rendered must fail loudly if required by validation; advisory unsupported controls can render a visible diagnostic text block.

- [ ] **Step 4: Add CLI live backend hook**

`run_app_file` writes the current UI JSON to a temp file and starts the WinUI host when backend is `winui` and no export path is requested. If the host is missing, error with:

```text
WinUI backend host not found; build it with dotnet publish hosts/winui/Ricochet.WinUI.Host/Ricochet.WinUI.Host.csproj or pass --winui-host PATH
```

- [ ] **Step 5: Implement click/change event roundtrip**

The WinUI host writes event JSON lines for button clicks and text changes. The Rust app launcher reads events, calls `app_update`, writes response JSON, and the host re-renders the document. This proves the backend is interactive, not only static.

- [ ] **Step 6: Add live smoke and host build docs**

Add a CLI smoke test that skips when the WinUI host executable is absent, and always tests host JSON model parsing through `dotnet test` or a console parse mode when available.

- [ ] **Step 7: Verify and commit Task 7**

Run:

```powershell
rtk dotnet restore hosts/winui/Ricochet.WinUI.Host/Ricochet.WinUI.Host.csproj
rtk dotnet build hosts/winui/Ricochet.WinUI.Host/Ricochet.WinUI.Host.csproj -c Release
rtk cargo test -p ricochet_cli --test cli_smoke app -- --nocapture
rtk cargo fmt --all -- --check
rtk cargo clippy -p ricochet_cli --all-targets -- -D warnings
rtk git diff --check
```

Commit:

```powershell
rtk proxy git add hosts/winui crates/ricochet_cli
rtk proxy git commit -m "feat: add WinUI native app host"
```

---

### Task 8: Docs, Reference, Acceptance, And Final Verification

**Files:**
- Modify: `docs/feature-map.md`
- Modify: `docs/reference/app.js`
- Modify: `packages/README.md`
- Modify: `README.md`
- Modify: `scripts/acceptance.ps1`

**Interfaces:**
- Consumes: implemented packages and CLI commands.
- Produces: documented native app UI feature and acceptance coverage.

- [ ] **Step 1: Update feature map**

Mark native app UI as beta with evidence pointing at `packages/ricochet_ui`, `packages/ricochet_winui`, `crates/ricochet_cli/src/lib.rs`, examples, and tests.

- [ ] **Step 2: Update reference word catalog**

Add package word groups for `ui` and `winui` in `docs/reference/app.js`. These are package words, not VM builtins.

- [ ] **Step 3: Update acceptance**

Add non-window checks:

```powershell
rco test packages/ricochet_ui packages/ricochet_winui
rco app packages/ricochet_ui/examples/counter_app.rco --backend winui --export-ui-json $tmp
```

- [ ] **Step 4: Run full verification**

Run:

```powershell
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo test --workspace
rtk cargo run -q -p ricochet_cli --bin rco -- test packages/ricochet_ui packages/ricochet_winui
rtk cargo run -q -p ricochet_cli --bin rco -- lint packages/ricochet_ui packages/ricochet_winui
rtk cargo run -q -p ricochet_cli --bin rco -- verify packages/ricochet_ui packages/ricochet_winui
rtk cargo run -q -p ricochet_cli --bin rco -- audit packages/ricochet_ui packages/ricochet_winui
rtk cargo run -q -p ricochet_cli --bin rco -- app packages/ricochet_ui/examples/counter_app.rco --backend winui --export-ui-json outputs/native-ui-counter.json
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1
rtk git diff --check
```

- [ ] **Step 5: Commit final docs and verification updates**

Commit:

```powershell
rtk proxy git add README.md docs packages scripts
rtk proxy git commit -m "docs: document native app UI backend"
```

---

## Plan Self-Review

Spec coverage:

- Backend-neutral public model: Tasks 1, 2, 4, and 8.
- WinUI first backend: Tasks 3, 6, and 7.
- Declarative documents plus imperative commands: Tasks 1, 2, and 5.
- No raw native handles in ordinary app code: Tasks 1, 3, and 7 keep handles inside host/backend.
- Tree, drag/drop, data grid, rich text: Task 2 and Task 4.
- JSON export and event replay smoke: Task 5 and Task 6.
- Live WinUI host: Task 7.
- Docs/reference/acceptance: Task 8.

Type consistency:

- UI documents are maps with `schema_version`, `id`, `type`, `props`, `children`, `events`, and `native_options`.
- Responses are maps with `schema_version`, `state`, `document`, `commands`, and `diagnostics`.
- Commands are maps with `schema_version`, `type`, and `required`.
- Events are maps with `schema_version`, `type`, `id`, and `value`.

Execution note:

- Subagents are useful for this plan, but the current user request did not explicitly ask for delegation. Execute inline unless the user asks to dispatch subagents.
