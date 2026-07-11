Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$validator = Join-Path $root "docs\reference\validate.ps1"
$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ricochet-packaged-docs-" + [guid]::NewGuid().ToString("N"))
$completeDocs = Join-Path $fixtureRoot "complete\docs"
$brokenDocs = Join-Path $fixtureRoot "broken\docs"

$parseTokens = $null
$parseErrors = $null
$validatorAst = [System.Management.Automation.Language.Parser]::ParseFile(
    $validator,
    [ref]$parseTokens,
    [ref]$parseErrors
)
if ($parseErrors.Count -gt 0) {
    throw "Docs validator has PowerShell parse errors: $($parseErrors -join '; ')"
}
$relativePathFunction = $validatorAst.Find(
    {
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
            $node.Name -eq "Get-DocsRelativePath"
    },
    $true
)
if ($null -eq $relativePathFunction) {
    throw "Docs validator is missing Get-DocsRelativePath."
}

$unixStyleProbe = @"
$($relativePathFunction.Extent.Text)
`$docsRootFull = "/Users/runner/work/Ricochet/docs"
`$relativePath = Get-DocsRelativePath -Path "/Users/runner/work/Ricochet/docs/reference/guides/index.html"
if (`$relativePath -ne "reference/guides/index.html") {
    throw "Docs relative-path helper mishandled Unix-style paths: `$relativePath"
}
"@
$encodedProbe = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($unixStyleProbe))
$modernPowerShell = Get-Command pwsh -ErrorAction SilentlyContinue
if ($null -ne $modernPowerShell) {
    $unixProbeOutput = @(& $modernPowerShell.Source -NoProfile -EncodedCommand $encodedProbe 2>&1)
    if ($LASTEXITCODE -ne 0) {
        throw "Docs relative-path helper failed for Unix-style paths:`n$($unixProbeOutput -join "`n")"
    }
} elseif ($relativePathFunction.Extent.Text -notmatch '\[System\.IO\.Path\]::GetRelativePath') {
    throw "Docs relative-path helper lacks the cross-platform Path.GetRelativePath implementation."
}

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
