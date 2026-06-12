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

$docsValidator = Join-Path $Root "docs\reference\validate.ps1"
Write-Host "==> docs reference validation"
& $docsValidator

$examplesRoot = Join-Path $Root "examples"
$examples = @(
    "basic-oop.rco",
    "collections.rco",
    "text_regex.rco",
    "loop_control.rco",
    "turing_complete.rco",
    "unary_counter.rco"
)

foreach ($example in $examples) {
    Invoke-Rco "example $example" @("run", (Join-Path $examplesRoot $example))
}

$env:RICOCHET_EXAMPLE_TEST = "present"
Invoke-Rco "example cli_system.rco" @(
    "run",
    (Join-Path $examplesRoot "cli_system.rco"),
    "--",
    "alpha",
    "beta"
)

if ([string]::IsNullOrWhiteSpace($TempRoot)) {
    $TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ricochet-acceptance-" + [System.Guid]::NewGuid().ToString("N"))
}

New-Item -ItemType Directory -Path $TempRoot -Force | Out-Null
$project = Join-Path $TempRoot "app"

Invoke-Rco "scaffold app" @("new", $project)
Invoke-Rco "list scaffold routes" @("routes", $project)
Invoke-Rco "check scaffold" @("check", $project)
Invoke-Rco "test scaffold" @("test", $project)
Invoke-Rco "test scaffold tests directory" @("test", (Join-Path $project "tests"))

$liveServerSmoke = Join-Path $Root "scripts\live-server-smoke.ps1"
Write-Host "==> live server smoke"
& $liveServerSmoke -Rco $Rco -Project $project

$sqliteProject = Join-Path $TempRoot "sqlite_app"
Invoke-Rco "scaffold SQLite beta app" @("new", "--with-sqlite", $sqliteProject)
Invoke-Rco "check SQLite beta app" @("check", $sqliteProject)
Invoke-Rco "test SQLite beta app" @("test", $sqliteProject)
Write-Host "==> SQLite live server smoke"
& $liveServerSmoke -Rco $Rco -Project $sqliteProject -RequestPath "/users" -ExpectedContent "ada@example.com"

Write-Host "Ricochet acceptance suite passed."
Write-Host "Generated scaffold left at: $project"
Write-Host "Generated SQLite scaffold left at: $sqliteProject"
