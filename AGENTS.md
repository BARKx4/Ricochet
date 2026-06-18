# Ricochet Agent Notes

## Data Safety

- Always ask the user before deleting anything.
- If a problem has no logical explanation from the evidence available, fail loudly and ask for diagnostic help.

## Feature Map

- Before making roadmap claims or planning a broad feature, read
  `docs/feature-map.md`.
- Treat the feature map as the agent-facing orientation layer. If it conflicts
  with live code, tests, or `docs/reference`, verify the live source and update
  the map.

## Syntax Guardrail

All future language and platform features must pass the RPN/postfix vibe check before implementation.

- Keep receivers before selectors: `user printEmail`, `user email.get`, `"ada@example.com" user email.set`.
- Keep arguments below the receiver for selectors: `10 User limit`, `"email" "ada@example.com" User where`.
- Keep containers before keys for global access/mutation: `request "method" at`, `settings "theme" "dark" put!`.
- Keep collection mutation as `collection value push!`.
- Do not introduce leading-dot source syntax, fake namespace-dot host APIs, or receiver-first pseudo-object calls like `http .request`.
- Public multiword Ricochet words use `_` as their separator, such as `json_encode`, `find_record`, `fs_read_text`, `http_request`, `webview_window`, and `tui_write`.
- Reserve `-` for the subtraction word and negative number literals, not word naming.
- OOP declarations use capitalized meta words in class bodies:

```ricochet
User Model Subclass
  "users" Table
  "email" Accessor

  [
    self email.get
  ] "displayName" Method
end
```

Use lowercase `end` for all block structures.
