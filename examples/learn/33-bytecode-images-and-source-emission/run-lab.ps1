$ErrorActionPreference = "Stop"

$exampleRoot = Split-Path -Parent $PSCommandPath
Push-Location $exampleRoot
try {
  cargo run -q -p ricochet_cli --bin rco -- build image_lab.rco
  cargo run -q -p ricochet_cli --bin rco -- run-bytecode build/app.rcob
  cargo run -q -p ricochet_cli --bin rco -- image save session.rci --source image_lab.rco
  cargo run -q -p ricochet_cli --bin rco -- image inspect session.rci
  cargo run -q -p ricochet_cli --bin rco -- emit-source build/app.rcob
}
finally {
  Pop-Location
}
