Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$WriterPath = Join-Path $Root "scripts/write-update-channel.ps1"
$ValidatorPath = Join-Path $Root "scripts/validate-update-channel.ps1"
$FixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ("ricochet-update-attestation-contract-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $FixtureRoot | Out-Null
$Utf8 = [Text.UTF8Encoding]::new($false)

function New-Artifact {
    param(
        [string] $DistDir,
        [string] $Name,
        [string] $Kind
    )

    $path = Join-Path $DistDir $Name
    [IO.File]::WriteAllText($path, "fixture $Name`n", $Utf8)
    [pscustomobject][ordered]@{
        name = $Name
        path = $Name
        kind = $Kind
        size_bytes = (Get-Item -LiteralPath $path).Length
        sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

function New-Fixture {
    param(
        [string] $Name,
        [string] $Version = "1.0.1"
    )

    $dist = Join-Path $FixtureRoot $Name
    New-Item -ItemType Directory -Path $dist | Out-Null
    foreach ($target in @("windows-x64", "linux-x64", "macos-arm64", "macos-x64")) {
        $artifacts = [System.Collections.Generic.List[object]]::new()
        $archiveName = "ricochet-v$Version-$target.zip"
        $archive = New-Artifact $dist $archiveName "archive"
        $artifacts.Add($archive)
        if ($target -eq "windows-x64") {
            $artifacts.Add((New-Artifact $dist "ricochet-v$Version-windows-x64-setup.exe" "installer"))
        }
        if ($target -eq "linux-x64") {
            $debian = New-Artifact $dist "ricochet_$($Version)_amd64.deb" "debian-package"
            if ($Version -eq "1.0.0") {
                foreach ($package in @($archive, $debian)) {
                    $signature = New-Artifact $dist "$($package.name).asc" "signature"
                    $package | Add-Member -NotePropertyName signature -NotePropertyValue $signature.name
                    $artifacts.Add($signature)
                }
            }
            $artifacts.Add($debian)
        }
        $artifacts.Add((New-Artifact $dist "SHA256SUMS-$target.txt" "checksums"))
        $artifacts.Add((New-Artifact $dist "SIGNING-$target.txt" "signing-report"))
        $manifest = [ordered]@{
            schema = "ricochet.release-artifacts"
            schema_version = 1
            target = $target
            package_version = $Version
            artifacts = @($artifacts)
        }
        [IO.File]::WriteAllText((Join-Path $dist "ARTIFACTS-$target.json"), ($manifest | ConvertTo-Json -Depth 12), $Utf8)
    }
    & $WriterPath -DistDir $dist -Version $Version -Channel stable -ReleaseTag "v$Version" | Out-Null
    return $dist
}

function Assert-Validation {
    param(
        [string] $Name,
        [string] $DistDir,
        [string] $Version,
        [bool] $ExpectSuccess,
        [string] $ExpectedError = "",
        [switch] $WithoutProduction
    )

    $succeeded = $true
    $failure = ""
    try {
        $parameters = @{ DistDir = $DistDir; Channel = "stable"; Version = $Version }
        if (-not $WithoutProduction) {
            $parameters.RequireProduction = $true
        }
        & $ValidatorPath @parameters | Out-Null
    } catch {
        $succeeded = $false
        $failure = $_.Exception.Message
    }
    if ($succeeded -ne $ExpectSuccess) {
        throw "$Name expected success=$ExpectSuccess, observed success=$succeeded. $failure"
    }
    if (-not $ExpectSuccess -and $failure -notlike "*$ExpectedError*") {
        throw "$Name failed for the wrong reason: $failure"
    }
}

$current = New-Fixture "current"
$currentChannelPath = Join-Path $current "UPDATE-CHANNEL-stable.json"
$currentChannel = Get-Content -LiteralPath $currentChannelPath -Raw | ConvertFrom-Json
foreach ($platform in @($currentChannel.platforms)) {
    $required = @($platform.required_verification)
    if ($required -cnotcontains "github-attestation" -or $required -cnotcontains "sha256" -or $required -ccontains "gpg-detached") {
        throw "1.0.1 $($platform.target) verification requirements are incorrect: $($required -join ', ')."
    }
}
Assert-Validation "current stable channel" $current "1.0.1" $true

$missingAttestation = New-Fixture "missing-attestation"
$path = Join-Path $missingAttestation "UPDATE-CHANNEL-stable.json"
$document = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
$document.platforms[0].required_verification = @("sha256")
[IO.File]::WriteAllText($path, ($document | ConvertTo-Json -Depth 20), $Utf8)
Assert-Validation "missing attestation requirement" $missingAttestation "1.0.1" $false "must require github-attestation"
Assert-Validation "missing attestation requirement without production switch" $missingAttestation "1.0.1" $false "must require github-attestation" -WithoutProduction

$wrongAttestation = New-Fixture "wrong-attestation"
$path = Join-Path $wrongAttestation "UPDATE-CHANNEL-stable.json"
$document = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
$document.platforms[1].required_verification = @("github-attestations", "sha256")
[IO.File]::WriteAllText($path, ($document | ConvertTo-Json -Depth 20), $Utf8)
Assert-Validation "wrong attestation token" $wrongAttestation "1.0.1" $false "must require github-attestation"

$missingSha = New-Fixture "missing-sha"
$path = Join-Path $missingSha "UPDATE-CHANNEL-stable.json"
$document = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
$document.platforms[2].required_verification = @("github-attestation")
[IO.File]::WriteAllText($path, ($document | ConvertTo-Json -Depth 20), $Utf8)
Assert-Validation "missing SHA-256 requirement" $missingSha "1.0.1" $false "must require sha256"

$historical = New-Fixture "historical" -Version "1.0.0"
$historicalChannel = Get-Content -LiteralPath (Join-Path $historical "UPDATE-CHANNEL-stable.json") -Raw | ConvertFrom-Json
$legacyLinux = @($historicalChannel.platforms | Where-Object { $_.target -eq "linux-x64" })[0]
if (@($legacyLinux.required_verification) -cnotcontains "gpg-detached") {
    throw "Historical 1.0.0 Linux update channel lost detached GPG verification."
}
Assert-Validation "historical stable channel" $historical "1.0.0" $true

Write-Host "Update channel attestation contract tests passed."
Write-Host "Retained fixtures at: $FixtureRoot"
