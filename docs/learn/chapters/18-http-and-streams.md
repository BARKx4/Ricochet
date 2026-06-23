# Chapter 18: HTTP And Streams

> Part III: Host Capabilities and Local App Surfaces

## Why this chapter matters

HTTP connects Ricochet programs to services. Streams let programs handle data progressively instead of pretending every response is a small string.

## What you will build

You will build a small API client that talks to a loopback test server, sends
JSON, awaits HTTP tasks, reads a retained response stream in bounded chunks,
and releases the stream when it is done.

## Concepts in plain English

An HTTP request is data sent to a host. An HTTP response is data returned from that host. Streams represent data you read or write in pieces.

The chapter uses these concepts:

* Simple GET and POST requests.
* Structured request maps, headers, JSON, timeouts, and bearer helpers.
* Task-returning HTTP requests for work that should not block the current
  flow.
* Retained response streams, bounded offset reads, cancellation, and release.
* Host capability flags that make outbound network access explicit.

## Vocabulary and commands

Primary coverage: `http_request_new`, `http_header_put`,
`http_bearer_auth`, `http_json_body`, `http_timeout`, `http_get`,
`http_get_task`, `http_request`, `http_post_json`, `http_post_json_task`,
`http_request_task`, `http_stream_start`, `http_streams`, `http_stream`,
`http_stream_read`, `http_stream_cancel`, and `http_stream_release`.

## Guided example

Open `examples/learn/18-http-streams/api-client.rco`. The example expects a
base URL, so run it through the local fixture:

```
powershell -ExecutionPolicy Bypass -File examples/learn/18-http-streams/run-local.ps1
```

The fixture starts a loopback HTTP server and runs the example with an
explicit host allow-list:

```
rco run --capability-profile trusted --http-allow-host 127.0.0.1 examples/learn/18-http-streams/api-client.rco http://127.0.0.1:PORT
```

Simple calls are the shortest path when you only need a URL and a small body:

```
$baseUrl "/ping" concat http_get value getResponse var
$getResponse "status" at println

payload map
$payload "message" "hello from Ricochet" put! drop
$baseUrl "/messages" concat $payload http_post_json value postResponse var
```

Use a structured request map when the request has policy, headers, or a
prepared body:

```
"POST" $baseUrl "/structured" concat http_request_new value request var
$request "X-Learn" "manual" http_header_put value request set
$request "learn-token" http_bearer_auth value request set
$request $payload http_json_body value request set
$request 3000 http_timeout value request set
$request http_request value structuredResponse var
```

The helper words return a new request map as a `Result`, so unwrap the value
and store it back into the same variable before the next helper.

Task variants let you start HTTP work and await the result later:

```
$baseUrl "/task" concat http_get_task getTask var
$getTask await value taskResponse var
$taskResponse "body" at println
$getTask release_task drop
```

Retained streams are for responses you want to inspect incrementally. Start
with a structured request, keep the returned `id`, and read with an options
map:

```
"GET" $baseUrl "/stream" concat http_request_new value streamRequest var
$streamRequest 3000 http_timeout value streamRequest set
$streamRequest "max_response_bytes" 128 put! drop
$streamRequest http_stream_start value stream var
$stream "id" at streamId var

readOptions map
$readOptions "max_bytes" 6 put! drop
$streamId $readOptions http_stream_read value firstChunk var
$readOptions "offset" $firstChunk "next_offset" at put! drop
$streamId $readOptions http_stream_read value secondChunk var
$readOptions "offset" $secondChunk "next_offset" at put! drop
```

`http_stream_read` reports `from_offset`, `next_offset`, `bytes_len`, and
`done`. Store `next_offset` as the next read’s `offset` to avoid rereading
bytes. When the retained job has stopped running, release it:

```
$streamId http_stream value streamState var
$streamState "running" at while
  25 sleep
  $streamId http_stream value streamState set
end

$streamId http_stream_release value println
```

If a retained request is no longer useful, call `http_stream_cancel` with the
stream id. Cancellation asks the host to stop the work; still check the stream
snapshot before release, because a running stream cannot be released.

## How to read the example

Read the command first, then the code. The command grants the host powers the program may use. Inside the program, host calls usually return `Result` values or retained resource handles, so keep the same habit from Chapter 09: check the result, unwrap deliberately, and release or close resources when you are done.

## Try it

Change the fixture response for `/stream` and rerun the script. Then lower the
example’s `max_response_bytes` from `128` to `10` and watch the stream snapshot
report truncation.

For a stricter run that does not use the stream polling sleep, you can run a
simple request under the sandboxed profile:

```
rco run --capability-profile sandboxed --http-allow-host 127.0.0.1 examples/learn/18-http-streams/api-client.rco http://127.0.0.1:PORT
```

The full fixture uses the trusted profile only because `sleep` is disabled in
the sandboxed profile. The HTTP host is still restricted to loopback by
`--http-allow-host 127.0.0.1`.

## Check your understanding

- Can you name the host capability this example needs?
- Can you point to the command flag or profile that grants it?
- Can you identify which calls may return `Result` values?
- Can you describe how the example avoids touching more of the host system than necessary?

## Common mistakes

* Forgetting capability flags for outbound HTTP.
* Leaving retained streams unreleased.
* Reading streams without carrying `next_offset` into the next read.
* Assuming redirects are followed. Ricochet HTTP requests keep redirects
  disabled.
* Treating task handles and stream handles as ordinary values that never need
  cleanup.

## Safety notes

The runnable example uses a local loopback server. It does not call public
services and it does not write files. Keep examples like this bounded: use
timeouts, byte caps, explicit host allow-lists, and release retained handles.
Do not print bearer tokens or resolved secrets.

## Production guidance

Production clients should set `timeout_ms`, bound response size with
`max_response_bytes`, and model failures as `Result` values. For external APIs,
pair `http_bearer_auth` with `secret_env` or another secret reference instead
of hard-coding tokens. Use task variants when an app can keep working while a
request is in flight, and use retained streams when the response may be large
or should be consumed incrementally.

## Reference links

* `docs/reference/guides/host-capabilities.html`
* `docs/reference/guides/language-runtime.html`

## What you know now

You know how to make simple and structured HTTP requests, run HTTP requests as
tasks, read retained streams with offsets, cancel work that is no longer
needed, and release completed streams.

## Next step

Continue to [Chapter 19: TCP And WebSocket Sockets](19-tcp-and-websocket-sockets.md).
