# Chapter 24: Routes, Controllers, And Responses

> Part IV: MVC, Data, Auth, Forms, and AI

## Why this chapter matters

Routes turn HTTP requests into controller actions. This chapter teaches how requests become data and how controller methods return responses.

## What you will build

You will build a small MVC route table and one controller that returns views,
plain text, JSON, redirects, status codes, and headers. The example stays
inside a sample app so route inspection and project checks are repeatable.

## Concepts in plain English

A route matches an HTTP method and path, then calls a controller action. The action returns a response.

The chapter uses these concepts:

* Route verb aliases and the postfix `route` word.
* Controller methods as named actions.
* Response helpers: `view`, `text`, `json`, `redirect`, `status`, and `header`.
* Declared route and request arguments for POST, PUT, PATCH, and DELETE.
* Keeping routes readable as an app grows.

## Vocabulary and commands

Primary coverage: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `route`, `view`,
`text`, `json`, `redirect`, `status`, `header`, and `println` for local
diagnostics while developing actions.

## Guided example

Open `examples/learn/23-mvc/controllers`. Its route file maps HTTP verbs and
paths to controller/action pairs:

```
GET "/" ResponsesController "index" route
GET "/ping" ResponsesController "ping" route
GET "/api/status" ResponsesController "status_json" route
GET "/old-dashboard" ResponsesController "legacy" route
POST "/messages" ResponsesController "create" route
PUT "/messages/:id" ResponsesController "update" route
PATCH "/messages/:id" ResponsesController "patch" route
DELETE "/messages/:id" ResponsesController "destroy" route
```

The receiver still comes before the selector-like word. A route declaration
reads as: method, path, controller, action, then `route`.

Inspect the route table:

```
rco routes examples/learn/23-mvc/controllers
```

The output should include every route in this order:

```
GET / ResponsesController#index
GET /ping ResponsesController#ping
GET /api/status ResponsesController#status_json
GET /old-dashboard ResponsesController#legacy
POST /messages ResponsesController#create
PUT /messages/:id ResponsesController#update
PATCH /messages/:id ResponsesController#patch
DELETE /messages/:id ResponsesController#destroy
```

The `index` action renders an escaped view through the request context:

```
[
  "Routes And Responses" title var
  "Response helpers keep each action result explicit." summary var
  $ctx
  "responses/index" swap view
] "index" Method
```

Text responses are values first, then response modifiers:

```
[
  "ping requested" println
  "pong" text
  201 status
  "x-ricochet" "learn" header
] "ping" Method
```

`json` wraps any Ricochet value that can be serialized:

```
[
  statusBody map
  $statusBody "ok" true put! drop
  $statusBody "service" "learn" put! drop
  $statusBody "routes" 8 put! drop
  $statusBody json
] "status_json" Method
```

Redirects are also explicit action results:

```
[
  "/api/status" redirect
] "legacy" Method
```

Declared arguments bind path and request data before the block runs. Because
Ricochet is stack-based, bind them from the top of the stack:

```
( id title body ) [
  body var
  title var
  id var

  updated map
  $updated "id" $id put! drop
  $updated "title" $title put! drop
  $updated "body" $body put! drop
  $updated json
] "update" Method
```

Run the project doctor after editing controllers or routes:

```
rco doctor examples/learn/23-mvc/controllers
```

## How to read the example

Read the MVC example in layers. First identify the route or command that enters the app. Then find the controller action. Then follow the data into the response, view, model, or database boundary. Each layer is still ordinary Ricochet: values first, words second, and explicit results at boundaries.

## Try it

Add a route for a health check:

```
GET "/health" ResponsesController "health" route
```

Then add an action:

```
[
  health map
  $health "ok" true put! drop
  $health json
] "health" Method
```

Run `rco routes` again and confirm `/health` appears before serving the app.

## Check your understanding

- Which route, controller, view, model, or form boundary is this chapter teaching?
- Where does request data become ordinary Ricochet data?
- Where can failure happen, and is it represented as a result, response, or diagnostic?
- What would you test before serving the app?

## Common mistakes

* Hiding route behavior in overly broad controller methods.
* Mixing request parsing with unrelated persistence work.
* Applying `status` or `header` before a response helper has created an action
  result.
* Forgetting that declared arguments arrive on the stack and should be bound in
  reverse order inside the block.

## Safety notes

The example only inspects routes and compiles the app. It does not start a
long-running server or create local generated state. When you do serve it, bind to
`127.0.0.1` for local development unless you have a reason to expose it.

## Production guidance

Production controllers should validate input, keep response shapes predictable,
and avoid doing persistence, rendering, and authorization work in one large
method. Let route names stay boring and obvious.

## Reference links

* `docs/reference/guides/web-and-data.html`
* `docs/reference/guides/host-capabilities.html`

## What you know now

You know how a Ricochet route reaches a controller action, how action helpers
turn ordinary values into web responses, and how declared arguments let request
data enter the stack without breaking the postfix shape.

## Next step

Continue to [Chapter 25: Templates, Static Assets, And Uploads](25-templates-static-assets-and-uploads.md).
