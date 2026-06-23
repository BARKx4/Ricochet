$ErrorActionPreference = "Stop"

$exampleRoot = Split-Path -Parent $PSCommandPath
$repoRoot = Resolve-Path (Join-Path $exampleRoot "..\..\..")
$buildDir = Join-Path $exampleRoot "build"
$output = Join-Path $buildDir "learn-task-dashboard.exe"

New-Item -ItemType Directory -Force -Path $buildDir | Out-Null

Push-Location $repoRoot
try {
  cargo run -q -p ricochet_cli --bin rco -- package examples/learn/21-tui/task-dashboard.rco --tui --output $output --package-name learn-task-dashboard --package-version 0.1.0 --package-description "Learn Ricochet task dashboard TUI package"
}
finally {
  Pop-Location
}

$artifact = Get-Item -LiteralPath $output
"packaged artifact: $($artifact.Name)"
"artifact bytes: $($artifact.Length)"
