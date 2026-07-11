Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$validator = Join-Path $root "docs\reference\validate.ps1"
$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ricochet-packaged-docs-" + [guid]::NewGuid().ToString("N"))
$completeDocs = Join-Path $fixtureRoot "complete\docs"
$brokenDocs = Join-Path $fixtureRoot "broken\docs"

foreach ($docsRoot in @($completeDocs, $brokenDocs)) {
    New-Item -ItemType Directory -Path $docsRoot -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $root "docs\assets") -Destination (Join-Path $docsRoot "assets") -Recurse
    Copy-Item -LiteralPath (Join-Path $root "docs\reference") -Destination (Join-Path $docsRoot "reference") -Recurse
    Set-Content -LiteralPath (Join-Path $docsRoot "README.md") -Value "Packaged Ricochet fixture"
}
Copy-Item -LiteralPath (Join-Path $root "docs\learn") -Destination (Join-Path $completeDocs "learn") -Recurse

& $validator -Root (Join-Path $completeDocs "reference") -PackageMode

$previousErrorAction = $ErrorActionPreference
try {
    $ErrorActionPreference = "Continue"
    $brokenOutput = @(
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $validator `
            -Root (Join-Path $brokenDocs "reference") `
            -PackageMode 2>&1
    )
    $brokenExitCode = $LASTEXITCODE
}
finally {
    $ErrorActionPreference = $previousErrorAction
}
if ($brokenExitCode -eq 0) {
    throw "Packaged docs validation accepted a Reference Docs layout with no public Learn manual."
}
if (($brokenOutput -join "`n") -notmatch "docs/learn/index\.html is missing|Broken local HTML link") {
    throw "Broken packaged docs failed for an unexpected reason:`n$($brokenOutput -join "`n")"
}

Write-Host "Packaged docs contract tests passed."
Write-Host "Retained fixtures at: $fixtureRoot"
