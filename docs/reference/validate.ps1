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
        "id=""turing-complete""",
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
        '"-"',
        '"subtract"',
        '"*"',
        '"/"',
        '"%"',
        '"negate"',
        '"abs"',
        '"min"',
        '"max"',
        '"clamp"',
        '"not"',
        '"and"',
        '"or"',
        '"equals"',
        '"not-equals?"',
        '"assert"',
        '"assert-true"',
        '"assert-false"',
        '"assert-equals"',
        '"assert-ok"',
        '"assert-error"',
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
        '"nip"',
        '"tuck"',
        '"pick"',
        '"roll"',
        '"depth"',
        '"clear"',
        '"call"',
        '"spawn"',
        '"await"',
        '"tasks"',
        '"while"',
        '"break"',
        '"continue"',
        '"send"',
        '"println"',
        '"view"',
        '"text"',
        '"json"',
        '"redirect"',
        '"status"',
        '"header"',
        '"value"',
        '"error"',
        '"array"',
        '"list"',
        '"map"',
        '"Set"',
        '"range"',
        '"!push"',
        '"!put"',
        '".push!"',
        '".put!"',
        '".insert!"',
        '".remove!"',
        '".remove-at!"',
        '".clear!"',
        '".count"',
        '".at"',
        '".first"',
        '".last"',
        '".take"',
        '".skip"',
        '".reverse"',
        '".has?"',
        '".keys"',
        '".values"',
        '".each"',
        '".transform"',
        '".select"',
        '".reduce"',
        '".find"',
        '".default-page"',
        '".limit"',
        '".any?"',
        '".all?"',
        '".join"',
        '".trim"',
        '".trim-start"',
        '".trim-end"',
        '".blank?"',
        '".slice"',
        '".index-of"',
        '".last-index-of"',
        '".repeat"',
        '".lines"',
        '".chars"',
        '".split"',
        '".replace"',
        '".contains?"',
        '".starts-with?"',
        '".ends-with?"',
        '".uppercase"',
        '".lowercase"',
        '".concat"',
        '".to-number"',
        '"to-string"',
        '"json-encode"',
        '"json-decode"',
        '"regex"',
        '".matches?"',
        '".captures"',
        '"!method"',
        '"ok?"',
        '"ok"',
        '"fail"',
        '".error?"',
        '".unwrap-or"',
        '".map-result"',
        '".and-then"',
        '"nil?"',
        '"empty?"',
        '"print"',
        '"eprint"',
        '"read-line"',
        '"args"',
        '"env"',
        '"cwd"',
        '"now"',
        '"sleep"',
        '"random"',
        '"exit"',
        '"fs"',
        '".read-text"',
        '".write-text!"',
        '".exists?"',
        '".list"',
        '".create-dir!"',
        '"http"',
        '".get-task"',
        '".post-json"',
        '".post-json-task"',
        '"inspect"',
        '"debug"',
        '"type"',
        '"class-of"',
        '"instance-of?"',
        '"responds-to?"',
        '".id"',
        '".status"',
        '".pending?"',
        '"fields"',
        '"methods"',
        '"callable?"'
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
        "10 User .limit",
        "User .count",
        "User .first",
        "1 User .exists?",
        "users array",
        "settings map",
        "tags Set",
        ".push!",
        ".put!",
        ".slice",
        "regex value",
        "className get `"Object`" subclass",
        "multiplier get 0 &gt; while",
        "rco run --debug --step app.rco",
        "rco fmt [--check] [path]",
        "rco run-bytecode [--debug] [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--http-allow-host HOST] &lt;path&gt; [args...]",
        "rco package [path] --output &lt;exe&gt;",
        "rco add &lt;source&gt; [--name NAME] [--no-fetch]",
        "`"lib/math`" import",
        "`"greeter/greeting`" import",
        "[dependencies.greeter]",
        "rco add github:BARKx4/ricochet_auth@v0.1.0 --no-fetch",
        "`"/dashboard`" redirect",
        "rco routes [path]",
        "rco test [--debug] [--filter PATTERN] [--capability-profile trusted|sandboxed] [--no-fs] [--fs-root PATH] [--fs-readonly] [--no-http] [--http-allow-host HOST] [path]",
        "task get .status",
        "tasks .count",
        "fs .read-text",
        "http .get",
        "http .get-task",
        "{ `$user .name get }"
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
