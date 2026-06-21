# Language And Runtime

Read an existing binding with `$name`. Declaration words still keep the
name-first shape, and `$` is useful when a declaration name is itself stored in
another variable:

```forth
"users" name var
$name array
$users "Ada" push! drop
$users count println
```

Top-level declarations are shared across the VM. Function and method calls get
fresh local declaration scopes, so helper locals declared with `var`, `array`,
`map`, `list`, or `Set` refresh within the active call and do not leak into
later calls.

## Persistent Images And Source Emission

Use a persistent image when an interactive session should survive process
restart. The image stores safe language state such as the stack, global
bindings, functions, classes, instances, blocks, and results:

```powershell
rco repl --image session.rci
```

Inside the REPL, image commands use a colon prefix:

```text
:bindings
:save
:save checkpoint.rci
:load checkpoint.rci
```

`--image` loads the file at startup when it already exists and saves it again
when the REPL exits cleanly. `:save` and `:load` default to the `--image` path;
pass an explicit path to checkpoint or switch images during a session.

For non-interactive workflows, create and inspect image files directly:

```powershell
rco image save session.rci --source main.rco
rco image inspect session.rci
rco image inspect session.rci --json
```

Images intentionally reject process-local or sensitive runtime state: tasks,
capabilities, regex internals, retained HTTP/upload/process/PTY/TCP/WebSocket
handles, active approvals, native-only class methods, and literal secret
references are not serialized.

Source emission prints a readable Ricochet-like view of source or compiled
bytecode:

```powershell
rco emit-source main.rco
rco emit-source build/app.rcob
```

Treat emitted source as an inspection and portability aid. It preserves useful
declarations, literals, calls, and block structure, but it is not a stable
byte-for-byte source reconstruction contract.

## Numbers

Ricochet has two numeric runtime values: `Number` is signed 64-bit integer
storage, and `Float` is finite 64-bit floating-point storage. Plain integer
literals stay exact `Number` values; decimal or exponent literals become
`Float`. Mixed numeric math promotes to `Float`, and conversion words provide
checked boundaries for database- or package-facing intent:

```forth
1.5 2 + println
12.0 to_integer value println
12.5 to_integer error "kind" at println
255 to_unsigned_tinyint value println
"1.23456789" to_float32 value println
```

## Tasks

Spawn a task and await its result:

```forth
[ 40 2 + ] spawn answer var
$answer task_status
$answer running?
tasks count
$answer await
$answer release_task
$answer task_status
handles array
$handles [ 20 2 + ] spawn push! drop
$handles [ 30 4 + ] spawn push! drop
$handles await_all
```

HTTP capability calls can also be launched as tasks:

```forth
"https://example.com" http_get_task request var
$request await value response var
$response "status" at println
```

Long-running HTTP responses can be retained and read incrementally by offset.
`http_stream_read` accepts `offset` and optional `max_bytes`; missing or nil
`max_bytes` reads all currently retained bytes from the offset. Read results
include the stream snapshot fields plus `body`, `from_offset`, `next_offset`,
backward-compatible `offset` as the next offset, `bytes_len`, and `done`.
`done` becomes true only after the stream stops and the read has consumed all
currently retained bytes:

```forth
"GET" "https://api.example/v1/events" http_request_new value request var
$request 60000 http_timeout value request set
$request http_stream_start value stream var

options map
$options "max_bytes" 4096 put! drop
false done var
$done false = while
  $stream "id" at $options http_stream_read value chunk var
  $chunk "body" at println
  $options "offset" $chunk "next_offset" at put! drop
  $chunk "done" at done set
end
```

## Date, Time, And Duration

Date, timestamp, and duration words use UTC Unix epoch milliseconds as their
timestamp boundary. RFC3339 parsing accepts offsets and normalizes to UTC;
format patterns use Chrono/strftime syntax:

```forth
"2026-06-18T13:14:15.250Z" timestamp_parse value startedAt var
$startedAt timestamp_format value println
$startedAt "%Y-%m-%d %H:%M:%S" timestamp_format_pattern value println
$startedAt timestamp_parts value parts var
$parts "year" at println

$startedAt 2 duration_hours value timestamp_add value later var
$startedAt $later timestamp_diff value println

"2026-02-28" date_parse value date var
$date 1 date_add_days value nextDate var
$nextDate "%Y-%m-%d" date_format value println
$date $nextDate date_diff_days value println
```

## Provider Settings And Secrets

For authenticated provider calls, keep settings as ordinary maps and store
secret references instead of secret values. `secret_env` creates an env-backed
reference, `config_get` reads required nested settings, and `secret_resolve`
reads the value only at the point of use under the environment capability.
`http_request_new`, `http_bearer_auth`, `http_json_body`, and `http_timeout`
cover the common request-map plumbing while preserving the same runtime host
allowlist and no-follow redirect policy as the simpler HTTP words:

```forth
settings map
provider map
$provider "api_key" "PROVIDER_API_KEY" secret_env put! drop
$settings "provider" $provider put! drop

path array
$path "provider" push! drop
$path "api_key" push! drop
$settings $path config_get value secret_resolve value token var

payload map
$payload "probe" true put! drop
"POST" "https://api.example/v1/models" http_request_new value request var
$request $token http_bearer_auth value request set
$request $payload http_json_body value request set
$request 30000 http_timeout value request set
$request http_request value response var
$response "status" at println
```

First-party AI package helpers can keep the provider contract separate from the
HTTP transport while still resolving secrets only at the point of use:

```forth
"ai/openai" import

"openai" "https://api.openai.com/v1" "gpt-4.1-mini" ai_provider provider var
messages array
$messages "Use one paragraph." ai_system_message push! drop
$messages "Explain Ricochet packages." ai_user_message push! drop
options map
tools array
3 100 1000 ai_retry_policy retry var
$provider "gpt-4.1-mini" $messages $options $tools $retry ai_chat_request contract var

"OPENAI_API_KEY" ai_secret_ref secret_resolve value token var
[
  attempt var
  chatRequest var
  $provider "base_url" at $token $chatRequest "model" at $chatRequest "messages" at ai_openai_chat_request request var
  $request http_request
] executor var

$contract $executor ai_openai_execute_chat value "text" at println
```

The same executor boundary is used by
`examples/showcase/ai_provider_probe/fake_provider.rco` for offline provider
tests and by `local_model_request.rco` for local OpenAI-compatible endpoints.

## Webview Documents

Build a webview document for desktop UI hosts:

```forth
state map
$state "count" 1 put! drop
"Count: " $state "count" at to_string concat webview_text countText var
"Increment" "increment" webview_button button var
actions array
$actions "Increment" "increment" "increment_counter" webview_action push! drop
"<main>" $countText concat
$countText concat
$button concat
"</main>" concat body var
"Counter" $body $state $actions webview_window_state value document var
```
