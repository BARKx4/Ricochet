# Appendix D: Syntax Guardrails

## Purpose

This appendix keeps accepted Ricochet syntax shapes easy to check while reading
or editing the manual. Ricochet reads postfix-first. Keep receivers and
containers before the operation that consumes them.

## Core Shapes

| Intent | Use | Avoid |
| --- | --- | --- |
| Read a static binding | `$name` | `name get` |
| Dynamic read by name | `"name" get` | `$"name"` |
| Selector read | `$user email.get` | `$user .email` |
| Selector write | `"ada@example.com" $user email.set` | `$user email.set "ada@example.com"` |
| Map or array access | `$settings "theme" at` | `"theme" $settings at` |
| Map mutation | `$settings "theme" "dark" put!` | `"theme" "dark" $settings put!` |
| Collection mutation | `$items "new" push!` | `"new" $items push!` |
| Function call | `20 22 +` | `+(20, 22)` |
| Block call | `[ 2 * ] call` | `call [ 2 * ]` |

## Names

Public multiword names use underscores:

```ricochet
json_encode
find_record
fs_read_text
http_request
webview_window
tui_write
```

Reserve `-` for subtraction and negative number literals:

```ricochet
10 3 -
-5
```

## OOP Declaration Shape

Use capitalized meta words in class bodies and lowercase `end`:

```ricochet
User Model Subclass
  "users" Table
  "email" Accessor

  [
    self email.get
  ] "displayName" Method
end
```

## Route Shape

Route verbs are words:

```ricochet
GET "/" HomeController "index" route
POST "/entries" JournalController "create" route
```

## Imports

Static imports use a string before `import`:

```ricochet
"lib/report" import
```

Dynamic runtime imports return a `Result`:

```ricochet
"math_tools/stats" import_dynamic value stats var
```

## Diagnostics To Trust

If lint or LSP suggests replacing `name get` with `$name`, do it for ordinary
static binding reads. Keep `"name" get` only when the name is dynamic data.

Status: drafted against the repo syntax guardrails.
