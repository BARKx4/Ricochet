param(
    [string]$Rco,
    [string]$TempRoot
)

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if ([string]::IsNullOrWhiteSpace($Rco)) {
    $Rco = Join-Path $Root "target\debug\rco.exe"
}

if (-not (Test-Path -LiteralPath $Rco -PathType Leaf)) {
    throw "Could not find rco at '$Rco'. Build it first with: cargo build -p ricochet_cli --bin rco"
}

function Invoke-Rco {
    param(
        [string]$Name,
        [string[]]$Arguments
    )

    Write-Host "==> $Name"
    & $Rco @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed with exit code $LASTEXITCODE"
    }
}

$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
function Write-Utf8File {
    param(
        [string]$Path,
        [string]$Content
    )

    [System.IO.File]::WriteAllText($Path, $Content, $Utf8NoBom)
}

$docsValidator = Join-Path $Root "docs\reference\validate.ps1"
Write-Host "==> docs reference validation"
& $docsValidator

$editorValidator = Join-Path $Root "scripts\validate-editor-assets.ps1"
Write-Host "==> editor asset validation"
& $editorValidator

Invoke-Rco "word inventory drift check" @(
    "words",
    "--check",
    "--docs-app",
    (Join-Path $Root "docs\reference\app.js"),
    "--grammar",
    (Join-Path $Root "editors\vscode\syntaxes\ricochet.tmLanguage.json")
)

$examplesRoot = Join-Path $Root "examples"
$examples = @(
    "basic-oop.rco",
    "collections.rco",
    "text_regex.rco",
    "loop_control.rco",
    "webview_ui.rco",
    "turing_complete.rco",
    "unary_counter.rco"
)

foreach ($example in $examples) {
    Invoke-Rco "example $example" @("run", (Join-Path $examplesRoot $example))
}

Invoke-Rco "check example tui_counter.rco" @("check", (Join-Path $examplesRoot "tui_counter.rco"))
Invoke-Rco "native UI package tests" @("test", (Join-Path $Root "packages\ricochet_ui"))
Invoke-Rco "WinUI package tests" @("test", (Join-Path $Root "packages\ricochet_winui"))

if ([string]::IsNullOrWhiteSpace($TempRoot)) {
    $TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ricochet-acceptance-" + [System.Guid]::NewGuid().ToString("N"))
}

New-Item -ItemType Directory -Path $TempRoot -Force | Out-Null
$nativeUiJson = Join-Path $TempRoot "native-ui-counter.json"
Invoke-Rco "native UI counter JSON export" @(
    "app",
    (Join-Path $Root "packages\ricochet_ui\examples\counter_app.rco"),
    "--backend",
    "winui",
    "--export-ui-json",
    $nativeUiJson
)
$nativeUiExport = Get-Content -LiteralPath $nativeUiJson -Raw | ConvertFrom-Json
if ($nativeUiExport.backend -ne "winui" -or $nativeUiExport.document.type -ne "window") {
    throw "Native UI counter export did not produce a WinUI window document"
}

$nativeShowcaseJson = Join-Path $TempRoot "native-ui-showcase.json"
Invoke-Rco "native UI showcase JSON export" @(
    "app",
    (Join-Path $Root "packages\ricochet_ui\examples\native_showcase_app.rco"),
    "--backend",
    "winui",
    "--export-ui-json",
    $nativeShowcaseJson
)
$nativeShowcaseRaw = Get-Content -LiteralPath $nativeShowcaseJson -Raw
$nativeShowcaseExport = $nativeShowcaseRaw | ConvertFrom-Json
if ($nativeShowcaseExport.backend -ne "winui" -or $nativeShowcaseExport.document.props.title -ne "Native Release Desk") {
    throw "Native UI showcase export did not produce the expected WinUI release desk"
}
$nativeShowcasePatterns = @(
    '"id"\s*:\s*"release_tree"',
    '"id"\s*:\s*"release_grid"',
    '"id"\s*:\s*"release_notes"',
    '"type"\s*:\s*"tree"',
    '"type"\s*:\s*"data_grid"',
    '"type"\s*:\s*"rich_text_input"',
    '"text"\s*:\s*"Ship confidence: 82%"'
)
foreach ($pattern in $nativeShowcasePatterns) {
    if ($nativeShowcaseRaw -notmatch $pattern) {
        throw "Native UI showcase export did not include pattern: $pattern"
    }
}

$env:RICOCHET_EXAMPLE_TEST = "present"
Invoke-Rco "example cli_system.rco" @(
    "run",
    (Join-Path $examplesRoot "cli_system.rco"),
    "--",
    "alpha",
    "beta"
)

$project = Join-Path $TempRoot "app"

Invoke-Rco "scaffold app" @("new", $project)
Invoke-Rco "list scaffold routes" @("routes", $project)
Invoke-Rco "check scaffold" @("check", $project)
Invoke-Rco "test scaffold" @("test", $project)
Invoke-Rco "test scaffold tests directory" @("test", (Join-Path $project "tests"))

$liveServerSmoke = Join-Path $Root "scripts\live-server-smoke.ps1"
Write-Host "==> live server smoke"
& $liveServerSmoke -Rco $Rco -Project $project

$uploadStreamSmoke = Join-Path $Root "scripts\upload-stream-smoke.ps1"
Write-Host "==> upload stream smoke"
& $uploadStreamSmoke -Rco $Rco -TempRoot $TempRoot

$sqliteProject = Join-Path $TempRoot "sqlite_app"
Invoke-Rco "scaffold SQLite beta app" @("new", "--with-sqlite", $sqliteProject)
Invoke-Rco "check SQLite beta app" @("check", $sqliteProject)
Invoke-Rco "test SQLite beta app" @("test", $sqliteProject)
Write-Host "==> SQLite live server smoke"
& $liveServerSmoke -Rco $Rco -Project $sqliteProject -RequestPath "/users" -ExpectedContent "ada@example.com"
$betaAppSmoke = Join-Path $Root "scripts\beta-app-smoke.ps1"
Write-Host "==> SQLite beta app smoke"
& $betaAppSmoke -Rco $Rco -Project $sqliteProject

$sqliteMigrations = Join-Path $sqliteProject "db\migrations"
Invoke-Rco "generate SQLite DSL acceptance migration" @("migrate", "new", "acceptance_notes", "--dsl", $sqliteProject)
$dslUp = Get-ChildItem -LiteralPath $sqliteMigrations -Filter "*_acceptance_notes.up.rco" | Sort-Object Name | Select-Object -Last 1
$dslDown = Get-ChildItem -LiteralPath $sqliteMigrations -Filter "*_acceptance_notes.down.rco" | Sort-Object Name | Select-Object -Last 1
if ($null -eq $dslUp -or $null -eq $dslDown) {
    throw "Generated SQLite DSL migration files were not found"
}
Write-Utf8File -Path $dslUp.FullName -Content @"
"acceptance_notes" table_create
"id" "integer" column primary_key
"body" "text" column not_null
"@
Write-Utf8File -Path $dslDown.FullName -Content @"
"acceptance_notes" table_drop
"@
Write-Utf8File -Path (Join-Path $sqliteProject "app\Models\AcceptanceNote.rco") -Content @"
AcceptanceNote Model Subclass
  "acceptance_notes" Table
  "id" Accessor
  "body" Accessor
end
"@
Invoke-Rco "apply SQLite acceptance migration" @("migrate", "apply", $sqliteProject)
Invoke-Rco "rollback SQLite acceptance migration" @("migrate", "rollback", "--steps", "1", $sqliteProject)
Invoke-Rco "reapply SQLite acceptance migration" @("migrate", "apply", $sqliteProject)
Invoke-Rco "dump SQLite acceptance schema" @("migrate", "dump", "--output", "db/schema.sql", $sqliteProject)
$schemaDump = Get-Content -LiteralPath (Join-Path $sqliteProject "db\schema.sql") -Raw
if ($schemaDump -notlike "*acceptance_notes*") {
    throw "SQLite schema dump did not include acceptance_notes"
}

$sqliteSeeds = Join-Path $sqliteProject "db\seeds"
New-Item -ItemType Directory -Path $sqliteSeeds -Force | Out-Null
Write-Utf8File -Path (Join-Path $sqliteSeeds "001_acceptance_notes.sql") -Content "insert into acceptance_notes (body) values ('from sql seed');`n"
Write-Utf8File -Path (Join-Path $sqliteSeeds "002_acceptance_notes.rco") -Content @"
map "body" "from rco seed" put! AcceptanceNote insert value drop
AcceptanceNote count_records value
2 assert_equals
"@
Invoke-Rco "seed SQLite acceptance data" @("seed", $sqliteProject)

Write-Host "Ricochet acceptance suite passed."
Write-Host "Generated scaffold left at: $project"
Write-Host "Generated SQLite scaffold left at: $sqliteProject"
