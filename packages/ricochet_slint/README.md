# @ricochet/slint

`@ricochet/slint` is the cross-platform native backend package for Ricochet's
backend-neutral `@ricochet/ui` app model.

The package provides backend metadata and scoped option helpers. Ricochet app
code should continue to build portable `@ricochet/ui` document maps; Slint
details belong in explicit native option maps.

## Backend Descriptor

```ricochet
"@ricochet/slint/backend" import

slint_backend

options map
$options "style" "fluent" put! drop
$options slint_required_options
```

Use the CLI backend name `slint` for deterministic native app payload exports
and exportable app packaging:

```powershell
rco app app.rco --backend slint --export-ui-json app-slint-ui.json
rco package app.rco --app --backend slint --output MyApp.exe
```

The first Slint slice is export/package ready. Live Slint rendering is kept as a
separate host implementation step so the portable `@ricochet/ui` contract can
settle before adding the GUI runtime.
