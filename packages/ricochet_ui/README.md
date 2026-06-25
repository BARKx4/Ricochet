# @ricochet/ui

`@ricochet/ui` is Ricochet's backend-neutral native app UI package. It defines
plain-data document, event, response, command, rich-text, grid, tree, and
drag/drop helpers for desktop app executables that are not WebView documents.

The package is the public app model. Renderer packages such as
`@ricochet/winui` and `@ricochet/slint` consume these document maps and
translate them into native controls.

## Modules

- `document.rco`: portable node, control, layout, collection, grid, tree, and
  drag/drop builders.
- `validation.rco`: document, node, and response validation helpers.
- `events.rco`: portable event map helpers.
- `commands.rco`: imperative command map helpers.
- `rich_text.rco`: structured rich-text document and control helpers.

## App Shape

Native app entrypoints use ordinary Ricochet functions:

```ricochet
( -> Map ) app_init function
  state map
  $state "count" 0 put! drop
  $state
end

( state -> Map ) app_view function
  state var
  children array
  $children "count_label" "Count: " $state "count" at to_string concat ui_text push! drop
  "main" "Counter" $children ui_window value
end

( state event -> Map ) app_update function
  event var
  state var
  $state app_view document var
  commands array
  $state $document $commands ui_response value
end
```

The backend owns native controls. Ricochet app code owns state, stable IDs,
documents, event handling, and command requests.

## Examples

Run the self-checking examples with:

```powershell
rco run packages/ricochet_ui/examples/counter_app.rco
rco run packages/ricochet_ui/examples/project_tree_drag_drop.rco
rco run packages/ricochet_ui/examples/data_grid_viewer.rco
rco run packages/ricochet_ui/examples/rich_text_note.rco
rco run packages/ricochet_ui/examples/native_showcase_app.rco
```

Export the native showcase app as a deterministic WinUI payload with:

```powershell
rco app packages/ricochet_ui/examples/native_showcase_app.rco --backend winui --export-ui-json native-showcase-ui.json
rco app packages/ricochet_ui/examples/native_showcase_app.rco --backend slint --export-ui-json native-showcase-slint-ui.json
rco app packages/ricochet_ui/examples/native_showcase_app.rco --backend slint --slint-validate-only
```
