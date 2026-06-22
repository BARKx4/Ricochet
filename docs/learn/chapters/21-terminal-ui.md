# Chapter 21: Terminal UI

## What You Will Build

This chapter will build a small interactive task dashboard.

## Concepts

- Alternate screen, cursor movement, terminal size, writes, flush, and key reading.
- Graceful terminal restoration.
- Packaging a terminal app.

## Words Introduced

Primary coverage: TUI system words and `rco package --tui`.

## Guided Example

Planned example: `examples/learn/21-tui/task-dashboard.rco`.

## Try It

Readers will run the dashboard and exit cleanly.

## Common Mistakes

- Forgetting to restore terminal state after an error.
- Treating key polling as a blocking prompt.

## Safety Notes

Examples will keep terminal state restoration visible.

## Production Notes

Production TUIs should handle resize, exit, and cleanup paths deliberately.

## Reference Links

Links will point to TUI and packaging references when drafted.

## What You Know Now

Readers will know the basic shape of a Ricochet terminal application.
