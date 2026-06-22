# Learn Ricochet Examples

These examples support the Learn Ricochet manual. They are intentionally small,
local, and safe to run from a source checkout while the RC1 feature set is
frozen.

Run any example with the workspace CLI:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/01-hello-world/main.rco
```

## Examples

| Chapter | Example | Command |
| --- | --- | --- |
| 01 | Hello World | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/01-hello-world/main.rco` |
| 03 | Stack Receipt | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/03-stack/main.rco` |
| 08 | Collections Task List | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/08-collections/main.rco` |
| 10 | Control-Flow Gradebook | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/10-control-flow/main.rco` |
| 11 | OOP Contact Book | `cargo run -q -p ricochet_cli --bin rco -- run examples/learn/11-oop/main.rco` |

`examples.json` is the runnable manifest used by validation and future manual
tooling.
