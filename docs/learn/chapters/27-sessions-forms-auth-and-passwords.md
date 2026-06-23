# Chapter 27: Sessions, Forms, Auth, And Passwords

## What You Will Build

You will build the logic for a local login flow: form maps, schema validation,
credential normalization, password hashing and verification, session maps, CSRF
checks, and route guards. The runnable example is a command-line harness so it
can validate the auth/form behavior without starting a server or logging
secrets.

## Concepts

- Sessions and form handling.
- Auth helpers and password policy words from `@ricochet/auth`.
- Form field and schema helpers from `@ricochet/forms`.
- Core password hashing and verification words.
- CSRF checks and route guard result maps.
- Local beta scaffold auth versus production auth decisions.

## Words Introduced

Primary coverage: sessions, auth package helpers, forms package helpers,
`password_hash`, and `password_verify`.

## Guided Example

Open `examples/learn/27-auth-forms/login_flow`. Its manifest imports the local
first-party auth and forms packages by path:

```toml
[dependencies.auth]
path = ".ricochet/packages/auth"

[dependencies.forms]
path = ".ricochet/packages/forms"
```

Run the harness:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/27-auth-forms/login_flow/auth_flow.rco
```

The first section demonstrates the core password words without printing the
hash:

```ricochet
"Long unique passphrase 2026" password_hash value rawHash var
"Long unique passphrase 2026" $rawHash password_verify value println
```

For application flows, prefer the auth package wrapper so password policy and
credential normalization stay in one place:

```ricochet
"ada@example.com" "Long unique passphrase 2026" auth_password_hash value storedHash var
" ADA@Example.COM " "Long unique passphrase 2026" "ada@example.com" $storedHash auth_credentials_verify login var
$login "authenticated" at println
```

`auth_password_hash` validates policy before calling `password_hash`.
`auth_credentials_verify` normalizes the submitted credential before comparing
it with the stored credential and hash.

Session helpers work with ordinary maps:

```ricochet
"ada" "csrf-token-123" auth_session_for_user session var
$session auth_user_present
$session "csrf-token-123" auth_csrf_valid
$session "/login" auth_route_guard
```

The route guard returns a map with `ok`, `redirect`, and `reason`. That shape is
easy for controllers to inspect before returning a view or redirect.

Form helpers build field maps and schema-shaped validation results:

```ricochet
"email" "ada@example.com" form_field emailField var
$emailField "value" at form_required

form_schema
  "email" "string" form_required_field
  "password" "string" form_required_field
schema var

formData map
$formData "email" " ada@example.com " auth_credential_normalize put! drop
$formData "password" "Long unique passphrase 2026" put! drop
$formData $schema form_schema_validate validForm var
```

The harness prints only booleans and policy names:

```text
Core password verify:true
Auth login authenticated:true
Session user present:true
CSRF valid:true
Route guard ok:true
Password storage:argon2id
Field required:true
Form valid:true
Empty form valid:false
Cookie same_site:Lax
```

## Try It

Change the submitted password in the `auth_credentials_verify` call and rerun
the harness. The result should keep `ok` true for the verification operation
itself, set `authenticated` to false, and report an invalid-credentials reason.

To move this logic into MVC controllers, bind declared args such as
`( email password session ctx -> Response )`, validate the form, verify the
credentials, then write the session map only after authentication succeeds.

## Common Mistakes

- Reusing scaffold auth choices without reviewing production needs.
- Logging password or session material.
- Treating `ok` from a credential verification map as the same as
  `authenticated`.
- Skipping CSRF checks because a route is "just local".
- Storing raw passwords instead of PHC-format hashes.

## Safety Notes

The example does not print password hashes, session cookies, or CSRF tokens.
The literal passphrase is a local learning fixture, not an application secret.
Do not log submitted credentials, stored hashes, session cookies, or CSRF
tokens in real apps.

## Production Notes

Production auth should document password policy, session lifetime, cookie
settings, CSRF policy, secret handling, reset/recovery flows, lockout/rate-limit
policy, audit logging, and deployment assumptions. The scaffold login loop is a
copyable local beta starting point, not a complete production auth system.

## Reference Links

- `docs/reference/guides/web-and-data.html`
- `packages/ricochet_auth/README.md`
- `packages/ricochet_forms/README.md`

## What You Know Now

You know where form validation, credential normalization, password hashing,
CSRF checks, route guards, and session maps fit in a Ricochet MVC login flow.
