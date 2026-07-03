# Backend-Neutral Native App UI Design

Date: 2026-06-25
Status: Design spec for future implementation

## Purpose

Ricochet should be able to build native desktop app executables whose primary
UI is not a WebView. WinUI3 is the first proving backend because it is the
current modern native Windows UI stack, but the public Ricochet design center is
not WinUI. The public design center is a backend-neutral app model that can
later render through other backends such as Slint, Avalonia, Qt, SwiftUI/AppKit,
GTK, or another native renderer.

The initial split is:

```text
@ricochet/ui      stable app model and document builders
@ricochet/winui   first native Windows backend
future backends   Slint, Avalonia, Qt, SwiftUI/AppKit, GTK, or others
```

The feature lets Ricochet own the visible packaged application while avoiding a
hard dependency on one vendor UI ecosystem. WinUI is a backend, not the shape of
the language.

## Goals

- Provide a non-WebView native app surface for packaged Ricochet executables.
- Keep public Ricochet UI APIs backend-neutral.
- Prove the model first with a real WinUI3 renderer on Windows.
- Keep ordinary app code testable without opening a native window.
- Keep the programming model postfix/RPN-shaped and consistent with existing
  Ricochet package conventions.
- Use declarative UI documents for layout and persistent UI state.
- Use imperative command maps for naturally imperative actions such as focus,
  dialogs, clipboard, file pickers, scrolling, window state, and drag/drop
  feedback.
- Include enough v1 controls to build credible desktop applications, including
  tree views, drag/drop, data grids, and rich text.
- Keep backend-specific options scoped so future renderers can be added without
  inheriting WinUI-shaped vocabulary.
- Fail loudly when a backend cannot support a required control, option,
  command, or runtime dependency.

## Non-Goals For V1

- No raw native control handles in ordinary Ricochet app code.
- No direct WinUI object lifetime management from Ricochet.
- No full spreadsheet implementation.
- No full word processor implementation.
- No custom drawing or retained canvas API.
- No multi-window docking system.
- No system tray or notification framework.
- No mobile-specific control set.
- No claim that every future backend can render every optional native detail.
- No replacement for existing WebView/MVC GUI paths. WebView remains useful for
  browser-shaped app surfaces and local MVC apps.

## Design Stance

The core model is declarative UI plus imperative commands.

Ricochet owns:

- application state;
- stable UI node IDs;
- the UI document tree;
- event handling;
- command requests;
- persistent UI state that matters to app behavior.

The backend owns:

- the native UI thread;
- native control instances;
- platform-specific lifetimes;
- event translation;
- command execution;
- renderer-specific diffing, rebuilding, and native option handling.

Ordinary Ricochet code should not hold a WinUI `Button`, `TextBox`, `TreeView`,
or `DataGrid` handle. It holds maps, arrays, state, stable IDs, events, and
commands. This keeps app logic portable and testable, while still letting the
backend produce a native Windows experience.

## Packages And Backends

### `@ricochet/ui`

`@ricochet/ui` defines the portable app model. It should be a first-party
package whenever practical, with VM/CLI support only where the host boundary
requires it.

Responsibilities:

- builder words for portable UI nodes;
- document and response validation;
- event helper predicates and accessors;
- command builder words;
- backend option map helpers;
- test helpers for document and event replay fixtures;
- reference docs and examples for the portable model.

Public portable words use `ui_` prefixes:

```text
ui_window
ui_stack
ui_grid
ui_button
ui_text_input
ui_tree
ui_data_grid
ui_rich_text
ui_rich_text_input
ui_focus
ui_open_dialog
ui_response
ui_native_options
```

The exact word list can be refined during implementation, but public multiword
names must use underscores and must not introduce leading-dot source syntax or
fake namespace-dot host APIs.

### `@ricochet/winui`

`@ricochet/winui` is the first backend package and host integration. It maps
the portable document contract into WinUI3 controls.

Responsibilities:

- WinUI backend selection and metadata;
- scoped WinUI option helpers;
- validation for WinUI-required options;
- host protocol mapping for WinUI controls;
- Windows-specific documentation and examples;
- Windows packaging and smoke-test support.

WinUI-specific words or options must not replace the portable model. They are
escape hatches for native polish:

```ricochet
"save_button" "Save" ui_button
  "winui" map
    "style" "AccentButtonStyle" put
    "icon" "Save" put
  ui_native_options
```

### Future Backends

Future backend packages should consume the same portable document contract.
Candidate backends include:

- `@ricochet/slint` for a Rust-friendly cross-platform non-WebView renderer;
- `@ricochet/avalonia` for a .NET cross-platform desktop renderer;
- `@ricochet/qt` for a mature cross-platform native app toolkit;
- `@ricochet/apple` for SwiftUI/AppKit;
- `@ricochet/gtk` for GTK/libadwaita.

The backend contract must make it possible to add these without renaming the
portable `ui_*` words.

## Programming Model

A native app entrypoint is ordinary Ricochet source that exposes lifecycle
functions. The initial shape is:

```ricochet
( -> Map ) app_init function
  state map
  $state "count" 0 put drop
  $state
end

( state -> Map ) app_view function
  state var
  (( returns a validated ui_window document map ))
end

( state event -> Map ) app_update function
  event var
  state var
  (( returns a validated ui_response map ))
end
```

The host lifecycle is:

```text
start app
  call app_init
  call app_view with the initial state
  render native window

user interacts
  translate native event into an event map
  call app_update with current state and event
  validate returned response
  update native UI from the returned document
  execute returned commands
```

The app may mutate its state map in place or return a replacement state map.
The backend treats the returned state as authoritative.

### Example

This example is representative. Exact word arities may be adjusted during
implementation, but the direction is stable.

```ricochet
"ui/app" import

( -> Map ) app_init function
  state map
  $state "count" 0 put drop
  $state
end

( state -> Map ) app_view function
  state var

  children array
  $children
    "count_label"
    "Count: " $state "count" at to_string concat
    ui_text
    push drop

  $children
    "increment_button"
    "Increment"
    ui_button
    push drop

  "Counter" $children ui_window value
end

( state event -> Map ) app_update function
  event var
  state var

  $event "id" at "increment_button" = if
    $state "count" at 1 + count var
    $state "count" $count put drop
  end

  $state app_view document var
  commands array
  $state $document $commands ui_response value
end
```

## App Manifest

Source-file-only apps may use conventional function names:

```text
app_init
app_view
app_update
```

Project apps should be able to declare app metadata in `ricochet.toml`:

```toml
[app]
entry = "app.rco"
init = "app_init"
view = "app_view"
update = "app_update"
default_backend = "winui"
title = "Counter"
```

The manifest is metadata for packaging and discovery. The UI document returned
by `app_view` remains the runtime source of truth for the current window title,
theme, layout, controls, and events.

## Document Contract

The stable host boundary is JSON-serializable data, not native objects.

A UI document is a map with:

```text
schema_version
type
id
props
children
events
native_options
```

Every rendered node has a stable app-authored `id`. IDs are used for:

- event routing;
- focus;
- selection;
- tree expansion;
- data-grid row identity;
- drag/drop;
- scroll commands;
- diffing or native-control reconciliation;
- command targets.

IDs must be unique within the active document unless a specific control defines
a nested ID namespace. V1 should prefer global document-wide IDs because that
keeps event replay and debugging straightforward.

Minimal representative JSON shape:

```json
{
  "schema_version": 1,
  "type": "window",
  "id": "main",
  "props": {
    "title": "Counter",
    "width": 900,
    "height": 640
  },
  "children": [
    {
      "type": "button",
      "id": "increment_button",
      "props": {
        "label": "Increment"
      },
      "children": [],
      "events": ["click"],
      "native_options": {}
    }
  ],
  "events": ["close"],
  "native_options": {
    "winui": {
      "theme": "system"
    }
  }
}
```

All document nodes must be representable as Ricochet maps and arrays. The host
may maintain richer internal models, but the app-facing contract remains plain
data.

## Response Contract

`app_update` returns a response map:

```text
state
document
commands
diagnostics
```

`state` is the next authoritative application state. `document` is the next
authoritative UI document. `commands` is an ordered list of imperative requests.
`diagnostics` is optional app-provided metadata for debugging, logs, or dev
tools.

Representative shape:

```json
{
  "schema_version": 1,
  "state": {
    "count": 1
  },
  "document": {
    "type": "window",
    "id": "main"
  },
  "commands": [
    {
      "type": "focus",
      "target": "name_input"
    }
  ],
  "diagnostics": []
}
```

The backend must validate the response before mutating native UI state. Invalid
responses fail loudly in preview, smoke, and packaged app modes.

## Events

Event maps are plain data sent from the backend to Ricochet:

```json
{
  "schema_version": 1,
  "type": "click",
  "id": "save_button",
  "value": null,
  "modifiers": ["ctrl"],
  "backend": "winui",
  "native": {}
}
```

Common event types:

```text
click
change
submit
select
activate
expand
collapse
sort
filter
drag_start
drag_over
drop
open
close
focus
blur
key
lifecycle
```

Event maps should include portable fields first. Backend-specific detail goes
under a scoped map such as `"native" { "winui": ... }` or a similarly explicit
shape chosen during implementation. App code can ignore backend detail and stay
portable.

Event dispatch must be deterministic for replay tests. Given the same starting
state and event sequence, Ricochet app code should produce the same response
documents, apart from explicitly allowed host-derived values such as selected
file paths.

## Commands

Commands are imperative requests from Ricochet to the backend. They are used for
actions that do not fit cleanly as persistent document state.

Initial command types:

```text
focus
blur
open_dialog
close_dialog
show_message
open_file_picker
save_file_picker
open_folder_picker
clipboard_read
clipboard_write
scroll_into_view
set_window_state
close_window
drag_feedback
```

Representative command map:

```json
{
  "schema_version": 1,
  "type": "focus",
  "target": "name_input",
  "required": true
}
```

Command rules:

- Commands are ordered.
- A command with `"required": true` fails loudly if unsupported.
- Advisory commands may be ignored with a diagnostic.
- Commands target stable node IDs when possible.
- Commands that can return values, such as file pickers or clipboard reads,
  produce follow-up event maps rather than mutating app state behind Ricochet's
  back.

## V1 Portable Surface

The v1 portable control set should support credible desktop apps, including
structured data and formatted text, while keeping advanced editors out of scope.

### App And Window

- app metadata;
- one main window;
- title;
- size hints;
- theme hints;
- lifecycle events.

V1 is one-main-window. Additional dialogs are supported through dialog controls
and commands. Multi-window app coordination is deferred.

### Layout

- stack;
- grid;
- split pane;
- scroll view;
- group or section;
- spacer.

Layout props should use portable concepts such as orientation, alignment,
spacing, row/column definitions, minimum size, preferred size, and grow/shrink
behavior. Backend-specific layout detail belongs in scoped native options.

### Text And Display

- text;
- heading;
- image or icon;
- status or message area;
- rich text view.

Text nodes are plain strings. Rich text nodes use the rich text document
contract below.

### Inputs

- button;
- text input;
- multiline text input;
- rich text input;
- checkbox;
- toggle;
- combo/select.

Inputs should emit `change`, `submit`, `focus`, `blur`, and keyboard events
where applicable. The current input value should be represented in Ricochet
state when app behavior depends on it.

### Collections

- list;
- tree view;
- data grid.

Collections use stable item IDs. Selection, activation, expansion, collapse,
sort, and filter events are portable.

### Commands And Navigation

- menu bar;
- command bar;
- context menu;
- keyboard shortcuts.

Menu and command items should produce ordinary event maps. Keyboard shortcuts
should be represented as portable accelerator descriptors where possible.

### Dialogs And Platform

- modal dialog;
- message dialog;
- file open picker;
- file save picker;
- folder picker;
- clipboard.

Dialogs may be document nodes or commands depending on whether they are part of
the current declarative UI state or a one-shot platform interaction.

### Interaction

- focus command;
- scroll-into-view command;
- selection;
- activation/click;
- submit/change events;
- drag/drop.

## Data Grid Contract

Data grids are included in v1 as a basic portable contract, not as a full
spreadsheet.

V1 grid supports:

- column definitions;
- row values;
- stable row IDs;
- text, number, boolean, nil, and simple string-rendered values;
- selection events;
- row activation events;
- sort intent events;
- filter intent events;
- column sizing hints;
- read-only display as the stable baseline.

Editable cells may be marked beta or backend-optional after implementation
proves the contract. If editable cells ship in v1, they must use explicit
`change` and `submit` events and must not hide state changes inside the backend.

Deferred:

- formula cells;
- pivot tables;
- frozen panes;
- spreadsheet-grade editing;
- guaranteed huge-table virtualization;
- arbitrary custom cell renderers.

The grid event model should report row IDs and column IDs rather than visual row
indices whenever possible. Visual indices may be included as advisory detail.

## Rich Text Contract

Rich text is included in v1 as a basic formatted document contract, not as a
full word processor.

V1 rich text supports:

- paragraphs;
- bold;
- italic;
- inline code;
- links;
- basic unordered and ordered lists;
- read-only rich text view;
- simple rich text input when the backend supports it;
- change and submit events for editable rich text.

Deferred:

- track changes;
- collaborative editing;
- embedded arbitrary widgets;
- complex pagination;
- page layout;
- full word processor behavior.

Rich text values should be structured data, not raw backend markup. A
representative portable shape:

```json
{
  "schema_version": 1,
  "blocks": [
    {
      "type": "paragraph",
      "spans": [
        {
          "text": "Hello ",
          "marks": []
        },
        {
          "text": "native UI",
          "marks": ["bold"]
        }
      ]
    }
  ]
}
```

Backends may translate this into WinUI rich text, Slint text, Avalonia runs, Qt
documents, or another native representation.

## Tree View Contract

Tree views are included in v1 as a first-class portable control.

Tree nodes have:

- stable node IDs;
- labels;
- optional icons;
- optional child nodes;
- expanded/collapsed state;
- selection state when app behavior depends on it;
- optional drag/drop roles.

Tree events:

```text
select
activate
expand
collapse
drag_start
drag_over
drop
```

Expanded node IDs should be represented in Ricochet state when app behavior,
lazy loading, or persistence depends on them.

## Drag And Drop Contract

Drag/drop is included in v1 and must be portable enough for app-local
reordering, tree operations, grid row moves, text drops, and shell file drops.

Payload types:

```text
text
files
app_local
rows
tree_nodes
custom_json
```

Event flow:

```text
drag_start
drag_over
drop
```

`drag_start` identifies the source node and payload. `drag_over` lets Ricochet
or the backend determine whether a drop target is valid. `drop` reports the
target, operation, and payload.

The backend may provide native drag visuals. Ricochet can request feedback
through command maps. Required drag/drop features must fail loudly when the
backend does not support them.

## Native Options

Portable fields are the stable contract. Backend options are scoped maps:

```json
{
  "native_options": {
    "winui": {
      "style": "AccentButtonStyle",
      "icon": "Save"
    }
  }
}
```

Backend option rules:

- Portable fields must work across compliant backends.
- Backend option maps are optional unless marked required.
- Required backend options must fail loudly if unsupported.
- Advisory backend options may be ignored with diagnostics.
- Backend-specific options must not become portable word names by accident.
- A backend must ignore other backend scopes unless a compatibility layer
  explicitly says otherwise.

This keeps WinUI polish possible without making the public API WinUI-shaped.

## Host And Packaging

The initial CLI direction is:

```powershell
rco app app.rco --backend winui
rco app app.rco --backend winui --export-ui-json build/ui.json
rco package app.rco --app --backend winui --output MyApp.exe
```

`rco gui` and `rco package --gui` remain the WebView/MVC GUI path. The native
app path uses `--app --backend`.

The packaged host should support non-window smoke validation:

```powershell
$env:RICOCHET_APP_EXPORT_UI_JSON = "build/ui.json"
.\MyApp.exe
```

The host should also support event replay fixtures:

```powershell
$env:RICOCHET_APP_REPLAY_EVENTS_JSON = "tests/events/counter.json"
$env:RICOCHET_APP_EXPORT_UI_JSON = "build/after-events.json"
.\MyApp.exe
```

These environment variable names are part of the initial smoke-test contract.
CI must be able to validate packaged native apps without opening a live window.

Host responsibilities:

- load source, bytecode, or packaged app bundle;
- call `app_init`, `app_view`, and `app_update`;
- validate UI documents and responses;
- own native UI thread and platform runtime;
- render or reconcile native controls from stable document IDs;
- route native events into Ricochet event maps;
- execute command maps;
- export UI JSON for smoke tests;
- report unsupported controls, options, commands, or runtime dependencies with
  actionable diagnostics.

## WinUI Backend

The WinUI backend is the first proving backend.

Implementation may use a C#/.NET WinUI host, a Rust host through
Rust-for-Windows, or a hybrid launcher if that proves more reliable. The design
does not require the host implementation language to be Ricochet or Rust as
long as the packaged app boundary remains deterministic and testable.

WinUI backend responsibilities:

- bootstrap Windows App SDK/WinUI runtime requirements;
- create the main native window;
- map portable controls to WinUI controls;
- map WinUI events to portable event maps;
- execute platform commands;
- support scoped WinUI options;
- fail loudly when the runtime or required native feature is unavailable;
- expose export/smoke mode that does not require opening a visible window.

The backend must not require ordinary Ricochet app code to use raw WinUI object
handles. If a future version adds handle-based advanced APIs, they should be
explicitly marked backend-specific and outside the portable v1 contract.

## Capabilities And Safety

Native app execution is a host capability boundary. The app host can trigger
platform actions such as file pickers, clipboard access, file drops, and window
management. The implementation should integrate with Ricochet's existing
capability posture instead of bypassing it.

Suggested capability model:

- app rendering requires a native app capability;
- file pickers require filesystem-related capability checks before exposing
  selected paths to app code;
- clipboard read/write should be separately visible in runtime capabilities;
- shell file-drop events should be treated as external input;
- backend option maps must be data, not arbitrary host code;
- event replay files and exported UI JSON should avoid writing outside
  explicitly requested paths.

The project data-safety policy still applies. Any implementation or tool that
would delete generated app output, temp bundles, exported JSON, or packaging
artifacts must ask first unless the operation is clearly limited to an
implementation-owned temp directory with a documented lifecycle.

## Error Handling

The implementation must fail loudly for:

- duplicate node IDs;
- unknown control types;
- malformed node maps;
- malformed data-grid columns or rows;
- malformed rich text documents;
- invalid drag/drop payloads;
- unsupported required backend options;
- unsupported required commands;
- missing lifecycle functions;
- event callback/update failures;
- command target missing;
- backend runtime unavailable;
- WinUI bootstrap failure;
- JSON export path errors.

Diagnostics should identify:

- the node ID when available;
- the control type;
- the backend;
- whether the failed feature was portable or backend-specific;
- whether the option or command was required or advisory;
- the source span when the document node can be traced to a builder call.

## Testing Strategy

Most tests should avoid opening a live native window. The test pyramid is:

### Core Package Tests

- `@ricochet/ui` builders return valid document maps.
- Document validation catches malformed nodes.
- Response validation catches malformed update results.
- Tree, grid, rich-text, and drag/drop contracts validate.
- Backend option maps validate required and advisory behavior.
- Event/update helpers produce deterministic responses.

### CLI And Host Smoke Tests

- `rco app app.rco --backend winui --export-ui-json` exports valid JSON.
- `rco package app.rco --app --backend winui` creates a packaged executable.
- Packaged executable exports UI JSON through environment-based smoke mode.
- Event replay fixture calls `app_update` and validates the next document.
- Unsupported backend/control/command failures are loud and actionable.

### WinUI Backend Tests

- document-to-host-model mapping tests;
- focused Windows CI smoke for launcher/package shape;
- optional local live-window smoke checking a visible top-level title;
- manual QA for real input, tree expansion, grid selection, rich text editing,
  drag/drop, file picker, and clipboard.

### Reference Examples

- counter app;
- settings form;
- project/file tree with drag/drop;
- data-grid viewer;
- rich-text note editor;
- packaged Windows app example.

## Verification For Implementation

A future implementation should include focused package and host tests plus the
ordinary Ricochet verification ladder. A representative full gate is:

```powershell
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo test --workspace
rco test packages/ricochet_ui packages/ricochet_winui
rco lint packages/ricochet_ui packages/ricochet_winui
rco verify packages/ricochet_ui packages/ricochet_winui
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\docs\reference\validate.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1
rtk git diff --check
```

The exact package paths may change during implementation. The verification
principle should not: validate pure UI documents, event replay, packaged app
smoke output, and live Windows behavior before calling the backend complete.

## Documentation Strategy

Documentation should keep the backend-neutral model visible:

- `@ricochet/ui` README explains the app model and portable controls.
- `@ricochet/winui` README explains Windows-specific setup, packaging, and
  native options.
- Reference docs list portable controls separately from backend-specific
  options.
- Learn docs introduce native app UI separately from WebView GUI and MVC.
- Examples should validate through JSON export before asking users to launch a
  live native window.

Public docs should avoid implying that WinUI is the only future route. WinUI is
the first backend because it proves native Windows app executables.

## Deferred Extensions

The following are intentionally deferred from v1:

- multiple coordinated windows;
- docking layouts;
- full accessibility customization APIs;
- tray icons;
- system notifications;
- custom drawing/canvas;
- retained native control handles;
- advanced grid virtualization;
- spreadsheet formulas;
- full rich document editing;
- mobile-specific adapters;
- backend marketplace or third-party backend plugin protocol.

These can be designed after the portable document/event/command contract and
WinUI proving backend are working.
