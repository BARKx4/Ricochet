param(
    [switch]$PassThru
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$CargoAboutVersion = "0.9.1"
$Targets = @(
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin"
)
$NoticeFilePatterns = @("NOTICE*", "COPYRIGHT*", "AUTHORS*", "PATENTS*")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$Utf8Strict = [System.Text.UTF8Encoding]::new($false, $true)
$Ordinal = [System.StringComparer]::Ordinal

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

function Normalize-Text {
    param([AllowEmptyString()][string]$Text)

    $normalized = $Text.Replace("`r`n", "`n").Replace("`r", "`n")
    while ($normalized.EndsWith("`n", [System.StringComparison]::Ordinal)) {
        $normalized = $normalized.Substring(0, $normalized.Length - 1)
    }
    return $normalized + "`n"
}

function Select-ActiveCargoAboutHtml {
    param(
        [string]$Html,
        [string[]]$DependencyIdentities
    )

    $activeIdentities = [System.Collections.Generic.HashSet[string]]::new($Ordinal)
    foreach ($identity in $DependencyIdentities) {
        if (-not $activeIdentities.Add([string]$identity)) {
            throw "Duplicate active dependency identity: $identity"
        }
    }

    $renderedIdentities = [System.Collections.Generic.HashSet[string]]::new($Ordinal)
    $singleline = [System.Text.RegularExpressions.RegexOptions]::Singleline -bor [System.Text.RegularExpressions.RegexOptions]::CultureInvariant
    $componentRows = [regex]::new(
        '\s*<tr>\s*<td><code>(?<name>[^<]+)</code></td>\s*<td><code>(?<version>[^<]+)</code></td>.*?</tr>',
        $singleline
    )
    $filtered = $componentRows.Replace(
        $Html,
        [System.Text.RegularExpressions.MatchEvaluator]{
            param($match)

            $name = [System.Net.WebUtility]::HtmlDecode($match.Groups['name'].Value)
            $version = [System.Net.WebUtility]::HtmlDecode($match.Groups['version'].Value)
            $identity = "$name@$version"
            if (-not $activeIdentities.Contains($identity)) {
                return ""
            }
            if (-not $renderedIdentities.Add($identity)) {
                throw "Cargo-about rendered duplicate component identity: $identity"
            }
            return $match.Value
        }
    )

    $missingIdentities = @($DependencyIdentities | Where-Object { -not $renderedIdentities.Contains([string]$_) })
    if ($missingIdentities.Count -gt 0) {
        throw "Cargo-about did not render active dependency identities: $($missingIdentities -join ', ')"
    }

    $usedByItems = [regex]::new(
        '\s*<li><code>(?<name>[^<\s]+)\s+(?<version>[^<\s]+)</code>.*?</li>',
        $singleline
    )
    $filtered = $usedByItems.Replace(
        $filtered,
        [System.Text.RegularExpressions.MatchEvaluator]{
            param($match)

            $name = [System.Net.WebUtility]::HtmlDecode($match.Groups['name'].Value)
            $version = [System.Net.WebUtility]::HtmlDecode($match.Groups['version'].Value)
            if ($activeIdentities.Contains("$name@$version")) {
                return $match.Value
            }
            return ""
        }
    )

    $emptyLicenseSections = [regex]::new(
        '\s*<section>\s*<h3>.*?</h3>\s*<p>Used by:</p>\s*<ul>\s*</ul>\s*<pre>.*?</pre>\s*</section>',
        $singleline
    )
    return Normalize-Text ($emptyLicenseSections.Replace($filtered, ""))
}

function Get-TextSha256 {
    param([string]$Text)

    $bytes = $Utf8NoBom.GetBytes($Text)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $hash = $sha.ComputeHash($bytes)
    }
    finally {
        $sha.Dispose()
    }
    return -join ($hash | ForEach-Object { $_.ToString("x2") })
}

function Get-RelativeSourcePath {
    param(
        [string]$SourceRoot,
        [string]$SourcePath
    )

    $rootPath = [System.IO.Path]::GetFullPath($SourceRoot).TrimEnd('\', '/')
    $filePath = [System.IO.Path]::GetFullPath($SourcePath)
    $prefix = $rootPath + [System.IO.Path]::DirectorySeparatorChar
    $comparison = if ([System.IO.Path]::DirectorySeparatorChar -eq '\') {
        [System.StringComparison]::OrdinalIgnoreCase
    }
    else {
        [System.StringComparison]::Ordinal
    }
    if (-not $filePath.StartsWith($prefix, $comparison)) {
        throw "Supplemental notice file is outside its dependency source root: '$SourcePath'"
    }
    return $filePath.Substring($prefix.Length).Replace('\', '/')
}

function Get-LockedMetadata {
    param([string]$Target)

    $json = @(& cargo metadata --locked --filter-platform $Target --format-version 1)
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata failed for target $Target with exit code $LASTEXITCODE"
    }

    try {
        return ($json -join "`n") | ConvertFrom-Json
    }
    catch {
        throw "cargo metadata returned invalid JSON for target $Target`: $($_.Exception.Message)"
    }
}

function Add-ActivePackages {
    param(
        [string]$Target,
        [System.Collections.Generic.Dictionary[string, System.Collections.Generic.List[object]]]$PackagesByIdentity,
        [System.Collections.Generic.Dictionary[string, object]]$PackageUnion
    )

    # `cargo metadata` exposes inactive optional dependency edges. `cargo tree`
    # applies Cargo's resolved feature set, while normal,build excludes dev-only
    # edges. Metadata remains the locked source of package paths and identities.
    $treeLines = @(& cargo tree --locked --workspace --target $Target --edges "normal,build" --prefix none --format "{p}" --color never --quiet)
    if ($LASTEXITCODE -ne 0) {
        throw "cargo tree failed for target $Target with exit code $LASTEXITCODE"
    }

    foreach ($treeLineValue in $treeLines) {
        $treeLine = [string]$treeLineValue
        if ([string]::IsNullOrWhiteSpace($treeLine)) {
            continue
        }

        $match = [regex]::Match(
            $treeLine,
            '^(?<name>[^\s]+)\s+v(?<version>[^\s]+)(?:\s|$)',
            [System.Text.RegularExpressions.RegexOptions]::CultureInvariant
        )
        if (-not $match.Success) {
            throw "Unable to parse cargo tree package identity for target $Target`: '$treeLine'"
        }

        $identity = [string]::Concat(
            $match.Groups['name'].Value,
            [char]0,
            $match.Groups['version'].Value
        )
        if (-not $packagesByIdentity.ContainsKey($identity)) {
            throw "cargo tree returned a package absent from locked metadata for target $Target`: '$treeLine'"
        }

        $candidates = $packagesByIdentity[$identity]
        if ($candidates.Count -ne 1) {
            $candidateIds = @($candidates | ForEach-Object { [string]$_.id }) -join "', '"
            throw "cargo tree package identity is ambiguous for target $Target`: '$treeLine' matched '$candidateIds'"
        }

        $package = $candidates[0]
        $PackageUnion[[string]$package.id] = $package
    }
}

Assert-CargoAboutVersion

$outputBase = Join-Path $Root "target/third-party-notices"
New-Item -ItemType Directory -Path $outputBase -Force | Out-Null
do {
    $outputId = [System.Guid]::NewGuid().ToString("N")
    $outputDirectory = Join-Path $outputBase $outputId
} while (Test-Path -LiteralPath $outputDirectory)
New-Item -ItemType Directory -Path $outputDirectory | Out-Null
Write-Host "Third-party notice output: $outputDirectory"

$licensesOutput = Join-Path $outputDirectory "THIRD_PARTY_LICENSES.html"
$noticesOutput = Join-Path $outputDirectory "THIRD_PARTY_NOTICES.txt"
$aboutConfig = Join-Path $Root "about.toml"
$aboutTemplate = Join-Path $Root "licenses/third-party-licenses.hbs"

Push-Location $Root
try {
    $packageUnion = [System.Collections.Generic.Dictionary[string, object]]::new($Ordinal)
    $workspacePackageIds = [System.Collections.Generic.HashSet[string]]::new($Ordinal)
    $packagesByIdentity = [System.Collections.Generic.Dictionary[string, System.Collections.Generic.List[object]]]::new($Ordinal)
    foreach ($target in $Targets) {
        $metadata = Get-LockedMetadata -Target $target
        foreach ($workspaceIdValue in @($metadata.workspace_members)) {
            [void]$workspacePackageIds.Add([string]$workspaceIdValue)
        }
        foreach ($package in @($metadata.packages)) {
            $identity = [string]::Concat([string]$package.name, [char]0, [string]$package.version)
            if (-not $packagesByIdentity.ContainsKey($identity)) {
                $packagesByIdentity[$identity] = [System.Collections.Generic.List[object]]::new()
            }
            if (@($packagesByIdentity[$identity] | Where-Object { [string]$_.id -ceq [string]$package.id }).Count -eq 0) {
                $packagesByIdentity[$identity].Add($package)
            }
        }
    }
    foreach ($target in $Targets) {
        Add-ActivePackages -Target $target -PackagesByIdentity $packagesByIdentity -PackageUnion $packageUnion
    }
}
finally {
    Pop-Location
}

$dependencies = [System.Collections.Generic.SortedDictionary[string, object]]::new($Ordinal)
foreach ($pair in $packageUnion.GetEnumerator()) {
    if ($workspacePackageIds.Contains($pair.Key)) {
        continue
    }

    $package = $pair.Value
    $repository = if ([string]::IsNullOrWhiteSpace([string]$package.repository)) { "" } else { [string]$package.repository }
    $sortKey = [string]::Concat(
        [string]$package.name, [char]0,
        [string]$package.version, [char]0,
        $repository, [char]0,
        [string]$package.id
    )
    $dependencies[$sortKey] = $package
}

$dependencyIdentities = [string[]]@($dependencies.Values | ForEach-Object { "$($_.name)@$($_.version)" })
Push-Location $Root
try {
    # Deliberately online-capable: authoritative generation must not use --offline or --frozen.
    # cargo-about evaluates all configured targets simultaneously, which can
    # cross-pollinate target-specific dependency edges. Filter its fully
    # resolved report to the same per-target Cargo feature union used below.
    $aboutLog = @(& cargo about generate --locked --workspace --fail --config $aboutConfig --output-file $licensesOutput $aboutTemplate)
    if ($LASTEXITCODE -ne 0) {
        throw "cargo-about generation failed with exit code $LASTEXITCODE`n$($aboutLog -join "`n")"
    }
}
finally {
    Pop-Location
}
$normalizedLicenses = Normalize-Text ([System.IO.File]::ReadAllText($licensesOutput))
$filteredLicenses = Select-ActiveCargoAboutHtml -Html $normalizedLicenses -DependencyIdentities $dependencyIdentities
[System.IO.File]::WriteAllText($licensesOutput, $filteredLicenses, $Utf8NoBom)

$noticeEntries = [System.Collections.Generic.SortedDictionary[string, object]]::new($Ordinal)
$noticePatternOptions = [System.Management.Automation.WildcardOptions]::IgnoreCase -bor [System.Management.Automation.WildcardOptions]::CultureInvariant
$noticeFileMatchers = @($NoticeFilePatterns | ForEach-Object {
        [System.Management.Automation.WildcardPattern]::new($_, $noticePatternOptions)
    })
foreach ($package in $dependencies.Values) {
    $sourceRoot = [System.IO.Path]::GetDirectoryName([string]$package.manifest_path)
    if ([string]::IsNullOrWhiteSpace($sourceRoot) -or -not (Test-Path -LiteralPath $sourceRoot -PathType Container)) {
        throw "Dependency source directory is unavailable for $($package.name) $($package.version): '$sourceRoot'"
    }

    foreach ($sourcePath in [System.IO.Directory]::EnumerateFiles($sourceRoot, "*", [System.IO.SearchOption]::AllDirectories)) {
        $fileName = [System.IO.Path]::GetFileName($sourcePath)
        $matchesNoticePattern = $false
        foreach ($noticeFileMatcher in $noticeFileMatchers) {
            if ($noticeFileMatcher.IsMatch($fileName)) {
                $matchesNoticePattern = $true
                break
            }
        }
        if (-not $matchesNoticePattern) {
            continue
        }

        $relativePath = Get-RelativeSourcePath -SourceRoot $sourceRoot -SourcePath $sourcePath
        $content = Normalize-Text ([System.IO.File]::ReadAllText($sourcePath, $Utf8Strict))
        $repository = if ([string]::IsNullOrWhiteSpace([string]$package.repository)) { "Not provided" } else { [string]$package.repository }
        $entry = [pscustomobject]@{
            Name = [string]$package.name
            Version = [string]$package.version
            Repository = $repository
            RelativePath = $relativePath
            Sha256 = Get-TextSha256 $content
            Content = $content
        }
        $sortKey = [string]::Concat(
            $entry.Name, [char]0,
            $entry.Version, [char]0,
            $entry.RelativePath, [char]0,
            [string]$package.id
        )
        if ($noticeEntries.ContainsKey($sortKey)) {
            throw "Duplicate supplemental notice identity: $($entry.Name) $($entry.Version) $($entry.RelativePath)"
        }
        $noticeEntries.Add($sortKey, $entry)
    }
}

$builder = [System.Text.StringBuilder]::new()
[void]$builder.Append("RICOCHET THIRD-PARTY NOTICES`n")
[void]$builder.Append("Generated deterministically from Cargo.lock; no timestamp is recorded.`n`n")
[void]$builder.Append("Targets:`n")
foreach ($target in $Targets) {
    [void]$builder.Append("- $target`n")
}
[void]$builder.Append("`nDependencies: $($dependencies.Count)`n")
[void]$builder.Append("Notice files: $($noticeEntries.Count)`n")

foreach ($entry in $noticeEntries.Values) {
    [void]$builder.Append("`n===============================================================================`n")
    [void]$builder.Append("Crate: $($entry.Name)`n")
    [void]$builder.Append("Version: $($entry.Version)`n")
    [void]$builder.Append("Repository: $($entry.Repository)`n")
    [void]$builder.Append("Source path: $($entry.RelativePath)`n")
    [void]$builder.Append("SHA-256 (normalized UTF-8): $($entry.Sha256)`n")
    [void]$builder.Append("-------------------------------------------------------------------------------`n")
    [void]$builder.Append($entry.Content)
}

[System.IO.File]::WriteAllText($noticesOutput, $builder.ToString(), $Utf8NoBom)

$result = [pscustomobject]@{
    OutputDirectory = $outputDirectory
    LicensesPath = $licensesOutput
    NoticesPath = $noticesOutput
    DependencyCount = $dependencies.Count
    DependencyIdentities = $dependencyIdentities
    NoticeFileCount = $noticeEntries.Count
}

Write-Host "Generated $($result.DependencyCount) dependencies and $($result.NoticeFileCount) supplemental notice files."
if ($PassThru) {
    Write-Output $result
}
