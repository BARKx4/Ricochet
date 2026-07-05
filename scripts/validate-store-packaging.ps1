param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("windows-x64", "linux-x64", "macos-arm64", "macos-x64")]
    [string] $Target,
    [string] $OutDir = "dist",
    [string] $ManifestPath,
    [string] $PackageVersion,
    [switch] $RequireProduction,
    [switch] $SkipArchiveInspection
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

function Get-Artifact {
    param(
        [object[]] $Artifacts,
        [scriptblock] $Predicate
    )

    $matches = @($Artifacts | Where-Object { & $Predicate $_ })
    if ($matches.Count -eq 0) {
        return $null
    }
    if ($matches.Count -gt 1) {
        throw "Expected one matching artifact for $Target, found $($matches.Count)."
    }
    return $matches[0]
}

function Resolve-ArtifactPath {
    param([object] $Artifact)

    if (-not $Artifact) {
        return $null
    }
    $path = [string]$Artifact.path
    if ([string]::IsNullOrWhiteSpace($path) -or [System.IO.Path]::IsPathRooted($path) -or $path.Contains("/") -or $path.Contains("\") -or $path -eq "." -or $path -eq "..") {
        throw "Manifest artifact '$($Artifact.name)' must use a top-level relative path."
    }
    $fullPath = Join-Path $OutDirPath $path
    if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) {
        throw "Manifest artifact '$($Artifact.name)' was not found in $OutDirPath."
    }
    return (Resolve-Path -LiteralPath $fullPath).Path
}

function Assert-Artifact {
    param(
        [System.Collections.Generic.List[string]] $Errors,
        [object] $Artifact,
        [string] $Description
    )

    if (-not $Artifact) {
        Add-Error $Errors "Missing $Description artifact for $Target."
        return $null
    }
    try {
        return Resolve-ArtifactPath $Artifact
    } catch {
        Add-Error $Errors $_.Exception.Message
        return $null
    }
}

function Assert-SigningReportStatus {
    param(
        [System.Collections.Generic.List[string]] $Errors,
        [string] $SigningReportPath,
        [string[]] $RequiredStatuses
    )

    if (-not $SigningReportPath) {
        return
    }

    $report = Get-Content -LiteralPath $SigningReportPath -Raw
    if ($RequireProduction) {
        foreach ($badStatus in @("status = dry-run", "status = skipped", "status = unsigned-fallback")) {
            if ($report.Contains($badStatus)) {
                Add-Error $Errors "Production store packaging must not contain '$badStatus' in $(Split-Path -Leaf $SigningReportPath)."
            }
        }
    }

    foreach ($requiredStatus in $RequiredStatuses) {
        if (-not $report.Contains($requiredStatus)) {
            Add-Error $Errors "Store packaging signing report for $Target must contain '$requiredStatus'."
        }
    }
}

function Get-ZipEntries {
    param([string] $Path)

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($Path)
    try {
        @($archive.Entries | ForEach-Object { $_.FullName.Replace("\", "/") })
    } finally {
        $archive.Dispose()
    }
}

function Get-TarEntries {
    param([string] $Path)

    $tar = Get-Command tar -ErrorAction SilentlyContinue
    if (-not $tar) {
        throw "tar is required to inspect $Path."
    }

    $entries = & $tar.Source -tzf $Path
    if ($LASTEXITCODE -ne 0) {
        throw "tar failed while inspecting $Path."
    }
    @($entries | ForEach-Object { $_.Replace("\", "/") })
}

function Assert-EntriesContain {
    param(
        [System.Collections.Generic.List[string]] $Errors,
        [string[]] $Entries,
        [string] $ArchiveName,
        [string[]] $RequiredPatterns
    )

    foreach ($pattern in $RequiredPatterns) {
        $found = $false
        foreach ($entry in $Entries) {
            if ($entry -like $pattern) {
                $found = $true
                break
            }
        }
        if (-not $found) {
            Add-Error $Errors "$ArchiveName is missing store packaging entry pattern '$pattern'."
        }
    }
}

function Get-DpkgOutput {
    param(
        [string] $DebPath,
        [string[]] $Arguments
    )

    $dpkgDeb = Get-Command dpkg-deb -ErrorAction SilentlyContinue
    if (-not $dpkgDeb) {
        throw "dpkg-deb is required to inspect $DebPath."
    }

    $output = & $dpkgDeb.Source @Arguments $DebPath
    if ($LASTEXITCODE -ne 0) {
        throw "dpkg-deb failed while inspecting $DebPath."
    }
    return @($output)
}

function Assert-DebContains {
    param(
        [System.Collections.Generic.List[string]] $Errors,
        [string[]] $Contents,
        [string] $Pattern
    )

    if (-not ($Contents | Where-Object { $_ -match $Pattern })) {
        Add-Error $Errors "Debian package is missing required entry matching '$Pattern'."
    }
}

$errors = [System.Collections.Generic.List[string]]::new()

try {
    $manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
} catch {
    throw "Failed to parse release artifact manifest ${ManifestPath}: $($_.Exception.Message)"
}

if ($manifest.schema -ne "ricochet.release-artifacts" -or $manifest.schema_version -ne 1) {
    Add-Error $errors "$ManifestPath must be a ricochet.release-artifacts v1 manifest."
}
if ($manifest.target -ne $Target) {
    Add-Error $errors "Manifest target must be '$Target', found '$($manifest.target)'."
}
if ($PackageVersion -and $manifest.package_version -ne $PackageVersion.Trim().TrimStart("v")) {
    Add-Error $errors "Manifest package_version must be '$($PackageVersion.Trim().TrimStart("v"))', found '$($manifest.package_version)'."
}

$artifacts = @($manifest.artifacts)
$archive = Get-Artifact $artifacts { param($artifact) $artifact.kind -eq "archive" }
$checksums = Get-Artifact $artifacts { param($artifact) $artifact.kind -eq "checksums" }
$signingReport = Get-Artifact $artifacts { param($artifact) $artifact.kind -eq "signing-report" -and $artifact.name -eq "SIGNING-$Target.txt" }

$archivePath = Assert-Artifact $errors $archive "primary archive"
$checksumsPath = Assert-Artifact $errors $checksums "checksum"
$signingReportPath = Assert-Artifact $errors $signingReport "signing report"

if ($checksumsPath) {
    $checksumText = Get-Content -LiteralPath $checksumsPath -Raw
    if ($archive -and -not $checksumText.Contains([string]$archive.name)) {
        Add-Error $errors "Checksum file does not include primary archive '$($archive.name)'."
    }
}

switch ($Target) {
    "windows-x64" {
        $installer = Get-Artifact $artifacts { param($artifact) $artifact.kind -eq "installer" -and $artifact.name -like "*-setup.exe" }
        $installerPath = $null
        if ($installer) {
            $installerPath = Assert-Artifact $errors $installer "Windows installer"
        } elseif ($RequireProduction) {
            Add-Error $errors "Production Windows store packaging requires the signed installer artifact."
        }
        if ($checksumsPath -and $installer -and -not (Get-Content -LiteralPath $checksumsPath -Raw).Contains([string]$installer.name)) {
            Add-Error $errors "Checksum file does not include Windows installer '$($installer.name)'."
        }
        $requiredStatuses = @()
        if ($RequireProduction) {
            $requiredStatuses += "status = signed"
        }
        Assert-SigningReportStatus $errors $signingReportPath $requiredStatuses

        if ($archivePath -and -not $SkipArchiveInspection) {
            try {
                $entries = Get-ZipEntries $archivePath
                Assert-EntriesContain $errors $entries (Split-Path -Leaf $archivePath) @(
                    "rco.exe",
                    "rco-gui.exe",
                    "ricochet.exe",
                    "README.md",
                    "LICENSE",
                    "RELEASE.txt",
                    "Ricochet Shell.cmd",
                    "docs/reference/index.html",
                    "examples/basic-oop.rco",
                    "editors/vscode/*"
                )
            } catch {
                Add-Error $errors $_.Exception.Message
            }
        }
    }
    "linux-x64" {
        $deb = Get-Artifact $artifacts { param($artifact) $artifact.kind -eq "debian-package" -and $artifact.name -like "*.deb" }
        $debPath = Assert-Artifact $errors $deb "Debian package"
        $requiredStatuses = @()
        if ($RequireProduction) {
            $requiredStatuses += "status = signed"
        }
        Assert-SigningReportStatus $errors $signingReportPath $requiredStatuses

        if (-not $SkipArchiveInspection) {
            if ($archivePath) {
                try {
                    $entries = Get-TarEntries $archivePath
                    Assert-EntriesContain $errors $entries (Split-Path -Leaf $archivePath) @(
                        "*/rco",
                        "*/rco-gui",
                        "*/ricochet",
                        "*/install.sh",
                        "*/share/applications/ricochet-repl.desktop",
                        "*/share/icons/hicolor/scalable/apps/ricochet.svg",
                        "*/share/metainfo/today.ricochet.rco.metainfo.xml",
                        "*/docs/reference/index.html",
                        "*/examples/basic-oop.rco"
                    )
                } catch {
                    Add-Error $errors $_.Exception.Message
                }
            }

            if ($debPath) {
                try {
                    $contents = Get-DpkgOutput $debPath @("--contents")
                    Assert-DebContains $errors $contents "usr/bin/rco$"
                    Assert-DebContains $errors $contents "usr/bin/rco-gui$"
                    Assert-DebContains $errors $contents "usr/bin/ricochet$"
                    Assert-DebContains $errors $contents "usr/share/applications/ricochet-repl\.desktop$"
                    Assert-DebContains $errors $contents "usr/share/icons/hicolor/scalable/apps/ricochet\.svg$"
                    Assert-DebContains $errors $contents "usr/share/metainfo/today\.ricochet\.rco\.metainfo\.xml$"
                    Assert-DebContains $errors $contents "usr/share/doc/ricochet/changelog$"

                    $fields = (Get-DpkgOutput $debPath @("--field")) -join "`n"
                    foreach ($requiredField in @(
                            "Package: ricochet",
                            "Section: devel",
                            "Architecture: amd64",
                            "Maintainer: Ricochet <noreply@ricochet.today>",
                            "Depends: libgtk-3-0, libwebkit2gtk-4.1-0"
                        )) {
                        if (-not $fields.Contains($requiredField)) {
                            Add-Error $errors "Debian package control metadata is missing '$requiredField'."
                        }
                    }
                } catch {
                    Add-Error $errors $_.Exception.Message
                }
            }
        }
    }
    { $_ -in @("macos-arm64", "macos-x64") } {
        $requiredStatuses = @()
        if ($RequireProduction) {
            $requiredStatuses += "status = signed"
            $requiredStatuses += "status = notarized"
        }
        Assert-SigningReportStatus $errors $signingReportPath $requiredStatuses

        $notary = Get-Artifact $artifacts { param($artifact) $artifact.kind -eq "notary-report" -and $artifact.name -eq "NOTARY-$Target.json" }
        if ($RequireProduction) {
            $notaryPath = Assert-Artifact $errors $notary "macOS notarization report"
            if ($notaryPath) {
                try {
                    $notaryJson = Get-Content -LiteralPath $notaryPath -Raw | ConvertFrom-Json
                    $notaryStatus = if (Test-JsonProperty $notaryJson "status") { [string]$notaryJson.status } else { "" }
                    if ($notaryStatus -ne "Accepted") {
                        Add-Error $errors "macOS notarization report must have status Accepted, found '$notaryStatus'."
                    }
                } catch {
                    Add-Error $errors "Failed to parse macOS notarization report: $($_.Exception.Message)"
                }
            }
        }

        if ($archivePath -and -not $SkipArchiveInspection) {
            try {
                $entries = Get-TarEntries $archivePath
                Assert-EntriesContain $errors $entries (Split-Path -Leaf $archivePath) @(
                    "*/rco",
                    "*/rco-gui",
                    "*/ricochet",
                    "*/install.sh",
                    "*/README.md",
                    "*/LICENSE",
                    "*/RELEASE.txt",
                    "*/docs/reference/index.html",
                    "*/examples/basic-oop.rco",
                    "*/editors/vscode/*"
                )
            } catch {
                Add-Error $errors $_.Exception.Message
            }
        }
    }
}

if ($errors.Count -gt 0) {
    $details = $errors | ForEach-Object { " - $_" }
    throw "Store packaging validation failed for ${Target}:`n$($details -join "`n")"
}

Write-Host "Validated store-ready packaging for $Target in $OutDirPath."
