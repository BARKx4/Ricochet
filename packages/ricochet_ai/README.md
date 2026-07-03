# @ricochet/ai

Beta helpers for building provider-neutral AI contracts,
OpenAI-compatible HTTP calls, Anthropic-compatible Messages API calls, and
local/Ollama-compatible request flows in Ricochet apps.

This package keeps provider workflows outside Ricochet core. It builds
provider, message, request, response, error, stream-event, retry-policy, tool,
schema-validation maps, retry predicates/delay helpers, local tool-dispatch
helpers, and fake-provider-testable provider response normalization. It also
provides OpenAI-compatible, Anthropic-compatible, and Ollama request maps on
top of core `secret_env`, `secret_resolve`, and HTTP request-map helpers, plus
SSE and NDJSON response-body parsers for providers that return streaming
payloads.

Apps still own provider selection, long-running stream orchestration, and
user-facing agent behavior. The package-level executor helpers let apps wire
their chosen transport into retry/error/tool-call normalization without
requiring a real provider during tests.

## Provider-neutral contracts

Build provider and message maps without making a network call:

```ricochet
"ai/openai" import

"openai" "https://api.openai.com/v1" "gpt-4.1-mini" ai_provider provider var
messages array
$messages "Keep answers concise." ai_system_message push drop
$messages "Summarize the release notes." ai_user_message push drop
options map
$options "temperature" 0.2 put drop
tools array
3 100 1000 ai_retry_policy retry var

$provider "gpt-4.1-mini" $messages $options $tools $retry ai_chat_request request var
$request "stream" at println
```

Use `ai_chat_stream` for the same shape with `stream = true`. The neutral chat
request fields are `provider`, `model`, `messages`, `stream`, `options`,
`tools`, and `retry`. Retry policy maps use `max_attempts`,
`initial_delay_ms`, and `max_delay_ms`.

Normalize provider output and errors at package boundaries:

```ricochet
toolCalls array
raw map
$provider "gpt-4.1-mini" "Done." $toolCalls $raw ai_chat_response response var
$provider "rate_limit" "try again later" 429 $raw ai_error errorMap var
"delta" $raw 12 false ai_stream_event event var
```

Tool contracts are plain maps too:

```ricochet
arguments map
$arguments "city" "Chicago" put drop
"call-1" "get_weather" $arguments ai_tool_call toolCall var
"call-1" "{\"ok\":true}" ai_tool_result toolResult var
```

## Retry and tool execution

Use the package retry helpers with local executor blocks that return ordinary
Ricochet `Result` values:

```ricochet
"ai/openai" import

request map
3 0 0 ai_retry_policy retry var
$request "retry" $retry put drop

[
  attempt var
  request var
  $attempt 3 < if
    "rate_limit" "retry later" fail result var
    $result error "status" 429 put drop
    $result
  else
    response map
    $response "attempt" $attempt put drop
    $response ok
  end
] executor var

$request $executor ai_execute_with_retry value "attempt" at println
```

`ai_retryable_error?` recognizes transient kinds such as `rate_limit`,
`timeout`, `server_error`, `network`, and `transient`, plus retryable HTTP
statuses like `429` and `500..599`. `ai_retry_delay_ms` computes deterministic
exponential backoff with no jitter.

Tool handlers stay package-local too:

```ricochet
"ai/openai" import

ai_tool_handlers handlers var
$handlers "get_weather" [
  arguments var
  toolCall var
  content map
  $content "city" $arguments "city" at put drop
  $content
] ai_tool_handler_put handlers set

arguments map
$arguments "city" "Chicago" put drop
"call-1" "get_weather" $arguments ai_tool_call toolCall var
$toolCall $handlers ai_execute_tool_call value "content" at "city" at println
```

Use `ai_execute_tool_calls` to run an array of tool-call maps in order and stop
on the first handler failure.

## Schema validation

AI schemas use the same field/rule map shape as `@ricochet/forms`: each field
rule has `type` and `required`, and validation returns `{ ok, errors }`.
Errors contain `field` and `message`.

```ricochet
"ai/openai" import

ai_schema
  "summary" "string" true ai_schema_field
  "score" "number" false ai_schema_field schema var

data map
$data "summary" "Ready to ship." put drop
$data $schema ai_validate_schema result var
$result "ok" at println
```

Accepted type names are `string`, `number`, `float`, `bool`, `array`, `list`,
`map`, `set`, and `any`. Missing required fields report `is required`; present
values of the wrong type report `must be TYPE`.

## OpenAI-compatible helpers

```ricochet
"ai/openai" import

messages array
$messages "Hello." ai_user_message push drop
"OPENAI_API_KEY" ai_secret_ref secret_resolve value token var
"https://api.openai.com/v1" $token "gpt-4.1-mini" $messages ai_openai_chat_request
```

For small stream responses, build a streaming request and parse the returned SSE
body:

```ricochet
"ai/openai" import

"https://api.openai.com/v1" $token "gpt-4.1-mini" $messages ai_openai_chat_stream_request request var
$request http_request value response var
$response "body" at ai_openai_stream_text
```

For provider-runtime flows that should be testable with fake providers, pass a
neutral chat request and an executor block to `ai_openai_execute_chat`. The
executor receives the neutral request plus the 1-based retry attempt and returns
an HTTP-like response map `Result` with `status` and `body` fields. The helper
normalizes OpenAI-compatible success bodies into `ai_chat_response` maps,
normalizes non-2xx statuses into rich `ai_error` results, applies retry policy
rules, and extracts OpenAI tool calls:

```ricochet
"ai/openai" import

[
  attempt var
  chatRequest var
  $provider "base_url" at $token $chatRequest "model" at $chatRequest "messages" at ai_openai_chat_request httpRequest var
  $httpRequest http_request
] executor var

$contract $executor ai_openai_execute_chat result var
$result ok? if
  $result value "text" at println
else
  $result error "message" at println
end
```

Use `ai_openai_stream_events` when tests or small responses need structured
stream events instead of only concatenated text. It returns a `Result` whose ok
value is an array of `ai_stream_event` maps and whose error value reports
malformed SSE JSON without crashing the caller.

For retained HTTP streams, use `ai_openai_stream_state` with
`ai_openai_stream_read_events` so SSE frames can be split across
`http_stream_read` chunks without being parsed too early.

## Anthropic-compatible helpers

Anthropic's Messages API uses `POST /v1/messages`, the `x-api-key` header, and
an explicit `anthropic-version` header. System messages from the neutral
message list are moved to the top-level `system` field, while user and
assistant messages remain in the `messages` array:

```ricochet
"ai/openai" import

messages array
$messages "Keep the answer brief." ai_system_message push drop
$messages "Return the word ricochet." ai_user_message push drop

"ANTHROPIC_API_KEY" ai_secret_ref secret_resolve value token var
"https://api.anthropic.com/v1" $token "2023-06-01" "claude-sonnet-4-5" $messages 256 ai_anthropic_chat_request request var
```

For fake-provider-testable runtime flows, use the same neutral contract and
executor shape as the OpenAI-compatible helpers:

```ricochet
"https://api.anthropic.com/v1" "claude-sonnet-4-5" ai_anthropic_provider provider var
options map
tools array
3 0 0 ai_retry_policy retry var
$provider "claude-sonnet-4-5" $messages $options $tools $retry ai_chat_request contract var

[
  attempt var
  chatRequest var
  $provider "base_url" at $token "2023-06-01" $chatRequest "model" at $chatRequest "messages" at 256 ai_anthropic_chat_request request var
  $request http_request
] executor var

$contract $executor ai_anthropic_execute_chat result var
```

`ai_anthropic_execute_chat` normalizes text content blocks and `tool_use`
blocks into the package-neutral `ai_chat_response` shape. Use
`ai_anthropic_stream_events` for small Anthropic SSE bodies; text deltas become
`delta` events, streamed tool input JSON becomes `tool_delta`, and
`message_stop` becomes `done`. Stream error events return a failed `Result`.
For retained HTTP streams, use `ai_anthropic_stream_state` and
`ai_anthropic_stream_read_events`.

## Local/Ollama helpers

For native Ollama `/api/chat` endpoints, use the local-provider request helpers
and the same fake-executor pattern:

```ricochet
"ai/openai" import

"http://127.0.0.1:11434" "llama3.2" ai_ollama_provider provider var
messages array
$messages "Return the word ricochet." ai_user_message push drop
options map
tools array
3 0 0 ai_retry_policy retry var
$provider "llama3.2" $messages $options $tools $retry ai_chat_request contract var

[
  attempt var
  chatRequest var
  $provider "base_url" at $chatRequest "model" at $chatRequest "messages" at ai_ollama_chat_request request var
  $request http_request
] executor var

$contract $executor ai_ollama_execute_chat result var
```

Use `ai_ollama_stream_events` for small native Ollama NDJSON stream bodies. It
returns the same `ai_stream_event` map shape as the OpenAI-compatible stream
parser. For retained HTTP streams, use `ai_ollama_stream_state` and
`ai_ollama_stream_read_events` so partial NDJSON lines are buffered until they
are complete.

For long-running streams, hand the same request map to Ricochet's retained HTTP
stream words and parse chunks through provider stream state:

```ricochet
$request http_stream_start value stream var
ai_openai_stream_state state var
false done var

$done false = while
  $state 4096 ai_stream_read_options options var
  $stream "id" at $options http_stream_read value chunk var
  $state $chunk ai_openai_stream_read_events value read var
  $read "events" at [
    event var
    $event "kind" at "delta" = if
      $event "data" at print
    end
  ] each drop
  $read "state" at state set
  $read "done" at done set
end
```

The retained-stream helpers track `offset`, `event_offset`, `buffer`,
`protocol_done`, `http_done`, and `done`. They parse only complete SSE frames or
NDJSON lines, flush a final unterminated event when `http_stream_read` reports
`done`, and keep malformed completed events on the existing failed `Result`
path.

See `examples/showcase/ai_provider_probe/fake_provider.rco` for an offline
provider executor flow and
`examples/showcase/ai_provider_probe/local_model_request.rco` or
`ollama_native_request.rco` for local model request shapes.
