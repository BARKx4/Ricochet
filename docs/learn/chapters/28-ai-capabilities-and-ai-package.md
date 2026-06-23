# Chapter 28: AI Capabilities And The AI Package

> Part IV: MVC, Data, Auth, Forms, and AI

## Why this chapter matters

AI features are application boundaries: prompts, providers, transcripts, tool permissions, and fallback behavior. The safest model is explicit capability, explicit provider, explicit result.

## What you will build

You will build a fake-provider AI chat flow that does not require live
credentials. The example uses the `@ricochet/ai` package to create a neutral
chat contract, retry a transient provider response, normalize a successful
OpenAI-compatible response, run a local tool handler, validate the response
shape, and parse a tiny server-sent-event stream body.

## Concepts in plain English

An AI provider is an external or local model boundary. Ricochet code should treat prompts, providers, responses, and tool permissions as explicit data.

The chapter uses these concepts:

* MVC AI capability boundaries and application ownership.
* Provider-neutral request, message, response, error, tool, retry, and stream
  maps from `@ricochet/ai`.
* Fake-provider executors for tests and local examples.
* Schema validation for AI responses.
* Small response stream parsing versus retained HTTP stream state.

## Vocabulary and commands

Primary coverage: the `@ricochet/ai` package vocabulary, including
`ai_provider`, `ai_system_message`, `ai_user_message`, `ai_chat_request`,
`ai_retry_policy`, `ai_openai_execute_chat`, `ai_schema`,
`ai_validate_schema`, `ai_tool_handlers`, `ai_tool_handler_put`,
`ai_execute_tool_calls`, and `ai_openai_stream_events`.

## Guided example

Open `examples/learn/28-ai/fake_provider_chat`. Its manifest vendors the AI
package into the example so the path stays inside the project:

```
[dependencies.ai]
path = ".ricochet/packages/ai"
```

Run the harness:

```
rco run examples/learn/28-ai/fake_provider_chat/fake_provider.rco
```

The first step builds the neutral provider and messages. This does not contact
the network:

```
"ai/openai" import

"openai" "https://api.example.test/v1" "gpt-fake" ai_provider provider var

messages array
$messages "Keep answers short and structured." ai_system_message push! drop
$messages "Return a fake provider response." ai_user_message push! drop
```

The request keeps model choice, messages, options, tool declarations, and retry
policy together:

```
options map
$options "temperature" 0.2 put! drop
tools array
3 0 0 ai_retry_policy retry var
$provider "gpt-fake" $messages $options $tools $retry ai_chat_request request var
```

The executor block is the fake provider. It receives the neutral request and
the 1-based retry attempt. The first attempt returns a `503` response map; the
second returns an OpenAI-compatible success body with text and a tool call:

```
[
  attempt var
  providerRequest var
  $attempts $attempt push! drop

  response map
  $attempt 1 = if
    $response "status" 503 put! drop
    $response "body" "{\"error\":{\"message\":\"busy\"}}" put! drop
  else
    $response "status" 200 put! drop
    $response "body" "{\"model\":\"gpt-fake\",\"choices\":[{\"message\":{\"content\":\"fake provider ready\",\"tool_calls\":[{\"id\":\"call-1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"{\\\"city\\\":\\\"Chicago\\\"}\"}}]}}]}" put! drop
  end
  $response ok
] executor var

$request $executor ai_openai_execute_chat result var
```

`ai_openai_execute_chat` applies the retry policy, converts non-2xx HTTP-like
responses into AI error maps, and converts success bodies into the neutral
`ai_chat_response` shape. The harness then validates the response map:

```
$result value chat var

ai_schema "text" "string" true ai_schema_field "provider" "map" true ai_schema_field schema var
$chat $schema ai_validate_schema schemaResult var
```

Tool handlers are local blocks keyed by tool name. The normalized tool call
keeps its `id`, `name`, and parsed `arguments` map:

```
ai_tool_handlers handlers var
$handlers "get_weather" [
  arguments var
  toolCall var
  content map
  $content "name" $toolCall "name" at put! drop
  $content "city" $arguments "city" at put! drop
  $content "forecast" "dry-run only" put! drop
  $content
] ai_tool_handler_put handlers set

$chat "tool_calls" at $handlers ai_execute_tool_calls toolResult var
```

Small stream bodies can be parsed directly:

```
"data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\ndata: [DONE]\n\n" ai_openai_stream_events streamResult var
```

The example prints a single JSON summary:

```
{"text":"fake provider ready","attempts":2,"provider":"openai","schema_ok":true,"tool_city":"Chicago","tool_forecast":"dry-run only","stream_events":3,"stream_done":true}
```

## How to read the example

Read the AI example as an application boundary, not as magic. Prompts, model configuration, provider responses, and tool permissions are data. Keep fallback behavior explicit and avoid giving a provider more capability than the example needs.

## Try it

Change the retry policy from `3 0 0 ai_retry_policy` to
`1 0 0 ai_retry_policy`. The harness should report the normalized error from
the fake `503` response instead of the success summary.

Then change the schema to require `"text" "number"`. The provider still
returns successfully, but `schema_ok` becomes false because the response text is
a string. That is the important split: transport success, model output shape,
and application validation are separate decisions.

When you are ready to use a real provider, keep the same neutral request shape
and replace the executor with one that builds an HTTP request using a resolved
secret, sends it through `http_request`, and returns that HTTP result. In MVC,
the `[ai.default]` capability belongs at the application boundary: controllers
may ask for AI work, but provider choice, credentials, rate limits, logging,
and data policy remain explicit app decisions.

## Check your understanding

- What new value shape, command, or host boundary did this chapter introduce?
- Which line is most important to trace with a stack diagram?
- Where would you add a binding to make the example easier to read?

## Common mistakes

* Mixing secret setup with first-time AI package learning.
* Treating provider success as proof that the response matches your app schema.
* Calling a live model before the fake-provider path is covered by tests.
* Hiding retry policy, model selection, or provider selection in a controller.
* Parsing partial stream frames without retained stream state.

## Safety notes

The example uses `https://api.example.test/v1` and a fake executor. It never
reads API keys, opens sockets, or sends data to a provider. Do not paste real
credentials into example files. Use secret references, explicit capability
flags, and provider allowlists when you move from fake executors to HTTP calls.

## Production guidance

Production AI integrations should make provider selection, credentials, rate
limits, retry budgets, timeout policy, request logging, prompt retention,
stream lifecycle, local tool permissions, schema validation, and user-visible
failure behavior explicit.

Use fake executors in tests for provider retry/error handling. Use retained
HTTP streams plus provider-specific stream state for long-running responses:
`ai_openai_stream_state` with `ai_openai_stream_read_events`,
`ai_anthropic_stream_state` with `ai_anthropic_stream_read_events`, or
`ai_ollama_stream_state` with `ai_ollama_stream_read_events`.

## Reference links

* `packages/ricochet_ai/README.md`
* `examples/showcase/ai_provider_probe/fake_provider.rco`
* `docs/feature-map.md`

## What you know now

You know how AI features fit into Ricochet’s package and capability model. You
can build a neutral chat request, test provider retry behavior without live
credentials, normalize provider responses, run local tool calls, validate
response maps, and choose the right stream parsing pattern for small bodies or
retained HTTP streams.

## Next step

Continue to [Chapter 29: Packages, Imports, And Dependencies](29-packages-imports-and-dependencies.md).
