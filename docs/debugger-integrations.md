# Ricochet Debugger Integrations

Ricochet debugger integrations should build on the runtime debug event stream
instead of inventing editor-specific state models.

Current CLI surface:

```powershell
rco run --debug --step app.rco
rco run --breakpoint 12 app.rco
rco debug --json app.rco
rco debug-tui --smoke app.rco
rco debug-tui --command step --command continue app.rco
rco debug-web --smoke app.rco
rco debug-web app.rco
rco debug-adapter
rco run --trace-file trace.json app.rco
rco run-bytecode --trace-file trace.json build/app.rcob
rco emit-source build/app.rcob
rco serve --debug
```

Trace files are JSON arrays. `rco debug --json` streams the same event objects
as newline-delimited JSON, plus `output` events for captured program stdout and
stderr. Events currently use these shapes:

```json
{
  "event": "instruction",
  "frame": "<main>",
  "source": "app.rco:3",
  "opcode": "CallWord(\"+\")",
  "stack_before": [{ "debug": "Number(2)" }],
  "stack_after": [{ "debug": "Number(5)" }]
}
```

```json
{
  "event": "paused",
  "reason": "breakpoint",
  "frame": "work",
  "source": "app.rco:3",
  "opcode": "CallWord(\"answer\")",
  "stack": [],
  "locals": [{ "name": "answer", "value": { "debug": "Number(41)" } }],
  "globals": [],
  "self": null,
  "tasks": [
    {
      "id": 0,
      "operation": "spawn",
      "status": "running",
      "pending": true,
      "running": true,
      "completed": false,
      "failed": false,
      "fault": null,
      "frames": [
        {
          "frame": "<task>",
          "source": "app.rco:1",
          "opcode": "CallWord(\"sleep\")",
          "stack": [{ "debug": "Number(100)" }],
          "locals": [],
          "self": null
        }
      ]
    }
  ]
}
```

```json
{
  "event": "fault",
  "frame": "<main>",
  "message": "unknown word: typo",
  "stack": [{ "debug": "Number(42)" }]
}
```

```json
{
  "event": "output",
  "stream": "stdout",
  "text": "42\n"
}
```

Editor integration guidance:

- `rco debug-adapter` speaks Debug Adapter Protocol over stdio. IDEs should send
  `initialize`, `launch` with a `program` path, `setBreakpoints`, and
  `configurationDone`, then use standard `stackTrace`, `scopes`, and
  `variables` requests while the VM is stopped.
- The DAP bridge maps `continue`, `next`, `stepIn`, and `stepOut` to the same VM
  controls as the terminal debugger, and returns stack, locals, globals, `self`,
  and tasks as DAP scopes. Each task variable expands into stable status fields
  and the latest worker-frame snapshots published by the task VM.
- VS Code renders saved trace files with `Ricochet: Run With Stack Visualizer`
  and renders live stopped-state scopes with `Ricochet: Show Debugger Stack`.
  The live panel expands nested DAP variable references, so the `Tasks` scope
  can show task detail fields and worker-frame snapshots as a tree.
- Other IDEs should consume the same event contract through either saved traces,
  `rco debug --json`, or `rco debug-adapter`.
- `rco debug-tui --smoke` renders a deterministic read-only terminal snapshot
  of the first debugger pause. Without `--smoke`, `rco debug-tui` opens a
  command-driven text session that renders a fresh pause snapshot and accepts
  `step`, `next`, `out`, `continue`, `abort`, `break <line>`, `clear <line>`,
  `clear_breakpoints`, and `breakpoints` from stdin; repeat `--command ACTION`
  for deterministic scripted sessions. A richer full-screen layout is still
  tracked as debugger UI polish.
- `rco debug-web --smoke` writes the same first-pause snapshot as standalone
  HTML. Without `--smoke`, `rco debug-web` serves a loopback-only browser
  debugger shell with grouped panes for source, current instruction, stack,
  locals, globals, `self`, tasks, program output, event log, and runtime
  breakpoints.
  `GET /events` streams debugger events as Server-Sent Events and replays the
  latest pause to late subscribers; `POST /control` accepts
  JSON actions shaped as `{ "action": "step" }` with optional `pause_id` stale
  control protection. Supported actions are `step`, `next`, `out`, `continue`,
  `abort`, `breakpoint_add`, `breakpoint_remove`, `breakpoint_clear`, and
  `breakpoints`; breakpoint add/remove actions include a `line` field and can
  optionally include `file`.
- Stack visualizers should treat `debug` strings as the stable beta display
  fallback. Future structured fields can be added without removing `debug`.
- Source breakpoints should stay line-based at the protocol boundary until the
  compiler exposes stable instruction IDs.
- Live debugger controls should map `step` to instruction stepping, `next` to
  step-over, `out` to step-out, and `continue` to normal execution.
- Live debugger integrations should keep the RPN mental model visible: current
  stack, current instruction, current frame locals, globals, `self`, and the
  current task snapshot. Terminal sessions can inspect those task snapshots with
  `tasks`, `tasks --tree`, `task <id> stack`, and `task <id> locals`.
- MVC servers built with debug mode install a request-fault pause hook. Before
  returning an HTTP 500 for controller action, view render, or response metadata
  failures, `rco serve --debug` prints a `FAULT request METHOD PATH
  Controller.action revision=N stage=... ...` line. Embedders can install a
  `RequestFaultSink` to observe the same `RequestFaultPause` event without
  blocking request handling.
- `rco emit-source` can provide a readable source-like bytecode view for
  debugger tooling and recovery workflows, but it is not a stable byte-for-byte
  source reconstruction contract.
- Persistent VM images preserve safe language state for REPL and tooling
  workflows. They are not debugger pause snapshots and they refuse retained
  host resources such as tasks, streams, processes, PTYs, sockets, approvals,
  capabilities, regex internals, and literal secret references.

## Manual Epic 10 debugger verification

Run this checklist before marking the debugger UI epic complete. Use a small
`.rco` file with at least three source lines, a stack-producing operation, a
binding/global, and an async task if the task view changed.

- `rco debug-tui --command step --command next --command out --command continue`
  shows a fresh text snapshot at each pause with the current source line,
  opcode, stack, locals, globals, and tasks.
- Interactive `rco debug-tui` accepts keyboard-entered `step`, `next`, `out`,
  `continue`, `abort`, `break <line>`, `clear <line>`, `clear_breakpoints`, and
  `breakpoints` commands without claiming to be a curses-style full-screen UI.
- `rco debug-web` binds only to loopback, renders grouped panes for source,
  current instruction, stack, locals, globals, `self`, tasks, output, event log,
  and breakpoints, and keeps those panes in sync while stepping.
- Browser controls work by click and keyboard shortcuts for step, next, out,
  continue, and abort; breakpoint add/remove/clear/list controls update the
  breakpoint pane and the event log.
- Variable inspection shows stack, locals, globals, `self` when present, and
  task frame details consistently with `rco debug --json` or DAP scopes.
- Screenshots are optional and should be committed only after the UI layout and
  assets are stable enough to avoid noisy churn.
