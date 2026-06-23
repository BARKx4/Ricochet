# Chapter 22: Webview And Desktop GUI

> Part III: Host Capabilities and Local App Surfaces

## Why this chapter matters

Desktop GUI work introduces a browser-like surface while keeping your application local. The key model is data flowing between Ricochet and a webview document.

## What you will build

You will build a small desktop notes GUI document. The example can be checked
with `rco run` without opening a window, and the same source can be previewed
with `rco gui`.

## Concepts in plain English

A webview is a desktop window that renders HTML/JS/CSS while Ricochet owns the local application logic.

The chapter uses these concepts:

* Webview document fragments.
* State/action document generation.
* Choosing GUI, TUI, or MVC for a local application.
* Escaping user-facing text before it becomes HTML.
* GUI preview and GUI packaging commands.

## Vocabulary and commands

Primary coverage: `webview_text`, `webview_heading`, `webview_button`,
`webview_action`, `webview_input`, `webview_link`, `webview_container`,
`webview_window`, `webview_window_state`, the `webview_document` alias,
`rco gui`, and `rco package --gui`.

## Guided example

Open `examples/learn/22-gui/notes_gui.rco` and run the non-interactive self-check:

```
rco run examples/learn/22-gui/notes_gui.rco
```

The example prints document facts and leaves the generated GUI document in a
variable named `document`, which is what `rco gui` expects.

Start with an action callback. GUI callbacks receive `(state event -> document)`:

```
( state event -> Map ) save_note function
  event var
  state var
  $state "saved" true put! drop

  actions array
  $actions "Save <Now>" "save-note" "save_note" webview_action push! drop

  "Saved: " $state "note" at concat webview_text saved var
  "Ricochet Notes" $saved $state $actions webview_window_state value
end
```

Build fragments with webview helpers instead of hand-writing unescaped HTML:

```
"Notes" 1 webview_heading heading var
"Preview: " $state "note" at concat webview_text preview var
"note" $state "note" at webview_input noteInput var
"Save <Now>" "save-note" webview_button saveButton var
"Learn Ricochet" "https://try.ricochet.today" webview_link docsLink var
```

Compose the fragments into a container, then build a document:

```
$heading $preview concat
$noteInput concat
$saveButton concat
$docsLink concat
webview_container body var

actions array
$actions "Save <Now>" "save-note" "save_note" webview_action push! drop

"Ricochet Notes" $body $state $actions webview_window_state value document var
```

`webview_window` is the simpler document builder when you do not need explicit
state or actions:

```
"Plain Notes" $body webview_window value plainDocument var
```

## How to read the example

Trace one representative line from left to right. Identify the values placed on the stack, the word that consumes them, and the result or binding that carries the work forward.

## Try it

Preview the same source as a GUI document:

```
rco gui examples/learn/22-gui/notes_gui.rco
```

To export HTML instead of opening a window, set `RICOCHET_GUI_EXPORT_HTML` to a
path before running `rco gui`. To package a GUI app, use:

```
rco package examples/learn/22-gui/notes_gui.rco --gui --output notes-gui.exe
```

## Check your understanding

- What new value shape, command, or host boundary did this chapter introduce?
- Which line is most important to trace with a stack diagram?
- Where would you add a binding to make the example easier to read?

## Common mistakes

* Mixing webview GUI concepts with MVC server routing too early.
* Generating unescaped document fragments.
* Forgetting that `rco gui` needs a document on the stack or in a variable
  named `document`.
* Building actions without matching callback word names.

## Safety notes

Webview helpers escape text, labels, input values, and links for you. Prefer
them for user-facing fragments. Keep GUI state local unless the app explicitly
needs filesystem, HTTP, process, or other host capabilities.

## Production guidance

Production GUIs should keep document generation, state updates, and packaging
metadata clear. Use `webview_window_state` when callbacks need state, and keep
callbacks small enough that they can return a fresh document after each action.
Use MVC when you need routes, server-side templates, uploads, sessions, or a
browser-accessible local app.

## Reference links

* `docs/reference/guides/host-capabilities.html`
* `docs/reference/guides/language-runtime.html`

## What you know now

You know the local desktop GUI path before moving into MVC: build escaped
fragments, compose a webview document, keep state/actions explicit, preview
with `rco gui`, and package with `rco package --gui`.

## Next step

Continue to [Chapter 23: MVC First App](23-mvc-first-app.md).
