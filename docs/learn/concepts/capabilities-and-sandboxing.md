# Concept: Capabilities and Sandboxing

A capability is permission for Ricochet code to ask the host process to do something outside the language runtime: read files, call HTTP, open sockets, spawn a process, use a terminal UI, open a webview, or read environment variables.

## Why capabilities are explicit

Local scripts are powerful. A language that can build real tools also needs a visible safety boundary. Ricochet’s capability flags make that boundary part of the command you run.

```bash
rco run --capability-profile sandboxed --fs-root . app.rco
rco run --capability-profile sandboxed --http-allow-host 127.0.0.1 app.rco
```

## A safe default habit

Start with the smallest permission that lets the example run. Add roots, hosts, variables, or UI powers deliberately. Treat `--allow-process`, `--allow-pty`, and raw sockets as higher-trust opt-ins.
