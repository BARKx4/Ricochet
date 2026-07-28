param(
    [Parameter(Mandatory = $true)]
    [string] $Version,
    [string] $DistDir = "dist",
    [string] $OutFile,
    [string] $Channel = "stable",
    [string] $ReleaseTag,
    [string] $ReleaseUrl,
    [int] $RolloutPercent = 100,
    [string] $MinimumSupportedVersion
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

if ($RolloutPercent -lt 0 -or $RolloutPercent -gt 100) {
    throw "-RolloutPercent must be between 0 and 100."
}

$Version = $Version.Trim().TrimStart("v")
if (-not $Version) {
    throw "-Version must not be empty."
}

if (-not $ReleaseTag) {
    $ReleaseTag = [Environment]::GetEnvironmentVariable("GITHUB_REF_NAME")
}
if (-not $ReleaseTag) {
    $ReleaseTag = "v$Version"
}

if (-not $ReleaseUrl) {
    $repository = [Environment]::GetEnvironmentVariable("GITHUB_REPOSITORY")
    if ($repository) {
        $ReleaseUrl = "https://github.com/$repository/releases/tag/$ReleaseTag"
    }
}

if (-not $OutFile) {
    $OutFile = "UPDATE-CHANNEL-$Channel.json"
}
$OutPath = if ([System.IO.Path]::IsPathRooted($OutFile)) {
    $OutFile
} else {
    Join-Path $DistPath $OutFile
}
if (Test-Path -LiteralPath $OutPath) {
    throw "$OutPath already exists. Remove it before writing a new update channel."
}

function Get-Sha256Hex {
    param([string] $Path)

    (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-TopLevelArtifactPath {
    param([string] $Name)

    if ([System.IO.Path]::IsPathRooted($Name) -or $Name.Contains("/") -or $Name.Contains("\") -or $Name -eq "." -or $Name -eq "..") {
        throw "Release artifact name must be a top-level file name: $Name"
    }

    $path = Join-Path $DistPath $Name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Release artifact '$Name' was not found in $DistPath."
    }
    return (Resolve-Path -LiteralPath $path).Path
}

function ConvertTo-ArtifactSummary {
    param([object] $Artifact)

    $summary = [ordered]@{
        name = [string]$Artifact.name
        kind = [string]$Artifact.kind
        size_bytes = [int64]$Artifact.size_bytes
        sha256 = [string]$Artifact.sha256
    }
    foreach ($property in @("signing_report", "signature", "signed_artifact")) {
        if ($Artifact.PSObject.Properties.Name -contains $property) {
            $summary[$property] = [string]$Artifact.$property
        }
    }
    [pscustomobject]$summary
}

function Get-FirstArtifactName {
    param(
        [object[]] $Artifacts,
        [scriptblock] $Predicate
    )

    $match = @($Artifacts | Where-Object { & $Predicate $_ } | Select-Object -First 1)
    if ($match.Count -eq 0) {
        return $null
    }
    return [string]$match[0].name
}

function Get-RequiredVerification {
    param(
        [string] $Target,
        [string] $SigningReport
    )

    if ($Channel -ne "stable") {
        return @("sha256")
    }

    $report = Get-Content -LiteralPath (Get-TopLevelArtifactPath -Name $SigningReport) -Raw

    switch ($Target) {
        "windows-x64" {
            if ($report -match '(?im)^\s*status\s*=\s*signed\s*$') {
                @("authenticode", "sha256")
            }
            else {
                @("sha256")
            }
        }
        "linux-x64" { @("gpg-detached", "sha256") }
        { $_ -in @("macos-arm64", "macos-x64") } {
            if ($report -match '(?im)^\s*status\s*=\s*signed\s*$') {
                "codesign"
            }
            if ($report -match '(?im)^\s*status\s*=\s*notarized\s*$') {
                "notarytool-accepted"
            }
            "sha256"
        }
        default { @("sha256") }
    }
}

$manifestFiles = @(Get-ChildItem -LiteralPath $DistPath -File -Filter "ARTIFACTS-*.json" | Sort-Object Name)
if ($manifestFiles.Count -eq 0) {
    throw "No ARTIFACTS-<target>.json manifests were found in $DistPath."
}

$platforms = foreach ($manifestFile in $manifestFiles) {
    try {
        $manifest = Get-Content -LiteralPath $manifestFile.FullName -Raw | ConvertFrom-Json
    } catch {
        throw "Failed to parse $($manifestFile.Name): $($_.Exception.Message)"
    }

    if ($manifest.schema -ne "ricochet.release-artifacts" -or $manifest.schema_version -ne 1) {
        throw "$($manifestFile.Name) is not a ricochet.release-artifacts v1 manifest."
    }
    if ($manifest.package_version -ne $Version) {
        throw "$($manifestFile.Name) package_version is '$($manifest.package_version)', expected '$Version'."
    }

    $target = [string]$manifest.target
    $artifacts = @($manifest.artifacts)
    $primary = Get-FirstArtifactName $artifacts { param($artifact) $artifact.kind -eq "archive" }
    $installer = Get-FirstArtifactName $artifacts { param($artifact) $artifact.kind -eq "installer" }
    $package = Get-FirstArtifactName $artifacts { param($artifact) $artifact.kind -eq "debian-package" }
    $checksums = Get-FirstArtifactName $artifacts { param($artifact) $artifact.kind -eq "checksums" }
    $signingReport = Get-FirstArtifactName $artifacts { param($artifact) $artifact.kind -eq "signing-report" }
    $notaryReport = Get-FirstArtifactName $artifacts { param($artifact) $artifact.kind -eq "notary-report" }

    if (-not $primary) {
        throw "$($manifestFile.Name) does not contain a primary archive artifact."
    }
    if (-not $checksums) {
        throw "$($manifestFile.Name) does not contain a checksum artifact."
    }
    if (-not $signingReport) {
        throw "$($manifestFile.Name) does not contain a signing report artifact."
    }

    foreach ($artifact in $artifacts) {
        $artifactPath = Get-TopLevelArtifactPath -Name ([string]$artifact.path)
        $actualSize = (Get-Item -LiteralPath $artifactPath).Length
        $actualSha = Get-Sha256Hex -Path $artifactPath
        if ([int64]$artifact.size_bytes -ne $actualSize) {
            throw "$($artifact.name) size_bytes is $($artifact.size_bytes), actual size is $actualSize."
        }
        if ($artifact.sha256 -ne $actualSha) {
            throw "$($artifact.name) sha256 is $($artifact.sha256), actual sha256 is $actualSha."
        }
    }

    [pscustomobject][ordered]@{
        target = $target
        manifest = [ordered]@{
            name = $manifestFile.Name
            size_bytes = $manifestFile.Length
            sha256 = Get-Sha256Hex -Path $manifestFile.FullName
        }
        primary_artifact = $primary
        installer_artifact = $installer
        package_artifact = $package
        checksum_artifact = $checksums
        signing_report = $signingReport
        notary_report = $notaryReport
        required_verification = @(Get-RequiredVerification -Target $target -SigningReport $signingReport)
        artifacts = @($artifacts | ForEach-Object { ConvertTo-ArtifactSummary $_ })
    }
}

$channelDocument = [ordered]@{
    schema = "ricochet.update-channel"
    schema_version = 1
    channel = $Channel
    version = $Version
    release_tag = $ReleaseTag
    release_url = if ($ReleaseUrl) { $ReleaseUrl } else { $null }
    generated_at = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    rollout_percent = $RolloutPercent
    minimum_supported_version = if ($MinimumSupportedVersion) { $MinimumSupportedVersion.Trim().TrimStart("v") } else { $null }
    rollback_policy = [ordered]@{
        default = "reject-older-or-equal"
        override = "manual-install-from-versioned-release"
        state_directory = "updater-managed install backup outside Ricochet runtime state"
    }
    platforms = @($platforms)
}

$channelDocument | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $OutPath -Encoding UTF8
Write-Host "Wrote update channel metadata to $OutPath"
