Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$validatorPath = Join-Path $PSScriptRoot "validate-learn-manual.ps1"
$source = [System.IO.File]::ReadAllText($validatorPath)
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
    $validatorPath,
    [ref]$tokens,
    [ref]$parseErrors
)
if ($parseErrors.Count -gt 0) {
    throw "Learn validator did not parse: $($parseErrors[0].Message)"
}

$failures = [System.Collections.Generic.List[string]]::new()
$resolver = $ast.Find({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
            $node.Name -eq "Resolve-CoveragePath"
    }, $true)
if ($null -eq $resolver) {
    $failures.Add("Learn validator is missing Resolve-CoveragePath.") | Out-Null
}
else {
    . ([scriptblock]::Create($resolver.Extent.Text))
    $syntheticRoot = Join-Path ([System.IO.Path]::GetTempPath()) "ricochet-learn-contract"
    $cases = [ordered]@{
        "06-numbers-math-and-truth" = "chapters\06-numbers-math-and-truth.html"
        "chapters/11-oop-and-dispatch.md" = "chapters\11-oop-and-dispatch.html"
        "appendix" = "appendices\a-word-catalog.html"
        "appendix-a-word-catalog" = "appendices\a-word-catalog.html"
    }
    foreach ($case in $cases.GetEnumerator()) {
        $actual = Resolve-CoveragePath -LearnRoot $syntheticRoot -PrimaryChapter $case.Key
        $expected = Join-Path $syntheticRoot $case.Value
        if ($actual -cne $expected) {
            $failures.Add("Coverage target '$($case.Key)' resolved to '$actual', expected public HTML '$expected'.") | Out-Null
        }
    }
}

if ($source -match '(?ms)if \(\$learnMarkdownFiles\.Count -gt 0\) \{\s*foreach \(\$row in \$coverageRows\)') {
    $failures.Add("Coverage target validation is incorrectly disabled when no Learn Markdown exists.") | Out-Null
}
if ($source -notmatch '\$learnHtmlFiles') {
    $failures.Add("Learn validator does not enumerate the canonical public HTML manual.") | Out-Null
}
if ($source -notmatch 'Learn HTML file contains an unrendered Liquid marker') {
    $failures.Add("RequireJekyllRawBlocks does not detect unrendered Liquid markers in canonical HTML.") | Out-Null
}

if ($failures.Count -gt 0) {
    $details = $failures | ForEach-Object { " - $_" }
    throw "Learn validator contract tests failed:`n$($details -join "`n")"
}

Write-Host "Learn validator contract tests passed."
