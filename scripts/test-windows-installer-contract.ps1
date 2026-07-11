Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$nsisPath = Join-Path $root "packaging\windows\ricochet.nsi"
$legacyVersion = "0.1.19-rc." + "4"
$legacyPath = Join-Path $root "packaging\windows\legacy-v$legacyVersion-files.nsh"
$packagerPath = Join-Path $PSScriptRoot "package-release.ps1"
$workflowPath = Join-Path $root ".github\workflows\release.yml"
$nsis = [System.IO.File]::ReadAllText($nsisPath)
$packager = [System.IO.File]::ReadAllText($packagerPath)
$workflow = [System.IO.File]::ReadAllText($workflowPath)
$failures = [System.Collections.Generic.List[string]]::new()

function Require-Text {
    param([string]$Text, [string]$Needle, [string]$Message)
    if (-not $Text.Contains($Needle)) {
        $script:failures.Add($Message) | Out-Null
    }
}

function Reject-Pattern {
    param([string]$Text, [string]$Pattern, [string]$Message)
    if ($Text -match $Pattern) {
        $script:failures.Add($Message) | Out-Null
    }
}

foreach ($requirement in @(
        @('!ifndef INSTALL_MANIFEST', 'NSIS must require a generated exact installed-file manifest.'),
        @('!ifndef LEGACY_CLEANUP_MANIFEST', 'NSIS must require the fixed rc.4 cleanup manifest.'),
        @('ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ricochet" "InstallLocation"', 'Installer must bind ownership to the registered Ricochet install location.'),
        @('IfFileExists "$INSTDIR\.ricochet-install-owner" destination_owned', 'Installer must recognize the current ownership marker.'),
        @('IfFileExists "$INSTDIR\rco.exe" destination_owned', 'Installer must recognize a registered legacy Ricochet install.'),
        @('SetErrorLevel 2', 'Installer must return a nonzero status for a foreign non-empty destination.'),
        @('!include "${LEGACY_CLEANUP_MANIFEST}"', 'Owned upgrades must execute the fixed legacy cleanup manifest.'),
        @('FileWrite $0 "Ricochet ${VERSION}$\r$\n"', 'Installer must write a versioned ownership marker.'),
        @('!include "${INSTALL_MANIFEST}"', 'Uninstall must execute only the generated exact-path manifest.'),
        @('RMDir "$SMPROGRAMS\Ricochet"', 'Uninstall must remove the Start Menu directory only when empty.')
    )) {
    Require-Text $nsis $requirement[0] $requirement[1]
}
Reject-Pattern $nsis '(?i)RMDir\s+/r' 'NSIS must never recursively remove the install or Start Menu directory.'

foreach ($requirement in @(
        @('function Write-NsisInstallManifest', 'Windows packager must define exact NSIS manifest generation.'),
        @('Write-NsisInstallManifest -PackageDir $PackageDir -Path $NsisInstallManifestPath', 'Windows packager must generate the uninstall manifest from the staged package.'),
        @('"/DINSTALL_MANIFEST=$NsisInstallManifestPath"', 'Windows packager must pass the generated manifest to NSIS.'),
        @('"/DLEGACY_CLEANUP_MANIFEST=$NsisLegacyCleanupPath"', 'Windows packager must pass the reviewed rc.4 cleanup manifest to NSIS.')
    )) {
    Require-Text $packager $requirement[0] $requirement[1]
}

$expectedLegacyFiles = @(
    'rco-app.exe',
    'packages\ricochet_slint\README.md',
    'packages\ricochet_slint\backend.rco',
    'packages\ricochet_slint\ricochet.toml',
    'packages\ricochet_slint\tests\SlintBackendPackageTest.rco',
    'packages\ricochet_ui\README.md',
    'packages\ricochet_ui\commands.rco',
    'packages\ricochet_ui\document.rco',
    'packages\ricochet_ui\events.rco',
    'packages\ricochet_ui\examples\counter_app.rco',
    'packages\ricochet_ui\examples\data_grid_viewer.rco',
    'packages\ricochet_ui\examples\native_showcase_app.rco',
    'packages\ricochet_ui\examples\project_tree_drag_drop.rco',
    'packages\ricochet_ui\examples\rich_text_note.rco',
    'packages\ricochet_ui\rich_text.rco',
    'packages\ricochet_ui\ricochet.toml',
    'packages\ricochet_ui\tests\UiDocumentPackageTest.rco',
    'packages\ricochet_ui\tests\UiInteractionPackageTest.rco',
    'packages\ricochet_ui\tests\UiValidationPackageTest.rco',
    'packages\ricochet_ui\validation.rco',
    'packages\ricochet_winui\README.md',
    'packages\ricochet_winui\backend.rco',
    'packages\ricochet_winui\ricochet.toml',
    'packages\ricochet_winui\tests\WinuiBackendPackageTest.rco'
)
if (-not (Test-Path -LiteralPath $legacyPath -PathType Leaf)) {
    $failures.Add("Missing reviewed rc.4 legacy cleanup manifest.") | Out-Null
}
else {
    $legacy = [System.IO.File]::ReadAllText($legacyPath)
    $deleteLines = @($legacy -split "`r?`n" | Where-Object { $_ -match '^Delete ' })
    if ($deleteLines.Count -ne $expectedLegacyFiles.Count) {
        $failures.Add("Legacy cleanup must contain exactly $($expectedLegacyFiles.Count) file deletions; found $($deleteLines.Count).") | Out-Null
    }
    foreach ($relativePath in $expectedLegacyFiles) {
        $line = 'Delete "$INSTDIR\{0}"' -f $relativePath
        if (@($deleteLines | Where-Object { $_ -ceq $line }).Count -ne 1) {
            $failures.Add("Legacy cleanup does not contain exactly one '$line'.") | Out-Null
        }
    }
    Reject-Pattern $legacy '(?i)RMDir\s+/r' 'Legacy cleanup must remove only exact files and empty directories.'
}

foreach ($workflowNeedle in @(
        'foreign-destination-sentinel.txt',
        'Expected installer rejection for a foreign non-empty destination',
        'user-owned-sentinel.txt',
        'legacyRc4Paths',
        'Seeded legacy path survived the rc.5 upgrade',
        'User-owned sentinel was removed or changed',
        'Installer-owned path survived uninstall'
    )) {
    Require-Text $workflow $workflowNeedle "Windows installer CI is missing upgrade/sentinel proof '$workflowNeedle'."
}

$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($packagerPath, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count -gt 0) {
    $failures.Add("Windows packager did not parse: $($parseErrors[0].Message)") | Out-Null
}
$manifestFunction = $ast.Find({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
            $node.Name -eq 'Write-NsisInstallManifest'
    }, $true)
if ($null -ne $manifestFunction) {
    . ([scriptblock]::Create($manifestFunction.Extent.Text))
    $fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ricochet-nsis-contract-" + [guid]::NewGuid().ToString('N'))
    $fixturePackage = Join-Path $fixtureRoot 'package'
    $fixtureNested = Join-Path $fixturePackage 'nested'
    $fixtureManifest = Join-Path $fixtureRoot 'installed-files.nsh'
    New-Item -ItemType Directory -Path $fixtureNested | Out-Null
    Set-Content -LiteralPath (Join-Path $fixturePackage 'root.txt') -Value 'root'
    Set-Content -LiteralPath (Join-Path $fixtureNested 'child.txt') -Value 'child'
    Write-NsisInstallManifest -PackageDir $fixturePackage -Path $fixtureManifest
    $generated = [System.IO.File]::ReadAllText($fixtureManifest)
    foreach ($line in @(
            'Delete "$INSTDIR\root.txt"',
            'Delete "$INSTDIR\nested\child.txt"',
            'Delete "$INSTDIR\.ricochet-install-owner"',
            'Delete "$INSTDIR\Uninstall.exe"',
            'RMDir "$INSTDIR\nested"',
            'RMDir "$INSTDIR"'
        )) {
        Require-Text $generated $line "Generated uninstall manifest omitted '$line'."
    }
    Reject-Pattern $generated '(?i)RMDir\s+/r' 'Generated uninstall manifest must never recursively remove directories.'
}

if ($failures.Count -gt 0) {
    $details = $failures | ForEach-Object { " - $_" }
    throw "Windows installer contract tests failed:`n$($details -join "`n")"
}

Write-Host "Windows installer contract tests passed."
