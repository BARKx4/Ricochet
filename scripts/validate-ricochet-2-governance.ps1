[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot

$requiredFiles = @(
    'RICOCHET_2_PLAN.md',
    '.github/workflows/ricochet-2-ci.yml',
    '.github/workflows/ricochet-2-codeql.yml',
    '.github/workflows/ricochet-2-release-contract.yml',
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
    'architecture/adr/ADR-010-compatibility-and-release-policy.md',
    'prototypes/adr-001-surface/Cargo.toml',
    'prototypes/adr-001-surface/src/lib.rs',
    'prototypes/adr-001-surface/src/main.rs',
    'prototypes/adr-001-surface/fixtures/typed_postfix.ricochet',
    'prototypes/adr-001-surface/fixtures/invalid_surface.ricochet',
    'prototypes/adr-001-surface/PROOF.html'
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
if (-not $ci.Contains('name: CI') -or
    -not $ci.Contains('branches: ["main"]') -or
    -not $ci.Contains('workflow_call:')) {
    throw 'The inherited CI definition must be reusable and direct-run only on main.'
}

$workspaceManifest = Get-Content -LiteralPath (Join-Path $repoRoot 'Cargo.toml') -Raw
if (-not $workspaceManifest.Contains('"prototypes/adr-001-surface"')) {
    throw 'The preserved ADR-001 surface proof must remain a tested workspace member.'
}

$prototypeManifest = Get-Content -LiteralPath (
    Join-Path $repoRoot 'prototypes/adr-001-surface/Cargo.toml'
) -Raw
$requiredPrototypeManifestText = @(
    'name = "ricochet2_surface_prototype"',
    'version = "0.0.0"',
    'publish = false',
    'name = "ricochet2-surface-proof"'
)
foreach ($expected in $requiredPrototypeManifestText) {
    if (-not $prototypeManifest.Contains($expected)) {
        throw "ADR-001 prototype manifest is missing its evidence boundary: $expected"
    }
}

$prototypeProof = Get-Content -LiteralPath (
    Join-Path $repoRoot 'prototypes/adr-001-surface/PROOF.html'
) -Raw
if (-not $prototypeProof.Contains('architecture evidence') -or
    -not $prototypeProof.Contains('not the production Ricochet 2 frontend')) {
    throw 'The ADR-001 proof must state that it is preserved evidence, not the production frontend.'
}

$v2Ci = Get-Content -LiteralPath (
    Join-Path $repoRoot '.github/workflows/ricochet-2-ci.yml'
) -Raw
if (-not $v2Ci.Contains('name: Ricochet 2 CI')) {
    throw 'The ricochet-2 branch must retain the distinct Ricochet 2 CI identity.'
}

$codeql = Get-Content -LiteralPath (Join-Path $repoRoot '.github/workflows/codeql.yml') -Raw
if (-not $codeql.Contains('name: "CodeQL Advanced"') -or
    -not $codeql.Contains('branches: [ "main" ]') -or
    -not $codeql.Contains('workflow_call:')) {
    throw 'The inherited CodeQL definition must be reusable and direct-run only on main.'
}

$v2Codeql = Get-Content -LiteralPath (
    Join-Path $repoRoot '.github/workflows/ricochet-2-codeql.yml'
) -Raw
if (-not $v2Codeql.Contains('name: Ricochet 2 CodeQL')) {
    throw 'The ricochet-2 branch must retain the distinct Ricochet 2 CodeQL identity.'
}

$releaseContract = Get-Content -LiteralPath (
    Join-Path $repoRoot '.github/workflows/ricochet-2-release-contract.yml'
) -Raw
if (-not $releaseContract.Contains('name: Ricochet 2 Release Contract')) {
    throw 'The Ricochet 2 release-contract workflow has lost its distinct identity.'
}

Write-Host 'Ricochet 2 governance contract passed.'
