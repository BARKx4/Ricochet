Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$ValidatorPath = Join-Path $Root "scripts\validate-release-version.ps1"
$ReleasePagePath = Join-Path $Root "docs\releases\v0.1.19-rc.5.html"
$HistoricalVersion = "0.1.19-rc." + "4"
$HistoricalReleasePagePath = Join-Path $Root "docs\releases\v$HistoricalVersion.html"
$HistoricalReleaseRef = "cad7afee286ac2170464c1282a876aca0d587d55"
$TaggedHistoricalReleaseSha256 = "c0b0fe0f86578efbd0a8a70b05302f590562f240ad2290f8438222abd860bad8"
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

$normalizeDefinition = $validatorAst.Find({
    param($node)
    $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
        $node.Name -eq "Normalize-Text"
}, $true)
$hashDefinition = $validatorAst.Find({
    param($node)
    $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
        $node.Name -eq "Get-Sha256"
}, $true)
$historicalIntegrityGuard = $validatorAst.Find({
    param($node)
    $node -is [System.Management.Automation.Language.IfStatementAst] -and
        $node.Extent.Text -match '\$actualHistoricalHash\s*-cne\s*\$HistoricalReleaseSha256'
}, $true)

$hashFunctionsLoaded = $null -ne $normalizeDefinition -and $null -ne $hashDefinition
if (-not $hashFunctionsLoaded) {
    Add-Failure "Release version validator is missing its historical-page normalization or SHA-256 function."
}
else {
    . ([scriptblock]::Create($normalizeDefinition.Extent.Text))
    . ([scriptblock]::Create($hashDefinition.Extent.Text))
}
if ($null -eq $historicalIntegrityGuard) {
    Add-Failure "Release version validator does not reject a historical-page hash that differs from the protected hash."
}

$validatorSource = [System.IO.File]::ReadAllText($ValidatorPath)
$historicalReleasePage = [System.IO.File]::ReadAllText($HistoricalReleasePagePath)
if ($hashFunctionsLoaded) {
    $actualHistoricalReleaseSha256 = Get-Sha256 (Normalize-Text $historicalReleasePage)
    if ($actualHistoricalReleaseSha256 -cne $TaggedHistoricalReleaseSha256) {
        Add-Failure "The immutable rc.4 page differs from tagged/base $HistoricalReleaseRef (expected normalized SHA-256 $TaggedHistoricalReleaseSha256; found $actualHistoricalReleaseSha256)."
    }

    $tamperedHistoricalReleaseSha256 = Get-Sha256 (Normalize-Text ($historicalReleasePage + "<!-- contract tamper -->"))
    if ($tamperedHistoricalReleaseSha256 -ceq $TaggedHistoricalReleaseSha256) {
        Add-Failure "Historical-page tampering did not change the protected normalized SHA-256."
    }
}

$historicalHashAssignment = [regex]::Match(
    $validatorSource,
    '(?m)^\$HistoricalReleaseSha256\s*=\s*"([0-9a-f]{64})"\s*$'
)
if (-not $historicalHashAssignment.Success) {
    Add-Failure "Release version validator does not declare the protected rc.4 normalized hash."
}
elseif ($historicalHashAssignment.Groups[1].Value -cne $TaggedHistoricalReleaseSha256) {
    Add-Failure "Release version validator blesses rewritten rc.4 hash $($historicalHashAssignment.Groups[1].Value) instead of tagged/base $HistoricalReleaseRef hash $TaggedHistoricalReleaseSha256."
}

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
