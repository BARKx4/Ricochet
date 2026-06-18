# @ricochet/ai

Beta helpers for building provider-agnostic AI HTTP calls in Ricochet apps.

This package keeps provider workflows outside Ricochet core. It builds request
maps, OpenAI-compatible starter bodies, and normalized response maps on top of
core `secret_env`, `secret_resolve`, and HTTP request-map helpers. It also
includes stream-shaped OpenAI-compatible request builders and SSE response-body
parsers for providers that return `text/event-stream` payloads. Apps still own
provider selection, retry policy, tool execution, and user-facing agent
behavior.

```ricochet
"ai/openai" import

messages array
$messages map push! drop
"OPENAI_API_KEY" secret_env secret_resolve value token var
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
