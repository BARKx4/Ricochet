Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$validatorPath = Join-Path $PSScriptRoot "validate-store-packaging.ps1"
$tokens = $null
$parseErrors = $null
$validatorAst = [System.Management.Automation.Language.Parser]::ParseFile(
    $validatorPath,
    [ref] $tokens,
    [ref] $parseErrors
)

if ($parseErrors.Count -gt 0) {
    throw "Store packaging validator did not parse: $($parseErrors[0].Message)"
}

$failures = [System.Collections.Generic.List[string]]::new()
$repoRoot = Split-Path -Parent $PSScriptRoot
$windowsPackager = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot "package-release.ps1"))
$linuxPackager = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot "package-release-linux.sh"))
$macosPackager = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot "package-release-macos.sh"))
$validatorSource = [System.IO.File]::ReadAllText($validatorPath)
$requiredFunctions = @(
    "Add-Error",
    "Assert-EntriesContain",
    "Assert-EntriesContainRegex",
    "Assert-DebContains"
)

foreach ($functionName in $requiredFunctions) {
    $definition = $validatorAst.Find({
            param($node)
            $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
                $node.Name -eq $functionName
        }, $true)
    if ($null -eq $definition) {
        $failures.Add("Store packaging validator is missing function '$functionName'.") | Out-Null
        continue
    }
    . ([scriptblock]::Create($definition.Extent.Text))
}

if ($windowsPackager -notmatch [regex]::Escape('Copy-ReleaseDirectory -Source (Join-Path $RepoRoot "docs\learn") -Destination (Join-Path $PackageDir "docs\learn")')) {
    $failures.Add("Windows release packaging does not bundle docs/learn beside docs/reference.") | Out-Null
}

$linuxLearnCopies = [regex]::Matches($linuxPackager, [regex]::Escape('copy_release_directory "$repo_root/docs/learn"')).Count
if ($linuxLearnCopies -ne 2) {
    $failures.Add("Linux release packaging must bundle docs/learn in both the portable archive and Debian package; found $linuxLearnCopies copy operations.") | Out-Null
}
$linuxAssetCopies = [regex]::Matches($linuxPackager, [regex]::Escape('copy_release_directory "$repo_root/docs/assets"')).Count
if ($linuxAssetCopies -ne 2) {
    $failures.Add("Linux release packaging must bundle docs/assets in both the portable archive and Debian documentation layout; found $linuxAssetCopies copy operations.") | Out-Null
}

if ($macosPackager -notmatch [regex]::Escape('copy_release_directory "$repo_root/docs/learn" "$package_dir/docs/learn"')) {
    $failures.Add("macOS release packaging does not bundle docs/learn beside docs/reference.") | Out-Null
}

foreach ($requiredLearnEntry in @(
        '"docs/learn/index.html"',
        '"*/docs/learn/index.html"',
        '"usr/share/doc/ricochet/learn/index\.html$"',
        '"usr/share/doc/ricochet/assets/ricochet-logo\.png$"'
    )) {
    if ($validatorSource -notmatch [regex]::Escape($requiredLearnEntry)) {
        $failures.Add("Store packaging validation does not require Learn entry $requiredLearnEntry.") | Out-Null
    }
}

foreach ($noticeName in @(
        "THIRD_PARTY_LICENSES.html",
        "THIRD_PARTY_NOTICES.txt"
    )) {
    if (Get-Command Assert-EntriesContain -ErrorAction SilentlyContinue) {
        $caseErrors = [System.Collections.Generic.List[string]]::new()
        Assert-EntriesContain $caseErrors @($noticeName.ToLowerInvariant()) "synthetic.zip" @($noticeName)
        if ($caseErrors.Count -ne 1) {
            $failures.Add("Windows ZIP matching accepted a mis-cased root entry for '$noticeName'.") | Out-Null
        }

        $depthErrors = [System.Collections.Generic.List[string]]::new()
        Assert-EntriesContain $depthErrors @("nested/$noticeName") "synthetic.zip" @($noticeName)
        if ($depthErrors.Count -ne 1) {
            $failures.Add("Windows ZIP matching accepted a nested entry for '$noticeName'.") | Out-Null
        }
    }

    if (Get-Command Assert-EntriesContainRegex -ErrorAction SilentlyContinue) {
        $archivePattern = '^[^/]+/{0}$' -f [regex]::Escape($noticeName)

        foreach ($invalidEntry in @(
                "package/$($noticeName.ToLowerInvariant())",
                "outer/nested/$noticeName"
            )) {
            $archiveErrors = [System.Collections.Generic.List[string]]::new()
            Assert-EntriesContainRegex $archiveErrors @($invalidEntry) "synthetic.tar.gz" @($archivePattern)
            if ($archiveErrors.Count -ne 1) {
                $failures.Add("Unix archive regex accepted invalid entry '$invalidEntry'.") | Out-Null
            }
        }

        $validArchiveErrors = [System.Collections.Generic.List[string]]::new()
        Assert-EntriesContainRegex $validArchiveErrors @("package/$noticeName") "synthetic.tar.gz" @($archivePattern)
        if ($validArchiveErrors.Count -ne 0) {
            $failures.Add("Unix archive regex rejected exact one-root entry 'package/$noticeName'.") | Out-Null
        }
    }

    if (Get-Command Assert-DebContains -ErrorAction SilentlyContinue) {
        $debPattern = 'usr/share/doc/ricochet/{0}$' -f [regex]::Escape($noticeName)
        $debErrors = [System.Collections.Generic.List[string]]::new()
        Assert-DebContains $debErrors @("./usr/share/doc/ricochet/$($noticeName.ToLowerInvariant())") $debPattern
        if ($debErrors.Count -ne 1) {
            $failures.Add("Debian matching accepted a mis-cased documentation entry for '$noticeName'.") | Out-Null
        }
    }
}

if ($failures.Count -gt 0) {
    $details = $failures | ForEach-Object { " - $_" }
    throw "Store packaging entry contract tests failed:`n$($details -join "`n")"
}

Write-Host "Store packaging entry contract tests passed."
