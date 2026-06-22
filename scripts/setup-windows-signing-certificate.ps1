[CmdletBinding()]
param(
    [switch] $Required,
    [string] $EnvFile
)

$ErrorActionPreference = "Stop"

function Normalize-Thumbprint {
    param([string] $Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return ""
    }

    return (($Value -replace "[^0-9A-Fa-f]", "").ToUpperInvariant())
}

function Add-GitHubEnv {
    param(
        [string] $Name,
        [string] $Value
    )

    if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_ENV)) {
        Add-Content -LiteralPath $env:GITHUB_ENV -Value "$Name=$Value" -Encoding utf8
    }
}

function ConvertTo-PowerShellLiteral {
    param([string] $Value)

    return "'" + ($Value -replace "'", "''") + "'"
}

function Add-LocalEnvFile {
    param(
        [string] $Name,
        [string] $Value
    )

    if ([string]::IsNullOrWhiteSpace($EnvFile)) {
        return
    }

    $envFilePath = if ([System.IO.Path]::IsPathRooted($EnvFile)) {
        $EnvFile
    } else {
        Join-Path (Get-Location) $EnvFile
    }
    $envFileParent = Split-Path -Parent $envFilePath
    if (-not [string]::IsNullOrWhiteSpace($envFileParent)) {
        New-Item -ItemType Directory -Path $envFileParent -Force | Out-Null
    }

    Add-Content -LiteralPath $envFilePath -Value ('$env:{0} = {1}' -f $Name, (ConvertTo-PowerShellLiteral $Value)) -Encoding utf8
}

function Add-EnvironmentOutput {
    param(
        [string] $Name,
        [string] $Value
    )

    Add-GitHubEnv -Name $Name -Value $Value
    Add-LocalEnvFile -Name $Name -Value $Value
}

function Add-GitHubOutput {
    param(
        [string] $Name,
        [string] $Value
    )

    if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_OUTPUT)) {
        Add-Content -LiteralPath $env:GITHUB_OUTPUT -Value "$Name=$Value" -Encoding utf8
    }
}

function Assert-SafeTempPath {
    param(
        [string] $RunnerTemp,
        [string] $Candidate
    )

    $runnerRoot = [System.IO.Path]::GetFullPath($RunnerTemp).TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
    $candidateRoot = [System.IO.Path]::GetFullPath($Candidate).TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
    return $candidateRoot.StartsWith($runnerRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)
}

$pfxBase64 = [Environment]::GetEnvironmentVariable("RICOCHET_WINDOWS_CERT_PFX_BASE64")
$pfxPassword = [Environment]::GetEnvironmentVariable("RICOCHET_WINDOWS_CERT_PASSWORD")
$expectedThumbprint = Normalize-Thumbprint ([Environment]::GetEnvironmentVariable("RICOCHET_WINDOWS_CERT_SHA1"))

$missing = @()
if ([string]::IsNullOrWhiteSpace($pfxBase64)) {
    $missing += "RICOCHET_WINDOWS_CERT_PFX_BASE64"
}
if ([string]::IsNullOrEmpty($pfxPassword)) {
    $missing += "RICOCHET_WINDOWS_CERT_PASSWORD"
}

if ($missing.Count -gt 0) {
    $message = "Windows signing certificate import skipped because required secret(s) are missing: $($missing -join ', ')."
    if ($Required) {
        throw $message
    }
    Write-Host $message
    exit 0
}

$runnerTemp = [Environment]::GetEnvironmentVariable("RUNNER_TEMP")
if ([string]::IsNullOrWhiteSpace($runnerTemp)) {
    $runnerTemp = [System.IO.Path]::GetTempPath()
}
$runnerTemp = [System.IO.Path]::GetFullPath($runnerTemp)
$tempDir = Join-Path $runnerTemp ("ricochet-windows-cert-" + [guid]::NewGuid().ToString("N"))

New-Item -ItemType Directory -Path $tempDir | Out-Null
$pfxPath = Join-Path $tempDir "signing-certificate.pfx"

try {
    $pfxBytes = [Convert]::FromBase64String($pfxBase64)
    [System.IO.File]::WriteAllBytes($pfxPath, $pfxBytes)

    $securePassword = ConvertTo-SecureString -String $pfxPassword -AsPlainText -Force
    $importedCertificates = @(Import-PfxCertificate -FilePath $pfxPath -CertStoreLocation "Cert:\CurrentUser\My" -Password $securePassword)
    $privateKeyCertificates = @($importedCertificates | Where-Object { $_.HasPrivateKey })

    if ($privateKeyCertificates.Count -eq 0) {
        throw "The imported Windows signing PFX did not contain a private-key certificate."
    }

    if (-not [string]::IsNullOrWhiteSpace($expectedThumbprint)) {
        $selectedCertificate = $privateKeyCertificates | Where-Object { (Normalize-Thumbprint $_.Thumbprint) -eq $expectedThumbprint } | Select-Object -First 1
        if (-not $selectedCertificate) {
            $importedThumbprints = @($privateKeyCertificates | ForEach-Object { Normalize-Thumbprint $_.Thumbprint })
            throw "Imported Windows signing certificate thumbprint(s) did not match RICOCHET_WINDOWS_CERT_SHA1. Imported: $($importedThumbprints -join ', ')."
        }
    } else {
        $selectedCertificate = $privateKeyCertificates | Select-Object -First 1
    }

    $thumbprint = Normalize-Thumbprint $selectedCertificate.Thumbprint
    Add-EnvironmentOutput -Name "RICOCHET_WINDOWS_CERT_SHA1" -Value $thumbprint
    Add-GitHubOutput -Name "windows_cert_sha1" -Value $thumbprint
    Write-Host "Imported Windows Authenticode certificate into Cert:\CurrentUser\My."
    Write-Host "RICOCHET_WINDOWS_CERT_SHA1=$thumbprint"
} finally {
    if ((Test-Path -LiteralPath $tempDir) -and (Assert-SafeTempPath -RunnerTemp $runnerTemp -Candidate $tempDir)) {
        Remove-Item -LiteralPath $tempDir -Recurse -Force
    }
}
