# Web And Data

Serve an MVC app from its project directory:

```powershell
rco serve --host 127.0.0.1 --port 3000
rco serve --allow-env --http-allow-host 127.0.0.1
rco serve --env-allow OPENAI_API_KEY --http-allow-host api.openai.com
rco serve --allow-process --fs-root .
rco serve --allow-process --process-root .\scripts
rco serve --allow-pty --fs-root .
rco serve --watch
rco serve --watch --fs-root . --http-allow-host 127.0.0.1
```

`--watch` reloads Ricochet MVC routes, controllers, models, views, and the
manifest between requests. If a reload fails, the request returns a clear MVC
error and the next request retries after you fix the source. Combine `--watch`
with `--debug` to print reload trace lines with the new revision and changed
files. The same filesystem, HTTP, environment, process, and PTY capability
flags used by ordinary `rco serve` are also honored by watched MVC runtimes and
by each hot-reloaded revision.

Use `rco doctor [path]` for a read-only health check of a source file, source
tree, package project, or MVC app. Add `--capabilities` to print the MVC
manifest capability surface that will matter for trusted local beta apps.
Package projects can also run `rco verify [path]` to check dependency
manifest/lock consistency, local path containment, git package cache commit
matches, and locked package-content integrity without fetching or rewriting
anything.

`rco serve` keeps MVC process environment access disabled unless you pass
`--allow-env` or one or more `--env-allow NAME` entries. Prefer `--env-allow`
for trusted local beta apps that store secret references as environment
variable names. `--no-env` keeps the default disabled behavior explicit, and
conflicts with both env-opening flags.

## SQLite Scaffold

For a zero-service local beta app, `rco new --with-sqlite my_beta_app` creates
`db/development.sqlite3`, seeds `users`, configures Active Record, and adds
`/login`, `/me`, and `/logout` routes that exercise form params and the session
cookie. The manifest shape is:

```toml
[database.default]
adapter = "sqlite"
url = "db/development.sqlite3"
```

For production credential flows, import `@ricochet/auth` and validate
credentials before storing password hashes. The core `password_hash` and
`password_verify` words use Argon2id PHC-format hashes; the auth package wraps
them with credential normalization, length/common-password checks, and a generic
`auth_credentials_verify` result map for login paths.

```ricochet
"auth/session" import

"Ada@Example.COM" "Long unique passphrase 2026" auth_password_hash value hash var
" ada@example.com " "Long unique passphrase 2026" "ada@example.com" $hash auth_credentials_verify
```

## PostgreSQL

For a Postgres-backed app, use the same manifest shape with a Postgres URL:

```toml
[database.default]
adapter = "postgres"
url = "${DATABASE_URL}" # use sslmode=require for remote databases
```

Ricochet requires TLS for remote Postgres connections. `sslmode=disable` is
accepted only for `localhost` or loopback development databases.

## MySQL And MariaDB

For a MySQL or MariaDB-backed app, use the MySQL adapter with a `mysql://` URL:

```toml
[database.default]
adapter = "mysql"
url = "${MYSQL_URL}"
```

Active Record maps model declarations to existing tables, and `rco migrate`
applies ordered SQL or Ricochet DSL migrations from `db/migrations` while
recording applied versions in `schema_migrations`.

## Migrations And Seeds

Use `rco migrate new NAME [path]` to create an apply-only SQL migration, or
`rco migrate new NAME --dsl [path]` to create paired Ricochet migration DSL
files. Use `rco migrate status [path]` to list ordered migration files and
`rco migrate apply [path]` to apply pending SQL or DSL. Existing
`VERSION_name.sql` files remain apply-only migrations. For reversible
migrations, use paired SQL or DSL files:

```text
db/migrations/0002_create_notes.up.sql
db/migrations/0002_create_notes.down.sql
db/migrations/0003_create_tags.up.rco
db/migrations/0003_create_tags.down.rco
```

Migration DSL files are compiled only by the migration command; the DSL words
are not global runtime words. The first slice is intentionally small and stays
postfix:

```ricochet
"notes" table_create
"id" "integer" column primary_key
"body" "text" column not_null
```

Rollback DSL uses the same postfix shape:

```ricochet
"notes" table_drop
```

`rco migrate rollback [path] --steps 1` rolls back applied SQLite migrations
newest-first and removes each version from `schema_migrations` after its down
SQL or DSL succeeds. Rollback fails loudly when the newest migration has no
matching down migration file.

`rco migrate dump [path] --output db/schema.sql` writes a deterministic SQLite
schema dump for user tables, indexes, views, and triggers. The dump excludes
`schema_migrations` and SQLite internal objects.

`rco seed [path]` runs `db/seeds/*.sql` and `db/seeds/*.rco` files in filename
order for SQLite projects. SQL seeds execute directly. Ricochet seed files run
with project models loaded and the `db` capability available, so Active Record
methods such as `Note insert` can populate tables. Seeds are not tracked;
non-idempotent seed files will run again on every `rco seed`.

## Request Data

MVC actions parse `application/x-www-form-urlencoded`, `application/json`, and
`multipart/form-data` request bodies for `POST`, `PUT`, `PATCH`, and `DELETE`.
Declared action Args bind route params first, then form fields, JSON object
fields, upload fields, query params, and finally context values. The same data
is available through `$ctx "request" at`: `form` holds text fields, `json` and
`body` hold parsed JSON values, `uploads` is keyed by multipart file field
name, and `files` contains every uploaded file. Upload values include `name`,
`field`, `stream_id`, `filename`, `content_type`, `size_known`, `size`, `text`
when the bytes stay under the configured memory threshold and are UTF-8, and
`data_base64` for small arbitrary file bytes. Larger files are retained as
temporary upload streams and can be consumed with `upload_read`, inspected with
`upload_stream`, listed with `upload_streams`, and released with
`upload_release`.

Configure upload bounds in `ricochet.toml`:

```toml
[web.uploads]
max_request_bytes = 20971520
max_file_bytes = 18874368
memory_threshold_bytes = 65536
max_retained_streams = 4
```
