# Chapter 20: Processes And PTYs

## What You Will Build

This chapter will build a harmless local tool-runner.

## Concepts

- Blocking process spawn and task-based process spawn.
- Retained process jobs, reads, cancellation, and release.
- PTY sessions, writes, reads, resize, stop, list, and detail.

## Words Introduced

Primary coverage: process and PTY system words.

## Guided Example

Planned example: `examples/learn/20-processes-and-ptys/tool-runner.rco`.

## Try It

Readers will run platform-appropriate harmless commands and inspect the result.

## Common Mistakes

- Assuming process behavior is identical across operating systems.
- Passing unchecked user input to a process command.

## Safety Notes

The chapter will use harmless commands and make process and PTY permissions explicit.

## Production Notes

Production process integrations should bound output, handle cancellation, and release retained jobs.

## Reference Links

Links will point to process and PTY references when drafted.

## What You Know Now

Readers will know how to call local tools without treating processes as invisible side effects.
