# Chapter 16: Capabilities And Sandboxing

## What You Will Build

You will build a capability report and walk an approval record from creation to
completion.

## Concepts

- Trusted and sandboxed profiles.
- Capability values, allowlists, and command flags.
- Runtime capability inspection.
- Runtime-local approval records.
- Why host powers are taught before filesystem, process, network, TUI, and GUI
  effects.

## Words Introduced

Primary coverage: `runtime_capabilities`, `approval_create`, `approval_claim`,
`approval_complete`, `approval_reject`, `approval_detail`, and
`approval_release`.

Capability-specific filesystem, workspace, HTTP, socket, process, PTY, TUI,
and webview words are taught in later chapters.

## Guided Example

Open `examples/learn/16-capabilities-and-sandboxing/capability-report.rco` and
run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/16-capabilities-and-sandboxing/capability-report.rco
```

Start by inspecting the runtime's capability map:

```ricochet
runtime_capabilities inspect println drop
```

Approval records are local runtime objects for exactly-once authorization
flows. Create an operation map, then create the approval:

```ricochet
operation map
$operation "capability" "workspace.write" put! drop
$operation "summary" "Approve a generated workspace write" put! drop

options map
$operation $options approval_create value approval var
```

The generated token can be claimed once:

```ricochet
$approval "id" at $approval "token" at approval_claim value claim var
$claim "claimed" at println
```

Complete or reject the approval after the protected action:

```ricochet
result map
$result "path" "generated/example.txt" put! drop
$approval "id" at $result approval_complete value completed var
```

Inspect the retained record when you need audit state, then release it when the
app no longer needs to query it:

```ricochet
$approval "id" at approval_detail value detail var
$approval "id" at approval_release value drop
```

## Try It

Run the same report with a sandboxed profile:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run --capability-profile sandboxed examples/learn/16-capabilities-and-sandboxing/capability-report.rco
```

The approval flow still works because it is runtime-local. Later chapters show
how filesystem, HTTP, sockets, process, PTY, TUI, and webview powers change
under sandboxing and allowlists.

## Common Mistakes

- Assuming a script should run with all host powers by default.
- Forgetting host allowlists for network or filesystem examples.
- Treating approvals as a full policy system. They are primitives for local
  exactly-once claims and retained audit state.
- Printing generated approval tokens in real operator-facing logs.
- Forgetting to release approval records after a local app has recorded or
  displayed the audit detail it needs.

## Safety Notes

Capability boundaries come before any destructive or network-capable examples.
This chapter does not delete, write files, open sockets, spawn processes, or
start UI sessions.

## Production Notes

Production commands should request the smallest useful set of host powers.
Prefer sandboxed runs for untrusted examples, bug reports, package reviews, and
third-party code. Open only the filesystem roots, HTTP hosts, socket hosts, or
process roots a task actually needs.

## Reference Links

- `docs/reference/guides/host-capabilities.html`
- `docs/reference/guides/features.html`

## What You Know Now

You know how Ricochet makes host access explicit, how to inspect the runtime's
current host powers, and how approval records can guard a local operation
without granting any power by themselves.
