# Chapter 21: Terminal UI

## What You Will Build

You will build a small terminal task dashboard that enters the alternate
screen, clears it, writes a few positioned rows, polls for a key without
blocking, flushes output, and restores the terminal.

## Concepts

- Alternate screen, cursor movement, terminal size, writes, flush, and key reading.
- Graceful terminal restoration.
- Packaging a terminal app.
- The difference between `rco run --allow-tui` and `rco tui`.
- Nonblocking key polling versus blocking key reads.

## Words Introduced

Primary coverage: `tui_enter`, `tui_leave`, `tui_clear`, `tui_move_to`,
`tui_write`, `tui_flush`, `tui_size`, `tui_poll_key`, `tui_read_key`, and the
`rco tui` / `rco package --tui` workflow.

## Guided Example

Open `examples/learn/21-tui/task-dashboard.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- tui examples/learn/21-tui/task-dashboard.rco
```

`rco tui` grants the terminal UI capability and suppresses the normal final
stack dump. That makes it the right command for terminal apps.

The example builds a small in-memory task list, then enters and clears the
terminal:

```ricochet
tasks array
$tasks "read chapter" push! drop
$tasks "run example" push! drop
$tasks "release terminal state" push! drop

tui_enter value drop
tui_clear value drop
```

Terminal size is a result map. Use it to adapt layout:

```ricochet
tui_size value size var

0 0 tui_move_to value drop
"Ricochet task dashboard" tui_write value drop

0 2 tui_move_to value drop
"Terminal: " tui_write value drop
$size "columns" at to_string tui_write value drop
"x" tui_write value drop
$size "rows" at to_string tui_write value drop
```

Writes are queued until you flush:

```ricochet
0 index var
$index $tasks count < while
  2 $index 5 + tui_move_to value drop
  "- " tui_write value drop
  $tasks $index at tui_write value drop
  $index 1 + index set
end

tui_flush value drop
```

Use `tui_poll_key` when the app should keep rendering even if no key is ready:

```ricochet
0 tui_poll_key value key var
$key nil? false = if
  1 keyReady set
end
```

Use `tui_read_key` for a blocking event loop:

```ricochet
tui_read_key value key var
$key "char" at "q" = if
  break
end
```

Always leave the terminal before the app ends:

```ricochet
tui_leave value drop
```

## Try It

Run the example with `rco run --capability-profile sandboxed` and no
`--allow-tui`; it should fail with a terminal capability error. Then run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run --capability-profile sandboxed --allow-tui examples/learn/21-tui/task-dashboard.rco
```

That command works, but it prints the normal final stack after terminal output.
For real terminal apps, prefer `rco tui`.

## Common Mistakes

- Forgetting to restore terminal state after an error.
- Treating key polling as a blocking prompt.
- Forgetting that `tui_write` is queued until `tui_flush`.
- Using `tui_read_key` in validation or startup paths where no key is available.

## Safety Notes

Terminal UI code changes the user's terminal state. Keep `tui_leave` visible in
simple examples, and structure larger apps so error paths also restore the
terminal. Avoid blocking reads in automated examples.

## Production Notes

Production TUIs should handle resize, exit, and cleanup paths deliberately.
Use `tui_poll_key` for render loops, `tui_read_key` for blocking waits, and
package terminal apps with `rco package --tui` so users do not see a final
stack dump after the app exits.

## Reference Links

- `docs/reference/guides/host-capabilities.html`
- `docs/reference/guides/language-runtime.html`

## What You Know Now

You know the basic shape of a Ricochet terminal application: enter, clear,
render with positioned writes, flush, read or poll input, and leave cleanly.
