# @ricochet/auth

Beta helpers for session-aware Ricochet MVC apps.

This package provides session guard predicates, route guard result maps,
fail-closed CSRF comparison helpers, secure cookie option defaults, and small
extension-point maps. It deliberately does not implement production password
storage or user credential policy.

```ricochet
"auth/session" import

"ada" "csrf-token" auth_session_for_user session var
$session auth_user_present
$session "csrf-token" auth_csrf_check "ok" at
auth_secure_cookie_options
```
