Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$workflowPath = Join-Path $root ".github\workflows\release.yml"
$workflow = [System.IO.File]::ReadAllText($workflowPath)
$failures = [System.Collections.Generic.List[string]]::new()

function Add-Failure {
    param([string]$Message)

    $script:failures.Add($Message) | Out-Null
}

function Get-JobText {
    param(
        [string]$Name,
        [AllowEmptyString()][string]$NextName
    )

    $startMatch = [regex]::Match(
        $workflow,
        "(?m)^  $([regex]::Escape($Name)):\r?$"
    )
    if (-not $startMatch.Success) {
        Add-Failure "Release workflow is missing job '$Name'."
        return ""
    }

    $endIndex = $workflow.Length
    if (-not [string]::IsNullOrEmpty($NextName)) {
        $nextMatch = [regex]::Match(
            $workflow.Substring($startMatch.Index + $startMatch.Length),
            "(?m)^  $([regex]::Escape($NextName)):\r?$"
        )
        if (-not $nextMatch.Success) {
            Add-Failure "Release workflow is missing job '$NextName' after '$Name'."
            return ""
        }
        $endIndex = $startMatch.Index + $startMatch.Length + $nextMatch.Index
    }

    return $workflow.Substring($startMatch.Index, $endIndex - $startMatch.Index)
}

function Get-StepText {
    param(
        [string]$JobText,
        [string]$Name
    )

    if ([string]::IsNullOrEmpty($JobText)) {
        return ""
    }

    $matches = [regex]::Matches(
        $JobText,
        "(?m)^      - name: $([regex]::Escape($Name))\r?$"
    )
    if ($matches.Count -ne 1) {
        Add-Failure "Expected exactly one '$Name' step, found $($matches.Count)."
        return ""
    }

    $startIndex = $matches[0].Index
    $remainingStart = $startIndex + $matches[0].Length
    $nextMatch = [regex]::Match(
        $JobText.Substring($remainingStart),
        '(?m)^      - name: .+\r?$'
    )
    $endIndex = if ($nextMatch.Success) {
        $remainingStart + $nextMatch.Index
    } else {
        $JobText.Length
    }

    return $JobText.Substring($startIndex, $endIndex - $startIndex)
}

function Require-Pattern {
    param(
        [string]$Text,
        [string]$Pattern,
        [string]$Description
    )

    if (-not [regex]::IsMatch($Text, $Pattern)) {
        Add-Failure $Description
    }
}

function Reject-Pattern {
    param(
        [string]$Text,
        [string]$Pattern,
        [string]$Description
    )

    if ([regex]::IsMatch($Text, $Pattern)) {
        Add-Failure $Description
    }
}

$windowsJob = Get-JobText -Name "package-windows" -NextName "package-linux"
$linuxJob = Get-JobText -Name "package-linux" -NextName "package-macos"
$macosJob = Get-JobText -Name "package-macos" -NextName "publish-release"

$windowsPortableStep = Get-StepText -JobText $windowsJob -Name "Smoke-test package executable"
$windowsInstallerStep = Get-StepText -JobText $windowsJob -Name "Smoke-test Windows installer"
$linuxSmokeStep = Get-StepText -JobText $linuxJob -Name "Smoke-test package executable"
$macosSmokeStep = Get-StepText -JobText $macosJob -Name "Smoke-test package executable"

$portableIndex = $windowsJob.IndexOf("      - name: Smoke-test package executable", [StringComparison]::Ordinal)
$installerIndex = $windowsJob.IndexOf("      - name: Smoke-test Windows installer", [StringComparison]::Ordinal)
$manifestIndex = $windowsJob.IndexOf("      - name: Validate release artifact manifest", [StringComparison]::Ordinal)
if ($portableIndex -lt 0 -or $installerIndex -le $portableIndex -or $manifestIndex -le $installerIndex) {
    Add-Failure "Windows installer smoke must follow portable smoke and precede artifact validation."
}

foreach ($noticeName in @(
        "THIRD_PARTY_LICENSES.html",
        "THIRD_PARTY_NOTICES.txt"
    )) {
    $escapedNoticeName = [regex]::Escape($noticeName)
    Require-Pattern $windowsPortableStep $escapedNoticeName "Windows portable smoke must hash-check $noticeName."
    Require-Pattern $windowsInstallerStep $escapedNoticeName "Windows installer smoke must hash-check source and installed $noticeName."
    Require-Pattern $linuxSmokeStep $escapedNoticeName "Linux smoke must hash-check tar and Debian $noticeName."
    Require-Pattern $macosSmokeStep $escapedNoticeName "macOS smoke must hash-check $noticeName in each matrix tarball."
}

Require-Pattern $windowsPortableStep 'Get-FileHash' "Windows portable smoke must use SHA-256 file hashes."
Require-Pattern $windowsInstallerStep 'Get-FileHash' "Windows installer smoke must use SHA-256 file hashes."
Require-Pattern $linuxSmokeStep 'sha256sum' "Linux smoke must use sha256sum for notice integrity."
Require-Pattern $linuxSmokeStep '\$package_dir' "Linux smoke must compare notice hashes from the tar root."
Require-Pattern $linuxSmokeStep '\$tmp/deb-root/usr/share/doc/ricochet' "Linux smoke must compare notice hashes from the extracted Debian documentation directory."
Require-Pattern $macosSmokeStep 'shasum -a 256' "macOS smoke must use shasum -a 256 for notice integrity."
Require-Pattern $macosSmokeStep '\$package_dir' "macOS smoke must compare notice hashes from each matrix tar root."

$installerRequirements = [ordered]@{
    'ricochet-v\$version-windows-x64-setup\.exe' = "Installer smoke must resolve the version-specific setup executable."
    'ricochet-v\$version-windows-x64' = "Installer smoke must resolve the version-specific staged package directory."
    'installerCandidates\.Count -ne 1' = "Installer smoke must require exactly one setup executable."
    'sourceCandidates\.Count -ne 1' = "Installer smoke must require exactly one staged package directory."
    '\$env:RUNNER_TEMP' = "Installer smoke must use RUNNER_TEMP."
    'ricochet-installer-smoke-' = "Installer smoke must use a unique disposable root."
    'outside-checkout' = "Installer smoke must use an outside-checkout working directory."
    'NSIS smoke install path contains whitespace or quotes' = "Installer smoke must reject a /D path that needs quoting."
    'Test-Path -LiteralPath \$installDir' = "Installer smoke must preflight its install directory."
    'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Ricochet' = "Installer smoke must inspect the HKCU uninstall key."
    '\[Environment\+SpecialFolder\]::Programs' = "Installer smoke must resolve the current-user Programs folder."
    'Start-Process' = "Installer smoke must execute native installer and runtime processes with explicit waits."
    '"/S /D=\$installDir"' = "Installer smoke must silently install with final unquoted /D."
    'rco\.exe' = "Installer smoke must require installed rco.exe."
    'rco-gui\.exe' = "Installer smoke must require installed rco-gui.exe."
    'ricochet\.exe' = "Installer smoke must require installed ricochet.exe."
    'Ricochet Shell\.cmd' = "Installer smoke must require the installed shell launcher."
    'Uninstall\.exe' = "Installer smoke must require the uninstaller."
    'LICENSE' = "Installer smoke must require and hash-check LICENSE."
    'examples\\basic-oop\.rco' = "Installer smoke must require installed basic-oop."
    'examples\\webview_ui\.rco' = "Installer smoke must require the installed WebView example."
    'docs\\reference\\index\.html' = "Installer smoke must require the installed reference index."
    'foreach \(\$binaryName in @\(''rco\.exe'', ''ricochet\.exe''\)\)' = "Installer smoke must check only CLI aliases with --version."
    'rco \$version' = "Installer smoke must require exact rco <version> output."
    'Push-Location -LiteralPath \$outsideDir' = "Installer smoke must package the installed WebView outside the checkout."
    '& \$rco package \$webviewSource' = "Installer smoke must package WebView with installed rco."
    'RICOCHET_GUI_EXPORT_HTML' = "Installer smoke must export installed WebView HTML."
    '<title>Ricochet Desktop UI</title>' = "Installer smoke must verify the installed WebView title."
    'DisplayName' = "Installer smoke must verify DisplayName."
    'DisplayVersion' = "Installer smoke must verify DisplayVersion."
    'Publisher' = "Installer smoke must verify Publisher."
    'InstallLocation' = "Installer smoke must verify InstallLocation."
    'UninstallString' = "Installer smoke must verify UninstallString."
    'NoModify' = "Installer smoke must verify NoModify."
    'NoRepair' = "Installer smoke must verify NoRepair."
    'Ricochet Shell\.lnk' = "Installer smoke must verify the shell shortcut."
    'Reference Docs\.lnk' = "Installer smoke must verify the docs shortcut."
    'Third-Party Licenses\.lnk' = "Installer smoke must verify the third-party licenses shortcut."
    'Uninstall Ricochet\.lnk' = "Installer smoke must verify the uninstall shortcut."
    "-ArgumentList '/S'" = "Installer smoke must run the normal silent uninstaller."
    'AddSeconds\(30\)' = "Installer smoke must bound uninstall polling to 30 seconds."
    'Start-Sleep -Milliseconds 250' = "Installer smoke must poll without a blocking delay."
    'uninstallStubExitCode -ne 0' = "Installer smoke must require the uninstaller stub exit code to be zero."
}

foreach ($requirement in $installerRequirements.GetEnumerator()) {
    Require-Pattern $windowsInstallerStep $requirement.Key $requirement.Value
}

Reject-Pattern $windowsInstallerStep '(?i)\bRemove-Item\b' "Installer smoke must not manually delete installer-owned state."
Reject-Pattern $windowsInstallerStep '_\?=' "Installer smoke must not disable NSIS uninstaller self-copying."
Reject-Pattern $windowsInstallerStep '(?i)rco-gui(?:\.exe)?[^\r\n]*--version' "Installer smoke must not invoke rco-gui --version."

if ($failures.Count -gt 0) {
    $details = $failures | ForEach-Object { " - $_" }
    throw "Release workflow contract tests failed:`n$($details -join "`n")"
}

Write-Host "Release workflow contract tests passed."
