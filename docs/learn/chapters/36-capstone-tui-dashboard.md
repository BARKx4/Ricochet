# Chapter 36: Capstone TUI Dashboard

> Part VI: Capstone Applications

## Why this chapter matters

The TUI capstone combines terminal rendering, data loading, tests, and safe cleanup into a complete local dashboard.

## What you will build

You will build a terminal service dashboard. It reads a sample service
status JSON file, summarizes health and latency, renders one nonblocking TUI
frame, polls for a key, flushes the frame, and restores the terminal.

## Concepts in plain English

This capstone proves terminal UI work can stay testable by separating data preparation from rendering and input.

The chapter uses these concepts:

* TUI words, async tasks, local or HTTP data, key handling, and packaging.
* Graceful exit and terminal restoration.
* Refresh loops and readable status views.
* Separating dashboard metrics from terminal rendering so tests can cover the
  logic without entering the alternate screen.

## Vocabulary and commands

This chapter consolidates TUI, async, and data words taught earlier.

## Guided example

Open `examples/learn/36-capstone-tui/service_dashboard`. The capstone is split
into data, logic, rendering, and tests:

```
data/services.json
lib/status.rco
dashboard.rco
ServiceDashboardTest.rco
```

The data file is deliberately small:

```
{
  "name": "jobs",
  "status": "degraded",
  "latency_ms": 118,
  "owner": "operations"
}
```

`lib/status.rco` contains the testable logic:

```
( services status -> Number ) service_count_status function
  status var
  services var
  0 matches var
  nil service var

  [
    service set
    $service "status" at $status = if
      $matches 1 + matches set
    end
  ] $services each drop

  $matches
end
```

The summary function returns one map for the renderer:

```
( services -> Map ) service_summary function
  services var
  summary map

  $summary "count" $services count put! drop
  $summary "ok" $services "ok" service_count_status put! drop
  $summary "degraded" $services "degraded" service_count_status put! drop
  $summary "down" $services "down" service_count_status put! drop
  $summary "average_latency_ms" $services service_average_latency put! drop
  $summary "health_score" $services service_health_score put! drop

  $summary
end
```

Run the dashboard with the TUI host:

```
rco tui examples/learn/36-capstone-tui/service_dashboard/dashboard.rco
```

The app enters the alternate screen, so captured output contains terminal
escape sequences. The visible frame contains:

```
Ricochet service dashboard
Terminal: 120x30
Services: 4
OK: 2  degraded: 1  down: 1
Average latency: 56ms
Health score: 50%
Service rows:
- api [ok] 42ms owner=platform
- jobs [degraded] 118ms owner=operations
- search [ok] 65ms owner=product
- billing [down] 0ms owner=finance
Key ready: 0
```

The rendering shape should look familiar from Chapter 21:

```
tui_enter value drop
tui_clear value drop
tui_size value size var

0 0 tui_move_to value drop
"Ricochet service dashboard" tui_write value drop

tui_flush value drop
tui_leave value drop
```

The capstone computes the service summary before entering the terminal. That
keeps load and JSON errors on the normal command line and reduces the chance of
leaving the terminal in raw mode after a data failure.

Run the tests:

```
rco test examples/learn/36-capstone-tui/service_dashboard/ServiceDashboardTest.rco
```

Expected output:

```
PASS ServiceDashboardTest.testServiceCounters
PASS ServiceDashboardTest.testSummaryMap
2 tests, 0 failed
```

This command points at the test file instead of the whole folder. In a plain
folder, `rco test PATH` can compile and execute other app sources under that
path; the file-specific command keeps the TUI renderer out of the test run.

## How to read the example

Read the capstone from the outside in. Start with the user command or app surface, identify the data boundary, follow the core transformation, and then check tests and packaging. The point is integration, not new syntax.

## Try it

Add a `"maintenance"` service to `data/services.json`, then add a
`"maintenance"` count to `service_summary`:

```
$summary "maintenance" $services "maintenance" service_count_status put! drop
```

Render it under the existing status row:

```
"  maintenance: " tui_write value drop
$summary "maintenance" at to_string tui_write value drop
```

Add a matching test assertion:

```
$services "maintenance" service_count_status
1 assert_equals
```

Then run:

```
rco test examples/learn/36-capstone-tui/service_dashboard/ServiceDashboardTest.rco
rco tui examples/learn/36-capstone-tui/service_dashboard/dashboard.rco
rco lint examples/learn/36-capstone-tui/service_dashboard
```

## Check your understanding

- What earlier chapters does this capstone combine?
- What is the user-facing entry point?
- What data boundary does the app use?
- Which tests or checks prove the capstone still works?

## Common mistakes

* Forgetting terminal cleanup in failure paths.
* Blocking input in a way that stops refresh updates.
* Running automated checks with `tui_read_key` and waiting forever for input. Use
  `tui_poll_key` in startup and smoke examples.
* Testing the whole plain folder when the folder contains top-level TUI app
  code. Use the file-specific test command shown above.
* Writing directly to the terminal before `tui_enter` or after `tui_leave`
  unless you intentionally want normal command-line output.

## Safety notes

This capstone reads a sample JSON file and uses terminal UI output only. It
does not open sockets, call HTTP, write files, or delete anything. If you later
replace the local JSON file with live HTTP or socket data, make those host
permissions explicit and keep timeouts bounded.

## Production guidance

Production dashboards should structure rendering as an explicit loop:
refresh data, recompute summary state, clear or redraw changed regions, poll
for input, and leave the terminal on every exit path. Larger apps should keep
the terminal renderer thin and put status policy into ordinary functions with
tests, as this capstone does.

Package terminal dashboards with `rco package --tui` when you are ready to hand
them to users.

## Reference links

* `docs/learn/chapters/15-async-and-tasks.md`
* `docs/learn/chapters/18-http-and-streams.md`
* `docs/learn/chapters/21-terminal-ui.md`
* `docs/learn/chapters/34-packaging-release-and-updates.md`
* `docs/reference/guides/host-capabilities.html`
* `docs/reference/guides/language-runtime.html`

## What you know now

You know how to build a usable Ricochet terminal app: load status data before
entering raw mode, compute a compact summary, render positioned rows, poll for
input without blocking automated checks, flush the frame, restore terminal state, and
test the non-terminal logic separately.

## Next step

Continue to [Chapter 37: Capstone MVC App](37-capstone-mvc-app.md).
