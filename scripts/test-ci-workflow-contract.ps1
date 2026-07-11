Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$workflowPath = Join-Path $root ".github/workflows/ci.yml"
$workflow = [System.IO.File]::ReadAllText($workflowPath)
$failures = [System.Collections.Generic.List[string]]::new()

function Add-Failure {
    param([string] $Message)
    $failures.Add($Message) | Out-Null
}

function Get-StepText {
    param([string] $Name)

    $matches = [regex]::Matches(
        $workflow,
        "(?m)^      - name: $([regex]::Escape($Name))\r?$"
    )
    if ($matches.Count -ne 1) {
        Add-Failure "Expected exactly one '$Name' step, found $($matches.Count)."
        return ""
    }
    $start = $matches[0].Index
    $remainingStart = $start + $matches[0].Length
    $next = [regex]::Match($workflow.Substring($remainingStart), '(?m)^      - name: .+\r?$')
    $end = if ($next.Success) { $remainingStart + $next.Index } else { $workflow.Length }
    return $workflow.Substring($start, $end - $start)
}

function Require-Pattern {
    param(
        [string] $Text,
        [string] $Pattern,
        [string] $Description
    )
    if ($Text -notmatch $Pattern) {
        Add-Failure $Description
    }
}

$linuxVersionGuard = Get-StepText "Test Linux release version guard"
Require-Pattern $linuxVersionGuard "(?m)^        if: matrix\.os == 'ubuntu-latest'\r?$" "Linux release version behavior must run only on the Ubuntu matrix job."
Require-Pattern $linuxVersionGuard '(?m)^        run: bash scripts/test-linux-release-version\.sh\r?$' "Ubuntu CI must execute the Linux release version behavior test."

$linuxDependencies = Get-StepText "Install Linux GUI build dependencies"
Require-Pattern $linuxDependencies "(?m)^        if: matrix\.os == 'ubuntu-latest'\r?$" "Linux GUI dependencies must install only on the Ubuntu matrix job."
Require-Pattern $linuxDependencies '(?m)^          sudo apt-get update\r?$' "Linux GUI dependency setup must refresh apt metadata."
Require-Pattern $linuxDependencies '(?m)^          sudo apt-get install -y libwebkit2gtk-4\.1-dev libxdo-dev\r?$' "Linux GUI dependency setup must install the WebKitGTK and libxdo development packages required by the active Linux graph."

$cargoAbout = Get-StepText "Install cargo-about"
Require-Pattern $cargoAbout "(?m)^        if: matrix\.os == 'windows-latest'\r?$" "cargo-about must install only on the Windows acceptance job."
Require-Pattern $cargoAbout '(?m)^          cargo install cargo-about --version 0\.9\.1 --locked --features cli\r?$' "CI must install the exact cargo-about 0.9.1 command required by acceptance."

$learnValidation = Get-StepText "Validate public Learn manual"
Require-Pattern $learnValidation "(?m)^        if: matrix\.os == 'windows-latest'\r?$" "Strict Learn validation must run once on the Windows matrix job."
Require-Pattern $learnValidation '(?m)^        run: \.\\scripts\\validate-learn-manual\.ps1 -RequireWordCoverage -RequireJekyllRawBlocks\r?$' "CI must validate every live word mapping and canonical public Learn HTML."

$linuxDependencyIndex = $workflow.IndexOf("      - name: Install Linux GUI build dependencies", [System.StringComparison]::Ordinal)
$linuxVersionGuardIndex = $workflow.IndexOf("      - name: Test Linux release version guard", [System.StringComparison]::Ordinal)
$formatIndex = $workflow.IndexOf("      - name: Check formatting", [System.StringComparison]::Ordinal)
if ($linuxVersionGuardIndex -lt 0 -or $linuxDependencyIndex -le $linuxVersionGuardIndex -or $formatIndex -le $linuxDependencyIndex) {
    Add-Failure "Linux version guards and native dependencies must run in order before formatting, clippy, and tests."
}

$cargoAboutIndex = $workflow.IndexOf("      - name: Install cargo-about", [System.StringComparison]::Ordinal)
$acceptanceIndex = $workflow.IndexOf("      - name: Run acceptance suite", [System.StringComparison]::Ordinal)
$learnValidationIndex = $workflow.IndexOf("      - name: Validate public Learn manual", [System.StringComparison]::Ordinal)
if ($cargoAboutIndex -lt 0 -or $acceptanceIndex -le $cargoAboutIndex) {
    Add-Failure "Pinned cargo-about must be installed before the Windows acceptance suite."
}
if ($learnValidationIndex -le $acceptanceIndex) {
    Add-Failure "Strict public Learn validation must run after the Windows acceptance suite."
}

if ($failures.Count -gt 0) {
    $details = $failures | ForEach-Object { " - $_" }
    throw "CI workflow contract tests failed:`n$($details -join "`n")"
}

Write-Host "CI workflow contract tests passed."
