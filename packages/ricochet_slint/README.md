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
$options "style" "fluent" put drop
$options slint_required_options
```

Use the CLI backend name `slint` for native app rendering, deterministic
renderer validation, payload exports, and app packaging:

```powershell
rco app app.rco --backend slint
rco app app.rco --backend slint --slint-validate-only
rco app app.rco --backend slint --export-ui-json app-slint-ui.json
rco package app.rco --app --backend slint --output MyApp.exe
```

Set `RICOCHET_SLINT_VALIDATE_ONLY=1` when launching packaged Slint apps to
compile the generated Slint renderer document without opening a window.
