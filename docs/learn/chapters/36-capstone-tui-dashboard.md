# Chapter 36: Capstone TUI Dashboard

## What You Will Build

This capstone will build a terminal service dashboard.

## Concepts

- TUI words, async tasks, local or HTTP data, key handling, and packaging.
- Graceful exit and terminal restoration.
- Refresh loops and readable status views.

## Words Introduced

This chapter consolidates TUI, async, and data words taught earlier.

## Guided Example

Planned example: `examples/learn/36-capstone-tui/service_dashboard`.

## Try It

Readers will add one status panel and verify clean exit behavior.

## Common Mistakes

- Forgetting terminal cleanup in failure paths.
- Blocking input in a way that stops refresh updates.

## Safety Notes

The capstone will keep network or local-data permissions explicit.

## Production Notes

Production dashboards should handle failure, resize, and shutdown paths.

## Reference Links

Links will point back to TUI, async, HTTP, and packaging chapters when drafted.

## What You Know Now

Readers will know how to build a usable Ricochet terminal app.
