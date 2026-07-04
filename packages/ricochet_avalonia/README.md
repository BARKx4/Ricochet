# @ricochet/avalonia

`@ricochet/avalonia` is the cross-platform desktop backend package for
Ricochet's backend-neutral `@ricochet/ui` app model.

The package does not replace `@ricochet/ui`. It provides an Avalonia backend
descriptor and scoped option maps that Windows, Linux, and macOS desktop hosts
can consume while ordinary app code keeps using portable `ui_*` words.

Use Avalonia options for cross-platform desktop polish only. App structure,
state, events, and commands should stay in the portable `@ricochet/ui` document
contract.

## Modules

- `backend.rco`: backend descriptor plus required/advisory Avalonia option maps.

## Example

```ricochet
"@ricochet/avalonia/backend" import

options map
$options "density" "compact" put drop
$options avalonia_required_options
```

Use the CLI backend name `avalonia` for native desktop app preview and
packaging once the app host is available:

```powershell
rco app app.rco --backend avalonia
rco package app.rco --app --backend avalonia --output MyApp.exe
```

For a source checkout, build the live host before launching a window:

```powershell
dotnet build hosts/avalonia/Ricochet.Avalonia.Host/Ricochet.Avalonia.Host.csproj -c Release
```

The host consumes the same JSON document/event/response protocol as the WinUI
host, so JSON export and event replay remain the stable smoke-test boundary.
