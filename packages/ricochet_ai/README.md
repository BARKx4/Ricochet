# @ricochet/ai

Beta helpers for building provider-agnostic AI HTTP calls in Ricochet apps.

This package keeps provider workflows outside Ricochet core. It builds request
maps, OpenAI-compatible starter bodies, and normalized response maps on top of
core `secret_env`, `secret_resolve`, and HTTP request-map helpers. Apps still
own provider selection, retry policy, tool execution, streaming, and user-facing
agent behavior.

```ricochet
"ai/openai" import

messages array
messages get map push! drop
"OPENAI_API_KEY" secret_env secret_resolve value token var
"https://api.openai.com/v1" token get "gpt-4.1-mini" messages get ai_openai_chat_request
```
