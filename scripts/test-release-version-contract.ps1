Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$ValidatorPath = Join-Path $Root "scripts\validate-release-version.ps1"
$ReleasePagePath = Join-Path $Root "docs\releases\v0.1.19-rc.5.html"
$Failures = [System.Collections.Generic.List[string]]::new()

function Add-Failure {
    param([string]$Message)

    [void]$script:Failures.Add($Message)
}

$releasePage = [System.IO.File]::ReadAllText($ReleasePagePath)
$releaseLines = @($releasePage -split "`r?`n")

$packageCommands = @($releaseLines | Where-Object { $_ -match 'package-release\.ps1' })
if ($packageCommands.Count -ne 1 -or $packageCommands[0] -notmatch '(?:^|\s)-RequireInstaller(?:\s|<|$)') {
    Add-Failure "The rc.5 package-release command must retain -RequireInstaller."
}

$artifactCommands = @($releaseLines | Where-Object { $_ -match 'validate-release-artifacts\.ps1' })
if ($artifactCommands.Count -ne 1 -or $artifactCommands[0] -notmatch '(?:^|\s)-RequireInstaller(?:\s|<|$)') {
    Add-Failure "The rc.5 artifact-validator command must retain -RequireInstaller."
}

$storeCommands = @($releaseLines | Where-Object { $_ -match 'validate-store-packaging\.ps1' })
if ($storeCommands.Count -ne 1) {
    Add-Failure "The rc.5 checklist must contain exactly one store-packaging validator command."
}
elseif ($storeCommands[0] -match '(?:^|\s)-RequireInstaller(?:\s|<|$)') {
    Add-Failure "The rc.5 store-packaging command passes unsupported -RequireInstaller."
}

$tokens = $null
$parseErrors = $null
$validatorAst = [System.Management.Automation.Language.Parser]::ParseFile(
    $ValidatorPath,
    [ref]$tokens,
    [ref]$parseErrors
)
if ($parseErrors.Count -gt 0) {
    Add-Failure "Release version validator did not parse: $($parseErrors[0].Message)"
}

$validatorSource = [System.IO.File]::ReadAllText($ValidatorPath)
if ($validatorSource -match '(?i)textExtensions|GetExtension') {
    Add-Failure "Release version scanning must not guess text files from extensions."
}
if ($validatorSource -notmatch 'ReadAllBytes') {
    Add-Failure "Release version scanning must inspect raw bytes for every Git-listed path."
}

$decoderDefinition = $validatorAst.Find({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
            $node.Name -eq "ConvertFrom-ReleaseTextBytes"
    }, $true)
if ($null -eq $decoderDefinition) {
    Add-Failure "Release version validator is missing ConvertFrom-ReleaseTextBytes."
}
else {
    . ([scriptblock]::Create($decoderDefinition.Extent.Text))

    $utf8 = [System.Text.UTF8Encoding]::new($false, $true)
    $staleVersion = "0.1.19-rc." + "4"
    foreach ($extension in @(".nsi", ".hbs", ".js")) {
        $expected = "fixture$extension=$staleVersion"
        $decoded = ConvertFrom-ReleaseTextBytes -Bytes $utf8.GetBytes($expected)
        if ($decoded -cne $expected) {
            Add-Failure "Strict UTF-8 text with extension $extension was not decoded for scanning."
        }
    }

    $binaryResult = ConvertFrom-ReleaseTextBytes -Bytes ([byte[]](0x41, 0x00, 0x42))
    if ($null -ne $binaryResult) {
        Add-Failure "NUL-containing binary content was treated as text."
    }

    $invalidUtf8Result = ConvertFrom-ReleaseTextBytes -Bytes ([byte[]](0xC3, 0x28))
    if ($null -ne $invalidUtf8Result) {
        Add-Failure "Invalid UTF-8 content was treated as text."
    }
}

if ($Failures.Count -gt 0) {
    $details = $Failures | ForEach-Object { " - $_" }
    throw "Release version contract tests failed:`n$($details -join "`n")"
}

Write-Host "Release version contract tests passed."
