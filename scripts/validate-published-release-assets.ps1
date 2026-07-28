param(
    [Parameter(Mandatory = $true)]
    [string] $ReleaseJsonPath,
    [Parameter(Mandatory = $true)]
    [string] $AssetDir,
    [Parameter(Mandatory = $true)]
    [string] $ExpectedTag,
    [switch] $RequireDraft,
    [switch] $RequirePublished,
    [switch] $RequirePrerelease,
    [switch] $RequireStable
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($RequireDraft -and $RequirePublished) {
    throw "-RequireDraft and -RequirePublished are mutually exclusive."
}
if ($RequirePrerelease -and $RequireStable) {
    throw "-RequirePrerelease and -RequireStable are mutually exclusive."
}

foreach ($pathRecord in @(
        [pscustomobject]@{ Path = $ReleaseJsonPath; Type = "Leaf"; Label = "Release JSON" },
        [pscustomobject]@{ Path = $AssetDir; Type = "Container"; Label = "Asset directory" }
    )) {
    if (-not (Test-Path -LiteralPath $pathRecord.Path -PathType $pathRecord.Type)) {
        throw "$($pathRecord.Label) was not found: $($pathRecord.Path)"
    }
}

$ReleaseJsonPath = (Resolve-Path -LiteralPath $ReleaseJsonPath).Path
$AssetDir = (Resolve-Path -LiteralPath $AssetDir).Path

try {
    $release = Get-Content -LiteralPath $ReleaseJsonPath -Raw | ConvertFrom-Json
}
catch {
    throw "Could not parse release JSON at ${ReleaseJsonPath}: $($_.Exception.Message)"
}

$errors = [System.Collections.Generic.List[string]]::new()
function Add-ValidationError {
    param([string] $Message)
    $script:errors.Add($Message) | Out-Null
}

if ([string]$release.tag_name -cne $ExpectedTag) {
    Add-ValidationError "Release tag is '$($release.tag_name)', expected '$ExpectedTag'."
}
if ($RequireDraft -and -not [bool]$release.draft) {
    Add-ValidationError "Release must remain a draft during hosted asset validation."
}
if ($RequirePublished -and [bool]$release.draft) {
    Add-ValidationError "Release must be published after draft promotion."
}
if ($RequirePrerelease -and -not [bool]$release.prerelease) {
    Add-ValidationError "Release must be marked as a prerelease."
}
if ($RequireStable -and [bool]$release.prerelease) {
    Add-ValidationError "Stable release must not be marked as a prerelease."
}

$localFiles = @(Get-ChildItem -LiteralPath $AssetDir -File | Sort-Object Name)
$safeNamePattern = '^[A-Za-z0-9][A-Za-z0-9._-]*$'
foreach ($file in $localFiles) {
    if ($file.Name -cnotmatch $safeNamePattern) {
        Add-ValidationError "Local release asset name is not GitHub-safe: $($file.Name)"
    }
}

$apiAssets = @($release.assets)
$localNames = @($localFiles.Name)
$apiNames = @($apiAssets | ForEach-Object { [string]$_.name })
if ($RequireStable) {
    foreach ($requiredName in @("RICOCHET-RELEASE-KEY.asc", "SHA256SUMS.txt.asc")) {
        if ($localNames -cnotcontains $requiredName) {
            Add-ValidationError "Stable release set is missing $requiredName."
        }
    }
}
$localNameSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
$apiNameSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
foreach ($name in $localNames) {
    $localNameSet.Add($name) | Out-Null
}
foreach ($name in $apiNames) {
    if (-not $apiNameSet.Add($name)) {
        Add-ValidationError "GitHub release contains duplicate asset name '$name'."
    }
}
foreach ($name in $localNames) {
    if (-not $apiNameSet.Contains($name)) {
        Add-ValidationError "Release asset name mismatch (local only): $name"
    }
}
foreach ($name in $apiNames) {
    if (-not $localNameSet.Contains($name)) {
        Add-ValidationError "Release asset name mismatch (GitHub only): $name"
    }
}

$apiByName = [System.Collections.Generic.Dictionary[string, object]]::new([System.StringComparer]::Ordinal)
foreach ($asset in $apiAssets) {
    $name = [string]$asset.name
    if (-not $apiByName.ContainsKey($name)) {
        $apiByName.Add($name, $asset)
    }
}

foreach ($file in $localFiles) {
    if (-not $apiByName.ContainsKey($file.Name)) {
        continue
    }
    $asset = $apiByName[$file.Name]
    if ([string]$asset.state -cne "uploaded") {
        Add-ValidationError "GitHub asset '$($file.Name)' state is '$($asset.state)', expected 'uploaded'."
    }
    if ([int64]$asset.size -ne $file.Length -or $file.Length -le 0) {
        Add-ValidationError "GitHub asset '$($file.Name)' size is $($asset.size), local size is $($file.Length)."
    }
    $actualSha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $expectedDigest = "sha256:$actualSha256"
    if ([string]$asset.digest -cne $expectedDigest) {
        Add-ValidationError "GitHub asset '$($file.Name)' digest is '$($asset.digest)', expected '$expectedDigest'."
    }
}

$combinedChecksumsPath = Join-Path $AssetDir "SHA256SUMS.txt"
if (-not (Test-Path -LiteralPath $combinedChecksumsPath -PathType Leaf)) {
    Add-ValidationError "Release set is missing SHA256SUMS.txt."
}
else {
    $checksumEntries = [System.Collections.Generic.Dictionary[string, string]]::new([System.StringComparer]::Ordinal)
    foreach ($line in Get-Content -LiteralPath $combinedChecksumsPath) {
        if ($line -cnotmatch '^(?<hash>[0-9a-f]{64})  (?<name>[^/\\]+)$') {
            Add-ValidationError "Malformed combined checksum line: $line"
            continue
        }
        if ($checksumEntries.ContainsKey($Matches.name)) {
            Add-ValidationError "Combined checksum contains duplicate entry '$($Matches.name)'."
            continue
        }
        $checksumEntries.Add($Matches.name, $Matches.hash)
    }

    $checksummedNames = @($localNames | Where-Object { $_ -cnotin @("SHA256SUMS.txt", "SHA256SUMS.txt.asc") })
    $checksummedNameSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($name in $checksummedNames) {
        $checksummedNameSet.Add($name) | Out-Null
        if (-not $checksumEntries.ContainsKey($name)) {
            Add-ValidationError "Combined checksum coverage mismatch (asset missing from checksum): $name"
        }
    }
    foreach ($name in $checksumEntries.Keys) {
        if (-not $checksummedNameSet.Contains($name)) {
            Add-ValidationError "Combined checksum coverage mismatch (checksum without asset): $name"
        }
    }

    foreach ($name in $checksumEntries.Keys) {
        $path = Join-Path $AssetDir $name
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            continue
        }
        $actualSha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        if ([string]$checksumEntries[$name] -cne $actualSha256) {
            Add-ValidationError "Combined checksum mismatch for '$name'."
        }
    }
}

if ($errors.Count -gt 0) {
    $details = $errors | ForEach-Object { " - $_" }
    throw "Published release asset validation failed:`n$($details -join "`n")"
}

Write-Host "Validated $($localFiles.Count) published release assets for $ExpectedTag in $AssetDir."
