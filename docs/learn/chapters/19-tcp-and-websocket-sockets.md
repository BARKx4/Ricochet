# Chapter 19: TCP And WebSocket Sockets

## What You Will Build

You will build two loopback echo examples: one with raw TCP sockets and one
with WebSockets. Each example binds a local listener, starts a client task,
accepts the inbound connection, echoes a message, and releases every retained
resource.

## Concepts

- Connecting, listening, accepting, reading, writing, closing, and releasing sockets.
- Socket host allow-lists under the sandboxed capability profile.
- Using a client task so the same program can both listen and connect.
- Cleanup and error paths for retained resources.
- The difference between TCP byte chunks and WebSocket text messages.

## Words Introduced

Primary coverage: `tcp_listen`, `tcp_listeners`, `tcp_listener`,
`tcp_accept`, `tcp_listener_close`, `tcp_listener_release`, `tcp_connect`,
`tcp_connections`, `tcp_connection`, `tcp_write`, `tcp_read`, `tcp_close`,
`tcp_release`, `ws_listen`, `ws_listeners`, `ws_listener`, `ws_accept`,
`ws_listener_close`, `ws_listener_release`, `ws_connect`, `ws_connections`,
`ws_connection`, `ws_send`, `ws_read`, `ws_close`, and `ws_release`.

## Guided Example

Open `examples/learn/19-sockets/tcp_echo.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run --capability-profile sandboxed --socket-allow-host 127.0.0.1 examples/learn/19-sockets/tcp_echo.rco
```

The TCP example starts by binding a listener on loopback. Port `0` asks the OS
for a free port:

```ricochet
listenOptions map
"127.0.0.1" 0 $listenOptions tcp_listen value listener var
$listener "id" at tcp_listener value "port" at println
$listener "port" at port var
```

Because `tcp_accept` waits for a client, the example starts the client in a
task. The client connects, writes text, waits for the echo, then closes and
releases its retained socket:

```ricochet
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

```ricochet
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

```ricochet
$clientTask await println
$listener "id" at tcp_listener_close value drop
$listener "id" at tcp_listener_release value println
tcp_listeners count println
tcp_connections count println
```

The WebSocket example follows the same shape. Run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run --capability-profile sandboxed --socket-allow-host 127.0.0.1 examples/learn/19-sockets/ws_echo.rco
```

The URL is built from the actual bound port:

```ricochet
"127.0.0.1" 0 $listenOptions ws_listen value listener var
"ws://127.0.0.1:" $listener "port" at to_string concat "/echo" concat url var
```

WebSocket reads return messages rather than arbitrary byte chunks:

```ricochet
$serverId $readOptions ws_read value serverRead var
$serverRead "message_type" at println
$serverRead "message" at println
$serverId "from-server" ws_send value drop
```

## Try It

Change the client message in either example and rerun it. Then change
`--socket-allow-host 127.0.0.1` to a different host and watch the sandboxed run
reject the bind or connect.

For TCP, lower `max_bytes` below the message length and observe the bounded
chunk behavior. For WebSockets, add `max_bytes` to the read options and set it
below the message length to get a size-cap error result.

## Common Mistakes

- Testing against a broad network host before understanding loopback behavior.
- Forgetting to close or release retained resources.
- Trying to release a listener before closing it.
- Forgetting that `tcp_accept` and `ws_accept` need a client to connect, which
  is why these examples use `spawn`.
- Treating TCP reads like message reads. TCP returns byte chunks; WebSocket
  reads return one message with a `message_type`.

## Safety Notes

The runnable examples bind only `127.0.0.1` and run with
`--capability-profile sandboxed --socket-allow-host 127.0.0.1`. Do not broaden
socket permissions until you have a clear reason and an operator-facing way to
explain which hosts are reachable.

## Production Notes

Production socket clients should use bounded timeouts, document host
allow-lists, close and release retained handles, and keep protocol framing
explicit. For TCP, define your message framing instead of assuming one read
equals one application message. For WebSockets, handle non-text message types,
timeouts, and close frames deliberately.

## Reference Links

- `docs/reference/guides/host-capabilities.html`
- `docs/reference/guides/language-runtime.html`

## What You Know Now

You know how Ricochet models retained TCP and WebSocket listeners, retained
connections, bounded reads, writes, closes, releases, and sandboxed socket host
allow-lists.
