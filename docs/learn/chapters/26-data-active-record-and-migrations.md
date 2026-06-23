# Chapter 26: Data, Active Record, And Migrations

## What You Will Build

You will build the data layer for a contacts MVC app: a model, a reversible
SQLite migration, seed files, and controller actions that use Active Record
query and write words. The runnable validation path is read-only and checks
migration status without creating `db/development.sqlite3`.

## Concepts

- Active Record model declarations over existing tables.
- SQLite database configuration.
- SQL migrations, reversible migration pairs, and seeds.
- Read words: `all`, `find_record`, `default_page`, `where`, `limit`,
  `count_records`, `first_record`, and `exists?`.
- Write words: `insert` and `update`.
- Notes for PostgreSQL and MySQL deployment.

## Words Introduced

Primary coverage: Active Record web words and the migration CLI family:
`rco migrate new`, `rco migrate status`, `rco migrate apply`,
`rco migrate rollback`, `rco migrate dump`, and `rco seed`.

## Guided Example

Open `examples/learn/26-data/contacts_app`. The manifest declares SQLite but
does not check in a database file:

```toml
[database.default]
adapter = "sqlite"
url = "db/development.sqlite3"
```

The model maps a class to the table that the migration creates:

```ricochet
Contact Model Subclass
  "contacts" Table
  "id" Accessor
  "name" Accessor
  "email" Accessor
  "status" Accessor
end
```

The migration is reversible:

```sql
create table contacts (
  id integer primary key,
  name text not null,
  email text not null unique,
  status text not null default 'active'
);
```

Its matching down migration drops the table:

```sql
drop table contacts;
```

Check migration status without creating the database:

```powershell
cargo run -q -p ricochet_cli --bin rco -- migrate status examples/learn/26-data/contacts_app
```

The output should show the configured SQLite target and one unapplied migration:

```text
Migrations for ...\examples\learn\26-data\contacts_app\db\development.sqlite3
[ ] 0001_create_contacts
```

Run the MVC doctor to compile controllers, models, routes, and views:

```powershell
cargo run -q -p ricochet_cli --bin rco -- doctor examples/learn/26-data/contacts_app
```

The index action uses read-only Active Record words and handles missing database
state by falling back to empty view data:

```ricochet
Contact default_page dup ok? if
  value contacts var
else
  drop
  contacts array
end

Contact count_records dup ok? if
  value totalContacts var
else
  drop
  0 totalContacts var
end

Contact first_record dup ok? if
  value firstContact var
else
  drop
  nil firstContact var
end
```

Routes that need narrower reads keep arguments below the model:

```ricochet
"email" $email Contact where
$id Contact find_record
$id Contact exists?
20 Contact limit
Contact all
```

Writes use a map of attributes:

```ricochet
contact map
$contact "name" $name put! drop
$contact "email" $email put! drop
$contact "status" "active" put! drop
$contact Contact insert
```

Updates put the id before the attributes map and the model:

```ricochet
changes map
$changes "name" $name put! drop
$changes "email" $email put! drop
$changes "status" $state put! drop
$id $changes Contact update
```

## Try It

Apply the migration when you are ready to create local database state:

```powershell
cargo run -q -p ricochet_cli --bin rco -- migrate apply examples/learn/26-data/contacts_app
```

Seed sample rows:

```powershell
cargo run -q -p ricochet_cli --bin rco -- seed examples/learn/26-data/contacts_app
```

Now serve the app locally:

```powershell
cargo run -q -p ricochet_cli --bin rco -- serve examples/learn/26-data/contacts_app --host 127.0.0.1 --port 3000
```

Use `rco migrate dump --output db/schema.sql` when you want a deterministic beta
schema snapshot. Use `rco migrate rollback --steps 1` only when you are
intentionally reversing the newest applied migration.

## Common Mistakes

- Treating migrations as optional once a database exists.
- Confusing model accessors with plain map keys.
- Describing Active Record as a schema-definition ORM; it maps models to
  existing schemas.
- Running seeds repeatedly and expecting them to be idempotent.
- Serving before checking migration status.

## Safety Notes

The validation command for this chapter is `rco migrate status`, which is
read-only when the SQLite database file is absent. `migrate apply`, `seed`,
`dump`, and `rollback` create or modify local database/schema files. Do not
delete or prune generated database state without explicit confirmation.

## Production Notes

Production apps should document database configuration, migration order, backup
and restore expectations, rollback policy, seed idempotency, and adapter
differences. `rco migrate dump` is a Ricochet beta schema snapshot, not a
replacement for `pg_dump`, `mysqldump`, or an operator's backup tooling.

## Reference Links

- `docs/reference/guides/web-and-data.html`
- `docs/reference/guides/host-capabilities.html`

## What You Know Now

You know how Ricochet models map to tables, how migrations and seeds fit the
local SQLite workflow, and how Active Record read/write words keep database
operations in postfix order.
