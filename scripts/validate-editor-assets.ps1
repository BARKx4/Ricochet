param()

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$PackagePath = Join-Path $Root "editors\vscode\package.json"
$ExtensionPath = Join-Path $Root "editors\vscode\extension.js"
$LanguageConfigurationPath = Join-Path $Root "editors\vscode\language-configuration.json"
$GrammarPath = Join-Path $Root "editors\vscode\syntaxes\ricochet.tmLanguage.json"
$DocsAppPath = Join-Path $Root "docs\reference\app.js"

foreach ($path in @($PackagePath, $ExtensionPath, $LanguageConfigurationPath, $GrammarPath, $DocsAppPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required editor validation input is missing: $path"
    }
}

$package = Get-Content -LiteralPath $PackagePath -Raw | ConvertFrom-Json
$languageConfiguration = Get-Content -LiteralPath $LanguageConfigurationPath -Raw | ConvertFrom-Json
$grammar = Get-Content -LiteralPath $GrammarPath -Raw | ConvertFrom-Json
$docsSource = Get-Content -LiteralPath $DocsAppPath -Raw

if ($package.contributes.languages[0].id -ne "ricochet") {
    throw "VS Code package must contribute the ricochet language id"
}
if ($package.main -ne "./extension.js") {
    throw "VS Code package must point main at extension.js"
}
if (-not ($package.activationEvents -contains "onLanguage:ricochet")) {
    throw "VS Code package must activate on the ricochet language"
}
if (-not ($package.activationEvents -contains "onCommand:ricochet.runWithStackVisualizer")) {
    throw "VS Code package must activate the stack visualizer command"
}
if (-not $package.dependencies."vscode-languageclient") {
    throw "VS Code package must depend on vscode-languageclient for LSP wiring"
}
if (-not $package.contributes.configuration.properties."ricochet.server.path") {
    throw "VS Code package must expose ricochet.server.path"
}
$commands = @($package.contributes.commands | ForEach-Object { $_.command })
if (-not ($commands -contains "ricochet.runWithStackVisualizer")) {
    throw "VS Code package must contribute the Ricochet stack visualizer command"
}
$extensionSource = Get-Content -LiteralPath $ExtensionPath -Raw
foreach ($marker in @("runWithStackVisualizer", "createWebviewPanel", "--trace-file")) {
    if (-not $extensionSource.Contains($marker)) {
        throw "VS Code extension is missing stack visualizer marker: $marker"
    }
}
if ($package.contributes.grammars[0].scopeName -ne "source.ricochet") {
    throw "VS Code grammar must use the source.ricochet scope"
}
if (-not $languageConfiguration.brackets) {
    throw "VS Code language configuration must define brackets"
}
if ($grammar.scopeName -ne "source.ricochet") {
    throw "TextMate grammar must use the source.ricochet scope"
}

function Collect-RegexPattern {
    param([object]$Node, [System.Collections.Generic.List[string]]$Patterns)

    if ($null -eq $Node) {
        return
    }

    if ($Node -is [System.Array]) {
        foreach ($item in $Node) {
            Collect-RegexPattern -Node $item -Patterns $Patterns
        }
        return
    }

    if ($Node -is [pscustomobject]) {
        foreach ($property in $Node.PSObject.Properties) {
            if ($property.Name -in @("match", "begin", "end", "firstLineMatch")) {
                if ($property.Value -is [string]) {
                    $Patterns.Add($property.Value) | Out-Null
                }
            } else {
                Collect-RegexPattern -Node $property.Value -Patterns $Patterns
            }
        }
    }
}

$patterns = [System.Collections.Generic.List[string]]::new()
Collect-RegexPattern -Node $grammar -Patterns $patterns

foreach ($pattern in $patterns) {
    try {
        [regex]::new($pattern) | Out-Null
    } catch {
        throw "TextMate regex does not compile under validation: $pattern`n$($_.Exception.Message)"
    }
}

$wordsMatch = [regex]::Match($docsSource, "(?s)const\s+WORDS\s*=\s*(\[.*?\]);")
if (-not $wordsMatch.Success) {
    throw "Could not find the WORDS catalog in docs/reference/app.js"
}

$words = $wordsMatch.Groups[1].Value | ConvertFrom-Json

function Test-RicochetTokenLiteral {
    param([string]$Word)

    if ([string]::IsNullOrWhiteSpace($Word)) {
        return $false
    }
    if ($Word -match "\s|/") {
        return $false
    }
    if ($Word -cmatch "^[A-Z][A-Za-z0-9_!?-]*$") {
        return $Word -in @(
            "Object", "Model", "Controller", "Result", "Array", "List", "Map", "Set",
            "Subclass", "Field", "Accessor", "Table", "Method",
            "GET", "POST", "PUT", "PATCH", "DELETE"
        )
    }
    return $Word -cmatch "^[a-z_][A-Za-z0-9_!?-]*$|^[+*/%<>=!-]+$"
}

$wordLiterals = [System.Collections.Generic.SortedSet[string]]::new()
foreach ($entry in $words) {
    if ($entry.word -is [string] -and (Test-RicochetTokenLiteral $entry.word)) {
        $wordLiterals.Add($entry.word) | Out-Null
    }
}

$grammarRegex = $patterns -join "`n"
$missing = @()
foreach ($word in $wordLiterals) {
    $escaped = [regex]::Escape($word)
    if (-not $grammarRegex.Contains($escaped)) {
        $missing += $word
    }
}

if ($missing.Count -gt 0) {
    throw "TextMate grammar is missing documented words: $($missing -join ', ')"
}

Write-Host "Ricochet editor asset validation passed."
