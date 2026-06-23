# Chapter 23: MVC First App

## What You Will Build

You will inspect and run the first Ricochet MVC app shape: manifest, routes,
controllers, models, views, static assets, and project checks. The checked-in
example mirrors the non-SQLite `rco new` scaffold so validation does not create
or delete generated files.

## Concepts

- `rco new`, SQLite scaffolding, `rco serve`, watch mode, and route inspection.
- Generated project layout.
- Capability boundaries for MVC apps.
- Why examples validate with `rco routes` and `rco doctor` before serving.

## Words Introduced

Primary coverage: project and MVC command family: `rco new`, `rco routes`,
`rco doctor`, `rco serve`, `rco serve --watch`, and the default MVC layout.

## Guided Example

Open `examples/learn/23-mvc/first_app`. Its manifest declares an MVC app,
route file, HTML-escaped views, and static assets:

```toml
[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[web.static]
dir = "public"
mount = "/assets"
```

Routes stay postfix:

```ricochet
GET "/" HomeController "index" route
GET "/users" UserController "index" route
```

Validate the route table without starting a server:

```powershell
cargo run -q -p ricochet_cli --bin rco -- routes examples/learn/23-mvc/first_app
```

The output should include:

```text
GET / HomeController#index
GET /users UserController#index
```

Run the project doctor for a broader compile/configuration check:

```powershell
cargo run -q -p ricochet_cli --bin rco -- doctor examples/learn/23-mvc/first_app
```

The home controller renders a view through the request context:

```ricochet
HomeController Controller Subclass
  [
    "Hello Ricochet" title var
    $ctx
    "home/index" swap view
  ] "index" Method
end
```

The users controller creates an in-memory model instance. Database-backed
queries start in Chapter 26:

```ricochet
users array
User new
"ada@example.com" swap email.set
"Ada Lovelace" swap name.set
$users swap push! drop
$users count userCount var
```

## Try It

Generate a throwaway scaffold somewhere outside this example tree:

```powershell
cargo run -q -p ricochet_cli --bin rco -- new scratch_app
```

Add `--with-sqlite` when you want the local beta SQLite app with seeded data,
login routes, and a development database. Start an app locally with:

```powershell
cargo run -q -p ricochet_cli --bin rco -- serve examples/learn/23-mvc/first_app --host 127.0.0.1 --port 3000
```

Use `--watch` during development so controllers, models, routes, views, and
manifest settings reload between requests.

## Common Mistakes

- Treating scaffold auth as production auth.
- Looking for broad trusted-script behavior inside MVC.
- Starting a server before checking routes and doctor output.
- Confusing desktop webview GUIs with MVC browser/server apps.

## Safety Notes

This checked-in example has no generated database. `rco new --with-sqlite`
creates `db/development.sqlite3`; treat that as generated local app state.
Serve examples on `127.0.0.1` unless you have a reason to bind more broadly.

## Production Notes

Production MVC apps should review configuration, database, auth, static asset,
session, and capability settings before deployment. The scaffold is a local
beta starting point, not a complete production policy.

## Reference Links

- `docs/reference/guides/host-capabilities.html`
- `docs/reference/guides/language-runtime.html`

## What You Know Now

You know the shape of a Ricochet web app: a manifest points to routes, routes
name controller actions, controllers prepare view data, views render escaped
HTML, and static assets are served from a configured public directory.
