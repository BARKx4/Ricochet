param(
    [string] $Channel = "stable",
    [string] $DistDir = "dist",
    [string] $ChannelPath,
    [string] $Version,
    [string] $CurrentVersion,
    [switch] $RequireProduction
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$DistPath = if ([System.IO.Path]::IsPathRooted($DistDir)) {
    $DistDir
} else {
    Join-Path $RepoRoot $DistDir
}

if (-not (Test-Path -LiteralPath $DistPath -PathType Container)) {
    throw "Release dist directory was not found: $DistPath"
}
$DistPath = (Resolve-Path -LiteralPath $DistPath).Path

if (-not $ChannelPath) {
    $ChannelPath = Join-Path $DistPath "UPDATE-CHANNEL-$Channel.json"
} elseif (-not [System.IO.Path]::IsPathRooted($ChannelPath)) {
    $ChannelPath = Join-Path $RepoRoot $ChannelPath
}

if (-not (Test-Path -LiteralPath $ChannelPath -PathType Leaf)) {
    throw "Update channel metadata was not found: $ChannelPath"
}
$ChannelPath = (Resolve-Path -LiteralPath $ChannelPath).Path

function Get-Sha256Hex {
    param([string] $Path)

    (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Add-Error {
    param(
        [System.Collections.Generic.List[string]] $Errors,
        [string] $Message
    )

    $Errors.Add($Message) | Out-Null
}

function Test-JsonProperty {
    param(
        [object] $Object,
        [string] $Name
    )

    return $Object.PSObject.Properties.Name -contains $Name
}

function Resolve-TopLevelPath {
    param(
        [System.Collections.Generic.List[string]] $Errors,
        [string] $Name,
        [string] $Description
    )

    if ([string]::IsNullOrWhiteSpace($Name)) {
        Add-Error $Errors "$Description is empty."
        return $null
    }
    if ([System.IO.Path]::IsPathRooted($Name) -or $Name.Contains("/") -or $Name.Contains("\") -or $Name -eq "." -or $Name -eq "..") {
        Add-Error $Errors "$Description must be a top-level relative file name: $Name"
        return $null
    }

    $path = Join-Path $DistPath $Name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        Add-Error $Errors "$Description '$Name' was not found in $DistPath."
        return $null
    }
    return (Resolve-Path -LiteralPath $path).Path
}

function ConvertTo-VersionCore {
    param([string] $Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return $null
    }
    $core = $Value.Trim().TrimStart("v").Split("-", 2)[0]
    try {
        [version]$core
    } catch {
        return $null
    }
}

function Test-VersionGreaterThan {
    param(
        [string] $Candidate,
        [string] $Baseline
    )

    $candidateVersion = ConvertTo-VersionCore $Candidate
    $baselineVersion = ConvertTo-VersionCore $Baseline
    if ($null -eq $candidateVersion -or $null -eq $baselineVersion) {
        return $false
    }
    return $candidateVersion -gt $baselineVersion
}

function Assert-ArtifactFile {
    param(
        [System.Collections.Generic.List[string]] $Errors,
        [object] $Artifact,
        [hashtable] $ArtifactByName
    )

    if (-not $Artifact.name) {
        Add-Error $Errors "Channel artifact entry is missing name."
        return
    }
    $name = [string]$Artifact.name
    if ($ArtifactByName.ContainsKey($name)) {
        Add-Error $Errors "Channel artifact '$name' is duplicated in its platform entry."
        return
    }
    $ArtifactByName[$name] = $Artifact

    $path = Resolve-TopLevelPath $Errors $name "Channel artifact"
    if (-not $path) {
        return
    }
    if ($null -eq $Artifact.size_bytes) {
        Add-Error $Errors "Channel artifact '$name' is missing size_bytes."
    } elseif ([int64]$Artifact.size_bytes -ne (Get-Item -LiteralPath $path).Length) {
        Add-Error $Errors "Channel artifact '$name' size_bytes does not match the file on disk."
    }
    if (-not $Artifact.sha256) {
        Add-Error $Errors "Channel artifact '$name' is missing sha256."
    } elseif ($Artifact.sha256 -ne (Get-Sha256Hex -Path $path)) {
        Add-Error $Errors "Channel artifact '$name' sha256 does not match the file on disk."
    }
}

$errors = [System.Collections.Generic.List[string]]::new()

try {
    $channelDocument = Get-Content -LiteralPath $ChannelPath -Raw | ConvertFrom-Json
} catch {
    throw "Failed to parse update channel metadata ${ChannelPath}: $($_.Exception.Message)"
}

if ($channelDocument.schema -ne "ricochet.update-channel") {
    Add-Error $errors "Update channel schema must be ricochet.update-channel, found '$($channelDocument.schema)'."
}
if ($channelDocument.schema_version -ne 1) {
    Add-Error $errors "Update channel schema_version must be 1, found '$($channelDocument.schema_version)'."
}
if ($channelDocument.channel -ne $Channel) {
    Add-Error $errors "Update channel must be '$Channel', found '$($channelDocument.channel)'."
}
if (-not $channelDocument.version) {
    Add-Error $errors "Update channel version must not be empty."
} elseif ($Version -and $channelDocument.version -ne $Version.Trim().TrimStart("v")) {
    Add-Error $errors "Update channel version must be '$($Version.Trim().TrimStart("v"))', found '$($channelDocument.version)'."
}
if ($null -eq $channelDocument.rollout_percent -or [int]$channelDocument.rollout_percent -lt 0 -or [int]$channelDocument.rollout_percent -gt 100) {
    Add-Error $errors "Update channel rollout_percent must be between 0 and 100."
}
if ($CurrentVersion -and -not (Test-VersionGreaterThan -Candidate ([string]$channelDocument.version) -Baseline $CurrentVersion)) {
    Add-Error $errors "Update channel version '$($channelDocument.version)' must be greater than current version '$CurrentVersion'."
}
if (-not (Test-JsonProperty $channelDocument "rollback_policy") -or $channelDocument.rollback_policy.default -ne "reject-older-or-equal") {
    Add-Error $errors "Update channel rollback_policy.default must be reject-older-or-equal."
}

$platforms = @($channelDocument.platforms)
if ($platforms.Count -eq 0) {
    Add-Error $errors "Update channel platforms array must not be empty."
}

$platformByTarget = @{}
foreach ($platform in $platforms) {
    if (-not $platform.target) {
        Add-Error $errors "Update channel platform entry is missing target."
        continue
    }
    $target = [string]$platform.target
    if ($platformByTarget.ContainsKey($target)) {
        Add-Error $errors "Update channel contains duplicate platform target '$target'."
        continue
    }
    $platformByTarget[$target] = $platform

    if (-not (Test-JsonProperty $platform "manifest")) {
        Add-Error $errors "Platform '$target' is missing manifest metadata."
        continue
    }
    $manifestName = [string]$platform.manifest.name
    $manifestPath = Resolve-TopLevelPath $errors $manifestName "Platform '$target' manifest"
    if (-not $manifestPath) {
        continue
    }
    if ([int64]$platform.manifest.size_bytes -ne (Get-Item -LiteralPath $manifestPath).Length) {
        Add-Error $errors "Platform '$target' manifest size_bytes does not match the file on disk."
    }
    if ($platform.manifest.sha256 -ne (Get-Sha256Hex -Path $manifestPath)) {
        Add-Error $errors "Platform '$target' manifest sha256 does not match the file on disk."
    }

    try {
        $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    } catch {
        Add-Error $errors "Failed to parse platform '$target' manifest '$manifestName': $($_.Exception.Message)"
        continue
    }
    if ($manifest.target -ne $target) {
        Add-Error $errors "Platform '$target' manifest target is '$($manifest.target)'."
    }
    if ($manifest.package_version -ne $channelDocument.version) {
        Add-Error $errors "Platform '$target' manifest package_version is '$($manifest.package_version)', expected '$($channelDocument.version)'."
    }

    $artifactByName = @{}
    foreach ($artifact in @($platform.artifacts)) {
        Assert-ArtifactFile $errors $artifact $artifactByName
    }
    foreach ($artifact in @($manifest.artifacts)) {
        if (-not $artifactByName.ContainsKey([string]$artifact.name)) {
            Add-Error $errors "Platform '$target' channel metadata omits manifest artifact '$($artifact.name)'."
        }
    }

    foreach ($requiredName in @($platform.primary_artifact, $platform.checksum_artifact, $platform.signing_report)) {
        if ($requiredName -and -not $artifactByName.ContainsKey([string]$requiredName)) {
            Add-Error $errors "Platform '$target' references missing required artifact '$requiredName'."
        }
    }

    $verification = @($platform.required_verification | ForEach-Object { [string]$_ })
    switch ($target) {
        "windows-x64" {
            if ($RequireProduction -and -not $verification.Contains("authenticode")) {
                Add-Error $errors "Windows update channel entry must require authenticode verification."
            }
            if ($RequireProduction -and -not $platform.installer_artifact) {
                Add-Error $errors "Windows production update channel entry must reference installer_artifact."
            }
        }
        "linux-x64" {
            if ($RequireProduction -and -not $verification.Contains("gpg-detached")) {
                Add-Error $errors "Linux update channel entry must require gpg-detached verification."
            }
            if ($RequireProduction) {
                foreach ($artifact in @($platform.artifacts | Where-Object { $_.kind -in @("archive", "debian-package") })) {
                    if (-not (Test-JsonProperty $artifact "signature") -or -not $artifactByName.ContainsKey([string]$artifact.signature)) {
                        Add-Error $errors "Linux update artifact '$($artifact.name)' must reference an included detached signature."
                    }
                }
            }
        }
        { $_ -in @("macos-arm64", "macos-x64") } {
            foreach ($required in @("codesign", "notarytool-accepted")) {
                if ($RequireProduction -and -not $verification.Contains($required)) {
                    Add-Error $errors "macOS update channel entry '$target' must require $required verification."
                }
            }
            if ($RequireProduction -and -not $platform.notary_report) {
                Add-Error $errors "macOS production update channel entry '$target' must reference notary_report."
            } elseif ($platform.notary_report) {
                $notaryPath = Resolve-TopLevelPath $errors ([string]$platform.notary_report) "Platform '$target' notary report"
                if ($notaryPath) {
                    try {
                        $notary = Get-Content -LiteralPath $notaryPath -Raw | ConvertFrom-Json
                        $notaryStatus = if (Test-JsonProperty $notary "status") { [string]$notary.status } else { "" }
                        if ($RequireProduction -and $notaryStatus -ne "Accepted") {
                            Add-Error $errors "macOS notary report for '$target' must have status Accepted, found '$notaryStatus'."
                        }
                    } catch {
                        Add-Error $errors "Failed to parse macOS notary report for '$target': $($_.Exception.Message)"
                    }
                }
            }
        }
        default {
            Add-Error $errors "Unknown update channel target '$target'."
        }
    }
}

if ($RequireProduction) {
    foreach ($target in @("windows-x64", "linux-x64", "macos-arm64", "macos-x64")) {
        if (-not $platformByTarget.ContainsKey($target)) {
            Add-Error $errors "Production update channel is missing target '$target'."
        }
    }
}

if ($errors.Count -gt 0) {
    $details = $errors | ForEach-Object { " - $_" }
    throw "Update channel validation failed:`n$($details -join "`n")"
}

Write-Host "Validated update channel $Channel for version $($channelDocument.version) in $DistPath."
