# Ricochet First-Party Packages

These packages are beta developer targets for apps built with Ricochet. They are
published through the static registry flow and intentionally live outside the
language core.

- `@ricochet/auth`: session guards, route guard result maps, fail-closed
  CSRF/form-token helpers, secure cookie option maps, credential normalization,
  password policy validation, and Argon2id password hash/verify wrappers.
- `@ricochet/ai`: provider-neutral provider/message/request/response/error
  contracts, retry policy and retry-execution helpers, OpenAI-compatible
  fake-provider execution/normalization helpers, tool call/result and
  tool-handler execution helpers, schema validation, OpenAI-compatible request
  builders, and SSE response-body/event parsing layered on core secret refs.
- `@ricochet/ui`: backend-neutral native app UI document, event, command, and
  validation helpers for Ricochet app executables.
- `@ricochet/winui`: WinUI backend descriptor and scoped native option helpers
  for `@ricochet/ui` apps.
- `@ricochet/avalonia`: Avalonia backend descriptor and scoped native option
  helpers for cross-platform desktop `@ricochet/ui` apps.
- `@ricochet/slint`: Slint backend descriptor and scoped native option helpers
  for lightweight and experimental `@ricochet/ui` app payloads.
- `@ricochet/python`: process-backed JSON-lines worker helpers for importing
  Python modules, calling SDK functions/classes/methods, retaining Python object
  references, inspecting module exports, and generating static Ricochet wrapper
  source for checked-in bindings.
- `@ricochet/forms`: form field maps, required-field validation, schema-shaped
  validation result maps, and multipart/upload helper maps.
- `@ricochet/test_helpers`: package and MVC test assertions, fixture maps,
  HTTP assertion helpers, and temporary workspace helper maps.

Publish the packages into a local static registry:

```powershell
rco publish packages/ricochet_auth --registry .registry
rco publish packages/ricochet_ai --registry .registry
rco publish packages/ricochet_ui --registry .registry
rco publish packages/ricochet_winui --registry .registry
rco publish packages/ricochet_avalonia --registry .registry
rco publish packages/ricochet_slint --registry .registry
rco publish packages/ricochet_python --registry .registry
rco publish packages/ricochet_forms --registry .registry
rco publish packages/ricochet_test_helpers --registry .registry
rco registry rebuild .registry
rco registry check .registry
```

Install from that registry with scoped identities and local aliases:

```powershell
rco add registry:@ricochet/forms --registry-url file:///E:/path/to/.registry/index.toml --as forms
"forms/validation" import
```
