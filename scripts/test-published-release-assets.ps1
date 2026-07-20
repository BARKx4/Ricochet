Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$ValidatorPath = Join-Path $Root "scripts\validate-published-release-assets.ps1"
if (-not (Test-Path -LiteralPath $ValidatorPath -PathType Leaf)) {
    throw "Published release asset validator is missing: scripts/validate-published-release-assets.ps1"
}

$Failures = [System.Collections.Generic.List[string]]::new()
function Add-Failure {
    param([string] $Message)
    $Failures.Add($Message) | Out-Null
}

function New-Fixture {
    param(
        [string] $RootPath,
        [string] $AssetName = "ricochet_1.0.0_amd64.deb",
        [bool] $Draft = $true,
        [bool] $Prerelease = $true,
        [string] $ApiAssetName = $AssetName,
        [string] $ChecksumAssetName = $AssetName,
        [switch] $CorruptDigest,
        [switch] $OmitApiAsset
    )

    $assetDir = Join-Path $RootPath "assets"
    New-Item -ItemType Directory -Path $assetDir | Out-Null
    $assetPath = Join-Path $assetDir $AssetName
    [IO.File]::WriteAllText($assetPath, "fixture artifact bytes`n", [Text.UTF8Encoding]::new($false))
    $assetHash = (Get-FileHash -LiteralPath $assetPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $checksumsPath = Join-Path $assetDir "SHA256SUMS.txt"
    [IO.File]::WriteAllText($checksumsPath, "$assetHash  $ChecksumAssetName`n", [Text.UTF8Encoding]::new($false))
    $checksumsHash = (Get-FileHash -LiteralPath $checksumsPath -Algorithm SHA256).Hash.ToLowerInvariant()

    $assets = [System.Collections.Generic.List[object]]::new()
    if (-not $OmitApiAsset) {
        $assets.Add([pscustomobject][ordered]@{
            name = $ApiAssetName
            state = "uploaded"
            size = (Get-Item -LiteralPath $assetPath).Length
            digest = if ($CorruptDigest) { "sha256:" + ("0" * 64) } else { "sha256:$assetHash" }
        })
    }
    $assets.Add([pscustomobject][ordered]@{
        name = "SHA256SUMS.txt"
        state = "uploaded"
        size = (Get-Item -LiteralPath $checksumsPath).Length
        digest = "sha256:$checksumsHash"
    })

    $release = [pscustomobject][ordered]@{
        tag_name = "v1.0.0"
        draft = $Draft
        prerelease = $Prerelease
        assets = @($assets)
    }
    $releasePath = Join-Path $RootPath "release.json"
    [IO.File]::WriteAllText(
        $releasePath,
        ($release | ConvertTo-Json -Depth 8),
        [Text.UTF8Encoding]::new($false)
    )

    [pscustomobject]@{
        AssetDir = $assetDir
        ReleaseJsonPath = $releasePath
    }
}

function Invoke-Case {
    param(
        [string] $Name,
        [object] $Fixture,
        [bool] $ExpectSuccess,
        [switch] $RequirePublished,
        [switch] $RequireStable
    )

    $succeeded = $true
    try {
        $parameters = @{
            ReleaseJsonPath = $Fixture.ReleaseJsonPath
            AssetDir = $Fixture.AssetDir
            ExpectedTag = "v1.0.0"
        }
        if ($RequireStable) {
            $parameters.RequireStable = $true
        }
        else {
            $parameters.RequirePrerelease = $true
        }
        if ($RequirePublished) {
            $parameters.RequirePublished = $true
        }
        else {
            $parameters.RequireDraft = $true
        }
        & $ValidatorPath @parameters | Out-Null
    }
    catch {
        $succeeded = $false
    }

    if ($succeeded -ne $ExpectSuccess) {
        Add-Failure "$Name expected success=$ExpectSuccess, observed success=$succeeded."
    }
}

$FixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ("ricochet-published-assets-contract-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $FixtureRoot | Out-Null

Invoke-Case "exact draft inventory" (New-Fixture (Join-Path $FixtureRoot "draft-ok")) $true
Invoke-Case "exact published inventory" (New-Fixture (Join-Path $FixtureRoot "published-ok") -Draft $false) $true -RequirePublished
Invoke-Case "exact stable inventory" (New-Fixture (Join-Path $FixtureRoot "stable-ok") -Draft $false -Prerelease $false) $true -RequirePublished -RequireStable
Invoke-Case "stable release marked prerelease" (New-Fixture (Join-Path $FixtureRoot "stable-wrong") -Draft $false) $false -RequirePublished -RequireStable
Invoke-Case "GitHub-renamed asset" (New-Fixture (Join-Path $FixtureRoot "renamed") -ApiAssetName "ricochet_1.0~0_amd64.deb") $false
Invoke-Case "API asset case mismatch" (New-Fixture (Join-Path $FixtureRoot "api-case") -ApiAssetName "Ricochet_1.0.0_amd64.deb") $false
Invoke-Case "checksum asset case mismatch" (New-Fixture (Join-Path $FixtureRoot "checksum-case") -ChecksumAssetName "Ricochet_1.0.0_amd64.deb") $false
Invoke-Case "incorrect API digest" (New-Fixture (Join-Path $FixtureRoot "bad-digest") -CorruptDigest) $false
Invoke-Case "missing API asset" (New-Fixture (Join-Path $FixtureRoot "missing") -OmitApiAsset) $false
Invoke-Case "unsafe local filename" (New-Fixture (Join-Path $FixtureRoot "unsafe") -AssetName "ricochet_1.0~0_amd64.deb") $false
Invoke-Case "wrong draft state" (New-Fixture (Join-Path $FixtureRoot "wrong-state") -Draft $false) $false

if ($Failures.Count -gt 0) {
    $details = $Failures | ForEach-Object { " - $_" }
    throw "Published release asset contract tests failed:`n$($details -join "`n")"
}

Write-Host "Published release asset contract tests passed."
Write-Host "Retained fixtures at: $FixtureRoot"
