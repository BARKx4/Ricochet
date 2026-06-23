# Chapter 16: Capabilities And Sandboxing

> Part III: Host Capabilities and Local App Surfaces

## Why this chapter matters

Host capabilities are the safety boundary between Ricochet code and your machine. You should understand that boundary before reading files, calling HTTP, opening sockets, or launching processes.

## What you will build

You will build a capability report and walk an approval record from creation to
completion.

## Concepts in plain English

A capability is permission. Sandboxing means running with fewer permissions until the program proves it needs a specific host power.

The chapter uses these concepts:

* Trusted and sandboxed profiles.
* Capability values, allowlists, and command flags.
* Runtime capability inspection.
* Runtime-local approval records.
* Why host powers are taught before filesystem, process, network, TUI, and GUI
  effects.

## Vocabulary and commands

Primary coverage: `runtime_capabilities`, `approval_create`, `approval_claim`,
`approval_complete`, `approval_reject`, `approval_detail`, and
`approval_release`.

Capability-specific filesystem, workspace, HTTP, socket, process, PTY, TUI,
and webview words are taught in later chapters.

## Guided example

Open `examples/learn/16-capabilities-and-sandboxing/capability-report.rco` and
run:

```
rco run examples/learn/16-capabilities-and-sandboxing/capability-report.rco
```

Start by inspecting the runtime’s capability map:

```
runtime_capabilities inspect println drop
```

Approval records are local runtime objects for exactly-once authorization
flows. Create an operation map, then create the approval:

```
operation map
$operation "capability" "workspace.write" put! drop
$operation "summary" "Approve a generated workspace write" put! drop

options map
$operation $options approval_create value approval var
```

The generated token can be claimed once:

```
$approval "id" at $approval "token" at approval_claim value claim var
$claim "claimed" at println
```

Complete or reject the approval after the protected action:

```
result map
$result "path" "generated/example.txt" put! drop
$approval "id" at $result approval_complete value completed var
```

Inspect the retained record when you need audit state, then release it when the
app no longer needs to query it:

```
$approval "id" at approval_detail value detail var
$approval "id" at approval_release value drop
```

## How to read the example

Read the command first, then the code. The command grants the host powers the program may use. Inside the program, host calls usually return `Result` values or retained resource handles, so keep the same habit from Chapter 09: check the result, unwrap deliberately, and release or close resources when you are done.

## Try it

Run the same report with a sandboxed profile:

```
rco run --capability-profile sandboxed examples/learn/16-capabilities-and-sandboxing/capability-report.rco
```

The approval flow still works because it is runtime-local. Later chapters show
how filesystem, HTTP, sockets, process, PTY, TUI, and webview powers change
under sandboxing and allowlists.

## Check your understanding

- Can you name the host capability this example needs?
- Can you point to the command flag or profile that grants it?
- Can you identify which calls may return `Result` values?
- Can you describe how the example avoids touching more of the host system than necessary?

## Common mistakes

* Assuming a script should run with all host powers by default.
* Forgetting host allowlists for network or filesystem examples.
* Treating approvals as a full policy system. They are primitives for local
  exactly-once claims and retained audit state.
* Printing generated approval tokens in real operator-facing logs.
* Forgetting to release approval records after a local app has recorded or
  displayed the audit detail it needs.

## Safety notes

Capability boundaries come before any destructive or network-capable examples.
This chapter does not delete, write files, open sockets, spawn processes, or
start UI sessions.

## Production guidance

Production commands should request the smallest useful set of host powers.
Prefer sandboxed runs for untrusted examples, bug reports, package reviews, and
third-party code. Open only the filesystem roots, HTTP hosts, socket hosts, or
process roots a task actually needs.

## Reference links

* `docs/reference/guides/host-capabilities.html`
* `docs/reference/guides/features.html`

## What you know now

You know how Ricochet makes host access explicit, how to inspect the runtime’s
current host powers, and how approval records can guard a local operation
without granting any power by themselves.

## Next step

Continue to [Chapter 17: Files, Workspaces, Environment, Config, And Secrets](17-files-workspaces-env-and-secrets.md).
