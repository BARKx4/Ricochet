# Chapter 16: Capabilities And Sandboxing

## What You Will Build

This chapter will build a capability report that shows what host powers are available.

## Concepts

- Trusted and sandboxed profiles.
- Capability values, allowlists, and command flags.
- Why host powers are taught before filesystem, process, network, TUI, and GUI effects.

## Words Introduced

Primary coverage: capability values, `runtime_capabilities`, `--allow-*` flags, and safety posture.

## Guided Example

Planned example: `examples/learn/16-capabilities-and-sandboxing/capability-report.rco`.

## Try It

Readers will run the same report with different capability flags.

## Common Mistakes

- Assuming a script should run with all host powers by default.
- Forgetting host allowlists for network or filesystem examples.

## Safety Notes

The chapter will teach capability boundaries before any destructive or network-capable examples.

## Production Notes

Production commands should request the smallest useful set of host powers.

## Reference Links

Links will point to capability and CLI reference pages when the chapter is drafted.

## What You Know Now

Readers will know how Ricochet makes host access explicit.
