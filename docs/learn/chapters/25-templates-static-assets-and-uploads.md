# Chapter 25: Templates, Static Assets, And Uploads

> Part IV: MVC, Data, Auth, Forms, and AI

## Why this chapter matters

Templates and assets turn controller data into user-facing pages. Uploads add a host-resource boundary, so the chapter keeps size, path, and cleanup concerns visible.

## What you will build

You will add a rendered page, a static stylesheet, and a bounded multipart
upload action to an MVC app. The upload controller reads from Ricochet’s
retained temporary upload stream and releases it before returning JSON.

## Concepts in plain English

A template renders data into markup. Static assets are files served directly. Uploads are incoming files that must be bounded and placed safely.

The chapter uses these concepts:

* Template expressions, setup blocks, conditionals, and loops.
* HTML escaping and text-context template limits.
* Static asset directories and `/assets` mounting.
* Multipart form fields, uploaded file maps, retained upload streams, and
  explicit stream release.
* Upload size and retention limits in `ricochet.toml`.

## Vocabulary and commands

Primary coverage: template syntax, static asset configuration, upload request
data, and the upload stream words `upload_streams`, `upload_stream`,
`upload_read`, and `upload_release`.

## Guided example

Open `examples/learn/23-mvc/templates_uploads`. The manifest keeps static files
and upload limits visible:

```
[web.static]
dir = "public"
mount = "/assets"

[web.uploads]
max_request_bytes = 4096
max_file_bytes = 2048
memory_threshold_bytes = 8
max_retained_streams = 4
```

The route table has one page and one upload action:

```
GET "/" PagesController "index" route
POST "/uploads/:id" UploadsController "create" route
```

The page controller prepares ordinary view data, then renders a view:

```
PagesController Controller Subclass
  [
    items array
    $items "Template expressions escape HTML" push! drop
    $items "Static assets live under /assets" push! drop
    $items "Upload streams must be released" push! drop
    true showUploads var
    "Templates, Assets, Uploads" title var
    $ctx
    "pages/index" swap view
  ] "index" Method
end
```

The view is plain HTML with Ricochet template markers in text context:

```
<link rel="stylesheet" href="/assets/styles/app.css">

{% "Upload checklist" "heading" var do %}
<h1>{ title get }</h1>
<h2>{ heading get }</h2>

{% showUploads get if %}
<ul>
  {% items get "item" each %}
  <li>{ item get }</li>
  {% end %}
</ul>
{% else %}
<p>No upload checklist is available.</p>
{% end %}
```

`{ ... }` renders one scalar value and escapes it as HTML. `{% ... do %}` runs
setup code without rendering. `{% condition if %}`, `{% else %}`, and
`{% end %}` form a conditional. `{% collection "item" each %}` loops through
arrays, lists, sets, and maps.

The upload action receives declared arguments. Route params bind first, then
multipart fields, upload fields, query params, and context values:

```
UploadsController Controller Subclass
  ( id title file ctx ) [
    ctx var
    file var
    title var
    id var

    readOptions map
    $readOptions "max_bytes" 64 put! drop
    $file "stream_id" at $readOptions upload_read value chunk var
    $file "stream_id" at upload_stream value detail var

    summary map
    $summary "id" $id put! drop
    $summary "title" $title put! drop
    $summary "filename" $file "filename" at put! drop
    $summary "preview" $chunk "text" at put! drop
    $summary "stream_size" $detail "size" at put! drop
    $summary "retained_before_release" upload_streams count put! drop
    $file "stream_id" at upload_release value released var
    $summary "released" $released put! drop
    $summary "retained_after_release" upload_streams count put! drop
    $summary json
  ] "create" Method
end
```

The example sets `memory_threshold_bytes` low so small manual test files become
retained streams. Production settings should choose thresholds based on real
request sizes and memory limits.

Validate the project without starting a server:

```
rco doctor examples/learn/23-mvc/templates_uploads
```

The doctor compiles the manifest, route table, controllers, and views.

## How to read the example

Read the MVC example in layers. First identify the route or command that enters the app. Then find the controller action. Then follow the data into the response, view, model, or database boundary. Each layer is still ordinary Ricochet: values first, words second, and explicit results at boundaries.

## Try it

Serve the example locally:

```
rco serve examples/learn/23-mvc/templates_uploads --host 127.0.0.1 --port 3000
```

Open `http://127.0.0.1:3000/`, choose a small text file, and submit the form.
The JSON response should show the route id, title, filename, stream preview,
release result, and retained stream count after release.

## Check your understanding

- Which route, controller, view, model, or form boundary is this chapter teaching?
- Where does request data become ordinary Ricochet data?
- Where can failure happen, and is it represented as a result, response, or diagnostic?
- What would you test before serving the app?

## Common mistakes

* Trusting uploaded names or content without validation.
* Treating static assets as application state.
* Putting template expressions inside HTML attributes, `<script>`, or
  `<style>` blocks.
* Reading an upload stream and forgetting to call `upload_release`.
* Assuming `filename` is a safe filesystem path.

## Safety notes

The sample self-check path does not start a server or store uploaded files.
The manual upload path is local-only and bounded by `[web.uploads]`. Treat
uploaded filenames and content types as user input, not trusted storage paths or
security decisions.

## Production guidance

Production upload flows should validate size, media type, extension, content,
authorization, and path containment before persistence. Prefer generated storage
names over user filenames, release retained streams promptly, and avoid leaking
local temporary paths in responses or logs.

## Reference links

* `docs/reference/guides/web-and-data.html`
* `docs/reference/guides/host-capabilities.html`

## What you know now

You know how controller data reaches an escaped template, how public assets are
mounted, and how multipart uploads appear as maps and retained streams that
must be read within configured limits and explicitly released.

## Next step

Continue to [Chapter 26: Data, Active Record, And Migrations](26-data-active-record-and-migrations.md).
