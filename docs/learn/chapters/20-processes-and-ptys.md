# Chapter 20: Processes And PTYs

> Part III: Host Capabilities and Local App Surfaces

## Why this chapter matters

Process and PTY control lets Ricochet coordinate external tools. This is high-trust host power, so the chapter teaches deliberate opt-ins and clear boundaries.

## What you will build

You will build a harmless local tool-runner that executes short Windows
commands, captures bounded output, starts and reads a retained process job,
requests cancellation for a long-running job, and captures output from a PTY
session.

## Concepts in plain English

A process runs another program. A PTY gives that program an interactive terminal-like session.

The chapter uses these concepts:

* Blocking process spawn and task-based process spawn.
* Retained process jobs, reads, cancellation, and release.
* PTY sessions, writes, reads, resize, stop, list, and detail.
* Process and PTY permissions as explicit host power.
* Output caps, timeouts, and environment maps.

## Vocabulary and commands

Primary coverage: `process_spawn`, `process_spawn_task`, `process_start`,
`process_jobs`, `process_job`, `process_cancel`, `process_release`,
`process_read`, `process_env_put`, `pty_start`, `pty_write`, `pty_read`,
`pty_resize`, `pty_stop`, `pty_release`, `pty_list`, and `pty_detail`.

## Guided example

Open `examples/learn/20-processes-and-ptys/tool-runner.rco` and run:

```
rco run --allow-process --allow-pty examples/learn/20-processes-and-ptys/tool-runner.rco
```

The runnable example uses Windows `cmd /C echo ...` commands so it can stay
harmless and deterministic. Process and PTY power is disabled unless you grant
it with flags.

Blocking process calls are best for short commands whose output you want all at
once:

```
processArgs array
$processArgs "/C" push! drop
$processArgs "echo %RICOCHET_LEARN_CHILD%" push! drop

processOptions map
$processOptions "timeout_ms" 5000 put! drop
$processOptions "stdout_max_bytes" 200 put! drop
$processOptions "RICOCHET_LEARN_CHILD" "process-ok" process_env_put value processOptions set

"cmd" $processArgs $processOptions process_spawn value spawnResult var
$spawnResult "stdout" at trim println
```

Use the task variant when the current flow can do other work before collecting
the result:

```
"cmd" $processArgs $processOptions process_spawn_task spawnTask var
$spawnTask await value taskResult var
$taskResult "stdout" at trim println
$spawnTask release_task drop
```

Retained jobs let you inspect output incrementally. Start the job, poll its
snapshot until it exits, then read retained stdout/stderr and release it:

```
"cmd" $jobArgs $jobOptions process_start value job var

nil jobSnapshot var
0 attempts var
$attempts 50 < while
  $job "id" at process_job value jobSnapshot set
  $jobSnapshot "status" at "exited" = if
    break
  end
  50 sleep
  $attempts 1 + attempts set
end

readOptions map
$job "id" at $readOptions process_read value jobRead var
$jobRead "stdout" at trim println
$job "id" at process_release value println
```

`process_cancel` requests cancellation for a retained job:

```
"cmd" $cancelArgs $cancelOptions process_start value cancelJob var
$cancelJob "id" at process_cancel value "cancelled" at println

nil cancelSnapshot var
0 cancelAttempts var
$cancelAttempts 50 < while
  $cancelJob "id" at process_job value cancelSnapshot set
  $cancelSnapshot "running" at false = if
    break
  end
  50 sleep
  $cancelAttempts 1 + cancelAttempts set
end

$cancelJob "id" at process_release value println
```

PTYs are for programs that expect a terminal. A PTY session is retained like a
process job, but reads come from terminal output:

```
ptyArgs array
$ptyArgs "/C" push! drop
$ptyArgs "echo ricochet-pty" push! drop

ptyOptions map
$ptyOptions "rows" 12 put! drop
$ptyOptions "cols" 80 put! drop
$ptyOptions "output_max_bytes" 4096 put! drop

"cmd" $ptyArgs $ptyOptions pty_start value session var
$session "id" at 100 24 pty_resize value drop

ptyReadOptions map
$session "id" at $ptyReadOptions pty_read value ptyRead var
$ptyRead "output" at "ricochet-pty" contains? println
$session "id" at pty_release value println
```

For an interactive shell, use `pty_write` to send input such as a command plus
a newline, `pty_read` with offsets to consume output, and `pty_stop` when the
session should end.

## How to read the example

Read the command first, then the code. The command grants the host powers the program may use. Inside the program, host calls usually return `Result` values or retained resource handles, so keep the same habit from Chapter 09: check the result, unwrap deliberately, and release or close resources when you are done.

## Try it

Remove `--allow-process` and rerun the example. Then restore it and remove
`--allow-pty`. Each failure should name the missing capability. After that,
lower `stdout_max_bytes` or `output_max_bytes` and watch truncation metadata
change.

## Check your understanding

- Can you name the host capability this example needs?
- Can you point to the command flag or profile that grants it?
- Can you identify which calls may return `Result` values?
- Can you describe how the example avoids touching more of the host system than necessary?

## Common mistakes

* Assuming process behavior is identical across operating systems.
* Passing unchecked user input to a process command.
* Forgetting that `process_spawn_task` awaits to a `Result`, while a plain
  spawned block awaits to the block value.
* Releasing retained jobs or PTY sessions before they finish.
* Leaving command output unbounded.

## Safety notes

Process and PTY execution can run arbitrary local programs. Keep command names
and arguments explicit, avoid unchecked user input, use timeouts and output
caps, and prefer narrow `--process-root` bounds for real tools. This chapter’s
runnable example uses short `cmd /C echo ...` commands and a loopback-only
`ping` cancellation target.

## Production guidance

Production process integrations should model command construction as a data
boundary. Build argument arrays directly instead of concatenating shell strings
when possible, bound output, handle cancellation paths, and release retained
jobs. PTY integrations should be reserved for truly terminal-oriented tools;
plain processes are easier to automate and test.

## Reference links

* `docs/reference/guides/host-capabilities.html`
* `docs/reference/guides/language-runtime.html`

## What you know now

You know how to call local tools without treating processes as invisible side
effects: grant the capability explicitly, bound the output, inspect retained
state, cancel when needed, and release process or PTY handles when finished.

## Next step

Continue to [Chapter 21: Terminal UI](21-terminal-ui.md).
