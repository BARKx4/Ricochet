# Chapter 19: TCP And WebSocket Sockets

## What You Will Build

This chapter will build loopback TCP and WebSocket echo examples.

## Concepts

- Connecting, listening, accepting, reading, writing, closing, and releasing sockets.
- Socket host allowlists.
- Cleanup and error paths for retained resources.

## Words Introduced

Primary coverage: TCP and WebSocket socket system words.

## Guided Example

Planned examples: `examples/learn/19-sockets/tcp_echo.rco` and `examples/learn/19-sockets/ws_echo.rco`.

## Try It

Readers will run loopback-only socket examples with explicit capability flags.

## Common Mistakes

- Testing against a broad network host before understanding loopback behavior.
- Forgetting to close or release retained resources.

## Safety Notes

Examples will stay local by default and will name every required socket permission.

## Production Notes

Production socket clients should document host allowlists and cleanup behavior.

## Reference Links

Links will point to TCP and WebSocket references when drafted.

## What You Know Now

Readers will know how Ricochet models socket clients and listeners.
