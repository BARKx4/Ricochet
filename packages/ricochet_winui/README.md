# @ricochet/winui

`@ricochet/winui` is the first native backend package for Ricochet's
backend-neutral `@ricochet/ui` app model.

The package does not replace `@ricochet/ui`. It provides a WinUI backend
descriptor and scoped option maps that native Windows renderers can consume
while ordinary app code keeps using portable `ui_*` words.

## Modules

- `backend.rco`: backend descriptor plus required/advisory WinUI option maps.

## Example

```ricochet
"@ricochet/winui/backend" import

options map
$options "style" "AccentButtonStyle" put! drop
$options winui_required_options
```

Use the CLI backend name `winui` for native Windows app preview and packaging
once the app host is available:

```powershell
rco app app.rco --backend winui
rco package app.rco --app --backend winui --output MyApp.exe
```
