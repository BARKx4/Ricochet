Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$validatorPath = Join-Path $PSScriptRoot "validate-store-packaging.ps1"
$tokens = $null
$parseErrors = $null
$validatorAst = [System.Management.Automation.Language.Parser]::ParseFile(
    $validatorPath,
    [ref] $tokens,
    [ref] $parseErrors
)

if ($parseErrors.Count -gt 0) {
    throw "Store packaging validator did not parse: $($parseErrors[0].Message)"
}

$failures = [System.Collections.Generic.List[string]]::new()
$requiredFunctions = @(
    "Add-Error",
    "Assert-EntriesContain",
    "Assert-EntriesContainRegex",
    "Assert-DebContains"
)

foreach ($functionName in $requiredFunctions) {
    $definition = $validatorAst.Find({
            param($node)
            $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
                $node.Name -eq $functionName
        }, $true)
    if ($null -eq $definition) {
        $failures.Add("Store packaging validator is missing function '$functionName'.") | Out-Null
        continue
    }
    . ([scriptblock]::Create($definition.Extent.Text))
}

foreach ($noticeName in @(
        "THIRD_PARTY_LICENSES.html",
        "THIRD_PARTY_NOTICES.txt"
    )) {
    if (Get-Command Assert-EntriesContain -ErrorAction SilentlyContinue) {
        $caseErrors = [System.Collections.Generic.List[string]]::new()
        Assert-EntriesContain $caseErrors @($noticeName.ToLowerInvariant()) "synthetic.zip" @($noticeName)
        if ($caseErrors.Count -ne 1) {
            $failures.Add("Windows ZIP matching accepted a mis-cased root entry for '$noticeName'.") | Out-Null
        }

        $depthErrors = [System.Collections.Generic.List[string]]::new()
        Assert-EntriesContain $depthErrors @("nested/$noticeName") "synthetic.zip" @($noticeName)
        if ($depthErrors.Count -ne 1) {
            $failures.Add("Windows ZIP matching accepted a nested entry for '$noticeName'.") | Out-Null
        }
    }

    if (Get-Command Assert-EntriesContainRegex -ErrorAction SilentlyContinue) {
        $archivePattern = '^[^/]+/{0}$' -f [regex]::Escape($noticeName)

        foreach ($invalidEntry in @(
                $noticeName.ToLowerInvariant(),
                "outer/nested/$noticeName"
            )) {
            $archiveErrors = [System.Collections.Generic.List[string]]::new()
            Assert-EntriesContainRegex $archiveErrors @($invalidEntry) "synthetic.tar.gz" @($archivePattern)
            if ($archiveErrors.Count -ne 1) {
                $failures.Add("Unix archive regex accepted invalid entry '$invalidEntry'.") | Out-Null
            }
        }

        $validArchiveErrors = [System.Collections.Generic.List[string]]::new()
        Assert-EntriesContainRegex $validArchiveErrors @("package/$noticeName") "synthetic.tar.gz" @($archivePattern)
        if ($validArchiveErrors.Count -ne 0) {
            $failures.Add("Unix archive regex rejected exact one-root entry 'package/$noticeName'.") | Out-Null
        }
    }

    if (Get-Command Assert-DebContains -ErrorAction SilentlyContinue) {
        $debPattern = 'usr/share/doc/ricochet/{0}$' -f [regex]::Escape($noticeName)
        $debErrors = [System.Collections.Generic.List[string]]::new()
        Assert-DebContains $debErrors @("./usr/share/doc/ricochet/$($noticeName.ToLowerInvariant())") $debPattern
        if ($debErrors.Count -ne 1) {
            $failures.Add("Debian matching accepted a mis-cased documentation entry for '$noticeName'.") | Out-Null
        }
    }
}

if ($failures.Count -gt 0) {
    $details = $failures | ForEach-Object { " - $_" }
    throw "Store packaging entry contract tests failed:`n$($details -join "`n")"
}

Write-Host "Store packaging entry contract tests passed."
