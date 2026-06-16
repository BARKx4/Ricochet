# Ricochet Agent Notes

## Data Safety

- Always ask the user before deleting anything.
- If a problem has no logical explanation from the evidence available, fail loudly and ask for diagnostic help.

## Syntax Guardrail

All future language and platform features must pass the RPN/postfix vibe check before implementation.

- Keep receivers before selectors: `user printEmail`, `user email.get`, `"ada@example.com" user email.set`.
- Keep arguments below the receiver for selectors: `10 User limit`, `"email" "ada@example.com" User where`.
- Keep containers before keys for global access/mutation: `request "method" at`, `settings "theme" "dark" put!`.
- Keep collection mutation as `collection value push!`.
- Do not introduce leading-dot source syntax, fake namespace-dot host APIs, or receiver-first pseudo-object calls like `http .request`.
- Host/platform APIs should be snake_case global words such as `fs_read_text`, `http_request`, `webview_window`, and `tui_write`.
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
