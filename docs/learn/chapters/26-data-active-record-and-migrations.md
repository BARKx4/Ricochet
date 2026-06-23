# Chapter 26: Data, Active Record, And Migrations

> Part IV: MVC, Data, Auth, Forms, and AI

## Why this chapter matters

Persistent data requires a stronger mental model than maps in memory. Models, migrations, and records connect Ricochet objects to database state.

## What you will build

You will build the data layer for a contacts MVC app: a model, a reversible
SQLite migration, seed files, and controller actions that use Active Record
query and write words. The runnable self-check path is read-only and checks
migration status without creating `db/development.sqlite3`.

## Concepts in plain English

Active Record maps a class to a table. A migration changes schema in an ordered, repeatable way.

The chapter uses these concepts:

* Active Record model declarations over existing tables.
* SQLite database configuration.
* SQL migrations, reversible migration pairs, and seeds.
* Read words: `all`, `find_record`, `default_page`, `where`, `limit`,
  `count_records`, `first_record`, and `exists?`.
* Write words: `insert` and `update`.
* Notes for PostgreSQL and MySQL deployment.

## Vocabulary and commands

Primary coverage: Active Record web words and the migration CLI family:
`rco migrate new`, `rco migrate status`, `rco migrate apply`,
`rco migrate rollback`, `rco migrate dump`, and `rco seed`.

## Guided example

Open `examples/learn/26-data/contacts_app`. The manifest declares SQLite but
does not check in a database file:

```
[database.default]
adapter = "sqlite"
url = "db/development.sqlite3"
```

The model maps a class to the table that the migration creates:

```
Contact Model Subclass
  "contacts" Table
  "id" Accessor
  "name" Accessor
  "email" Accessor
  "status" Accessor
end
```

The migration is reversible:

```
create table contacts (
  id integer primary key,
  name text not null,
  email text not null unique,
  status text not null default 'active'
);
```

Its matching down migration drops the table:

```
drop table contacts;
```

Check migration status without creating the database:

```
rco migrate status examples/learn/26-data/contacts_app
```

The output should show the configured SQLite target and one unapplied migration:

```
Migrations for ...\examples\learn\26-data\contacts_app\db\development.sqlite3
[ ] 0001_create_contacts
```

Run the MVC doctor to compile controllers, models, routes, and views:

```
rco doctor examples/learn/26-data/contacts_app
```

The index action uses read-only Active Record words and handles missing database
state by falling back to empty view data:

```
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

```
"email" $email Contact where
$id Contact find_record
$id Contact exists?
20 Contact limit
Contact all
```

Writes use a map of attributes:

```
contact map
$contact "name" $name put! drop
$contact "email" $email put! drop
$contact "status" "active" put! drop
$contact Contact insert
```

Updates put the id before the attributes map and the model:

```
changes map
$changes "name" $name put! drop
$changes "email" $email put! drop
$changes "status" $state put! drop
$id $changes Contact update
```

## How to read the example

Read the MVC example in layers. First identify the route or command that enters the app. Then find the controller action. Then follow the data into the response, view, model, or database boundary. Each layer is still ordinary Ricochet: values first, words second, and explicit results at boundaries.

## Try it

Apply the migration when you are ready to create local database state:

```
rco migrate apply examples/learn/26-data/contacts_app
```

Seed sample rows:

```
rco seed examples/learn/26-data/contacts_app
```

Now serve the app locally:

```
rco serve examples/learn/26-data/contacts_app --host 127.0.0.1 --port 3000
```

Use `rco migrate dump --output db/schema.sql` when you want a deterministic beta
schema snapshot. Use `rco migrate rollback --steps 1` only when you are
intentionally reversing the newest applied migration.

## Check your understanding

- Which route, controller, view, model, or form boundary is this chapter teaching?
- Where does request data become ordinary Ricochet data?
- Where can failure happen, and is it represented as a result, response, or diagnostic?
- What would you test before serving the app?

## Common mistakes

* Treating migrations as optional once a database exists.
* Confusing model accessors with plain map keys.
* Describing Active Record as a schema-definition ORM; it maps models to
  existing schemas.
* Running seeds repeatedly and expecting them to be idempotent.
* Serving before checking migration status.

## Safety notes

The self-check command for this chapter is `rco migrate status`, which is
read-only when the SQLite database file is absent. `migrate apply`, `seed`,
`dump`, and `rollback` create or modify local database/schema files. Do not
delete or prune generated database state without explicit confirmation.

## Production guidance

Production apps should document database configuration, migration order, backup
and restore expectations, rollback policy, seed idempotency, and adapter
differences. `rco migrate dump` is a Ricochet beta schema snapshot, not a
replacement for `pg_dump`, `mysqldump`, or an operator’s backup tooling.

## Reference links

* `docs/reference/guides/web-and-data.html`
* `docs/reference/guides/host-capabilities.html`

## What you know now

You know how Ricochet models map to tables, how migrations and seeds fit the
local SQLite workflow, and how Active Record read/write words keep database
operations in postfix order.

## Next step

Continue to [Chapter 27: Sessions, Forms, Auth, And Passwords](27-sessions-forms-auth-and-passwords.md).
