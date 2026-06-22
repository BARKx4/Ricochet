param(
    [Parameter(Mandatory = $true)]
    [string] $Target,
    [string] $OutDir = "dist",
    [string] $ManifestPath,
    [string] $PackageVersion,
    [switch] $RequireInstaller,
    [switch] $RequireDeb
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$OutDirPath = if ([System.IO.Path]::IsPathRooted($OutDir)) {
    $OutDir
} else {
    Join-Path $RepoRoot $OutDir
}

if (-not (Test-Path -LiteralPath $OutDirPath -PathType Container)) {
    throw "Release output directory was not found: $OutDirPath"
}
$OutDirPath = (Resolve-Path -LiteralPath $OutDirPath).Path

if (-not $ManifestPath) {
    $ManifestPath = Join-Path $OutDirPath "ARTIFACTS-$Target.json"
} elseif (-not [System.IO.Path]::IsPathRooted($ManifestPath)) {
    $ManifestPath = Join-Path $RepoRoot $ManifestPath
}

if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
    throw "Release artifact manifest was not found: $ManifestPath"
}
$ManifestPath = (Resolve-Path -LiteralPath $ManifestPath).Path

function Get-Sha256Hex {
    param([string] $Path)

    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $sha256 = [System.Security.Cryptography.SHA256]::Create()
        try {
            $hash = $sha256.ComputeHash($stream)
            -join ($hash | ForEach-Object { $_.ToString("x2") })
        } finally {
            $sha256.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
}

function Add-Error {
    param(
        [System.Collections.Generic.List[string]] $Errors,
        [string] $Message
    )

    $Errors.Add($Message) | Out-Null
}

function Test-HasArtifact {
    param(
        [object[]] $Artifacts,
        [scriptblock] $Predicate
    )

    foreach ($artifact in $Artifacts) {
        if (& $Predicate $artifact) {
            return $true
        }
    }
    return $false
}

function Assert-RequiredArtifact {
    param(
        [System.Collections.Generic.List[string]] $Errors,
        [object[]] $Artifacts,
        [string] $Description,
        [scriptblock] $Predicate
    )

    if (-not (Test-HasArtifact -Artifacts $Artifacts -Predicate $Predicate)) {
        Add-Error $Errors "Missing required $Description artifact for target $Target."
    }
}

$errors = [System.Collections.Generic.List[string]]::new()

try {
    $manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
} catch {
    throw "Failed to parse release artifact manifest ${ManifestPath}: $($_.Exception.Message)"
}

if ($manifest.schema -ne "ricochet.release-artifacts") {
    Add-Error $errors "Manifest schema must be ricochet.release-artifacts, found '$($manifest.schema)'."
}
if ($manifest.schema_version -ne 1) {
    Add-Error $errors "Manifest schema_version must be 1, found '$($manifest.schema_version)'."
}
if ($manifest.target -ne $Target) {
    Add-Error $errors "Manifest target must be '$Target', found '$($manifest.target)'."
}
if (-not $manifest.package_version) {
    Add-Error $errors "Manifest package_version must not be empty."
} elseif ($PackageVersion -and $manifest.package_version -ne $PackageVersion) {
    Add-Error $errors "Manifest package_version must be '$PackageVersion', found '$($manifest.package_version)'."
}
if (-not $manifest.artifacts) {
    Add-Error $errors "Manifest artifacts array must not be empty."
}

$artifacts = @($manifest.artifacts)
$artifactByPath = @{}
$artifactByName = @{}

foreach ($artifact in $artifacts) {
    if (-not $artifact.name) {
        Add-Error $errors "Manifest artifact is missing name."
        continue
    }
    if (-not $artifact.path) {
        Add-Error $errors "Manifest artifact '$($artifact.name)' is missing path."
        continue
    }
    if ($artifact.name -ne $artifact.path) {
        Add-Error $errors "Manifest artifact '$($artifact.name)' must use a top-level relative path matching its name."
    }
    if ([System.IO.Path]::IsPathRooted([string]$artifact.path) -or ([string]$artifact.path).Contains("/") -or ([string]$artifact.path).Contains("\") -or ([string]$artifact.path) -eq "." -or ([string]$artifact.path) -eq "..") {
        Add-Error $errors "Manifest artifact '$($artifact.name)' path must be a top-level relative file name."
        continue
    }
    if (-not $artifact.kind) {
        Add-Error $errors "Manifest artifact '$($artifact.name)' is missing kind."
    }
    if ($null -eq $artifact.size_bytes) {
        Add-Error $errors "Manifest artifact '$($artifact.name)' is missing size_bytes."
    }
    if (-not $artifact.sha256) {
        Add-Error $errors "Manifest artifact '$($artifact.name)' is missing sha256."
    } elseif ($artifact.sha256 -notmatch '^[0-9a-f]{64}$') {
        Add-Error $errors "Manifest artifact '$($artifact.name)' sha256 must be 64 lowercase hex characters."
    }

    if ($artifactByPath.ContainsKey([string]$artifact.path)) {
        Add-Error $errors "Manifest contains duplicate artifact path '$($artifact.path)'."
    } else {
        $artifactByPath[[string]$artifact.path] = $artifact
    }
    if ($artifactByName.ContainsKey([string]$artifact.name)) {
        Add-Error $errors "Manifest contains duplicate artifact name '$($artifact.name)'."
    } else {
        $artifactByName[[string]$artifact.name] = $artifact
    }

    $artifactPath = Join-Path $OutDirPath ([string]$artifact.path)
    if (-not (Test-Path -LiteralPath $artifactPath -PathType Leaf)) {
        Add-Error $errors "Manifest artifact '$($artifact.path)' does not exist in $OutDirPath."
        continue
    }

    $item = Get-Item -LiteralPath $artifactPath
    if ($null -ne $artifact.size_bytes -and [int64]$artifact.size_bytes -ne $item.Length) {
        Add-Error $errors "Manifest artifact '$($artifact.path)' size_bytes is $($artifact.size_bytes), actual size is $($item.Length)."
    }
    if ($artifact.sha256) {
        $actualSha256 = Get-Sha256Hex -Path $item.FullName
        if ($artifact.sha256 -ne $actualSha256) {
            Add-Error $errors "Manifest artifact '$($artifact.path)' sha256 is $($artifact.sha256), actual sha256 is $actualSha256."
        }
    }
}

$manifestFileName = Split-Path -Leaf $ManifestPath
$topLevelFiles = @(Get-ChildItem -LiteralPath $OutDirPath -File | Where-Object { $_.Name -ne $manifestFileName })
foreach ($file in $topLevelFiles) {
    if (-not $artifactByPath.ContainsKey($file.Name)) {
        Add-Error $errors "Top-level release file '$($file.Name)' is not represented in $manifestFileName."
    }
}

$signingReports = @($artifacts | Where-Object { $_.kind -eq "signing-report" })
foreach ($artifact in $artifacts) {
    if ($artifact.PSObject.Properties.Name -contains "signing_report") {
        if (-not $artifactByName.ContainsKey([string]$artifact.signing_report)) {
            Add-Error $errors "Manifest artifact '$($artifact.name)' references missing signing_report '$($artifact.signing_report)'."
        } elseif ($artifactByName[[string]$artifact.signing_report].kind -ne "signing-report") {
            Add-Error $errors "Manifest artifact '$($artifact.name)' signing_report '$($artifact.signing_report)' is not kind signing-report."
        }
    } elseif ($artifact.kind -ne "signing-report" -and $signingReports.Count -gt 0) {
        Add-Error $errors "Manifest artifact '$($artifact.name)' is missing signing_report relationship."
    }

    if ($artifact.PSObject.Properties.Name -contains "signature") {
        if (-not $artifactByName.ContainsKey([string]$artifact.signature)) {
            Add-Error $errors "Manifest artifact '$($artifact.name)' references missing signature '$($artifact.signature)'."
        } elseif ($artifactByName[[string]$artifact.signature].kind -ne "detached-signature") {
            Add-Error $errors "Manifest artifact '$($artifact.name)' signature '$($artifact.signature)' is not kind detached-signature."
        }
    }
    if ($artifact.PSObject.Properties.Name -contains "signed_artifact") {
        if (-not $artifactByName.ContainsKey([string]$artifact.signed_artifact)) {
            Add-Error $errors "Manifest detached signature '$($artifact.name)' references missing signed_artifact '$($artifact.signed_artifact)'."
        }
    }
}

Assert-RequiredArtifact $errors $artifacts "archive" { param($artifact) $artifact.kind -eq "archive" -and $artifact.name -like "ricochet-v*-$Target.*" }
Assert-RequiredArtifact $errors $artifacts "signing report" { param($artifact) $artifact.kind -eq "signing-report" -and $artifact.name -eq "SIGNING-$Target.txt" }

switch ($Target) {
    "windows-x64" {
        Assert-RequiredArtifact $errors $artifacts "portable ZIP" { param($artifact) $artifact.kind -eq "archive" -and $artifact.name -like "*.zip" }
        Assert-RequiredArtifact $errors $artifacts "checksum" { param($artifact) $artifact.kind -eq "checksums" -and $artifact.name -eq "SHA256SUMS.txt" }
        if ($RequireInstaller) {
            Assert-RequiredArtifact $errors $artifacts "installer" { param($artifact) $artifact.kind -eq "installer" -and $artifact.name -like "*-setup.exe" }
        }
    }
    "linux-x64" {
        Assert-RequiredArtifact $errors $artifacts "portable tarball" { param($artifact) $artifact.kind -eq "archive" -and $artifact.name -like "*.tar.gz" }
        Assert-RequiredArtifact $errors $artifacts "checksum" { param($artifact) $artifact.kind -eq "checksums" -and $artifact.name -eq "SHA256SUMS-$Target.txt" }
        if ($RequireDeb) {
            Assert-RequiredArtifact $errors $artifacts "Debian package" { param($artifact) $artifact.kind -eq "debian-package" -and $artifact.name -like "*.deb" }
        }
    }
    { $_ -in @("macos-arm64", "macos-x64") } {
        Assert-RequiredArtifact $errors $artifacts "portable tarball" { param($artifact) $artifact.kind -eq "archive" -and $artifact.name -like "*.tar.gz" }
        Assert-RequiredArtifact $errors $artifacts "checksum" { param($artifact) $artifact.kind -eq "checksums" -and $artifact.name -eq "SHA256SUMS-$Target.txt" }
    }
    default {
        Add-Error $errors "Unknown release target '$Target'."
    }
}

$ascFiles = @(Get-ChildItem -LiteralPath $OutDirPath -File -Filter "*.asc")
foreach ($ascFile in $ascFiles) {
    if (-not $artifactByName.ContainsKey($ascFile.Name)) {
        Add-Error $errors "Detached signature '$($ascFile.Name)' is not represented in $manifestFileName."
        continue
    }
    $signatureEntry = $artifactByName[$ascFile.Name]
    if ($signatureEntry.kind -ne "detached-signature") {
        Add-Error $errors "Detached signature '$($ascFile.Name)' must be kind detached-signature."
    }
    $signedArtifactName = $ascFile.Name.Substring(0, $ascFile.Name.Length - 4)
    if (-not $artifactByName.ContainsKey($signedArtifactName)) {
        Add-Error $errors "Detached signature '$($ascFile.Name)' has no matching signed artifact '$signedArtifactName'."
    } else {
        $signedEntry = $artifactByName[$signedArtifactName]
        if (-not ($signatureEntry.PSObject.Properties.Name -contains "signed_artifact") -or $signatureEntry.signed_artifact -ne $signedArtifactName) {
            Add-Error $errors "Detached signature '$($ascFile.Name)' must reference signed_artifact '$signedArtifactName'."
        }
        if (-not ($signedEntry.PSObject.Properties.Name -contains "signature") -or $signedEntry.signature -ne $ascFile.Name) {
            Add-Error $errors "Signed artifact '$signedArtifactName' must reference signature '$($ascFile.Name)'."
        }
    }
}

if ($errors.Count -gt 0) {
    $details = $errors | ForEach-Object { " - $_" }
    throw "Release artifact manifest validation failed for ${Target}:`n$($details -join "`n")"
}

Write-Host "Validated $($artifacts.Count) release artifact manifest entries for $Target in $OutDirPath."
