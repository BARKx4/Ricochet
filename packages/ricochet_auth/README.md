# @ricochet/auth

Beta helpers for session-aware Ricochet MVC apps.

This package provides session guard predicates, route guard result maps, CSRF
token comparison helpers, and small extension-point maps. It deliberately does
not implement production password storage or user credential policy.

```ricochet
"auth/session" import

session map
session get "user_id" "ada" put! drop
session get auth_user_present
```
