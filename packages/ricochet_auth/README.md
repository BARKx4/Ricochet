# @ricochet/auth

Beta helpers for session-aware Ricochet MVC apps.

This package provides session guard predicates, route guard result maps,
fail-closed CSRF comparison helpers, secure cookie option defaults,
credential normalization, production-oriented password policy validation, and
Argon2id password hash/verify wrappers.

```ricochet
"auth/session" import

"ada" "csrf-token" auth_session_for_user session var
$session auth_user_present
$session "csrf-token" auth_csrf_check "ok" at
auth_secure_cookie_options

"Ada@Example.COM" "Long unique passphrase 2026" auth_password_hash value hash var
" ada@example.com " "Long unique passphrase 2026" "ada@example.com" $hash auth_credentials_verify
```
