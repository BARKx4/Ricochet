# Chapter 19: TCP And WebSocket Sockets

> Part III: Host Capabilities and Local App Surfaces

## Why this chapter matters

Raw sockets and WebSockets are lower-level network tools. They are powerful, so this chapter emphasizes explicit hosts, loopback practice, and cleanup.

## What you will build

You will build two loopback echo examples: one with raw TCP sockets and one
with WebSockets. Each example binds a local listener, starts a client task,
accepts the inbound connection, echoes a message, and releases every retained
resource.

## Concepts in plain English

TCP is a bidirectional byte connection. WebSocket is a message connection often used for long-running client/server interaction.

The chapter uses these concepts:

* Connecting, listening, accepting, reading, writing, closing, and releasing sockets.
* Socket host allow-lists under the sandboxed capability profile.
* Using a client task so the same program can both listen and connect.
* Cleanup and error paths for retained resources.
* The difference between TCP byte chunks and WebSocket text messages.

## Vocabulary and commands

Primary coverage: `tcp_listen`, `tcp_listeners`, `tcp_listener`,
`tcp_accept`, `tcp_listener_close`, `tcp_listener_release`, `tcp_connect`,
`tcp_connections`, `tcp_connection`, `tcp_write`, `tcp_read`, `tcp_close`,
`tcp_release`, `ws_listen`, `ws_listeners`, `ws_listener`, `ws_accept`,
`ws_listener_close`, `ws_listener_release`, `ws_connect`, `ws_connections`,
`ws_connection`, `ws_send`, `ws_read`, `ws_close`, and `ws_release`.

## Guided example

Open `examples/learn/19-sockets/tcp_echo.rco` and run:

```
rco run --capability-profile sandboxed --socket-allow-host 127.0.0.1 examples/learn/19-sockets/tcp_echo.rco
```

The TCP example starts by binding a listener on loopback. Port `0` asks the OS
for a free port:

```
listenOptions map
"127.0.0.1" 0 $listenOptions tcp_listen value listener var
$listener "id" at tcp_listener value "port" at println
$listener "port" at port var
```

Because `tcp_accept` waits for a client, the example starts the client in a
task. The client connects, writes text, waits for the echo, then closes and
releases its retained socket:

```
[
  options map
  $options "timeout_ms" 5000 put! drop
  "127.0.0.1" $port $options tcp_connect value client var
  $client "id" at clientId var
  $clientId "from-client" tcp_write value drop

  readOptions map
  $readOptions "timeout_ms" 5000 put! drop
  $readOptions "max_bytes" 64 put! drop
  $clientId $readOptions tcp_read value clientRead var

  $clientId tcp_close value drop
  $clientId tcp_release value drop
  $clientRead "data" at
] spawn clientTask var
```

The server side accepts, reads, writes, closes, and releases the accepted
connection:

```
acceptOptions map
$acceptOptions "timeout_ms" 5000 put! drop
$listener "id" at $acceptOptions tcp_accept value accepted var
$accepted "id" at serverId var

readOptions map
$readOptions "timeout_ms" 5000 put! drop
$readOptions "max_bytes" 64 put! drop
$serverId $readOptions tcp_read value serverRead var
$serverId "from-server" tcp_write value drop
$serverId tcp_close value drop
$serverId tcp_release value drop
```

After the accepted socket is cleaned up, close and release the listener:

```
$clientTask await println
$listener "id" at tcp_listener_close value drop
$listener "id" at tcp_listener_release value println
tcp_listeners count println
tcp_connections count println
```

The WebSocket example follows the same shape. Run:

```
rco run --capability-profile sandboxed --socket-allow-host 127.0.0.1 examples/learn/19-sockets/ws_echo.rco
```

The URL is built from the actual bound port:

```
"127.0.0.1" 0 $listenOptions ws_listen value listener var
"ws://127.0.0.1:" $listener "port" at to_string concat "/echo" concat url var
```

WebSocket reads return messages rather than arbitrary byte chunks:

```
$serverId $readOptions ws_read value serverRead var
$serverRead "message_type" at println
$serverRead "message" at println
$serverId "from-server" ws_send value drop
```

## How to read the example

Read the command first, then the code. The command grants the host powers the program may use. Inside the program, host calls usually return `Result` values or retained resource handles, so keep the same habit from Chapter 09: check the result, unwrap deliberately, and release or close resources when you are done.

## Try it

Change the client message in either example and rerun it. Then change
`--socket-allow-host 127.0.0.1` to a different host and watch the sandboxed run
reject the bind or connect.

For TCP, lower `max_bytes` below the message length and observe the bounded
chunk behavior. For WebSockets, add `max_bytes` to the read options and set it
below the message length to get a size-cap error result.

## Check your understanding

- Can you name the host capability this example needs?
- Can you point to the command flag or profile that grants it?
- Can you identify which calls may return `Result` values?
- Can you describe how the example avoids touching more of the host system than necessary?

## Common mistakes

* Testing against a broad network host before understanding loopback behavior.
* Forgetting to close or release retained resources.
* Trying to release a listener before closing it.
* Forgetting that `tcp_accept` and `ws_accept` need a client to connect, which
  is why these examples use `spawn`.
* Treating TCP reads like message reads. TCP returns byte chunks; WebSocket
  reads return one message with a `message_type`.

## Safety notes

The runnable examples bind only `127.0.0.1` and run with
`--capability-profile sandboxed --socket-allow-host 127.0.0.1`. Do not broaden
socket permissions until you have a clear reason and an operator-facing way to
explain which hosts are reachable.

## Production guidance

Production socket clients should use bounded timeouts, document host
allow-lists, close and release retained handles, and keep protocol framing
explicit. For TCP, define your message framing instead of assuming one read
equals one application message. For WebSockets, handle non-text message types,
timeouts, and close frames deliberately.

## Reference links

* `docs/reference/guides/host-capabilities.html`
* `docs/reference/guides/language-runtime.html`

## What you know now

You know how Ricochet models retained TCP and WebSocket listeners, retained
connections, bounded reads, writes, closes, releases, and sandboxed socket host
allow-lists.

## Next step

Continue to [Chapter 20: Processes And PTYs](20-processes-and-ptys.md).
