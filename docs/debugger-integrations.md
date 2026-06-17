# Ricochet Debugger Integrations

Ricochet debugger integrations should build on the runtime debug event stream
instead of inventing editor-specific state models.

Current CLI surface:

```powershell
rco run --trace-file trace.json app.rco
rco run-bytecode --trace-file trace.json build/app.rcob
```

Trace files are JSON arrays. Events currently use these shapes:

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
  "self": null
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

Editor integration guidance:

- VS Code should render stack visualization in a Webview panel fed by this trace
  contract first, then later by live debug events.
- Other IDEs should consume the same event contract through either saved traces,
  a future `rco debug --json` stream, or a Debug Adapter Protocol bridge.
- Stack visualizers should treat `debug` strings as the stable beta display
  fallback. Future structured fields can be added without removing `debug`.
- Source breakpoints should stay line-based at the protocol boundary until the
  compiler exposes stable instruction IDs.
- Live debugger integrations should keep the RPN mental model visible: current
  stack, current instruction, current frame locals, globals, and `self`.
