Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$ValidatorPath = Join-Path $Root "scripts\validate-release-version.ps1"
$ReleasePagePath = Join-Path $Root "docs\releases\v0.1.19-rc.6.html"
$HistoricalReleases = @(
    [pscustomobject]@{
        Version = "0.1.19-rc.3"
        PagePath = Join-Path $Root "docs\releases\v0.1.19-rc.3.html"
        Ref = "v0.1.19-rc.3"
        Sha256 = "1e438d503b5f01245f260aac4ec1ca5575fe9f82bee0bc1e63bf7f686d687f96"
    },
    [pscustomobject]@{
        Version = "0.1.19-rc." + "4"
        PagePath = Join-Path $Root ("docs\releases\v0.1.19-rc." + "4.html")
        Ref = "cad7afee286ac2170464c1282a876aca0d587d55"
        Sha256 = "c0b0fe0f86578efbd0a8a70b05302f590562f240ad2290f8438222abd860bad8"
    },
    [pscustomobject]@{
        Version = "0.1.19-rc." + "5"
        PagePath = Join-Path $Root ("docs\releases\v0.1.19-rc." + "5.html")
        Ref = "ab779a732d44cee32bf98f74764342da090cc576"
        Sha256 = "b495f9bdb591ca35c0d66837fcbbd0215b0d91a7e8eeec06828fd0a7a4c7cc8c"
    }
)
$Failures = [System.Collections.Generic.List[string]]::new()

function Add-Failure {
    param([string]$Message)

    [void]$script:Failures.Add($Message)
}

$releasePage = [System.IO.File]::ReadAllText($ReleasePagePath)
$releaseLines = @($releasePage -split "`r?`n")

$packageCommands = @($releaseLines | Where-Object { $_ -match 'package-release\.ps1' })
if ($packageCommands.Count -ne 1 -or $packageCommands[0] -notmatch '(?:^|\s)-RequireInstaller(?:\s|<|$)') {
    Add-Failure "The rc.6 package-release command must retain -RequireInstaller."
}

$artifactCommands = @($releaseLines | Where-Object { $_ -match 'validate-release-artifacts\.ps1' })
if ($artifactCommands.Count -ne 1 -or $artifactCommands[0] -notmatch '(?:^|\s)-RequireInstaller(?:\s|<|$)') {
    Add-Failure "The rc.6 artifact-validator command must retain -RequireInstaller."
}

$storeCommands = @($releaseLines | Where-Object { $_ -match 'validate-store-packaging\.ps1' })
if ($storeCommands.Count -ne 1) {
    Add-Failure "The rc.6 checklist must contain exactly one store-packaging validator command."
}
elseif ($storeCommands[0] -match '(?:^|\s)-RequireInstaller(?:\s|<|$)') {
    Add-Failure "The rc.6 store-packaging command passes unsupported -RequireInstaller."
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
        $node.Extent.Text -match '\$actualHistoricalHash\s*-cne\s*\$historicalRelease\.Sha256'
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
if ($hashFunctionsLoaded) {
    foreach ($historicalRelease in $HistoricalReleases) {
        $historicalReleasePage = [System.IO.File]::ReadAllText($historicalRelease.PagePath)
        $actualHistoricalReleaseSha256 = Get-Sha256 (Normalize-Text $historicalReleasePage)
        if ($actualHistoricalReleaseSha256 -cne $historicalRelease.Sha256) {
            Add-Failure "The immutable $($historicalRelease.Version) page differs from $($historicalRelease.Ref) (expected normalized SHA-256 $($historicalRelease.Sha256); found $actualHistoricalReleaseSha256)."
        }

        $tamperedHistoricalReleaseSha256 = Get-Sha256 (Normalize-Text ($historicalReleasePage + "<!-- contract tamper -->"))
        if ($tamperedHistoricalReleaseSha256 -ceq $historicalRelease.Sha256) {
            Add-Failure "$($historicalRelease.Version) tampering did not change the protected normalized SHA-256."
        }
    }
}

foreach ($historicalRelease in $HistoricalReleases) {
    $relativePath = "docs/releases/v$($historicalRelease.Version).html"
    $pathExpression = if ($historicalRelease.Version -ceq ("0.1.19-rc." + "5")) {
        '\$StaleHistoricalReleasePath'
    } else {
        '"' + [regex]::Escape($relativePath) + '"'
    }
    $protectedRecordPattern = '(?ms)Path\s*=\s*' + $pathExpression + '.*?Sha256\s*=\s*"' + [regex]::Escape($historicalRelease.Sha256) + '"'
    if ($validatorSource -notmatch $protectedRecordPattern) {
        Add-Failure "Release version validator does not bind $relativePath to its tagged normalized SHA-256."
    }
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
    $staleVersion = "0.1.19-rc." + "5"
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
