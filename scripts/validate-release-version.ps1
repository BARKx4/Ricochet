param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$ExpectedVersion = "0.1.19-rc.5"
$ExpectedTag = "v$ExpectedVersion"
$StaleVersion = "0.1.19-rc." + "4"
$HistoricalReleasePath = "docs/releases/v$StaleVersion.html"
# Normalized UTF-8 SHA-256 of the historical page at cad7afee286ac2170464c1282a876aca0d587d55.
$HistoricalReleaseSha256 = "c0b0fe0f86578efbd0a8a70b05302f590562f240ad2290f8438222abd860bad8"
$Failures = [System.Collections.Generic.List[string]]::new()

function Add-Failure {
    param([string]$Message)

    [void]$script:Failures.Add($Message)
}

function Get-RepoPath {
    param([string]$RelativePath)

    return Join-Path $Root ($RelativePath -replace '/', [System.IO.Path]::DirectorySeparatorChar)
}

function Read-RequiredFile {
    param([string]$RelativePath)

    $path = Get-RepoPath $RelativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        Add-Failure "Missing required file: $RelativePath"
        return $null
    }
    return [System.IO.File]::ReadAllText($path)
}

function Require-Match {
    param(
        [string]$RelativePath,
        [AllowNull()]$Contents,
        [string]$Pattern,
        [string]$Description
    )

    if ($null -ne $Contents -and $Contents -notmatch $Pattern) {
        Add-Failure "$RelativePath must $Description"
    }
}

function Normalize-Text {
    param([string]$Text)

    return $Text.Replace("`r`n", "`n").Replace("`r", "`n")
}

function Get-Sha256 {
    param([string]$Text)

    $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes($Text)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $hash = $sha.ComputeHash($bytes)
        return -join ($hash | ForEach-Object { $_.ToString("x2") })
    }
    finally {
        $sha.Dispose()
    }
}

function ConvertFrom-ReleaseTextBytes {
    param([byte[]]$Bytes)

    if ([System.Array]::IndexOf($Bytes, [byte]0) -ge 0) {
        return $null
    }

    $strictUtf8 = [System.Text.UTF8Encoding]::new($false, $true)
    try {
        return $strictUtf8.GetString($Bytes)
    }
    catch [System.Text.DecoderFallbackException] {
        return $null
    }
}

$workspaceManifest = Read-RequiredFile "Cargo.toml"
if ($null -ne $workspaceManifest) {
    $workspacePackage = [regex]::Match(
        $workspaceManifest,
        '(?ms)^\[workspace\.package\]\s*(.*?)(?=^\[|\z)'
    )
    if (-not $workspacePackage.Success) {
        Add-Failure "Cargo.toml must contain a [workspace.package] section"
    }
    else {
        Require-Match "Cargo.toml [workspace.package]" $workspacePackage.Groups[1].Value ('(?m)^version\s*=\s*"' + [regex]::Escape($ExpectedVersion) + '"\s*$') "set version to $ExpectedVersion"
    }
}

$lock = Read-RequiredFile "Cargo.lock"
if ($null -ne $lock) {
    $packageBlocks = [regex]::Matches($lock, '(?ms)^\[\[package\]\]\s*(.*?)(?=^\[\[package\]\]|\z)')
    foreach ($packageName in @(
        "ricochet_bytecode",
        "ricochet_cli",
        "ricochet_compiler",
        "ricochet_syntax",
        "ricochet_vm",
        "ricochet_web"
    )) {
        $matchingBlocks = @(
            $packageBlocks | Where-Object {
                $_.Groups[1].Value -match ('(?m)^name\s*=\s*"' + [regex]::Escape($packageName) + '"\s*$')
            }
        )
        if ($matchingBlocks.Count -ne 1) {
            Add-Failure "Cargo.lock must contain exactly one package entry for $packageName"
            continue
        }
        Require-Match "Cargo.lock [$packageName]" $matchingBlocks[0].Groups[1].Value ('(?m)^version\s*=\s*"' + [regex]::Escape($ExpectedVersion) + '"\s*$') "set version to $ExpectedVersion"
    }
}

$historicalReleaseFullPath = Get-RepoPath $HistoricalReleasePath
if (-not (Test-Path -LiteralPath $historicalReleaseFullPath -PathType Leaf)) {
    Add-Failure "Missing immutable historical release page: $HistoricalReleasePath"
}
else {
    $actualHistoricalHash = Get-Sha256 (Normalize-Text ([System.IO.File]::ReadAllText($historicalReleaseFullPath)))
    if ($actualHistoricalHash -cne $HistoricalReleaseSha256) {
        Add-Failure "$HistoricalReleasePath must remain content-identical (expected normalized SHA-256 $HistoricalReleaseSha256; found $actualHistoricalHash)"
    }
}

$currentVersionFiles = @(
    "docs/wiki/README.md",
    "docs/wiki/README.html",
    "docs/learn/chapters/33-bytecode-images-and-source-emission.html",
    "docs/learn/chapters/34-packaging-release-and-updates.html",
    "docs/reference/guides/development-release.html",
    "docs/reference/guides/store-packaging.html",
    "docs/reference/guides/updater-workflow.html",
    "docs/wiki/development-release.html",
    "docs/wiki/store-packaging.html",
    "docs/wiki/updater-workflow.html"
)
foreach ($relativePath in $currentVersionFiles) {
    $contents = Read-RequiredFile $relativePath
    Require-Match $relativePath $contents ([regex]::Escape($ExpectedVersion)) "use current release version $ExpectedVersion"
}

$tagExampleFiles = @(
    "docs/wiki/README.md",
    "docs/wiki/README.html",
    "docs/learn/chapters/34-packaging-release-and-updates.html",
    "docs/reference/guides/development-release.html",
    "docs/reference/guides/updater-workflow.html",
    "docs/wiki/development-release.html",
    "docs/wiki/updater-workflow.html"
)
foreach ($relativePath in $tagExampleFiles) {
    $contents = Read-RequiredFile $relativePath
    Require-Match $relativePath $contents ([regex]::Escape($ExpectedTag)) "use current release tag $ExpectedTag"
}

$releasePagePath = "docs/releases/$ExpectedTag.html"
$releasePage = Read-RequiredFile $releasePagePath
Require-Match $releasePagePath $releasePage ([regex]::Escape($ExpectedVersion)) "identify release candidate $ExpectedVersion"
Require-Match $releasePagePath $releasePage ([regex]::Escape($ExpectedTag)) "use tag spelling $ExpectedTag"
foreach ($releaseRequirement in @(
    [pscustomobject]@{ Pattern = '(?i)WebView-only'; Description = 'describe the reconciled WebView-only application surface' },
    [pscustomobject]@{ Pattern = 'Apache-2\.0|Apache License 2\.0'; Description = 'describe Apache license governance' },
    [pscustomobject]@{ Pattern = '(?i)numeric'; Description = 'describe numeric correctness fixes' },
    [pscustomobject]@{ Pattern = '(?i)cycle'; Description = 'describe cycle handling fixes' },
    [pscustomobject]@{ Pattern = '(?i)probe'; Description = 'describe probe fixes' },
    [pscustomobject]@{ Pattern = '(?i)Learn'; Description = 'describe Learn coverage' },
    [pscustomobject]@{ Pattern = '(?i)tracked[- ]source'; Description = 'describe clean tracked-source packaging' },
    [pscustomobject]@{ Pattern = '(?i)prerelease ordering'; Description = 'describe exact prerelease ordering' },
    [pscustomobject]@{ Pattern = 'THIRD_PARTY_LICENSES\.html'; Description = 'name the generated third-party license bundle' },
    [pscustomobject]@{ Pattern = 'THIRD_PARTY_NOTICES\.txt'; Description = 'name the supplemental third-party notice bundle' },
    [pscustomobject]@{ Pattern = '(?i)Windows installer'; Description = 'describe Windows installer verification' },
    [pscustomobject]@{ Pattern = '(?i)\bCI\b'; Description = 'describe CI verification' },
    [pscustomobject]@{ Pattern = '(?i)production signing'; Description = 'state the production-signing boundary' },
    [pscustomobject]@{ Pattern = '(?i)credential'; Description = 'state the external credential requirement' },
    [pscustomobject]@{ Pattern = 'UPDATE-CHANNEL-candidate\.json'; Description = 'identify the candidate update channel' },
    [pscustomobject]@{ Pattern = '(?i)dry-run'; Description = 'retain candidate dry-run signing semantics' }
)) {
    Require-Match $releasePagePath $releasePage $releaseRequirement.Pattern $releaseRequirement.Description
}

$releaseMarkdownPath = "docs/releases/$ExpectedTag.md"
if (Test-Path -LiteralPath (Get-RepoPath $releaseMarkdownPath) -PathType Leaf) {
    Add-Failure "$releaseMarkdownPath must not exist; release notes are published as pre-rendered HTML only"
}

$acceptancePath = "scripts/acceptance.ps1"
$acceptance = Read-RequiredFile $acceptancePath
Require-Match $acceptancePath $acceptance 'validate-release-version\.ps1' 'run release version consistency validation'

$trackedFiles = @(& git -C $Root ls-files --cached --others --exclude-standard)
if ($LASTEXITCODE -ne 0) {
    Add-Failure "git ls-files failed while scanning for stale release-candidate references"
}
else {
    $stalePattern = [regex]::Escape($StaleVersion)
    foreach ($relativePathValue in $trackedFiles) {
        $relativePath = ([string]$relativePathValue).Replace('\', '/')
        if ($relativePath -ceq $HistoricalReleasePath) {
            continue
        }

        $fullPath = Get-RepoPath $relativePath
        if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) {
            continue
        }

        $contents = ConvertFrom-ReleaseTextBytes -Bytes ([System.IO.File]::ReadAllBytes($fullPath))
        if ($null -eq $contents) {
            continue
        }
        if ($contents -match $stalePattern) {
            Add-Failure "$relativePath contains stale current-version reference $StaleVersion"
        }
    }
}

if ($Failures.Count -gt 0) {
    $details = $Failures | ForEach-Object { " - $_" }
    throw "Release version consistency validation failed:`n$($details -join "`n")"
}

Write-Host "Release version consistency validation passed for $ExpectedTag."
