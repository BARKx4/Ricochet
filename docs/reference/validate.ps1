param(
    [string]$Root = (Split-Path -Parent $MyInvocation.MyCommand.Path)
)

$ErrorActionPreference = "Stop"

$requiredFiles = @(
    "index.html",
    "styles.css",
    "app.js",
    "README.md"
)

$failures = New-Object System.Collections.Generic.List[string]

foreach ($file in $requiredFiles) {
    $path = Join-Path $Root $file
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        $failures.Add("Missing required docs file: $file")
    }
}

if ($failures.Count -eq 0) {
    $index = Get-Content -LiteralPath (Join-Path $Root "index.html") -Raw
    $styles = Get-Content -LiteralPath (Join-Path $Root "styles.css") -Raw
    $app = Get-Content -LiteralPath (Join-Path $Root "app.js") -Raw
    $readme = Get-Content -LiteralPath (Join-Path $Root "README.md") -Raw

    $requiredIndexMarkers = @(
        "Ricochet Reference",
        "id=""syntax""",
        "id=""words""",
        "id=""oop""",
        "id=""mvc""",
        "id=""active-record""",
        "id=""debugging""",
        "id=""cli""",
        "id=""limits"""
    )

    foreach ($marker in $requiredIndexMarkers) {
        if (-not $index.Contains($marker)) {
            $failures.Add("index.html is missing marker: $marker")
        }
    }

    $requiredWords = @(
        '"+"',
        '"add"',
        '"equals"',
        '"not-equals?"',
        '"assert-equals"',
        '"less-than?"',
        '"greater-than?"',
        '"less-or-equals?"',
        '"greater-or-equals?"',
        '"self"',
        '"get"',
        '"set"',
        '"var"',
        '"field"',
        '"table"',
        '"subclass"',
        '"new"',
        '"swap"',
        '"dup"',
        '"drop"',
        '"over"',
        '"rot"',
        '"call"',
        '"send"',
        '"println"',
        '"view"',
        '"text"',
        '"json"',
        '"value"',
        '"error"',
        '"array"',
        '"map"',
        '"!push"',
        '"!put"',
        '"!method"',
        '"ok?"',
        '"nil?"',
        '"empty?"'
    )

    foreach ($word in $requiredWords) {
        if (-not $app.Contains($word)) {
            $failures.Add("app.js is missing reference entry for: $word")
        }
    }

    $requiredExamples = @(
        "User Model subclass",
        "HomeController Controller subclass",
        'GET "/" HomeController "index" route',
        "User .all",
        "className get `"Object`" subclass",
        "rco run --debug --step app.rco",
        "{ user get .name get }"
    )

    foreach ($example in $requiredExamples) {
        if (-not $index.Contains($example) -and -not $app.Contains($example)) {
            $failures.Add("Docs are missing example text: $example")
        }
    }

    $requiredCss = @(
        "--ink",
        ".word-grid",
        ".stack-rail",
        "@media"
    )

    foreach ($marker in $requiredCss) {
        if (-not $styles.Contains($marker)) {
            $failures.Add("styles.css is missing marker: $marker")
        }
    }

    if (-not $readme.Contains("Open index.html")) {
        $failures.Add("README.md does not explain how to open the static site")
    }
}

if ($failures.Count -gt 0) {
    foreach ($failure in $failures) {
        Write-Error $failure
    }
    exit 1
}

Write-Host "Ricochet reference docs validation passed."
