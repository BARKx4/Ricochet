[CmdletBinding()]
param(
    [string]$Rco,
    [string]$Manifest
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if ([string]::IsNullOrWhiteSpace($Rco)) {
    $Rco = Join-Path $RepoRoot "target\debug\rco.exe"
}
if ([string]::IsNullOrWhiteSpace($Manifest)) {
    $Manifest = Join-Path $RepoRoot "examples\words\manifest.json"
}

if (-not (Test-Path -LiteralPath $Rco -PathType Leaf)) {
    throw "Could not find rco at '$Rco'. Build it first with: cargo build -p ricochet_cli --bin rco"
}
if (-not (Test-Path -LiteralPath $Manifest -PathType Leaf)) {
    throw "Could not find word example manifest at '$Manifest'. Run scripts/generate-word-examples.ps1 first."
}

function Add-Failure {
    param(
        [System.Collections.Generic.List[string]]$Failures,
        [string]$Message
    )

    $Failures.Add($Message)
}

function Invoke-RcoCommand {
    param(
        [string[]]$Arguments,
        [AllowNull()][string]$InputText
    )

    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        if ($null -eq $InputText) {
            $output = @(& $Rco @Arguments 2>&1)
        } else {
            $output = @($InputText | & $Rco @Arguments 2>&1)
        }
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousPreference
    }

    return [pscustomobject]@{
        ExitCode = $exitCode
        Output = (($output | ForEach-Object { $_.ToString() }) -join "`n")
    }
}

function Test-SourceToken {
    param(
        [string]$Source,
        [string]$Token
    )

    $escaped = [regex]::Escape($Token)
    return [regex]::IsMatch(
        $Source,
        "(?m)(^|[\s\[\]\(\)])$escaped(?=`$|[\s\[\]\(\)])"
    )
}

function Test-EvidenceToken {
    param(
        [string]$Source,
        [string]$Token
    )

    $escaped = [regex]::Escape($Token)
    return [regex]::IsMatch(
        $Source,
        "(?<![A-Za-z0-9_?])$escaped(?![A-Za-z0-9_?])"
    )
}

$failures = [System.Collections.Generic.List[string]]::new()
$manifestRoot = Split-Path -Parent ([System.IO.Path]::GetFullPath($Manifest))
$manifestRootWithSeparator = $manifestRoot.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
$manifestData = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json

if ([string]$manifestData.schema -ne "ricochet.word-examples.v1") {
    Add-Failure $failures "Unexpected word example manifest schema: $($manifestData.schema)"
}
$entries = @($manifestData.examples)
if ([int]$manifestData.count -ne $entries.Count) {
    Add-Failure $failures "Manifest count says $($manifestData.count), but contains $($entries.Count) entries"
}

$wordsJson = (& $Rco words --json | Out-String)
if ($LASTEXITCODE -ne 0) {
    throw "rco words --json failed with exit code $LASTEXITCODE"
}
$liveWords = $wordsJson | ConvertFrom-Json

if ($entries.Count -ne $liveWords.Count) {
    Add-Failure $failures "Manifest has $($entries.Count) examples, but the live inventory has $($liveWords.Count) words"
}

$seenWords = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
$seenPaths = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
$allowedModes = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::Ordinal
)
foreach ($mode in @(
    "run-sandboxed", "run-environment", "run-filesystem-readonly", "run-webview",
    "run-sleep", "run-stdin", "check-mvc", "check-filesystem-write",
    "check-http-loopback", "check-upload-context", "check-socket-loopback",
    "check-tui", "check-webview-interactive", "check-process"
)) {
    [void]$allowedModes.Add($mode)
}

$runCounts = [System.Collections.Generic.Dictionary[string,int]]::new(
    [System.StringComparer]::Ordinal
)
$evidenceTextCache = [System.Collections.Generic.Dictionary[string,string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
)
$compiledCount = 0

Push-Location $RepoRoot
try {
    for ($index = 0; $index -lt $entries.Count; $index++) {
        $entry = $entries[$index]
        $word = [string]$entry.word
        $group = [string]$entry.group
        $relativePath = ([string]$entry.path).Replace('/', [System.IO.Path]::DirectorySeparatorChar)
        $mode = [string]$entry.validation

        if (-not $seenWords.Add($word)) {
            Add-Failure $failures "Duplicate case-sensitive word entry: $word"
        }
        if (-not $seenPaths.Add($relativePath)) {
            Add-Failure $failures "Duplicate example path: $relativePath"
        }
        if (-not $allowedModes.Contains($mode)) {
            Add-Failure $failures "Unknown validation mode '$mode' for word '$word'"
        }

        if ($index -lt $liveWords.Count) {
            $liveWord = [string]$liveWords[$index].word
            $liveGroup = [string]$liveWords[$index].detail
            if (-not [string]::Equals($word, $liveWord, [System.StringComparison]::Ordinal)) {
                Add-Failure $failures "Inventory mismatch at position $($index + 1): manifest '$word', live '$liveWord'"
            }
            if (-not [string]::Equals($group, $liveGroup, [System.StringComparison]::Ordinal)) {
                Add-Failure $failures "Group mismatch for '$word': manifest '$group', live '$liveGroup'"
            }
        }

        $fullPath = [System.IO.Path]::GetFullPath((Join-Path $manifestRoot $relativePath))
        if (-not $fullPath.StartsWith($manifestRootWithSeparator, [System.StringComparison]::OrdinalIgnoreCase)) {
            Add-Failure $failures "Example path escapes the corpus root for '$word': $relativePath"
            continue
        }
        if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) {
            Add-Failure $failures "Missing example for '$word': $relativePath"
            continue
        }

        $source = Get-Content -LiteralPath $fullPath -Raw
        $expectedHeader = "(( Word: $word | Group: $group | Validation: $mode ))"
        if (-not $source.Contains($expectedHeader)) {
            Add-Failure $failures "Example header does not match manifest for '$word': $relativePath"
        }
        $sourceLines = $source -split "`r?`n"
        $exampleBody = ($sourceLines | Select-Object -Skip 2) -join "`n"
        foreach ($token in @($entry.tokens)) {
            if (-not (Test-SourceToken -Source $exampleBody -Token ([string]$token))) {
                Add-Failure $failures "Example '$relativePath' does not exercise token '$token' for '$word'"
            }
        }

        if ($mode.StartsWith("check-", [System.StringComparison]::Ordinal)) {
            if (-not ($entry.PSObject.Properties.Name -contains "reason") -or [string]::IsNullOrWhiteSpace([string]$entry.reason)) {
                Add-Failure $failures "Compile-only example '$word' is missing a reason"
            }
            $evidenceTexts = [System.Collections.Generic.List[string]]::new()
            foreach ($evidence in @($entry.evidence)) {
                $evidencePath = Join-Path $RepoRoot ([string]$evidence).Replace('/', [System.IO.Path]::DirectorySeparatorChar)
                if (-not (Test-Path -LiteralPath $evidencePath)) {
                    Add-Failure $failures "Evidence path for '$word' does not exist: $evidence"
                    continue
                }
                $evidencePath = [System.IO.Path]::GetFullPath($evidencePath)
                if (-not $evidenceTextCache.ContainsKey($evidencePath)) {
                    if (Test-Path -LiteralPath $evidencePath -PathType Leaf) {
                        $evidenceTextCache[$evidencePath] = Get-Content -LiteralPath $evidencePath -Raw
                    } else {
                        $evidenceFiles = @(
                            Get-ChildItem -LiteralPath $evidencePath -Recurse -File |
                                Where-Object { $_.Extension -in @(".rco", ".rs", ".ps1", ".json", ".toml") }
                        )
                        $evidenceTextCache[$evidencePath] = @(
                            $evidenceFiles | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw }
                        ) -join "`n"
                    }
                }
                $evidenceTexts.Add($evidenceTextCache[$evidencePath])
            }
            foreach ($token in @($entry.tokens)) {
                $hasEvidence = $false
                foreach ($evidenceText in $evidenceTexts) {
                    if (Test-EvidenceToken -Source $evidenceText -Token ([string]$token)) {
                        $hasEvidence = $true
                        break
                    }
                }
                if (-not $hasEvidence) {
                    Add-Failure $failures "Integration evidence for '$word' does not mention token '$token'"
                }
            }
        }

        $check = Invoke-RcoCommand -Arguments @("check", $fullPath) -InputText $null
        if ($check.ExitCode -ne 0) {
            Add-Failure $failures "Compile failed for '$word' ($relativePath):`n$($check.Output)"
            continue
        }
        $compiledCount++

        $arguments = $null
        $inputText = $null
        switch ($mode) {
            "run-sandboxed" {
                $arguments = @(
                    "run", "--capability-profile", "sandboxed", "--no-fs", "--no-http",
                    "--no-env", "--no-sleep", "--no-tui", "--no-webview", $fullPath
                )
            }
            "run-environment" {
                $env:RICOCHET_WORD_EXAMPLE = "present"
                $arguments = @(
                    "run", "--capability-profile", "sandboxed", "--no-fs", "--no-http",
                    "--no-sleep", "--no-tui", "--no-webview", "--env-allow",
                    "RICOCHET_WORD_EXAMPLE", $fullPath
                )
            }
            "run-filesystem-readonly" {
                $arguments = @(
                    "run", "--capability-profile", "sandboxed", "--fs-root", $RepoRoot,
                    "--fs-readonly", "--no-http", "--no-env", "--no-sleep", "--no-tui",
                    "--no-webview", $fullPath
                )
            }
            "run-webview" {
                $arguments = @(
                    "run", "--capability-profile", "sandboxed", "--no-fs", "--no-http",
                    "--no-env", "--no-sleep", "--no-tui", "--allow-webview", $fullPath
                )
            }
            "run-sleep" {
                $arguments = @(
                    "run", "--capability-profile", "trusted", "--no-fs", "--no-http",
                    "--no-env", "--no-tui", "--no-webview", $fullPath
                )
            }
            "run-stdin" {
                $arguments = @(
                    "run", "--capability-profile", "sandboxed", "--no-fs", "--no-http",
                    "--no-env", "--no-sleep", "--no-tui", "--no-webview", $fullPath
                )
                $inputText = "Ada"
            }
        }

        if ($null -ne $arguments) {
            $run = Invoke-RcoCommand -Arguments $arguments -InputText $inputText
            if ($run.ExitCode -ne 0) {
                Add-Failure $failures "Run failed for '$word' in mode '$mode' ($relativePath):`n$($run.Output)"
            }
            if (-not $runCounts.ContainsKey($mode)) {
                $runCounts[$mode] = 0
            }
            $runCounts[$mode]++
        }
    }

    $appsRoot = Join-Path $manifestRoot "apps"
    $manifestPaths = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::OrdinalIgnoreCase
    )
    foreach ($entry in $entries) {
        [void]$manifestPaths.Add(([string]$entry.path).Replace('/', [System.IO.Path]::DirectorySeparatorChar))
    }
    foreach ($file in Get-ChildItem -LiteralPath $appsRoot -Filter "*.rco" -File) {
        if ($file.Name -match '^\d{4}-') {
            $relative = Join-Path "apps" $file.Name
            if (-not $manifestPaths.Contains($relative)) {
                Add-Failure $failures "Unmanifested generated example: $relative"
            }
        } elseif ($file.Name.StartsWith("_", [System.StringComparison]::Ordinal)) {
            $check = Invoke-RcoCommand -Arguments @("check", $file.FullName) -InputText $null
            if ($check.ExitCode -ne 0) {
                Add-Failure $failures "Fixture compile failed ($($file.Name)):`n$($check.Output)"
            }
        }
    }
} finally {
    Pop-Location
}

if ($failures.Count -gt 0) {
    Write-Host "Word example validation failed:"
    foreach ($failure in $failures) {
        Write-Host " - $failure"
    }
    exit 1
}

$runSummary = @(
    $runCounts.Keys |
        Sort-Object |
        ForEach-Object { "$_=$($runCounts[$_])" }
) -join ", "
$runTotal = ($runCounts.Values | Measure-Object -Sum).Sum
$checkOnly = $entries.Count - $runTotal
Write-Host "Word example validation passed: $compiledCount compiled, $runTotal executed, $checkOnly host-context examples linked to integration evidence ($runSummary)."
