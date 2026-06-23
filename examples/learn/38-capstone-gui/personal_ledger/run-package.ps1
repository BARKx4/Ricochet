$ErrorActionPreference = "Stop"

$exampleRoot = Split-Path -Parent $PSCommandPath
$repoRoot = Resolve-Path (Join-Path $exampleRoot "..\..\..\..")
$buildDir = Join-Path $exampleRoot "build"
$output = Join-Path $buildDir "personal-ledger.exe"

New-Item -ItemType Directory -Force -Path $buildDir | Out-Null

Push-Location $repoRoot
try {
  cargo run -q -p ricochet_cli --bin rco -- package examples/learn/38-capstone-gui/personal_ledger/ledger_gui.rco --gui --output $output --package-name personal-ledger --package-version 0.1.0 --package-description "Learn Ricochet personal ledger GUI package"
  if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
  }
}
finally {
  Pop-Location
}

$artifact = Get-Item -LiteralPath $output
"packaged artifact: $($artifact.Name)"
"artifact bytes: $($artifact.Length)"
