# Chapter 32: Debugger, DAP, LSP, And Editor Tools

## What You Will Build

You will build a debuggable app and inspect it through Ricochet tooling. The
example has normal program output, a spawned task, and a clear breakpoint line
at `$worker await`. You will run it normally, inspect a deterministic debugger
snapshot, read JSON debug events, and check LSP/lint diagnostics.

## Concepts

- Terminal debugger commands.
- Trace files, JSON debug output, DAP, debug TUI, and debug web.
- LSP diagnostics, completion, hover, definitions, symbols, formatting, quick fixes, and rename.
- VS Code editor assets and word inventory checks.

## Words Introduced

Primary coverage: `rco debug`, `rco debug-tui`, `rco debug-web`,
`rco debug-adapter`, `rco run --trace-file`, `rco lsp`,
`rco lsp-diagnostics`, `rco lint --json`, `rco fmt`, and editor asset
validation.

## Guided Example

Open `examples/learn/32-debugger-editor/debuggable_app.rco`:

```ricochet
(( Set a breakpoint on `$worker await` to inspect the task snapshot. ))

"debuggable app start" println

[ 40 2 + ] spawn worker var

"ada" name var
$name uppercase upperName var

$worker await answer var
$answer 42 assert_equals
$worker release_task drop

"answer:" print
$answer println

"upper:" print
$upperName println

"debuggable app done" println
```

Run it normally first:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/32-debugger-editor/debuggable_app.rco
```

Expected output:

```text
debuggable app start
answer:42
upper:ADA
debuggable app done
[]
```

Now inspect the breakpoint snapshot. In the checked-in file, `$worker await` is
on line 10:

```powershell
cargo run -q -p ricochet_cli --bin rco -- debug-tui --smoke --breakpoint 10 examples/learn/32-debugger-editor/debuggable_app.rco
```

The smoke view is read-only and deterministic:

```text
Ricochet Debug TUI
status: paused (breakpoint)
source: examples/learn/32-debugger-editor/debuggable_app.rco:10
source line: $worker await answer var
frame: <main>
opcode: PushString("worker")
stack:
  <empty>
globals:
  name = String("ada")
  upperName = String("ADA")
  worker = Task(0)
tasks:
  task 0: running operation=spawn pending=true running=true completed=false failed=false frames=0
stdout:
debuggable app start
```

Run the same pause through the browser snapshot renderer:

```powershell
cargo run -q -p ricochet_cli --bin rco -- debug-web --smoke --breakpoint 10 examples/learn/32-debugger-editor/debuggable_app.rco
```

Without `--smoke`, `debug-web` serves a loopback-only browser shell with
grouped panes for source, current instruction, stack, locals, globals, `self`,
tasks, output, event log, and runtime breakpoints.

For editor adapters and custom tools, use JSON Lines:

```powershell
cargo run -q -p ricochet_cli --bin rco -- debug --json --breakpoint 10 examples/learn/32-debugger-editor/debuggable_app.rco
```

The stream includes instruction events, a paused event, and captured output
events. The paused event reports the reason, source line, opcode, stack,
locals, globals, `self`, and task snapshots.

Run LSP diagnostics directly:

```powershell
cargo run -q -p ricochet_cli --bin rco -- lsp-diagnostics --pretty examples/learn/32-debugger-editor/debuggable_app.rco
```

Expected shape:

```json
{
  "diagnostics": [],
  "uri": "file:///.../debuggable_app.rco"
}
```

Use lint JSON for CI-shaped diagnostics:

```powershell
cargo run -q -p ricochet_cli --bin rco -- lint --json examples/learn/32-debugger-editor/debuggable_app.rco
```

Expected output:

```json
{
  "diagnostic_count": 0,
  "file_count": 1,
  "files": [
    {
      "diagnostics": [],
      "path": "examples/learn/32-debugger-editor/debuggable_app.rco"
    }
  ]
}
```

## Try It

Run a scripted debugger session:

```powershell
cargo run -q -p ricochet_cli --bin rco -- debug-tui --command step --command next --command continue examples/learn/32-debugger-editor/debuggable_app.rco
```

Write a trace file in a scratch location:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run --trace-file scratch-debuggable-app.trace.json examples/learn/32-debugger-editor/debuggable_app.rco
```

Trace files are JSON arrays. They are useful for stack visualizers and bug
reports, but they can contain runtime values, so review them before sharing.

Start the language server only from an editor or LSP client:

```powershell
cargo run -q -p ricochet_cli --bin rco -- lsp
```

The VS Code extension under `editors/vscode` launches that server, provides
TextMate highlighting, and exposes commands such as restarting the language
server, running with the stack visualizer, and showing live debugger stacks.

## Common Mistakes

- Confusing debugger stepping with normal program output.
- Ignoring diagnostics before running an app.
- Setting a breakpoint on a blank line and expecting a pause.
- Treating `debug-tui --smoke` as the full interactive debugger. It is a
  deterministic preview.
- Writing trace files into source directories and forgetting to review them.
- Expecting `rco fmt` to migrate unsupported old syntax. Diagnostics and quick
  fixes do that job.

## Safety Notes

The checked-in example is local and does not mutate files. Trace commands write
debug events to the path you provide, so put scratch traces somewhere obvious
and review them for secrets, tokens, personal data, and large runtime values
before sharing. Debugger and LSP tools should never make hidden edits.

## Production Notes

Production debugging should preserve useful traces without exposing secrets.
When debugging app servers, `rco serve --debug` prints request-fault pause lines
before controller, view, or response metadata failures become HTTP 500
responses. Use DAP (`rco debug-adapter`) for IDE integration, JSON Lines for
custom tooling, and trace files for reproducible reports.

Maintainers should keep editor assets in sync with the word inventory:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-editor-assets.ps1
cargo run -q -p ricochet_cli --bin rco -- words --check --docs-app docs/reference/app.js --grammar editors/vscode/syntaxes/ricochet.tmLanguage.json
```

## Reference Links

- `docs/wiki/editor-debugging.md`
- `docs/debugger-integrations.md`
- `docs/reference/guides/editor-debugging.html`
- `editors/vscode`

## What You Know Now

You know the professional tooling available around Ricochet code: terminal and
browser debugger views, JSON debug events, trace files, DAP, LSP diagnostics,
lint JSON, formatting, VS Code integration, and editor asset validation.
