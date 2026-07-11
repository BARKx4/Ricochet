param(
    [string] $Version,
    [string] $Target = "windows-x64",
    [string] $OutDir = "dist",
    [string] $Configuration = "release",
    [string] $NsisPath,
    [switch] $SkipBuild,
    [switch] $RequireInstaller,
    [ValidateSet("auto", "require", "skip", "dry-run")]
    [string] $SigningMode = "auto",
    [string] $SignToolPath,
    [string] $SigningCertificateThumbprint,
    [string] $SigningTimestampUrl
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

function Get-WorkspaceVersion {
    $manifest = Get-Content -LiteralPath (Join-Path $RepoRoot "Cargo.toml") -Raw
    $match = [regex]::Match(
        $manifest,
        '(?ms)^\[workspace\.package\].*?^version\s*=\s*"(?<version>[^"]+)"'
    )
    if (-not $match.Success) {
        throw "Could not find workspace.package version in Cargo.toml"
    }

    $match.Groups["version"].Value
}

function Resolve-Nsis {
    param([string] $RequestedPath)

    if ($RequestedPath) {
        if (-not (Test-Path -LiteralPath $RequestedPath)) {
            throw "NSIS compiler was not found at $RequestedPath"
        }
        return (Resolve-Path -LiteralPath $RequestedPath).Path
    }

    $command = Get-Command makensis.exe -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    $programFilesX86 = [Environment]::GetEnvironmentVariable("ProgramFiles(x86)")
    $programFiles = [Environment]::GetEnvironmentVariable("ProgramFiles")
    $candidates = @()
    if ($programFilesX86) {
        $candidates += (Join-Path $programFilesX86 "NSIS\makensis.exe")
    }
    if ($programFiles) {
        $candidates += (Join-Path $programFiles "NSIS\makensis.exe")
    }
    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path -LiteralPath $candidate)) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }

    return $null
}

function Assert-NewPath {
    param([string] $Path)

    if (Test-Path -LiteralPath $Path) {
        throw "$Path already exists. Choose a fresh -OutDir or remove the existing artifact first."
    }
}

function Compress-ReleaseArchive {
    param(
        [string] $SourcePath,
        [string] $DestinationPath
    )

    $maxAttempts = 5
    for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
        try {
            if (Test-Path -LiteralPath $DestinationPath) {
                Remove-Item -LiteralPath $DestinationPath -Force
            }
            Compress-Archive -Path $SourcePath -DestinationPath $DestinationPath -ErrorAction Stop
            return
        } catch {
            if ($attempt -eq $maxAttempts) {
                throw
            }
            Write-Warning "Compress-Archive failed on attempt $attempt of ${maxAttempts}: $($_.Exception.Message)"
            Start-Sleep -Milliseconds (250 * $attempt)
        }
    }
}

function Copy-ReleaseDirectory {
    param(
        [string] $Source,
        [string] $Destination
    )

    if (-not (Test-Path -LiteralPath $Source -PathType Container)) {
        return
    }

    $repoRootPath = [System.IO.Path]::GetFullPath([string] $RepoRoot).TrimEnd("\", "/")
    $sourcePath = [System.IO.Path]::GetFullPath($Source)
    $repoPrefix = $repoRootPath + [System.IO.Path]::DirectorySeparatorChar
    if (-not $sourcePath.StartsWith($repoPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Release source directory must be inside the repository: $Source"
    }

    $relativeSource = [System.IO.Path]::GetRelativePath($repoRootPath, $sourcePath).Replace("\", "/")
    $trackedFiles = @(& git -C $repoRootPath ls-files -- $relativeSource)
    if ($LASTEXITCODE -ne 0) {
        throw "git ls-files failed while enumerating release source directory $relativeSource"
    }

    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    $relativePrefix = "$relativeSource/"
    foreach ($trackedFile in $trackedFiles) {
        if (-not $trackedFile.StartsWith($relativePrefix, [System.StringComparison]::Ordinal)) {
            continue
        }
        $relativePath = $trackedFile.Substring($relativePrefix.Length)
        $sourceFile = Join-Path $repoRootPath ($trackedFile.Replace("/", [System.IO.Path]::DirectorySeparatorChar))
        $destinationFile = Join-Path $Destination ($relativePath.Replace("/", [System.IO.Path]::DirectorySeparatorChar))
        New-Item -ItemType Directory -Path (Split-Path -Parent $destinationFile) -Force | Out-Null
        Copy-Item -LiteralPath $sourceFile -Destination $destinationFile
    }
}

function Write-NsisInstallManifest {
    param(
        [Parameter(Mandatory = $true)]
        [string] $PackageDir,
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    $packageRoot = [System.IO.Path]::GetFullPath($PackageDir).TrimEnd("\", "/")
    $lines = [System.Collections.Generic.List[string]]::new()

    $toSafeRelativePath = {
        param([string] $FullName)

        $fullPath = [System.IO.Path]::GetFullPath($FullName)
        $packagePrefix = $packageRoot + [System.IO.Path]::DirectorySeparatorChar
        if (-not $fullPath.StartsWith($packagePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "NSIS manifest path is outside the staged package: $FullName"
        }
        $relative = $fullPath.Substring($packagePrefix.Length).Replace("/", "\")
        if (
            [System.IO.Path]::IsPathRooted($relative) -or
            $relative -eq ".." -or
            $relative.StartsWith("..\", [System.StringComparison]::Ordinal) -or
            $relative.IndexOfAny([char[]]@("`r", "`n", '"', '$')) -ge 0
        ) {
            throw "Package path cannot be represented safely in an NSIS manifest: $relative"
        }
        return $relative
    }

    foreach ($file in Get-ChildItem -LiteralPath $packageRoot -Recurse -File -Force | Sort-Object FullName) {
        $relative = & $toSafeRelativePath $file.FullName
        $lines.Add(('Delete "$INSTDIR\{0}"' -f $relative))
    }
    $lines.Add('Delete "$INSTDIR\.ricochet-install-owner"')
    $lines.Add('Delete "$INSTDIR\Uninstall.exe"')

    foreach ($directory in Get-ChildItem -LiteralPath $packageRoot -Recurse -Directory -Force | Sort-Object { $_.FullName.Length } -Descending) {
        $relative = & $toSafeRelativePath $directory.FullName
        $lines.Add(('RMDir "$INSTDIR\{0}"' -f $relative))
    }
    $lines.Add('RMDir "$INSTDIR"')

    $parent = Split-Path -Parent $Path
    if ($parent -and -not (Test-Path -LiteralPath $parent -PathType Container)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    [System.IO.File]::WriteAllLines($Path, $lines, [System.Text.UTF8Encoding]::new($false))
}

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

function Get-SourceInfo {
    $commit = [Environment]::GetEnvironmentVariable("GITHUB_SHA")
    if (-not $commit) {
        $git = Get-Command git -ErrorAction SilentlyContinue
        if ($git) {
            $commit = (& $git.Source -C $RepoRoot rev-parse HEAD 2>$null)
            if ($LASTEXITCODE -ne 0) {
                $commit = $null
            }
        }
    }

    $ref = [Environment]::GetEnvironmentVariable("GITHUB_REF_NAME")
    if (-not $ref) {
        $ref = [Environment]::GetEnvironmentVariable("GITHUB_REF")
    }
    if (-not $ref) {
        $git = Get-Command git -ErrorAction SilentlyContinue
        if ($git) {
            $ref = (& $git.Source -C $RepoRoot rev-parse --abbrev-ref HEAD 2>$null)
            if ($LASTEXITCODE -ne 0) {
                $ref = $null
            }
        }
    }

    [ordered]@{
        commit = if ($commit) { ($commit | Select-Object -First 1).Trim() } else { $null }
        ref = if ($ref) { ($ref | Select-Object -First 1).Trim() } else { $null }
    }
}

function Get-ArtifactKind {
    param([string] $Path)

    $name = Split-Path -Leaf $Path
    if ($name -like "*.zip") { return "archive" }
    if ($name -like "*-setup.exe") { return "installer" }
    if ($name -like "SIGNING-*.txt") { return "signing-report" }
    if ($name -like "SHA256SUMS*.txt") { return "checksums" }
    return "artifact"
}

function Write-ArtifactManifest {
    param(
        [string] $Path,
        [string[]] $Artifacts,
        [string] $SigningReportPath
    )

    $source = Get-SourceInfo
    $signingReportName = Split-Path -Leaf $SigningReportPath
    $entries = foreach ($artifact in $Artifacts) {
        $item = Get-Item -LiteralPath $artifact
        $entry = [ordered]@{
            name = $item.Name
            path = $item.Name
            kind = Get-ArtifactKind -Path $item.FullName
            size_bytes = $item.Length
            sha256 = Get-Sha256Hex -Path $item.FullName
        }
        if ($item.Name -ne $signingReportName) {
            $entry.signing_report = $signingReportName
        }
        [pscustomobject]$entry
    }

    $manifest = [ordered]@{
        schema = "ricochet.release-artifacts"
        schema_version = 1
        target = $Target
        package_version = $Version
        generated_at = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
        source = $source
        artifacts = @($entries)
    }

    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $Path -Encoding UTF8
}

function Resolve-SignTool {
    param([string] $RequestedPath)

    if ($RequestedPath) {
        if (-not (Test-Path -LiteralPath $RequestedPath)) {
            throw "signtool.exe was not found at $RequestedPath"
        }
        return (Resolve-Path -LiteralPath $RequestedPath).Path
    }

    $command = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    $programFilesX86 = [Environment]::GetEnvironmentVariable("ProgramFiles(x86)")
    if ($programFilesX86) {
        $kitsRoot = Join-Path $programFilesX86 "Windows Kits\10\bin"
        if (Test-Path -LiteralPath $kitsRoot) {
            $candidate = Get-ChildItem -LiteralPath $kitsRoot -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
                Where-Object { $_.FullName -match '\\x64\\signtool\.exe$' } |
                Sort-Object FullName -Descending |
                Select-Object -First 1
            if ($candidate) {
                return $candidate.FullName
            }
        }
    }

    return $null
}

function Invoke-WindowsSigning {
    param(
        [string[]] $Paths,
        [string] $Stage
    )

    $mode = $SigningMode.ToLowerInvariant()
    $report = @("[$Stage]")
    if ($mode -eq "skip") {
        $report += "status = skipped"
        $report += "reason = signing mode is skip"
        return $report
    }

    $thumbprint = $SigningCertificateThumbprint
    if (-not $thumbprint) {
        $thumbprint = [Environment]::GetEnvironmentVariable("RICOCHET_WINDOWS_CERT_SHA1")
    }
    $timestampUrl = $SigningTimestampUrl
    if (-not $timestampUrl) {
        $timestampUrl = [Environment]::GetEnvironmentVariable("RICOCHET_WINDOWS_TIMESTAMP_URL")
    }
    if (-not $timestampUrl) {
        $timestampUrl = "http://timestamp.digicert.com"
    }

    $signTool = Resolve-SignTool -RequestedPath $SignToolPath
    $missing = @()
    if (-not $signTool) {
        $missing += "signtool.exe is not available on PATH, in the Windows SDK, or via -SignToolPath"
    }
    if (-not $thumbprint) {
        $missing += "RICOCHET_WINDOWS_CERT_SHA1 or -SigningCertificateThumbprint is not set"
    } else {
        $certPath = "Cert:\CurrentUser\My\$thumbprint"
        if (-not (Test-Path -LiteralPath $certPath)) {
            $missing += "certificate $thumbprint is not installed in Cert:\CurrentUser\My"
        }
    }

    if ($mode -eq "dry-run") {
        $report += "status = dry-run"
        $report += "timestamp_url = $timestampUrl"
        if ($thumbprint) {
            $report += "certificate_thumbprint = $thumbprint"
        }
        foreach ($path in $Paths) {
            $report += "would_sign = $path"
        }
        if ($missing.Count -gt 0) {
            $report += "missing = $($missing -join '; ')"
        }
        return $report
    }

    if ($missing.Count -gt 0) {
        $message = "Windows signing prerequisites missing for ${Stage}: $($missing -join '; ')."
        if ($mode -eq "require") {
            throw "$message Import the production signing certificate into Cert:\CurrentUser\My before running with -SigningMode require."
        }
        Write-Warning "$message Continuing unsigned because -SigningMode auto permits beta/nightly fallback."
        $report += "status = unsigned-fallback"
        $report += "reason = $($missing -join '; ')"
        return $report
    }

    foreach ($path in $Paths) {
        $signArgs = @("sign", "/fd", "SHA256", "/td", "SHA256", "/tr", $timestampUrl, "/sha1", $thumbprint, $path)
        & $signTool @signArgs
        if ($LASTEXITCODE -ne 0) {
            throw "signtool failed for $path with exit code $LASTEXITCODE"
        }
        $report += "signed = $path"
    }
    $report += "status = signed"
    $report += "timestamp_url = $timestampUrl"
    $report += "certificate_thumbprint = $thumbprint"
    return $report
}

if (-not $Version) {
    $Version = Get-WorkspaceVersion
}
$Version = $Version.Trim()
if ($Version.StartsWith("v")) {
    $Version = $Version.Substring(1)
}
if (-not $Version) {
    throw "Release version must not be empty"
}

$PackageName = "ricochet-v$Version-$Target"
$OutDirPath = if ([System.IO.Path]::IsPathRooted($OutDir)) {
    $OutDir
} else {
    Join-Path $RepoRoot $OutDir
}
$PackageDir = Join-Path $OutDirPath $PackageName
$ArchivePath = Join-Path $OutDirPath "$PackageName.zip"
$InstallerPath = Join-Path $OutDirPath "$PackageName-setup.exe"
$ChecksumsPath = Join-Path $OutDirPath "SHA256SUMS-$Target.txt"
$SigningReportPath = Join-Path $OutDirPath "SIGNING-$Target.txt"
$ManifestPath = Join-Path $OutDirPath "ARTIFACTS-$Target.json"
$NsisInstallManifestPath = Join-Path $OutDirPath "$PackageName-installed-files.nsh"
$LegacyRcVersion = "0.1.19-rc." + "4"
$NsisLegacyCleanupPath = Join-Path $RepoRoot "packaging\windows\legacy-v$LegacyRcVersion-files.nsh"

Assert-NewPath $PackageDir
Assert-NewPath $ArchivePath
Assert-NewPath $InstallerPath
Assert-NewPath $ChecksumsPath
Assert-NewPath $SigningReportPath
Assert-NewPath $ManifestPath
Assert-NewPath $NsisInstallManifestPath

if (-not $SkipBuild) {
    Push-Location $RepoRoot
    try {
        cargo build -p ricochet_cli --$Configuration --locked
    } finally {
        Pop-Location
    }
}

$IsWindowsHost = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
    [System.Runtime.InteropServices.OSPlatform]::Windows
)
$ExeSuffix = if ($IsWindowsHost) { ".exe" } else { "" }
$TargetDir = Join-Path $RepoRoot "target\$Configuration"
$Binaries = @(
    (Join-Path $TargetDir "rco$ExeSuffix")
    (Join-Path $TargetDir "rco-gui$ExeSuffix")
    (Join-Path $TargetDir "ricochet$ExeSuffix")
)

foreach ($binary in $Binaries) {
    if (-not (Test-Path -LiteralPath $binary)) {
        throw "Expected release binary was not found: $binary"
    }
}

New-Item -ItemType Directory -Path $PackageDir | Out-Null
foreach ($binary in $Binaries) {
    Copy-Item -LiteralPath $binary -Destination $PackageDir
}
$PackageBinaries = foreach ($binary in $Binaries) {
    Join-Path $PackageDir (Split-Path -Leaf $binary)
}
$SigningReport = @(
    "Ricochet Windows signing report",
    "version = $Version",
    "target = $Target",
    "mode = $SigningMode"
)
$SigningReport += Invoke-WindowsSigning -Paths $PackageBinaries -Stage "staged executables"

Copy-Item -LiteralPath (Join-Path $RepoRoot "README.md") -Destination $PackageDir
Copy-Item -LiteralPath (Join-Path $RepoRoot "LICENSE") -Destination $PackageDir
Copy-Item -LiteralPath (Join-Path $RepoRoot "THIRD_PARTY_LICENSES.html") -Destination $PackageDir
Copy-Item -LiteralPath (Join-Path $RepoRoot "THIRD_PARTY_NOTICES.txt") -Destination $PackageDir
Copy-ReleaseDirectory -Source (Join-Path $RepoRoot "examples") -Destination (Join-Path $PackageDir "examples")
Copy-ReleaseDirectory -Source (Join-Path $RepoRoot "packages") -Destination (Join-Path $PackageDir "packages")
Copy-ReleaseDirectory -Source (Join-Path $RepoRoot "docs\assets") -Destination (Join-Path $PackageDir "docs\assets")
Copy-ReleaseDirectory -Source (Join-Path $RepoRoot "docs\reference") -Destination (Join-Path $PackageDir "docs\reference")
Copy-ReleaseDirectory -Source (Join-Path $RepoRoot "docs\learn") -Destination (Join-Path $PackageDir "docs\learn")
Copy-ReleaseDirectory -Source (Join-Path $RepoRoot "editors\vscode") -Destination (Join-Path $PackageDir "editors\vscode")

$releaseNotes = @"
Ricochet v$Version ($Target)

Commands:
  rco --help
  rco gui examples\webview_ui.rco
  rco package examples\webview_ui.rco --gui --output webview-ui.exe
  ricochet --help

On Windows, run "Ricochet Shell.cmd" to open a command prompt with this folder
temporarily added to PATH.
"@
Set-Content -LiteralPath (Join-Path $PackageDir "RELEASE.txt") -Value $releaseNotes -NoNewline

$shellLauncher = @'
@echo off
set "PATH=%~dp0;%PATH%"
echo Ricochet CLI is ready.
echo.
echo Try:
echo   rco --help
echo   rco new my_app
echo.
cmd /K
'@
Set-Content -LiteralPath (Join-Path $PackageDir "Ricochet Shell.cmd") -Value $shellLauncher -NoNewline

Compress-ReleaseArchive -SourcePath (Join-Path $PackageDir "*") -DestinationPath $ArchivePath

$Assets = @($ArchivePath)
$makensis = Resolve-Nsis -RequestedPath $NsisPath
if ($makensis) {
    $nsisScript = Join-Path $RepoRoot "packaging\windows\ricochet.nsi"
    $license = Join-Path $RepoRoot "LICENSE"
    if (-not (Test-Path -LiteralPath $NsisLegacyCleanupPath -PathType Leaf)) {
        throw "Legacy NSIS cleanup manifest was not found: $NsisLegacyCleanupPath"
    }
    Write-NsisInstallManifest -PackageDir $PackageDir -Path $NsisInstallManifestPath
    & $makensis `
        "/DVERSION=$Version" `
        "/DINPUT_DIR=$PackageDir" `
        "/DOUT_FILE=$InstallerPath" `
        "/DLICENSE_FILE=$license" `
        "/DINSTALL_MANIFEST=$NsisInstallManifestPath" `
        "/DLEGACY_CLEANUP_MANIFEST=$NsisLegacyCleanupPath" `
        $nsisScript
    if ($LASTEXITCODE -ne 0) {
        throw "NSIS installer build failed with exit code $LASTEXITCODE"
    }
    $SigningReport += Invoke-WindowsSigning -Paths @($InstallerPath) -Stage "installer"
    $Assets += $InstallerPath
} elseif ($RequireInstaller) {
    throw "NSIS makensis.exe was not found. Install NSIS or pass -NsisPath."
} else {
    Write-Warning "NSIS makensis.exe was not found; skipping Windows installer."
}

Set-Content -LiteralPath $SigningReportPath -Value $SigningReport
$Assets += $SigningReportPath

$checksumLines = foreach ($asset in $Assets) {
    "{0}  {1}" -f (Get-Sha256Hex -Path $asset), (Split-Path -Leaf $asset)
}
Set-Content -LiteralPath $ChecksumsPath -Value $checksumLines
Write-ArtifactManifest -Path $ManifestPath -Artifacts @($Assets + $ChecksumsPath) -SigningReportPath $SigningReportPath

Write-Host "Release assets written to $OutDirPath"
foreach ($asset in $Assets) {
    Write-Host " - $asset"
}
Write-Host " - $ChecksumsPath"
Write-Host " - $ManifestPath"
