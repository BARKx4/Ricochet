[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot

$requiredFiles = @(
    'RICOCHET_2_PLAN.md',
    'architecture/README.md',
    'architecture/adr/ADR-001-typed-postfix-surface.md',
    'architecture/adr/ADR-002-type-and-stack-solver.md',
    'architecture/adr/ADR-003-managed-heap-and-resources.md',
    'architecture/adr/ADR-004-object-and-value-representation.md',
    'architecture/adr/ADR-005-effects-and-capability-authority.md',
    'architecture/adr/ADR-006-async-runtime.md',
    'architecture/adr/ADR-007-backend-bakeoff.md',
    'architecture/adr/ADR-008-modules-environments-packages-and-trust.md',
    'architecture/adr/ADR-009-application-platform-boundaries.md',
    'architecture/adr/ADR-010-compatibility-and-release-policy.md'
)

foreach ($relativePath in $requiredFiles) {
    $path = Join-Path $repoRoot $relativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing Ricochet 2 governance file: $relativePath"
    }
}

$plan = Get-Content -LiteralPath (Join-Path $repoRoot 'RICOCHET_2_PLAN.md') -Raw
$requiredPlanText = @(
    '| 1.x | Ricochet | `rco` | `main` | `v1.0.x` |',
    '| 2.x | Ricochet | `ricochet` | `ricochet-2` | `ricochet-v2.*` |',
    '| Project environment | `.rvenv`',
    '| Release artifacts | `ricochet-v2.<semver>-<target>.<format>`'
)

foreach ($expected in $requiredPlanText) {
    if (-not $plan.Contains($expected)) {
        throw "Ricochet 2 plan is missing required identity text: $expected"
    }
}

$expectedAdrStatuses = [ordered]@{
    'architecture/adr/ADR-001-typed-postfix-surface.md' = 'Status: Proposed'
    'architecture/adr/ADR-002-type-and-stack-solver.md' = 'Status: Proposed'
    'architecture/adr/ADR-003-managed-heap-and-resources.md' = 'Status: Proposed'
    'architecture/adr/ADR-004-object-and-value-representation.md' = 'Status: Open'
    'architecture/adr/ADR-005-effects-and-capability-authority.md' = 'Status: Open'
    'architecture/adr/ADR-006-async-runtime.md' = 'Status: Open'
    'architecture/adr/ADR-007-backend-bakeoff.md' = 'Status: Open'
    'architecture/adr/ADR-008-modules-environments-packages-and-trust.md' = 'Status: Open'
    'architecture/adr/ADR-009-application-platform-boundaries.md' = 'Status: Open'
    'architecture/adr/ADR-010-compatibility-and-release-policy.md' = 'Status: Accepted'
}

foreach ($entry in $expectedAdrStatuses.GetEnumerator()) {
    $content = Get-Content -LiteralPath (Join-Path $repoRoot $entry.Key) -Raw
    if (-not $content.Contains($entry.Value)) {
        throw "$($entry.Key) must contain '$($entry.Value)'."
    }
}

$ci = Get-Content -LiteralPath (Join-Path $repoRoot '.github/workflows/ci.yml') -Raw
if (-not $ci.Contains('name: Ricochet 2 CI')) {
    throw 'The ricochet-2 branch must retain the distinct Ricochet 2 CI identity.'
}

$releaseContract = Get-Content -LiteralPath (
    Join-Path $repoRoot '.github/workflows/ricochet-2-release-contract.yml'
) -Raw
if (-not $releaseContract.Contains('name: Ricochet 2 Release Contract')) {
    throw 'The Ricochet 2 release-contract workflow has lost its distinct identity.'
}

Write-Host 'Ricochet 2 governance contract passed.'
