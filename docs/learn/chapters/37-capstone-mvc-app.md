# Chapter 37: Capstone MVC App

## What You Will Build

You will build a database-backed project journal MVC app. The checked-in app
has routes, a controller, an Active Record model, an escaped HTML view, static
CSS, reversible SQL migrations, seeds, and MVC tests. The validation commands
inspect and compile the app without applying migrations or creating local
database state.

## Concepts

- Routes, controllers, templates, static assets, uploads, sessions, forms, auth helpers, migrations, seeds, and tests.
- SQLite locally, with deployment notes for other databases.
- Debug mode, watch mode, and route inspection.
- Read-only validation before serving or changing database state.
- Keeping model behavior, controller response policy, and template rendering
  in separate places.

## Words Introduced

This chapter consolidates MVC, data, auth, forms, and testing concepts.

## Guided Example

Open `examples/learn/37-capstone-mvc/project_journal`:

```text
ricochet.toml
config/routes.rco
app/Controllers/JournalController.rco
app/Models/JournalEntry.rco
app/Views/journal/index.html
public/app.css
db/migrations/0001_create_journal_entries.up.sql
db/migrations/0001_create_journal_entries.down.sql
db/seeds/001_journal_entries.sql
tests/JournalEntryTest.rco
```

The manifest declares an MVC app, static assets, escaped views, and a local
SQLite development database:

```toml
[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[database.default]
adapter = "sqlite"
url = "db/development.sqlite3"
```

Routes are still postfix:

```ricochet
GET "/" JournalController "index" route
GET "/entries" JournalController "index" route
GET "/entries/export" JournalController "export_json" route
GET "/entries/status" JournalController "by_status" route
GET "/entries/:id" JournalController "show" route
POST "/entries" JournalController "create" route
POST "/entries/:id" JournalController "update" route
```

Validate the route table:

```powershell
cargo run -q -p ricochet_cli --bin rco -- routes examples/learn/37-capstone-mvc/project_journal
```

Expected output:

```text
GET / JournalController#index
GET /entries JournalController#index
GET /entries/export JournalController#export_json
GET /entries/status JournalController#by_status
GET /entries/:id JournalController#show
POST /entries JournalController#create
POST /entries/:id JournalController#update
```

The model maps to the migration table and adds one display method:

```ricochet
JournalEntry Model Subclass
  "journal_entries" Table
  "id" Accessor
  "title" Accessor
  "body" Accessor
  "status" Accessor
  "mood" Accessor
  "created_on" Accessor

  [
    self title.get
    " [" concat
    self status.get concat
    "]" concat
  ] "label" Method
end
```

The index action uses database results defensively, so an uncreated local
development database still gives the view empty arrays and counts during
compile-time checks:

```ricochet
JournalEntry default_page dup ok? if
  value entries var
else
  drop
  entries array
end

JournalEntry count_records dup ok? if
  value totalEntries var
else
  drop
  0 totalEntries var
end
```

Run the doctor:

```powershell
cargo run -q -p ricochet_cli --bin rco -- doctor examples/learn/37-capstone-mvc/project_journal
```

Expected output includes:

```text
OK manifest: package learn_project_journal
OK project kind: MVC app
OK routes: 7 route(s)
OK MVC app build: controllers, models, routes, and views compile
Doctor found no issues.
```

Inspect migrations without applying them:

```powershell
cargo run -q -p ricochet_cli --bin rco -- migrate status examples/learn/37-capstone-mvc/project_journal
```

Expected output:

```text
[ ] 0001_create_journal_entries
```

Run the MVC tests:

```powershell
cargo run -q -p ricochet_cli --bin rco -- test examples/learn/37-capstone-mvc/project_journal
```

Expected output:

```text
PASS JournalEntryTest.testEntriesCollection
PASS JournalEntryTest.testEntryLabel
2 tests, 0 failed
```

The tests exercise model behavior without creating the database:

```ricochet
JournalEntry new
"Manual outline" swap title.set
"published" swap status.set
label
"Manual outline [published]" assert_equals
```

## Try It

Add a `"tag"` column:

1. Add `"tag" Accessor` to `JournalEntry.rco`.
2. Add `tag text not null default 'general'` to the up migration.
3. Add `tag` data to the seed file.
4. Print `{ entry get "tag" at }` in the view.
5. Add a model test that sets `tag` and verifies the value.

Then run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- routes examples/learn/37-capstone-mvc/project_journal
cargo run -q -p ricochet_cli --bin rco -- doctor examples/learn/37-capstone-mvc/project_journal
cargo run -q -p ricochet_cli --bin rco -- test examples/learn/37-capstone-mvc/project_journal
```

When you intentionally want local database state, apply migrations and seeds:

```powershell
cargo run -q -p ricochet_cli --bin rco -- migrate apply examples/learn/37-capstone-mvc/project_journal
cargo run -q -p ricochet_cli --bin rco -- seed examples/learn/37-capstone-mvc/project_journal
```

Those commands create or update `db/development.sqlite3`, so they are not part
of the default Learn validation manifest.

## Common Mistakes

- Mixing scaffold convenience with production assumptions.
- Letting controller, model, and template responsibilities blur.
- Running `migrate apply` or `seed` just to check syntax. Use `routes`,
  `doctor`, `migrate status`, and `test` first.
- Treating a scaffold login loop or local seed data as production policy.
- Returning raw database failures to users without choosing response status and
  message shape deliberately.
- Forgetting that route action args bind request data and context according to
  the MVC binding rules.

## Safety Notes

The main validation path is read-only. `migrate status` inspects migration
state, while `migrate apply`, `migrate rollback`, and `seed` intentionally
change the local development database. The down migration contains the rollback
SQL needed for a reversible schema, but this chapter does not run it.

## Production Notes

Production MVC apps should review database adapter settings, migration policy,
session secrets, authentication and CSRF policy, upload limits, static assets,
logging, and capability declarations. Keep environment-backed secrets out of
the repository, and keep seed data separate from production migrations.

Use `rco serve --watch` for local development and `rco serve --debug` when you
want request-fault pause reporting before HTTP 500 responses.

## Reference Links

- `docs/learn/chapters/23-mvc-first-app.md`
- `docs/learn/chapters/24-routes-controllers-and-responses.md`
- `docs/learn/chapters/25-templates-static-assets-and-uploads.md`
- `docs/learn/chapters/26-data-active-record-and-migrations.md`
- `docs/learn/chapters/27-sessions-forms-auth-and-passwords.md`
- `docs/learn/chapters/32-debugger-dap-lsp-and-editor-tools.md`
- `docs/reference/guides/web-and-data.html`

## What You Know Now

You know how the Ricochet web stack fits together in a complete app: manifest,
routes, controller actions, model mapping, templates, static assets, migrations,
seeds, route inspection, project doctor checks, and MVC tests.
