# Ricochet Debugger Integrations

Ricochet debugger integrations should build on the runtime debug event stream
instead of inventing editor-specific state models.

Current CLI surface:

```powershell
rco run --debug --step app.rco
rco run --breakpoint 12 app.rco
rco debug --json app.rco
rco debug-adapter
rco run --trace-file trace.json app.rco
rco run-bytecode --trace-file trace.json build/app.rcob
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
      "status": "running",
      "pending": true,
      "running": true,
      "completed": false,
      "failed": false
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
  and tasks as DAP scopes.
- VS Code renders saved trace files with `Ricochet: Run With Stack Visualizer`
  and renders live stopped-state scopes with `Ricochet: Show Debugger Stack`.
- Other IDEs should consume the same event contract through either saved traces,
  `rco debug --json`, or `rco debug-adapter`.
- Stack visualizers should treat `debug` strings as the stable beta display
  fallback. Future structured fields can be added without removing `debug`.
- Source breakpoints should stay line-based at the protocol boundary until the
  compiler exposes stable instruction IDs.
- Live debugger controls should map `step` to instruction stepping, `next` to
  step-over, `out` to step-out, and `continue` to normal execution.
- Live debugger integrations should keep the RPN mental model visible: current
  stack, current instruction, current frame locals, globals, `self`, and the
  current task snapshot.
