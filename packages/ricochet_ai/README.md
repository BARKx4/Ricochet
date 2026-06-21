# @ricochet/ai

Beta helpers for building provider-neutral AI contracts and OpenAI-compatible
HTTP calls in Ricochet apps.

This package keeps provider workflows outside Ricochet core. It builds
provider, message, request, response, error, stream-event, retry-policy, tool,
and schema-validation maps. It also provides OpenAI-compatible starter bodies
and request maps on top of core `secret_env`, `secret_resolve`, and HTTP
request-map helpers, plus SSE response-body parsers for providers that return
`text/event-stream` payloads.

Apps still own provider selection, actual retry execution, tool execution,
long-running stream orchestration, and user-facing agent behavior.

## Provider-neutral contracts

Build provider and message maps without making a network call:

```ricochet
"ai/openai" import

"openai" "https://api.openai.com/v1" "gpt-4.1-mini" ai_provider provider var
messages array
$messages "Keep answers concise." ai_system_message push! drop
$messages "Summarize the release notes." ai_user_message push! drop
options map
$options "temperature" 0.2 put! drop
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
$arguments "city" "Chicago" put! drop
"call-1" "get_weather" $arguments ai_tool_call toolCall var
"call-1" "{\"ok\":true}" ai_tool_result toolResult var
```

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
$data "summary" "Ready to ship." put! drop
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
$messages "Hello." ai_user_message push! drop
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

For long-running streams, hand the same request map to Ricochet's retained HTTP
stream words and parse chunks as you read offsets:

```ricochet
$request http_stream_start value stream var
options map
$stream "id" at $options http_stream_read value chunk var
$chunk "body" at ai_openai_stream_text
```
