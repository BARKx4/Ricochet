# Chapter 32: Debugger, DAP, LSP, And Editor Tools

> Part V: Packages, Registries, Macros, Tooling, and Release

## Why this chapter matters

Large projects need editor feedback, traces, breakpoints, and diagnostics. This chapter shows the professional tooling surface around Ricochet programs.

## What you will build

You will build a debuggable app and inspect it through Ricochet tooling. The
example has normal program output, a spawned task, and a clear breakpoint line
at `$worker await`. You will run it normally, inspect a deterministic debugger
snapshot, read JSON debug events, and check LSP/lint diagnostics.

## Concepts in plain English

DAP connects Ricochet debugging to editors. LSP connects diagnostics, hover, completion, formatting, and symbols to editors.

The chapter uses these concepts:

* Terminal debugger commands.
* Trace files, JSON debug output, DAP, debug TUI, and debug web.
* LSP diagnostics, completion, hover, definitions, symbols, formatting, quick fixes, and rename.
* VS Code editor assets and word inventory checks.

## Vocabulary and commands

Primary coverage: `rco debug`, `rco debug-tui`, `rco debug-web`,
`rco debug-adapter`, `rco run --trace-file`, `rco lsp`,
`rco lsp-diagnostics`, `rco lint --json`, `rco fmt`, and editor asset
validation.

## Guided example

Open `examples/learn/32-debugger-editor/debuggable_app.rco`:

```
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

```
rco run examples/learn/32-debugger-editor/debuggable_app.rco
```

Expected output:

```
debuggable app start
answer:42
upper:ADA
debuggable app done
[]
```

Now inspect the breakpoint snapshot. In the sample file, `$worker await` is
on line 10:

```
rco debug-tui --smoke --breakpoint 10 examples/learn/32-debugger-editor/debuggable_app.rco
```

The smoke view is read-only and deterministic:

```
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

```
rco debug-web --smoke --breakpoint 10 examples/learn/32-debugger-editor/debuggable_app.rco
```

Without `--smoke`, `debug-web` serves a loopback-only browser shell with
grouped panes for source, current instruction, stack, locals, globals, `self`,
tasks, output, event log, and runtime breakpoints.

For editor adapters and custom tools, use JSON Lines:

```
rco debug --json --breakpoint 10 examples/learn/32-debugger-editor/debuggable_app.rco
```

The stream includes instruction events, a paused event, and captured output
events. The paused event reports the reason, source line, opcode, stack,
locals, globals, `self`, and task snapshots.

Run LSP diagnostics directly:

```
rco lsp-diagnostics --pretty examples/learn/32-debugger-editor/debuggable_app.rco
```

Expected shape:

```
{
  "diagnostics": [],
  "uri": "file:///.../debuggable_app.rco"
}
```

Use lint JSON for CI-shaped diagnostics:

```
rco lint --json examples/learn/32-debugger-editor/debuggable_app.rco
```

Expected output:

```
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

## How to read the example

Read tooling examples as feedback loops. A trace, breakpoint, lint result, or LSP diagnostic is useful only when it points you to a smaller program shape you can understand.

## Try it

Run a scripted debugger session:

```
rco debug-tui --command step --command next --command continue examples/learn/32-debugger-editor/debuggable_app.rco
```

Write a trace file in a scratch location:

```
rco run --trace-file scratch-debuggable-app.trace.json examples/learn/32-debugger-editor/debuggable_app.rco
```

Trace files are JSON arrays. They are useful for stack visualizers and bug
reports, but they can contain runtime values, so review them before sharing.

Start the language server only from an editor or LSP client:

```
rco lsp
```

The VS Code extension under `editors/vscode` launches that server, provides
TextMate highlighting, and exposes commands such as restarting the language
server, running with the stack visualizer, and showing live debugger stacks.

## Check your understanding

- What new value shape, command, or host boundary did this chapter introduce?
- Which line is most important to trace with a stack diagram?
- Where would you add a binding to make the example easier to read?

## Common mistakes

* Confusing debugger stepping with normal program output.
* Ignoring diagnostics before running an app.
* Setting a breakpoint on a blank line and expecting a pause.
* Treating `debug-tui --smoke` as the full interactive debugger. It is a
  deterministic preview.
* Writing trace files into source directories and forgetting to review them.
* Expecting `rco fmt` to migrate unsupported old syntax. Diagnostics and quick
  fixes do that job.

## Safety notes

The sample app is local and does not mutate files. Trace commands write
debug events to the path you provide, so put scratch traces somewhere obvious
and review them for secrets, tokens, personal data, and large runtime values
before sharing. Debugger and LSP tools should never make hidden edits.

## Production guidance

Production debugging should preserve useful traces without exposing secrets.
When debugging app servers, `rco serve --debug` prints request-fault pause lines
before controller, view, or response metadata failures become HTTP 500
responses. Use DAP (`rco debug-adapter`) for IDE integration, JSON Lines for
custom tooling, and trace files for reproducible reports.

Maintainers should keep editor assets in sync with the word inventory:

```
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-editor-assets.ps1
rco words --check --docs-app docs/reference/app.js --grammar editors/vscode/syntaxes/ricochet.tmLanguage.json
```

## Reference links

* `docs/wiki/editor-debugging.md`
* `docs/debugger-integrations.md`
* `docs/reference/guides/editor-debugging.html`
* `editors/vscode`

## What you know now

You know the professional tooling available around Ricochet code: terminal and
browser debugger views, JSON debug events, trace files, DAP, LSP diagnostics,
lint JSON, formatting, VS Code integration, and editor asset validation.

## Next step

Continue to [Chapter 33: Bytecode, Images, And Source Emission](33-bytecode-images-and-source-emission.md).
