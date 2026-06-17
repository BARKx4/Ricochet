# @ricochet/ai

Beta helpers for building provider-agnostic AI HTTP calls in Ricochet apps.

This package keeps provider workflows outside Ricochet core. It builds request
maps, env-secret reference maps, OpenAI-compatible starter bodies, and normalized
response maps. Apps still own provider selection, retry policy, tool execution,
streaming, and user-facing agent behavior.

```ricochet
"ai/openai" import

messages array
messages get map push! drop
"https://api.openai.com/v1" "token" "gpt-4.1-mini" messages get ai_openai_chat_request
```
