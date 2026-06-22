param(
    [string]$Root,
    [switch]$Force
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

function HtmlEncode {
    param([string]$Text)
    return [System.Net.WebUtility]::HtmlEncode($Text)
}

function Render-MinimalMarkdown {
    param([string]$Markdown)

    $lines = $Markdown -split "`r?`n"
    $html = New-Object System.Collections.Generic.List[string]
    $inCode = $false
    $inList = $false

    foreach ($line in $lines) {
        if ($line -match '^```') {
            if ($inList) {
                [void]$html.Add("</ul>")
                $inList = $false
            }
            if ($inCode) {
                [void]$html.Add("</code></pre>")
                $inCode = $false
            } else {
                [void]$html.Add("<pre><code>")
                $inCode = $true
            }
            continue
        }

        if ($inCode) {
            [void]$html.Add((HtmlEncode $line))
            continue
        }

        if ([string]::IsNullOrWhiteSpace($line)) {
            if ($inList) {
                [void]$html.Add("</ul>")
                $inList = $false
            }
            continue
        }

        if ($line -match '^(#{1,3})\s+(.+)$') {
            if ($inList) {
                [void]$html.Add("</ul>")
                $inList = $false
            }
            $level = $Matches[1].Length
            [void]$html.Add("<h$level>$(HtmlEncode $Matches[2])</h$level>")
            continue
        }

        if ($line -match '^\s*-\s+(.+)$') {
            if (-not $inList) {
                [void]$html.Add("<ul>")
                $inList = $true
            }
            [void]$html.Add("<li>$(HtmlEncode $Matches[1])</li>")
            continue
        }

        if ($inList) {
            [void]$html.Add("</ul>")
            $inList = $false
        }
        [void]$html.Add("<p>$(HtmlEncode $line)</p>")
    }

    if ($inCode) {
        [void]$html.Add("</code></pre>")
    }
    if ($inList) {
        [void]$html.Add("</ul>")
    }

    return ($html -join "`n")
}

$repoRoot = if ($Root) {
    (Resolve-Path -LiteralPath $Root).Path
} else {
    Find-RepoRoot -Start $PSScriptRoot
}

$learnRoot = Join-Path $repoRoot "docs/learn"
if (-not (Test-Path -LiteralPath $learnRoot -PathType Container)) {
    throw "Missing manual source directory: docs/learn"
}

$sourcePath = Join-Path $learnRoot "index.md"
$outputDir = Join-Path $repoRoot "docs/reference/learn"
$outputPath = Join-Path $outputDir "index.html"

if (Test-Path -LiteralPath $sourcePath -PathType Leaf) {
    $markdown = Get-Content -LiteralPath $sourcePath -Raw
} else {
    $markdown = @"
# Learn Ricochet

The Learn Ricochet manual source will render here when docs/learn/index.md is available.
"@
}

$body = Render-MinimalMarkdown -Markdown $markdown
$page = @"
<!doctype html>
<html lang="en" data-generated-by="render-learn-manual">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Learn Ricochet</title>
  <link rel="stylesheet" href="../styles.css">
</head>
<body class="guide-page">
  <header class="topbar">
    <a class="brand" href="../index.html#top" aria-label="Ricochet Reference home">
      <span class="brand-mark">rco</span>
      <span>Ricochet Reference</span>
    </a>
    <nav class="nav-links" aria-label="Reference sections">
      <a href="../index.html#syntax">Syntax</a>
      <a href="../index.html#words">Words</a>
      <a href="../index.html#oop">OOP</a>
      <a href="../index.html#mvc">MVC</a>
      <a href="../index.html#cli">CLI</a>
      <a href="index.html">Learn</a>
      <a href="../guides/index.html">Guides</a>
    </nav>
  </header>

  <main id="top">
    <section class="hero section-band">
      <div class="hero-copy">
        <p class="eyebrow">Manual preview</p>
        <h1>Learn Ricochet</h1>
        <p class="lede">A planned guided manual for learning Ricochet from the stack model through real applications, packages, and release workflows.</p>
      </div>
    </section>

    <section class="section-band" aria-labelledby="manual-chapters-title">
      <div class="section-heading">
        <p class="eyebrow">Manual Chapters</p>
        <h2 id="manual-chapters-title">Manual Chapters</h2>
        <p class="guide-kicker"><a href="../index.html">Use The Reference Today</a> or browse <a href="../guides/index.html">the current guides</a> while the full manual pages are drafted.</p>
      </div>
      <div class="guide-kicker">
$body
      </div>
    </section>
  </main>

  <footer class="footer">
    <span>Learn Ricochet</span>
    <a href="../index.html">Back to Reference</a>
  </footer>
  <script src="../app.js"></script>
</body>
</html>
"@

New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
$shouldWrite = $true
if ((Test-Path -LiteralPath $outputPath -PathType Leaf) -and -not $Force) {
    $existing = Get-Content -LiteralPath $outputPath -Raw
    if (-not $existing.Contains('data-generated-by="render-learn-manual"')) {
        $shouldWrite = $false
    }
}

if (-not $shouldWrite) {
    Write-Host "Skipped existing hand-authored Learn page: $outputPath"
    Write-Host "Use -Force to replace it with generated manual HTML."
    exit 0
}

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($outputPath, $page, $utf8NoBom)

Write-Host "Rendered Learn Ricochet index: $outputPath"
