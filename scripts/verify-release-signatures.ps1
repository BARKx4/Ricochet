param(
    [Parameter(Mandatory = $true)]
    [string] $Target,
    [string] $OutDir = "dist",
    [string] $ManifestPath,
    [string] $ExpectedWindowsThumbprint,
    [string] $GpgKey,
    [string] $ExpectedMacosIdentity,
    [switch] $RequireProduction
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

try {
    $Manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
} catch {
    throw "Failed to parse release artifact manifest ${ManifestPath}: $($_.Exception.Message)"
}

if ($Manifest.target -ne $Target) {
    throw "Manifest target must be '$Target', found '$($Manifest.target)'."
}

$Artifacts = @($Manifest.artifacts)
if ($Artifacts.Count -eq 0) {
    throw "Manifest artifacts array must not be empty."
}

function Test-JsonProperty {
    param(
        [object] $Object,
        [string] $Name
    )

    return $Object.PSObject.Properties.Name -contains $Name
}

function Normalize-HexId {
    param([string] $Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return ""
    }

    return (($Value -replace "^0x", "") -replace "[^0-9A-Fa-f]", "").ToUpperInvariant()
}

function Resolve-ArtifactPath {
    param([object] $Artifact)

    $path = [string]$Artifact.path
    if ([string]::IsNullOrWhiteSpace($path)) {
        throw "Manifest artifact '$($Artifact.name)' is missing path."
    }
    if ([System.IO.Path]::IsPathRooted($path) -or $path.Contains("/") -or $path.Contains("\") -or $path -eq "." -or $path -eq "..") {
        throw "Manifest artifact '$($Artifact.name)' path must be a top-level relative file name."
    }

    $artifactPath = Join-Path $OutDirPath $path
    if (-not (Test-Path -LiteralPath $artifactPath -PathType Leaf)) {
        throw "Manifest artifact '$path' does not exist in $OutDirPath."
    }
    return (Resolve-Path -LiteralPath $artifactPath).Path
}

function Find-Artifacts {
    param([scriptblock] $Predicate)

    @($Artifacts | Where-Object { & $Predicate $_ })
}

function Get-SingleArtifact {
    param(
        [string] $Description,
        [scriptblock] $Predicate
    )

    $matches = @(Find-Artifacts $Predicate)
    if ($matches.Count -ne 1) {
        $names = @($matches | ForEach-Object { $_.name }) -join ", "
        throw "Expected exactly one $Description artifact for $Target, found $($matches.Count): $names"
    }
    return $matches[0]
}

function Read-SigningReport {
    $report = Get-SingleArtifact "signing report" { param($artifact) $artifact.kind -eq "signing-report" -and $artifact.name -eq "SIGNING-$Target.txt" }
    $path = Resolve-ArtifactPath $report
    return Get-Content -LiteralPath $path -Raw
}

function Assert-SigningReportStatus {
    param(
        [string] $ReportText,
        [string] $Status
    )

    $escaped = [regex]::Escape($Status)
    if ($ReportText -notmatch "(?im)^\s*status\s*=\s*$escaped\s*$") {
        throw "SIGNING-$Target.txt does not record status = $Status."
    }
}

function Assert-NoProductionFallback {
    param([string] $ReportText)

    if (-not $RequireProduction) {
        return
    }

    if ($ReportText -match "(?im)^\s*status\s*=\s*(dry-run|skipped|unsigned-fallback)\s*$") {
        throw "SIGNING-$Target.txt records a non-production signing status in a production verification run."
    }
}

function New-VerificationTempDirectory {
    $root = [System.IO.Path]::GetTempPath()
    $path = Join-Path $root ("ricochet-release-verify-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $path | Out-Null
    return (Resolve-Path -LiteralPath $path).Path
}

function Invoke-ExternalCommand {
    param(
        [string] $CommandName,
        [string[]] $Arguments
    )

    $command = Get-Command $CommandName -ErrorAction Stop
    $rawOutput = @(& $command.Source @Arguments 2>&1)
    $exitCode = $LASTEXITCODE
    $output = @($rawOutput | ForEach-Object { $_.ToString() })
    if ($exitCode -ne 0) {
        throw "Command '$CommandName $($Arguments -join ' ')' failed with exit code ${exitCode}:`n$($output -join "`n")"
    }
    return $output
}

function Assert-WindowsHost {
    if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
        throw "Windows signature verification for $Target must run on Windows."
    }
}

function Assert-LinuxHost {
    if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)) {
        throw "Linux signature verification for $Target must run on Linux."
    }
}

function Assert-MacosHost {
    if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)) {
        throw "macOS signature verification for $Target must run on macOS."
    }
}

function Get-ExtractedFile {
    param(
        [string] $Root,
        [string] $FileName
    )

    $matches = @(Get-ChildItem -LiteralPath $Root -Recurse -File -Filter $FileName)
    if ($matches.Count -ne 1) {
        throw "Expected exactly one $FileName in extracted $Target archive, found $($matches.Count)."
    }
    return $matches[0].FullName
}

function Assert-AuthenticodeSignature {
    param([string] $Path)

    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne "Valid") {
        throw "Authenticode signature for $Path is $($signature.Status): $($signature.StatusMessage)"
    }

    if (-not [string]::IsNullOrWhiteSpace($ExpectedWindowsThumbprint)) {
        if ($null -eq $signature.SignerCertificate) {
            throw "Authenticode signature for $Path did not expose a signer certificate."
        }
        $expected = Normalize-HexId $ExpectedWindowsThumbprint
        $actual = Normalize-HexId $signature.SignerCertificate.Thumbprint
        if ($actual -ne $expected) {
            throw "Authenticode signer thumbprint for $Path is $actual, expected $expected."
        }
    }
}

function Assert-WindowsReleaseSignatures {
    Assert-WindowsHost
    $report = Read-SigningReport
    Assert-NoProductionFallback $report
    Assert-SigningReportStatus $report "signed"

    $zipArtifact = Get-SingleArtifact "Windows portable ZIP" { param($artifact) $artifact.kind -eq "archive" -and $artifact.name -like "*.zip" }
    $zipPath = Resolve-ArtifactPath $zipArtifact
    $tempDir = New-VerificationTempDirectory
    try {
        Expand-Archive -LiteralPath $zipPath -DestinationPath $tempDir
        foreach ($binaryName in @("rco.exe", "rco-gui.exe", "rco-app.exe", "ricochet.exe")) {
            Assert-AuthenticodeSignature (Get-ExtractedFile -Root $tempDir -FileName $binaryName)
        }
    } finally {
        if (Test-Path -LiteralPath $tempDir) {
            Remove-Item -LiteralPath $tempDir -Recurse -Force
        }
    }

    $installerArtifacts = @(Find-Artifacts { param($artifact) $artifact.kind -eq "installer" -and $artifact.name -like "*-setup.exe" })
    if ($RequireProduction -and $installerArtifacts.Count -eq 0) {
        throw "Production Windows verification requires a signed installer artifact."
    }
    foreach ($installer in $installerArtifacts) {
        Assert-AuthenticodeSignature (Resolve-ArtifactPath $installer)
    }
}

function Assert-LinuxReleaseSignatures {
    Assert-LinuxHost
    $report = Read-SigningReport
    Assert-NoProductionFallback $report
    Assert-SigningReportStatus $report "signed"

    if ($RequireProduction -and [string]::IsNullOrWhiteSpace($GpgKey)) {
        throw "Production Linux verification requires RICOCHET_LINUX_GPG_KEY or -GpgKey so the verified signer can be checked."
    }

    $expectedKey = Normalize-HexId $GpgKey
    $artifactsByName = @{}
    foreach ($artifact in $Artifacts) {
        $artifactsByName[[string]$artifact.name] = $artifact
    }

    $signableArtifacts = @(Find-Artifacts { param($artifact) $artifact.kind -in @("archive", "debian-package") })
    foreach ($artifact in $signableArtifacts) {
        if (-not (Test-JsonProperty $artifact "signature")) {
            throw "Linux artifact '$($artifact.name)' is missing a detached signature relationship."
        }
        $signatureName = [string]$artifact.signature
        if (-not $artifactsByName.ContainsKey($signatureName)) {
            throw "Linux artifact '$($artifact.name)' references missing signature '$signatureName'."
        }
        $signatureArtifact = $artifactsByName[$signatureName]
        if ($signatureArtifact.kind -ne "detached-signature") {
            throw "Linux artifact '$($artifact.name)' signature '$signatureName' is not kind detached-signature."
        }

        $artifactPath = Resolve-ArtifactPath $artifact
        $signaturePath = Resolve-ArtifactPath $signatureArtifact
        $output = Invoke-ExternalCommand -CommandName "gpg" -Arguments @("--batch", "--status-fd", "1", "--verify", $signaturePath, $artifactPath)
        $outputText = $output -join "`n"
        $validSigLine = @($output | Where-Object { $_ -match "^\[GNUPG:\]\s+VALIDSIG\s+" } | Select-Object -First 1)
        if ($validSigLine.Count -eq 0) {
            throw "GPG verification for '$($artifact.name)' did not report a VALIDSIG fingerprint."
        }
        $validSigFingerprints = @($validSigLine[0] -split "\s+" |
            Where-Object { $_ -match "^[0-9A-Fa-f]{8,}$" } |
            ForEach-Object { Normalize-HexId $_ })
        if ($validSigFingerprints.Count -eq 0) {
            throw "GPG verification for '$($artifact.name)' did not expose a fingerprint in VALIDSIG output:`n$outputText"
        }
        if ($expectedKey) {
            $matchedExpectedKey = $false
            foreach ($fingerprint in $validSigFingerprints) {
                if ($fingerprint.EndsWith($expectedKey, [System.StringComparison]::OrdinalIgnoreCase)) {
                    $matchedExpectedKey = $true
                    break
                }
            }
            if (-not $matchedExpectedKey) {
                throw "GPG signer for '$($artifact.name)' was one of $($validSigFingerprints -join ', '), expected suffix $expectedKey."
            }
        }
    }
}

function Assert-CodesignedBinary {
    param([string] $Path)

    Invoke-ExternalCommand -CommandName "codesign" -Arguments @("--verify", "--strict", "--verbose=2", $Path) | Out-Null
    $display = Invoke-ExternalCommand -CommandName "codesign" -Arguments @("--display", "--verbose=4", $Path)
    if (-not [string]::IsNullOrWhiteSpace($ExpectedMacosIdentity)) {
        $displayText = $display -join "`n"
        if (-not $displayText.Contains($ExpectedMacosIdentity)) {
            throw "codesign identity for $Path did not include expected identity '$ExpectedMacosIdentity'."
        }
    }
}

function Assert-MacosReleaseSignatures {
    Assert-MacosHost
    $report = Read-SigningReport
    Assert-NoProductionFallback $report
    Assert-SigningReportStatus $report "signed"
    Assert-SigningReportStatus $report "notarized"

    $notaryArtifact = Get-SingleArtifact "macOS notarization report" { param($artifact) ($artifact.kind -eq "notary-report") -or ($artifact.name -eq "NOTARY-$Target.json") }
    $notaryPath = Resolve-ArtifactPath $notaryArtifact
    try {
        $notary = Get-Content -LiteralPath $notaryPath -Raw | ConvertFrom-Json
    } catch {
        throw "Failed to parse macOS notarization report ${notaryPath}: $($_.Exception.Message)"
    }
    $notaryStatus = if (Test-JsonProperty $notary "status") { [string]$notary.status } else { "" }
    if ($notaryStatus -ne "Accepted") {
        throw "macOS notarization report for $Target must have status Accepted, found '$notaryStatus'."
    }
    $notaryId = if (Test-JsonProperty $notary "id") { [string]$notary.id } else { "" }
    if (-not $notaryId) {
        throw "macOS notarization report for $Target is missing submission id."
    }

    $archiveArtifact = Get-SingleArtifact "macOS tarball" { param($artifact) $artifact.kind -eq "archive" -and $artifact.name -like "*.tar.gz" }
    $archivePath = Resolve-ArtifactPath $archiveArtifact
    $tempDir = New-VerificationTempDirectory
    try {
        Invoke-ExternalCommand -CommandName "tar" -Arguments @("-xzf", $archivePath, "-C", $tempDir) | Out-Null
        foreach ($binaryName in @("rco", "rco-gui", "rco-app", "ricochet")) {
            Assert-CodesignedBinary (Get-ExtractedFile -Root $tempDir -FileName $binaryName)
        }
    } finally {
        if (Test-Path -LiteralPath $tempDir) {
            Remove-Item -LiteralPath $tempDir -Recurse -Force
        }
    }
}

switch ($Target) {
    "windows-x64" {
        Assert-WindowsReleaseSignatures
    }
    "linux-x64" {
        Assert-LinuxReleaseSignatures
    }
    { $_ -in @("macos-arm64", "macos-x64") } {
        Assert-MacosReleaseSignatures
    }
    default {
        throw "Unknown release target '$Target'."
    }
}

Write-Host "Verified production release signatures for $Target in $OutDirPath."
