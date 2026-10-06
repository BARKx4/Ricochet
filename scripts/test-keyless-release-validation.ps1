Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$validator = Join-Path $PSScriptRoot "validate-release-artifacts.ps1"
$storeValidator = Join-Path $PSScriptRoot "validate-store-packaging.ps1"
$tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$testRoot = Join-Path $tempRoot ("ricochet-keyless-release-test-" + [guid]::NewGuid().ToString("N"))

function New-TestFixture {
    param(
        [string] $Name,
        [string] $PackageVersion = "1.0.1",
        [string] $Mode = "attestation",
        [string] $Status = "pending-attestation",
        [switch] $OmitReportChecksum
    )

    $outDir = Join-Path $testRoot $Name
    New-Item -ItemType Directory -Path $outDir | Out-Null
    $files = [ordered]@{}
    $files["ricochet-v$PackageVersion-linux-x64.tar.gz"] = "portable fixture"
    $files["ricochet_${PackageVersion}_amd64.deb"] = "debian fixture"
    $files["SIGNING-linux-x64.txt"] = "[detached signatures]`nmode = $Mode`nstatus = $Status`n"
    foreach ($entry in $files.GetEnumerator()) {
        Set-Content -LiteralPath (Join-Path $outDir $entry.Key) -Value $entry.Value -NoNewline
    }

    $artifacts = @()
    $checksumLines = @()
    foreach ($name in $files.Keys) {
        $path = Join-Path $outDir $name
        $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        $kind = switch -Wildcard ($name) {
            "*.tar.gz" { "archive" }
            "*.deb" { "debian-package" }
            default { "signing-report" }
        }
        $artifact = [ordered]@{
            name = $name
            path = $name
            kind = $kind
            size_bytes = (Get-Item -LiteralPath $path).Length
            sha256 = $hash
        }
        if ($kind -ne "signing-report") {
            $artifact.signing_report = "SIGNING-linux-x64.txt"
        }
        $artifacts += $artifact
        if (-not ($OmitReportChecksum -and $kind -eq "signing-report")) {
            $checksumLines += "$hash  $name"
        }
    }
    $checksumPath = Join-Path $outDir "SHA256SUMS-linux-x64.txt"
    Set-Content -LiteralPath $checksumPath -Value $checksumLines
    $artifacts += [ordered]@{
        name = "SHA256SUMS-linux-x64.txt"
        path = "SHA256SUMS-linux-x64.txt"
        kind = "checksums"
        size_bytes = (Get-Item -LiteralPath $checksumPath).Length
        sha256 = (Get-FileHash -LiteralPath $checksumPath -Algorithm SHA256).Hash.ToLowerInvariant()
        signing_report = "SIGNING-linux-x64.txt"
    }
    $manifest = [ordered]@{
        schema = "ricochet.release-artifacts"
        schema_version = 1
        target = "linux-x64"
        package_version = $PackageVersion
        artifacts = $artifacts
    }
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $outDir "ARTIFACTS-linux-x64.json")
    return $outDir
}

function Assert-Rejected {
    param([scriptblock] $Check, [string] $ExpectedMessage)

    try {
        & $Check | Out-Null
    } catch {
        if ($_.Exception.Message -notlike "*$ExpectedMessage*") {
            throw "Expected rejection containing '$ExpectedMessage', got: $($_.Exception.Message)"
        }
        return
    }
    throw "Invalid release fixture was accepted; expected '$ExpectedMessage'."
}

try {
    $good = New-TestFixture "valid-attestation"
    & $validator -Target linux-x64 -OutDir $good -RequireDeb | Out-Null
    & $storeValidator -Target linux-x64 -OutDir $good -RequireProduction -SkipArchiveInspection | Out-Null

    $missingChecksum = New-TestFixture "missing-report-checksum" -OmitReportChecksum
    Assert-Rejected { & $validator -Target linux-x64 -OutDir $missingChecksum -RequireDeb } "Checksum file is missing artifact 'SIGNING-linux-x64.txt'"
    Assert-Rejected { & $storeValidator -Target linux-x64 -OutDir $missingChecksum -RequireProduction -SkipArchiveInspection } "Linux release asset integrity validation failed"

    $skipped = New-TestFixture "skipped-signature" -Mode skip -Status skipped
    Assert-Rejected { & $storeValidator -Target linux-x64 -OutDir $skipped -RequireProduction -SkipArchiveInspection } "Production store packaging must not contain 'status = skipped'"

    $wrongMode = New-TestFixture "wrong-mode" -Mode skip -Status pending-attestation
    Assert-Rejected { & $storeValidator -Target linux-x64 -OutDir $wrongMode -RequireProduction -SkipArchiveInspection } "Production Linux release must use attestation mode"

    $historical = New-TestFixture "historical-release" -PackageVersion "1.0.0"
    Assert-Rejected { & $storeValidator -Target linux-x64 -OutDir $historical -RequireProduction -SkipArchiveInspection } "historical releases require GPG verification"

    Write-Host "Keyless Linux release validation contract tests passed."
} finally {
    $absoluteTestRoot = [System.IO.Path]::GetFullPath($testRoot)
    if (-not $absoluteTestRoot.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to clean a test path outside the temporary directory: $absoluteTestRoot"
    }
    if (Test-Path -LiteralPath $absoluteTestRoot) {
        Remove-Item -LiteralPath $absoluteTestRoot -Recurse -Force
    }
}
