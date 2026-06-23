param(
    [string]$Root
)

$ErrorActionPreference = "Stop"

function Find-RepoRoot {
    param([string]$Start)

    $current = (Resolve-Path -LiteralPath $Start).Path
    while ($current) {
        if ((Test-Path -LiteralPath (Join-Path $current "Cargo.toml") -PathType Leaf) -and
            (Test-Path -LiteralPath (Join-Path $current ".git"))) {
            return $current
        }

        $parent = Split-Path -Parent $current
        if ($parent -eq $current) {
            break
        }
        $current = $parent
    }

    throw "Could not locate the Ricochet repository root from $Start"
}

function Add-Failure {
    param([System.Collections.Generic.List[string]]$Failures, [string]$Message)
    [void]$Failures.Add($Message)
}

function Read-JsonFile {
    param([string]$Path)

    try {
        return (Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json)
    } catch {
        throw "Failed to parse JSON file $Path`: $($_.Exception.Message)"
    }
}

function Resolve-CoveragePath {
    param([string]$LearnRoot, [string]$PrimaryChapter)

    $target = $PrimaryChapter.Trim()
    if ($target -eq "appendix") {
        return Join-Path $LearnRoot "appendices/a-word-catalog.md"
    }

    if ($target -match '^(chapters|appendices)[/\\]') {
        $relative = $target -replace '/', '\'
        if (-not [System.IO.Path]::HasExtension($relative)) {
            $relative = "$relative.md"
        }
        return Join-Path $LearnRoot $relative
    }

    if ($target -match '^appendix[-_:]') {
        $slug = ($target -replace '^appendix[-_:]', '')
        if (-not $slug) {
            $slug = "a-word-catalog"
        }
        if (-not [System.IO.Path]::HasExtension($slug)) {
            $slug = "$slug.md"
        }
        return Join-Path (Join-Path $LearnRoot "appendices") $slug
    }

    if (-not [System.IO.Path]::HasExtension($target)) {
        $target = "$target.md"
    }
    return Join-Path (Join-Path $LearnRoot "chapters") $target
}

function Convert-CommandToText {
    param($Command)

    if ($Command -is [array]) {
        $parts = foreach ($part in $Command) {
            $text = [string]$part
            "'" + ($text -replace "'", "''") + "'"
        }
        return ($parts -join " ")
    }

    return [string]$Command
}

function Invoke-ExampleCommand {
    param([string]$CommandText, [string]$WorkingDirectory)

    $encodedCommand = [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes($CommandText))
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = "powershell.exe"
    $startInfo.Arguments = "-NoProfile -ExecutionPolicy Bypass -EncodedCommand $encodedCommand"
    $startInfo.WorkingDirectory = $WorkingDirectory
    $startInfo.UseShellExecute = $false

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    [void]$process.Start()

    if (-not $process.WaitForExit(120000)) {
        try {
            $process.Kill()
        } catch {
        }
        return [pscustomobject]@{
            ExitCode = -1
            Error = "Command timed out after 120 seconds."
        }
    }

    return [pscustomobject]@{
        ExitCode = $process.ExitCode
        Error = ""
    }
}

$repoRoot = if ($Root) {
    (Resolve-Path -LiteralPath $Root).Path
} else {
    Find-RepoRoot -Start $PSScriptRoot
}

$failures = New-Object System.Collections.Generic.List[string]
$learnRoot = Join-Path $repoRoot "docs/learn"
$coveragePath = Join-Path $learnRoot "word-coverage.json"
$examplesManifestPath = Join-Path $repoRoot "examples/learn/examples.json"

Push-Location $repoRoot
try {
    $wordsJson = & cargo run -q -p ricochet_cli --bin rco -- words --json
    if ($LASTEXITCODE -ne 0) {
        Add-Failure $failures "Failed to run live word inventory command."
        $wordsJson = $null
    }
} finally {
    Pop-Location
}

$liveWords = @()
if ($wordsJson) {
    try {
        $wordsJsonText = $wordsJson -join [Environment]::NewLine
        $parsedLiveWords = ConvertFrom-Json -InputObject $wordsJsonText
        $liveWords = @($parsedLiveWords | ForEach-Object { $_ })
    } catch {
        Add-Failure $failures "Live word inventory was not valid JSON: $($_.Exception.Message)"
    }
}

$liveByWord = New-Object "System.Collections.Generic.Dictionary``2[System.String,System.String]"
foreach ($row in $liveWords) {
    if ($null -eq $row.word -or $null -eq $row.detail) {
        Add-Failure $failures "Live word inventory row is missing word or detail."
        continue
    }
    $liveByWord[[string]$row.word] = [string]$row.detail
}

if (-not (Test-Path -LiteralPath $coveragePath -PathType Leaf)) {
    Add-Failure $failures "Missing coverage file: docs/learn/word-coverage.json"
    $coverageRows = @()
} else {
    try {
        $coverageRows = @(Read-JsonFile -Path $coveragePath)
    } catch {
        Add-Failure $failures $_.Exception.Message
        $coverageRows = @()
    }
}

$allowedStatuses = @("planned", "drafted", "validated", "appendix")
$coverageByWord = New-Object "System.Collections.Generic.Dictionary``2[System.String,System.Object]"
$seenWords = New-Object "System.Collections.Generic.HashSet``1[System.String]"

foreach ($row in $coverageRows) {
    $properties = @($row.PSObject.Properties.Name)
    foreach ($field in @("word", "detail", "primary_chapter", "status")) {
        if ($properties -notcontains $field) {
            Add-Failure $failures "Coverage row is missing required field '$field'."
        } elseif ([string]::IsNullOrWhiteSpace([string]$row.$field)) {
            Add-Failure $failures "Coverage row has an empty '$field' value."
        }
    }

    if ([string]::IsNullOrWhiteSpace([string]$row.word)) {
        continue
    }

    $word = [string]$row.word
    if (-not $seenWords.Add($word)) {
        Add-Failure $failures "Coverage contains duplicate word: $word"
    }
    $coverageByWord[$word] = $row

    if ($allowedStatuses -notcontains [string]$row.status) {
        Add-Failure $failures "Coverage row for '$word' has invalid status '$($row.status)'."
    }

    if (-not $liveByWord.ContainsKey($word)) {
        Add-Failure $failures "Coverage contains stale word not present in live inventory: $word"
    } elseif ([string]$row.detail -ne $liveByWord[$word]) {
        Add-Failure $failures "Coverage detail mismatch for '$word': expected '$($liveByWord[$word])', found '$($row.detail)'."
    }
}

foreach ($word in ($liveByWord.Keys | Sort-Object)) {
    if (-not $coverageByWord.ContainsKey($word)) {
        Add-Failure $failures "Coverage is missing live word: $word"
    }
}

$learnMarkdownFiles = @()
if (Test-Path -LiteralPath $learnRoot -PathType Container) {
    $learnMarkdownFiles = @(Get-ChildItem -LiteralPath $learnRoot -Recurse -Filter "*.md" -File -ErrorAction SilentlyContinue)
}

if ($learnMarkdownFiles.Count -gt 0) {
    foreach ($row in $coverageRows) {
        if ([string]::IsNullOrWhiteSpace([string]$row.primary_chapter)) {
            continue
        }

        $targetPath = Resolve-CoveragePath -LearnRoot $learnRoot -PrimaryChapter ([string]$row.primary_chapter)
        if (-not (Test-Path -LiteralPath $targetPath -PathType Leaf)) {
            $relativeTarget = Resolve-Path -LiteralPath (Split-Path -Parent $targetPath) -ErrorAction SilentlyContinue
            Add-Failure $failures "Coverage row for '$($row.word)' points at missing manual file: $targetPath"
        }
    }
}

foreach ($file in $learnMarkdownFiles) {
    $content = Get-Content -LiteralPath $file.FullName -Raw
    $insideRawBlock = $false
    $lines = $content -split "`r?`n"

    for ($i = 0; $i -lt $lines.Count; $i++) {
        $line = $lines[$i]
        if ($line -match '\{%\s*raw\s*%\}') {
            $insideRawBlock = $true
            continue
        }
        if ($line -match '\{%\s*endraw\s*%\}') {
            if (-not $insideRawBlock) {
                Add-Failure $failures "Learn Markdown file has an unmatched Jekyll raw end marker at $($file.FullName):$($i + 1)"
            }
            $insideRawBlock = $false
            continue
        }
        if (-not $insideRawBlock -and $line -match '(\{%|\{\{)') {
            Add-Failure $failures "Learn Markdown file contains an unguarded Liquid marker at $($file.FullName):$($i + 1)"
        }
    }

    if ($insideRawBlock) {
        Add-Failure $failures "Learn Markdown file has an unclosed Jekyll raw block: $($file.FullName)"
    }

    if ($content -match '(?im)manual_status\s*[:=]\s*(complete|completed|validated)\b') {
        for ($i = 0; $i -lt $lines.Count; $i++) {
            if ($lines[$i] -match '(?i)\b(TODO|TBD|FIXME)\b') {
                Add-Failure $failures "Completed manual file contains placeholder marker at $($file.FullName):$($i + 1)"
            }
        }
    }
}

if (Test-Path -LiteralPath $examplesManifestPath -PathType Leaf) {
    try {
        $manifest = Read-JsonFile -Path $examplesManifestPath
        $entries = if ($manifest -is [array]) {
            @($manifest)
        } elseif ($manifest.PSObject.Properties.Name -contains "examples") {
            @($manifest.examples)
        } else {
            @($manifest)
        }

        foreach ($entry in $entries) {
            $expectedStatus = [string]$entry.expected_status
            if ($expectedStatus -ne "success" -or $null -eq $entry.command) {
                continue
            }

            $commandText = Convert-CommandToText -Command $entry.command
            if ([string]::IsNullOrWhiteSpace($commandText)) {
                continue
            }

            $result = Invoke-ExampleCommand -CommandText $commandText -WorkingDirectory $repoRoot
            if ($result.ExitCode -ne 0) {
                Add-Failure $failures "Learn example command failed with exit code $($result.ExitCode): $commandText`n$result.Error"
            }
        }
    } catch {
        Add-Failure $failures "Failed to validate examples manifest: $($_.Exception.Message)"
    }
}

if ($failures.Count -gt 0) {
    Write-Host "Learn manual validation failed:"
    foreach ($failure in $failures) {
        Write-Host " - $failure"
    }
    exit 1
}

Write-Host "Learn manual validation passed: $($liveByWord.Count) live words covered."
