param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$CargoAboutVersion = "0.9.1"
$ValidationOutputRelativeRoot = "target/third-party-notices"
$Generator = Join-Path $Root "scripts/generate-third-party-notices.ps1"

function Assert-CargoAboutVersion {
    $install = "cargo install cargo-about --version $CargoAboutVersion --locked --features cli"
    try {
        $reported = @(& cargo about --version 2>$null)
        $exitCode = $LASTEXITCODE
    }
    catch {
        throw "cargo-about $CargoAboutVersion is required. Install it with: $install`n$($_.Exception.Message)"
    }

    $actual = ($reported -join "`n").Trim()
    if ($exitCode -ne 0 -or $actual -cne "cargo-about $CargoAboutVersion") {
        throw "cargo-about $CargoAboutVersion is required (found '$actual'). Install it with: $install"
    }
}

function Get-FileSha256 {
    param([string]$Path)

    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $sha = [System.Security.Cryptography.SHA256]::Create()
        try {
            $hash = $sha.ComputeHash($stream)
            return -join ($hash | ForEach-Object { $_.ToString("x2") })
        }
        finally {
            $sha.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }
}

Assert-CargoAboutVersion
$generated = & $Generator -PassThru
if ($null -eq $generated -or [string]::IsNullOrWhiteSpace([string]$generated.OutputDirectory)) {
    throw "Third-party notice generator did not return its retained output directory under $ValidationOutputRelativeRoot"
}
if ($generated.PSObject.Properties.Name -notcontains "DependencyIdentities") {
    throw "Third-party notice generator did not return its feature-aware dependency identities"
}

$dependencyIdentities = @($generated.DependencyIdentities)
foreach ($disabledHttp3Crate in @("quinn", "quinn-proto", "quinn-udp", "lru-slab")) {
    $prefix = "$disabledHttp3Crate@"
    if (@($dependencyIdentities | Where-Object { $_.StartsWith($prefix, [System.StringComparison]::Ordinal) }).Count -gt 0) {
        throw "Inactive reqwest HTTP/3 dependency leaked into the supplemental notice union: $disabledHttp3Crate"
    }
}

$failures = [System.Collections.Generic.List[string]]::new()
$snapshots = @(
    [pscustomobject]@{
        Name = "THIRD_PARTY_LICENSES.html"
        Tracked = Join-Path $Root "THIRD_PARTY_LICENSES.html"
        Generated = [string]$generated.LicensesPath
    },
    [pscustomobject]@{
        Name = "THIRD_PARTY_NOTICES.txt"
        Tracked = Join-Path $Root "THIRD_PARTY_NOTICES.txt"
        Generated = [string]$generated.NoticesPath
    }
)

foreach ($snapshot in $snapshots) {
    if (-not (Test-Path -LiteralPath $snapshot.Tracked -PathType Leaf)) {
        [void]$failures.Add("Missing tracked snapshot: $($snapshot.Name)")
        continue
    }
    if (-not (Test-Path -LiteralPath $snapshot.Generated -PathType Leaf)) {
        [void]$failures.Add("Generator did not create: $($snapshot.Name)")
        continue
    }

    $trackedHash = Get-FileSha256 $snapshot.Tracked
    $generatedHash = Get-FileSha256 $snapshot.Generated
    $trackedBytes = [System.IO.File]::ReadAllBytes($snapshot.Tracked)
    $generatedBytes = [System.IO.File]::ReadAllBytes($snapshot.Generated)
    $byteEqual = [System.Collections.StructuralComparisons]::StructuralEqualityComparer.Equals($trackedBytes, $generatedBytes)
    if ($trackedHash -cne $generatedHash -or -not $byteEqual) {
        [void]$failures.Add("$($snapshot.Name) drifted (tracked SHA-256 $trackedHash; generated SHA-256 $generatedHash)")
    }
    else {
        Write-Host "$($snapshot.Name): $trackedHash"
    }
}

if ($failures.Count -gt 0) {
    $details = $failures | ForEach-Object { " - $_" }
    throw "Third-party notice validation failed. Fresh output retained at $($generated.OutputDirectory):`n$($details -join "`n")"
}

Write-Host "Third-party notice validation passed for $($generated.DependencyCount) dependencies and $($generated.NoticeFileCount) notice files."
Write-Host "Fresh validation output retained at: $($generated.OutputDirectory)"
