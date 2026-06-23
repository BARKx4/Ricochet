# Host Capabilities And Safety

Ricochet keeps powerful host behavior behind explicit capability controls.

## Workspace Filesystem Helpers

Workspace helpers provide structured filesystem access for local apps while
preserving the same `--fs-root` and `--fs-readonly` bounds as the lower-level
`fs_*` words. `fs_delete` removes a file, symlink, or empty directory;
`workspace_delete` adds explicit `recursive` and `missing_ok` options and
refuses to delete the configured filesystem root:

```forth
options map
writeOptions map
$writeOptions "create_parent_dirs" true put! drop
"README.md" $options workspace_read_text value readme var
"." $options workspace_list value entries var
"generated/out.txt" "hello" $writeOptions workspace_write_text value written var
$written "relative_path" at println
deleteOptions map
$deleteOptions "recursive" true put! drop
"generated" $deleteOptions workspace_delete value drop
runtime_capabilities "workspace" at "root" at println
```

## Process Environment

Use `process_env_put` when a child process needs a bounded environment value
without hand-building the nested `env` options map:

```forth
args array
options map
$options "GIT_TERMINAL_PROMPT" "0" process_env_put value options set
"git" $args $options process_spawn value
```

## Process Execution

Process execution is stronger than the ordinary trusted local profile and stays
disabled unless you pass `--allow-process`. Use `process_spawn` with a direct
command string, an argument array, and an options map when you want to run a
process to completion. `--process-root PATH` narrows process and PTY `cwd`
values independently from `--fs-root`; when omitted, process cwd containment
falls back to the filesystem root if one is configured:

```forth
args array
$args "status" push! drop
options map
$options "timeout_ms" 10000 put! drop
"git" $args $options process_spawn value result var
$result "success" at println
```

Use `process_start` for long-running jobs. The runtime keeps a retained job
registry with bounded captured output, and it can be inspected across MVC
requests with `process_jobs`, `process_job`, `process_read`, and
`process_cancel`. Release completed jobs with `process_release`:

```forth
args array
$args "status" push! drop
options map
$options "stdout_max_bytes" 1048576 put! drop
"git" $args $options process_start value job var
readOptions map
$job "id" at $readOptions process_read value output var
$output "stdout" at println
$job "id" at process_release value drop
```

## PTY Sessions

PTY sessions are separate and also opt in. Use `--allow-pty` for trusted
scripts or MVC apps that need a real pseudo-terminal. Command names and shell
newlines are host-specific:

```forth
args array
$args "repl" push! drop
options map
"rco" $args $options pty_start value session var
$session "id" at "1 2 +\r\n" pty_write value drop
readOptions map
$session "id" at $readOptions pty_read value screen var
$screen "output" at println
stopOptions map
$session "id" at $stopOptions pty_stop value drop
$session "id" at pty_release value drop
```

## Approvals

Approval records are runtime-local and shared across MVC requests.
`approval_create` returns a generated token once; `approval_claim` consumes that
token exactly once before the caller performs the mutating operation:

```forth
operation map
$operation "capability" "workspace.write" put! drop
$operation "summary" "Write generated file" put! drop
options map
$operation $options approval_create value approval var
$approval "id" at $approval "token" at approval_claim value claim var
$claim "claimed" at println
$approval "id" at approval_detail value detail var
$approval "id" at approval_release value drop
```

## Runtime Safety Notes

The CLI uses the `trusted` capability profile by default for local scripts.
Pass `--capability-profile sandboxed` with `rco run`, `rco run-bytecode`,
`rco repl`, or `rco test` to start with filesystem, HTTP, TUI, and webview
disabled. Process execution, PTY sessions, and TCP/WebSocket sockets are
disabled unless explicitly enabled in either profile.

In the sandboxed profile, `--fs-root <path>` enables filesystem access only
under that directory, `--fs-readonly` denies writes, `--http-allow-host <host>`
enables HTTP only for named hosts, and `--allow-tui` enables terminal UI access,
while `--allow-webview` enables webview document building. `--no-fs`,
`--no-http`, `--no-tui`, and `--no-webview` still deny those host powers
explicitly in either profile.

`--allow-sockets` enables raw TCP and WebSocket clients/listeners; prefer
`--socket-allow-host <host>` for bounded socket access when running examples,
tests, or third-party code. Listener bind hosts are checked with the same
allowlist as outbound destinations.

`--allow-process` enables direct child process execution with captured
stdout/stderr, bounded output, blocking `process_spawn`, and long-running
`process_start` jobs retained up to a per-host cap; use `process_release` to
drop completed jobs. `--process-root <path>` can make process and PTY cwd
resolution narrower than the filesystem workspace. `--allow-pty` enables
retained pseudo-terminal sessions through `pty_start`, `pty_write`, `pty_read`,
`pty_resize`, `pty_stop`, `pty_release`, `pty_list`, and `pty_detail`.

`--env-allow <name>` enables or narrows environment variable reads and writes
to named variables, `--no-env` denies environment/current-directory access, and
`--no-sleep` denies script sleeps. Embedded hosts can leave capabilities
disabled. HTTP calls do not follow redirects, use a timeout and response body
cap, and filesystem access remains powerful CLI behavior unless you deny or
bound it with these flags.

For MVC servers, `rco serve` enables filesystem and HTTP only through
`--fs-root` and `--http-allow-host`, and enables process environment access only
through `--allow-env` or `--env-allow`. MVC process execution requires
`--allow-process`, MVC PTY sessions require `--allow-pty`, and `--process-root`
can narrow execution cwd values. Watched MVC runtimes use the same capability
setup and share retained approval, process, and PTY registries across hot
reloads.

For v1 beta testing, keep `trusted` for your own local scripts and generated
apps. Use `sandboxed` for untrusted examples, bug reports, package reviews, or
third-party code, opening only the filesystem root or HTTP hosts the test
actually needs.
