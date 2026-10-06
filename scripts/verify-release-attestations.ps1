param(
    [Parameter(Mandatory = $true)]
    [string] $AssetDir,
    [Parameter(Mandatory = $true)]
    [string] $ExpectedTag,
    [Parameter(Mandatory = $true)]
    [string] $SourceDigest,
    [string] $SourceRepository = "BARKx4/Ricochet",
    [string] $SourceRef,
    [string] $SignerWorkflow
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($SourceRepository -cne "BARKx4/Ricochet") {
    throw "Attestation source repository must be BARKx4/Ricochet."
}
if ($ExpectedTag -cnotmatch '^v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$') {
    throw "Attestation source ref requires a version tag, found '$ExpectedTag'."
}
$expectedSourceRef = "refs/tags/$ExpectedTag"
if (-not $SourceRef) {
    $SourceRef = $expectedSourceRef
}
if ($SourceRef -cne $expectedSourceRef) {
    throw "Attestation source ref must be $expectedSourceRef."
}
if ($SourceDigest -cnotmatch '^[0-9a-f]{40}$') {
    throw "Attestation source digest must be a 40-character lowercase Git commit SHA."
}
$expectedSignerWorkflow = "$SourceRepository/.github/workflows/release.yml"
if (-not $SignerWorkflow) {
    $SignerWorkflow = $expectedSignerWorkflow
}
if ($SignerWorkflow -cne $expectedSignerWorkflow) {
    throw "Attestation signer workflow must be $expectedSignerWorkflow."
}
if (-not (Test-Path -LiteralPath $AssetDir -PathType Container)) {
    throw "Release asset directory was not found: $AssetDir"
}
$AssetDir = (Resolve-Path -LiteralPath $AssetDir).Path
if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    throw "GitHub CLI is required to verify release attestations."
}

$files = @(Get-ChildItem -LiteralPath $AssetDir -File | Sort-Object Name)
if ($files.Count -eq 0) {
    throw "Release asset directory is empty: $AssetDir"
}
$safeNamePattern = '^[A-Za-z0-9][A-Za-z0-9._-]*$'
foreach ($file in $files) {
    if ($file.Name -cnotmatch $safeNamePattern) {
        throw "Release asset name is not GitHub-safe: $($file.Name)"
    }
    if (($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Release asset must not be a symbolic link or reparse point: $($file.Name)"
    }
}

$checksumsPath = Join-Path $AssetDir "SHA256SUMS.txt"
if (-not (Test-Path -LiteralPath $checksumsPath -PathType Leaf)) {
    throw "Release set is missing SHA256SUMS.txt."
}
$entries = [System.Collections.Generic.Dictionary[string, string]]::new([System.StringComparer]::Ordinal)
foreach ($line in Get-Content -LiteralPath $checksumsPath) {
    if ($line -cnotmatch '^(?<hash>[0-9a-f]{64})  (?<name>[A-Za-z0-9][A-Za-z0-9._-]*)$') {
        throw "Malformed combined checksum line: $line"
    }
    if ($Matches.name -ceq "SHA256SUMS.txt") {
        throw "Combined checksum inventory must not include itself."
    }
    if (-not $entries.TryAdd($Matches.name, $Matches.hash)) {
        throw "Combined checksum contains duplicate entry '$($Matches.name)'."
    }
}

$expectedNames = @($files | Where-Object { $_.Name -cne "SHA256SUMS.txt" } | ForEach-Object { $_.Name })
if ($entries.Count -ne $expectedNames.Count) {
    throw "Combined checksum coverage does not match the release asset set."
}
foreach ($file in $files) {
    if ($file.Name -ceq "SHA256SUMS.txt") {
        continue
    }
    if (-not $entries.ContainsKey($file.Name)) {
        throw "Combined checksum omits release asset '$($file.Name)'."
    }
    $actualHash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($entries[$file.Name] -cne $actualHash) {
        throw "Combined checksum mismatch for '$($file.Name)'."
    }
}

foreach ($file in $files) {
    & gh attestation verify $file.FullName `
        --repo $SourceRepository `
        --source-ref $SourceRef `
        --source-digest $SourceDigest `
        --signer-workflow $SignerWorkflow | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "GitHub attestation verification failed for '$($file.Name)' with source $SourceRepository at $SourceRef ($SourceDigest) and signer $SignerWorkflow."
    }
}

Write-Host "Verified SHA-256 inventory and GitHub attestations for $($files.Count) release assets at $SourceRef ($SourceDigest)."
