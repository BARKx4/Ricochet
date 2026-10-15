Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Extract only the pure filter functions; do not run the generator or use the network.
$Ordinal = [System.StringComparer]::Ordinal
$generatorPath = Join-Path $PSScriptRoot "generate-third-party-notices.ps1"
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
    $generatorPath, [ref]$tokens, [ref]$parseErrors
)
if ($parseErrors.Count -ne 0) {
    throw "Third-party notice generator must parse before filter qualification"
}
foreach ($functionName in @("Normalize-Text", "Select-ActiveCargoAboutHtml")) {
    $node = $ast.Find({
        param($candidate)
        $candidate -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
        $candidate.Name -eq $functionName
    }, $true)
    if ($null -eq $node) { throw "Missing pure filter function: $functionName" }
    . ([scriptblock]::Create($node.Extent.Text))
}

$components = '<table><tr><td><code>active</code></td><td><code>1</code></td></tr></table>'
$active = '<section><h3>active license</h3><p>Used by:</p><ul><li><code>active 1</code></li></ul><pre>Required text with &lt;escaped&gt; markup</pre></section>'
$inactive = '<section><h3>inactive license</h3><p>Used by:</p><ul><li><code>inactive 1</code></li></ul><pre>Inactive text</pre></section>'
foreach ($sections in @($active + $inactive, $inactive + $active, $active + $inactive + $active)) {
    $filtered = Select-ActiveCargoAboutHtml -Html ($components + $sections) -DependencyIdentities @("active@1")
    $expectedCount = [regex]::Matches($sections, 'Required text').Count
    if ([regex]::Matches($filtered, 'Required text').Count -ne $expectedCount) {
        throw "Filtering an inactive section discarded a required active license text"
    }
    if ($filtered.Contains("Inactive text")) {
        throw "Filtered report retained a license section with no active dependency"
    }
    if (-not $filtered.Contains("&lt;escaped&gt;")) {
        throw "Filtering changed escaped license text"
    }
}
Write-Host "Third-party notice filter regression tests passed."
