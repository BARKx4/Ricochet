# Ricochet First-Party Packages

These packages are beta developer targets for apps built with Ricochet. They are
published through the static registry flow and intentionally live outside the
language core.

- `@ricochet/auth`: session guards, route guard result maps, fail-closed
  CSRF/form-token helpers, secure cookie option maps, credential normalization,
  password policy validation, and Argon2id password hash/verify wrappers.
- `@ricochet/ai`: provider-agnostic HTTP request maps, OpenAI-compatible
  request builders, SSE response-body parsing, and response normalization
  layered on core secret refs.
- `@ricochet/forms`: form field maps, required-field validation, schema-shaped
  validation result maps, and multipart/upload helper maps.
- `@ricochet/test_helpers`: package and MVC test assertions, fixture maps,
  HTTP assertion helpers, and temporary workspace helper maps.

Publish the packages into a local static registry:

```powershell
rco publish packages/ricochet_auth --registry .registry
rco publish packages/ricochet_ai --registry .registry
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
