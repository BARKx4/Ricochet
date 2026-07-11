Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$linuxPackager = Get-Content -LiteralPath (Join-Path $PSScriptRoot "package-release-linux.sh") -Raw
$packageCommand = Get-Content -LiteralPath (Join-Path $repoRoot "crates/ricochet_cli/src/commands/package.rs") -Raw
$cliSmoke = Get-Content -LiteralPath (Join-Path $repoRoot "crates/ricochet_cli/tests/cli_smoke.rs") -Raw
$releaseWorkflow = Get-Content -LiteralPath (Join-Path $repoRoot ".github/workflows/release.yml") -Raw

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

Assert-ContractText $packageCommand 'let debian_version = debian_package_version\(version\)\?;' "Downstream packaging does not derive a fallible Debian version."
Assert-ContractText $packageCommand '\{name\}_\{debian_version\}_amd64\.deb' "Downstream Debian artifact naming does not use the Debian version."
Assert-ContractText $packageCommand 'Version: \{debian_version\}' "Downstream Debian control metadata does not use the Debian version."

Assert-ContractText $cliSmoke 'linux-gui-app_1\.2\.3~rc\.4_amd64\.deb' "Linux CLI smoke does not exercise a Debian-normalized prerelease filename."
Assert-ContractText $cliSmoke 'dpkg-deb --field Version' "Linux CLI smoke does not inspect the Debian Version field."
Assert-ContractText $cliSmoke '(?s)Command::new\("dpkg"\).*?\.arg\("--compare-versions"\)' "Linux CLI smoke does not verify Debian prerelease ordering."

Assert-ContractText $releaseWorkflow 'dpkg-deb --field.*Version' "Release workflow does not inspect the generated Debian Version field."
Assert-ContractText $releaseWorkflow 'dpkg --compare-versions' "Release workflow does not prove rc.5 sorts before the corresponding stable Debian version."

if ($failures.Count -gt 0) {
    $details = $failures | ForEach-Object { " - $_" }
    throw "Debian version contract tests failed:`n$($details -join "`n")"
}

Write-Host "Debian version contract tests passed."
