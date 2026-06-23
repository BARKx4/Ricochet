# Chapter 23: MVC First App

> Part IV: MVC, Data, Auth, Forms, and AI

## Why this chapter matters

MVC apps are the first large application shape in the guide. Routes, controllers, views, and models are easier when you see them as named layers around the same Ricochet language.

## What you will build

You will inspect and run the first Ricochet MVC app shape: manifest, routes,
controllers, models, views, static assets, and project checks. The sample app
is shaped like a fresh non-SQLite `rco new` project, but it avoids creating or
deleting local app state while you are learning the layout.

## Concepts in plain English

MVC separates an app into models for data, views for rendering, and controllers for request behavior.

The chapter uses these concepts:

* `rco new`, SQLite scaffolding, `rco serve`, watch mode, and route inspection.
* Generated project layout.
* Capability boundaries for MVC apps.
* Why examples validate with `rco routes` and `rco doctor` before serving.

## Vocabulary and commands

Primary coverage: project and MVC command family: `rco new`, `rco routes`,
`rco doctor`, `rco serve`, `rco serve --watch`, and the default MVC layout.

## Guided example

Open `examples/learn/23-mvc/first_app`. Its manifest declares an MVC app,
route file, HTML-escaped views, and static assets:

```
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

```
GET "/" HomeController "index" route
GET "/users" UserController "index" route
```

Validate the route table without starting a server:

```
rco routes examples/learn/23-mvc/first_app
```

The output should include:

```
GET / HomeController#index
GET /users UserController#index
```

Run the project doctor for a broader compile/configuration check:

```
rco doctor examples/learn/23-mvc/first_app
```

The home controller renders a view through the request context:

```
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

```
users array
User new
"ada@example.com" swap email.set
"Ada Lovelace" swap name.set
$users swap push! drop
$users count userCount var
```

## How to read the example

Read the MVC example in layers. First identify the route or command that enters the app. Then find the controller action. Then follow the data into the response, view, model, or database boundary. Each layer is still ordinary Ricochet: values first, words second, and explicit results at boundaries.

## Try it

Generate a throwaway scaffold somewhere outside this example tree:

```
rco new scratch_app
```

Add `--with-sqlite` when you want the local beta SQLite app with seeded data,
login routes, and a development database. Start an app locally with:

```
rco serve examples/learn/23-mvc/first_app --host 127.0.0.1 --port 3000
```

Use `--watch` during development so controllers, models, routes, views, and
manifest settings reload between requests.

## Check your understanding

- Which route, controller, view, model, or form boundary is this chapter teaching?
- Where does request data become ordinary Ricochet data?
- Where can failure happen, and is it represented as a result, response, or diagnostic?
- What would you test before serving the app?

## Common mistakes

* Treating scaffold auth as production auth.
* Looking for broad trusted-script behavior inside MVC.
* Starting a server before checking routes and doctor output.
* Confusing desktop webview GUIs with MVC browser/server apps.

## Safety notes

This sample app has no generated database. `rco new --with-sqlite`
creates `db/development.sqlite3`; treat that as generated local app state.
Serve examples on `127.0.0.1` unless you have a reason to bind more broadly.

## Production guidance

Production MVC apps should review configuration, database, auth, static asset,
session, and capability settings before deployment. The scaffold is a local
beta starting point, not a complete production policy.

## Reference links

* `docs/reference/guides/host-capabilities.html`
* `docs/reference/guides/language-runtime.html`

## What you know now

You know the shape of a Ricochet web app: a manifest points to routes, routes
name controller actions, controllers prepare view data, views render escaped
HTML, and static assets are served from a configured public directory.

## Next step

Continue to [Chapter 24: Routes, Controllers, And Responses](24-routes-controllers-and-responses.md).
