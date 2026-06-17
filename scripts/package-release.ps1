param(
    [string] $Version,
    [string] $Target = "windows-x64",
    [string] $OutDir = "dist",
    [string] $Configuration = "release",
    [string] $NsisPath,
    [switch] $SkipBuild,
    [switch] $RequireInstaller
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

function Copy-ReleaseDirectory {
    param(
        [string] $Source,
        [string] $Destination
    )

    if (Test-Path -LiteralPath $Source) {
        New-Item -ItemType Directory -Path (Split-Path -Parent $Destination) -Force | Out-Null
        Copy-Item -LiteralPath $Source -Destination $Destination -Recurse
    }
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
$ChecksumsPath = Join-Path $OutDirPath "SHA256SUMS.txt"

Assert-NewPath $PackageDir
Assert-NewPath $ArchivePath
Assert-NewPath $InstallerPath
Assert-NewPath $ChecksumsPath

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

Copy-Item -LiteralPath (Join-Path $RepoRoot "README.md") -Destination $PackageDir
Copy-Item -LiteralPath (Join-Path $RepoRoot "LICENSE") -Destination $PackageDir
Copy-ReleaseDirectory -Source (Join-Path $RepoRoot "examples") -Destination (Join-Path $PackageDir "examples")
Copy-ReleaseDirectory -Source (Join-Path $RepoRoot "docs\assets") -Destination (Join-Path $PackageDir "docs\assets")
Copy-ReleaseDirectory -Source (Join-Path $RepoRoot "docs\reference") -Destination (Join-Path $PackageDir "docs\reference")
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

Compress-Archive -Path (Join-Path $PackageDir "*") -DestinationPath $ArchivePath

$Assets = @($ArchivePath)
$makensis = Resolve-Nsis -RequestedPath $NsisPath
if ($makensis) {
    $nsisScript = Join-Path $RepoRoot "packaging\windows\ricochet.nsi"
    $license = Join-Path $RepoRoot "LICENSE"
    & $makensis `
        "/DVERSION=$Version" `
        "/DINPUT_DIR=$PackageDir" `
        "/DOUT_FILE=$InstallerPath" `
        "/DLICENSE_FILE=$license" `
        $nsisScript
    if ($LASTEXITCODE -ne 0) {
        throw "NSIS installer build failed with exit code $LASTEXITCODE"
    }
    $Assets += $InstallerPath
} elseif ($RequireInstaller) {
    throw "NSIS makensis.exe was not found. Install NSIS or pass -NsisPath."
} else {
    Write-Warning "NSIS makensis.exe was not found; skipping Windows installer."
}

$checksumLines = foreach ($asset in $Assets) {
    "{0}  {1}" -f (Get-Sha256Hex -Path $asset), (Split-Path -Leaf $asset)
}
Set-Content -LiteralPath $ChecksumsPath -Value $checksumLines

Write-Host "Release assets written to $OutDirPath"
foreach ($asset in $Assets) {
    Write-Host " - $asset"
}
Write-Host " - $ChecksumsPath"
