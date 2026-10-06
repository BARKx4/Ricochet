Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$ValidatorPath = Join-Path $Root "scripts/verify-release-attestations.ps1"
$FixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ("ricochet-attestation-contract-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $FixtureRoot | Out-Null
$SourceDigest = "0123456789abcdef0123456789abcdef01234567"

# The command stub executes in a separate process, so tests exercise the same
# argument passing and exit-status handling as the real GitHub CLI.
$mockScript = Join-Path $FixtureRoot "mock-gh.ps1"
[IO.File]::WriteAllText($mockScript, @'
$values = @($args)
if ($values.Count -ne 11 -or $values[0] -cne "attestation" -or $values[1] -cne "verify") { exit 2 }
if ($values[3] -cne "--repo" -or $values[4] -cne "BARKx4/Ricochet") { exit 2 }
if ($values[5] -cne "--source-ref" -or $values[6] -cne "refs/tags/v1.0.1") { exit 2 }
if ($values[7] -cne "--source-digest" -or $values[8] -cne "0123456789abcdef0123456789abcdef01234567") { exit 2 }
if ($values[9] -cne "--signer-workflow" -or $values[10] -cne "BARKx4/Ricochet/.github/workflows/release.yml") { exit 2 }
if ($values[2] -match "missing-attestation") { exit 1 }
exit 0
'@, [Text.UTF8Encoding]::new($false))

if ($IsWindows) {
    $ghStub = Join-Path $FixtureRoot "gh.cmd"
    [IO.File]::WriteAllText($ghStub, "@echo off`r`npwsh -NoProfile -File `"%~dp0mock-gh.ps1`" %*`r`nexit /b %errorlevel%`r`n", [Text.ASCIIEncoding]::new())
} else {
    $ghStub = Join-Path $FixtureRoot "gh"
    [IO.File]::WriteAllText($ghStub, "#!/usr/bin/env bash`nexec pwsh -NoProfile -File `"`$(dirname `"`$0`")/mock-gh.ps1`" `"`$@`"`n", [Text.UTF8Encoding]::new($false))
    & chmod +x $ghStub
    if ($LASTEXITCODE -ne 0) { throw "Could not make the GitHub CLI stub executable." }
}

function New-Fixture {
    param([string] $Name)

    $dir = Join-Path $FixtureRoot $Name
    New-Item -ItemType Directory -Path $dir | Out-Null
    $asset = Join-Path $dir "ricochet-v1.0.1-windows-x64.zip"
    [IO.File]::WriteAllText($asset, "fixture release asset`n", [Text.UTF8Encoding]::new($false))
    $hash = (Get-FileHash -LiteralPath $asset -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $dir "SHA256SUMS.txt"), "$hash  ricochet-v1.0.1-windows-x64.zip`n", [Text.UTF8Encoding]::new($false))
    return $dir
}

function Invoke-Case {
    param(
        [string] $Name,
        [string] $AssetDir,
        [bool] $ExpectSuccess,
        [string] $ExpectedError,
        [hashtable] $Overrides = @{}
    )

    $parameters = @{
        AssetDir = $AssetDir
        ExpectedTag = "v1.0.1"
        SourceDigest = $SourceDigest
    }
    foreach ($key in $Overrides.Keys) {
        $parameters[$key] = $Overrides[$key]
    }

    $succeeded = $true
    $failure = ""
    try {
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

$oldPath = $env:PATH
try {
    $env:PATH = "$FixtureRoot$([IO.Path]::PathSeparator)$oldPath"
    Invoke-Case "valid inventory and attestations" (New-Fixture "valid") $true ""
    Invoke-Case "missing attestation" (New-Fixture "missing-attestation") $false "GitHub attestation verification failed"
    Invoke-Case "wrong source ref" (New-Fixture "wrong-ref") $false "Attestation source ref must be" @{ SourceRef = "refs/heads/main" }
    Invoke-Case "wrong source digest" (New-Fixture "wrong-digest") $false "GitHub attestation verification failed" @{ SourceDigest = ("0" * 40) }
    Invoke-Case "wrong signer workflow" (New-Fixture "wrong-signer") $false "Attestation signer workflow must be" @{ SignerWorkflow = "BARKx4/Ricochet/.github/workflows/other.yml" }

    $corruptAsset = New-Fixture "corrupt-asset"
    [IO.File]::AppendAllText((Join-Path $corruptAsset "ricochet-v1.0.1-windows-x64.zip"), "tampered")
    Invoke-Case "corrupt asset" $corruptAsset $false "Combined checksum mismatch"

    $corruptChecksum = New-Fixture "corrupt-checksum"
    [IO.File]::WriteAllText((Join-Path $corruptChecksum "SHA256SUMS.txt"), ("0" * 64) + "  ricochet-v1.0.1-windows-x64.zip`n")
    Invoke-Case "corrupt checksum" $corruptChecksum $false "Combined checksum mismatch"

    $missingCoverage = New-Fixture "missing-coverage"
    [IO.File]::WriteAllText((Join-Path $missingCoverage "extra.txt"), "not checksummed")
    Invoke-Case "incomplete checksum coverage" $missingCoverage $false "Combined checksum coverage"
} finally {
    $env:PATH = $oldPath
}

Write-Host "Release attestation contract tests passed."
Write-Host "Retained fixtures at: $FixtureRoot"
