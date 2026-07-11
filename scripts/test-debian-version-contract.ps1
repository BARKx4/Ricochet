Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$linuxPackager = Get-Content -LiteralPath (Join-Path $PSScriptRoot "package-release-linux.sh") -Raw
$packageCommand = Get-Content -LiteralPath (Join-Path $repoRoot "crates/ricochet_cli/src/commands/package.rs") -Raw
$cliSmoke = Get-Content -LiteralPath (Join-Path $repoRoot "crates/ricochet_cli/tests/cli_smoke.rs") -Raw
$releaseWorkflow = Get-Content -LiteralPath (Join-Path $repoRoot ".github/workflows/release.yml") -Raw
$ciWorkflow = Get-Content -LiteralPath (Join-Path $repoRoot ".github/workflows/ci.yml") -Raw
$linuxVersionGuardPath = Join-Path $PSScriptRoot "test-linux-release-version.sh"
$linuxVersionGuard = if (Test-Path -LiteralPath $linuxVersionGuardPath -PathType Leaf) {
    Get-Content -LiteralPath $linuxVersionGuardPath -Raw
} else {
    ""
}

$failures = [System.Collections.Generic.List[string]]::new()
function Assert-ContractText {
    param(
        [string] $Text,
        [string] $Pattern,
        [string] $Description
    )

    if ($Text -cnotmatch $Pattern) {
        $failures.Add($Description) | Out-Null
    }
}

Assert-ContractText $linuxPackager 'validate_semver\(\)' "Official Linux packaging does not define input SemVer validation."
Assert-ContractText $linuxPackager '(?m)^validate_semver "\$version"\s*$' "Official Linux packaging does not reject invalid versions before deriving artifact paths."
Assert-ContractText $linuxPackager '\^\(0\|\[1-9\]\[0-9\]\*\)\\\.\(0\|\[1-9\]\[0-9\]\*\)\\\.\(0\|\[1-9\]\[0-9\]\*\)' "Official Linux packaging does not anchor the numeric SemVer core."
Assert-ContractText $linuxPackager 'numeric prerelease identifiers must not contain leading zeroes' "Official Linux packaging does not reject invalid numeric prerelease identifiers."
Assert-ContractText $linuxPackager 'semver_to_debian_version\(\)' "Official Linux packaging does not define SemVer-to-Debian conversion."
Assert-ContractText $linuxPackager 'debian_version="\$\(semver_to_debian_version "\$version"\)"' "Official Linux packaging does not derive a Debian version from the Ricochet SemVer."
Assert-ContractText $linuxPackager 'deb_path="\$out_dir_path/ricochet_\$\{debian_version\}_amd64\.deb"' "Official Debian artifact naming does not use the Debian version."
Assert-ContractText $linuxPackager 'Version: \$debian_version' "Official Debian control metadata does not use the Debian version."
Assert-ContractText $linuxPackager 'Depends: libgtk-3-0, libwebkit2gtk-4\.1-0, libxdo3' "Official Debian metadata does not declare every direct Linux GUI runtime package."

Assert-ContractText $packageCommand 'let debian_version = debian_package_version\(version\)\?;' "Downstream packaging does not derive a fallible Debian version."
Assert-ContractText $packageCommand '\{name\}_\{debian_version\}_amd64\.deb' "Downstream Debian artifact naming does not use the Debian version."
Assert-ContractText $packageCommand 'Version: \{debian_version\}' "Downstream Debian control metadata does not use the Debian version."
Assert-ContractText $packageCommand 'Depends: libgtk-3-0, libwebkit2gtk-4\.1-0, libxdo3' "Downstream Debian metadata does not declare every direct Linux runtime package."

Assert-ContractText $cliSmoke 'linux-gui-app_1\.2\.3~rc\.4_amd64\.deb' "Linux CLI smoke does not exercise a Debian-normalized prerelease filename."
Assert-ContractText $cliSmoke 'dpkg-deb --field Version' "Linux CLI smoke does not inspect the Debian Version field."
Assert-ContractText $cliSmoke '(?s)Command::new\("dpkg"\).*?\.arg\("--compare-versions"\)' "Linux CLI smoke does not verify Debian prerelease ordering."
Assert-ContractText $cliSmoke 'Shared library: \[libxdo\.so\.3\]' "Linux CLI smoke does not prove the packaged binary links libxdo."
Assert-ContractText $cliSmoke 'libgtk-3-0, libwebkit2gtk-4\.1-0, libxdo3' "Linux CLI smoke does not verify the matching Debian runtime dependencies."

Assert-ContractText $releaseWorkflow 'dpkg-deb --field.*Version' "Release workflow does not inspect the generated Debian Version field."
Assert-ContractText $releaseWorkflow 'dpkg --compare-versions' "Release workflow does not prove rc.5 sorts before the corresponding stable Debian version."
Assert-ContractText $releaseWorkflow ([regex]::Escape('Shared library: \[libxdo\.so\.3\]')) "Release workflow does not prove official Linux binaries link libxdo."
Assert-ContractText $releaseWorkflow 'Depends: libgtk-3-0, libwebkit2gtk-4\.1-0, libxdo3' "Release workflow does not verify Debian metadata covers the direct Linux runtime dependencies."

$validateIndex = $linuxPackager.IndexOf('validate_semver "$version"', [System.StringComparison]::Ordinal)
$firstDerivedPathIndex = @(
    $linuxPackager.IndexOf('package_name="ricochet-v${version}-${target}"', [System.StringComparison]::Ordinal),
    $linuxPackager.IndexOf('package_dir="$out_dir_path/$package_name"', [System.StringComparison]::Ordinal),
    $linuxPackager.IndexOf('deb_path="$out_dir_path/ricochet_${debian_version}_amd64.deb"', [System.StringComparison]::Ordinal)
) | Where-Object { $_ -ge 0 } | Measure-Object -Minimum | Select-Object -ExpandProperty Minimum
if ($validateIndex -lt 0 -or $null -eq $firstDerivedPathIndex -or $validateIndex -ge $firstDerivedPathIndex) {
    $failures.Add("Official Linux SemVer validation must execute before every version-derived artifact path.") | Out-Null
}

if ([string]::IsNullOrWhiteSpace($linuxVersionGuard)) {
    $failures.Add("Linux release version behavior test is missing: scripts/test-linux-release-version.sh") | Out-Null
} else {
    Assert-ContractText $linuxVersionGuard '(?s)for invalid in.*dev.*\.\./escape.*1\.2.*1\.2\.3-01.*01\.2\.3' "Linux release version behavior test does not cover malformed and path-traversal versions."
    Assert-ContractText $linuxVersionGuard 'test ! -e "\$out"' "Linux release version behavior test does not prove rejection happens before artifact creation."
}
Assert-ContractText $ciWorkflow 'bash scripts/test-linux-release-version\.sh' "Ubuntu CI does not execute the Linux release version behavior test."
Assert-ContractText $releaseWorkflow 'bash scripts/test-linux-release-version\.sh' "Linux release audit does not execute the release version behavior test."

if ($failures.Count -gt 0) {
    $details = $failures | ForEach-Object { " - $_" }
    throw "Debian version contract tests failed:`n$($details -join "`n")"
}

Write-Host "Debian version contract tests passed."
