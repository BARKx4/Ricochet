# Compile-Time Macros

Compile-time macros are a beta source-transformation surface. They expand
parsed Ricochet AST before bytecode is generated, so generated code keeps the
same postfix shape as handwritten code.

Use macros when a repeated declaration or expression pattern is clearer as a
small local language feature. Do not use them for host effects: macro bodies run
in a restricted compile-time evaluator with no filesystem, network, process,
database, clock, randomness, task, or capability access.

## Expression Macros

Macro names are string literals and calls are explicit:

```forth
"unless" Macro
  ( condition body -> expansion )
  [
    [
      $condition ast_splice not if
        $body ast_splice call
      end
    ] quote_ast
  ]
end

$ready [
  "not ready" println
] "unless" macro_call
```

`quote_ast` converts a quoted block into expression AST. Inside quoted output,
`$arg ast_splice` inserts the caller's parsed operand instead of generating a
runtime `$arg` read.

## Item Row Macros

Use `quote_items` when a macro call owns a whole item and should emit one or
more item rows. Rows that do not match a declaration encoding stay ordinary
expression items, so class-body declaration rows such as `Accessor`, `Field`,
`Table`, and `Method` keep the same shape as handwritten class bodies:

```forth
"accessors" Macro
  [
    [
      "email" Accessor
      "name" Accessor
    ] quote_items
  ]
end

User Model Subclass
  "accessors" macro_call
end
```

`quote_items` is intentionally rejected inside larger expressions. Use
`quote_ast` for expression-position expansion.

At top level, `quote_items` can also emit real declaration items with the
macro-only row encodings below:

```forth
"function_rows" Macro
  [
    [
      [ "hello from generated function" println ] "greet" function
      ( name -> String ) [ "hello " name + ] "greeting" function
    ] quote_items
  ]
end

"model_row" Macro
  [
    [
      [
        "users" Table
        "email" Accessor
        [ self email.get ] "label" Method
      ] User Model Subclass
    ] quote_items
  ]
end
```

Generated declaration bodies expand in the macro definition scope. If an
imported public macro emits a function body that calls a private helper macro
from the imported module, that private helper resolves from the imported module
rather than from the caller.

## Imports

Static imports expose public macros from the imported module during
compilation. Macro declarations whose names start with `_` are private to their
defining module but can still be used by public macros from that module:

```forth
(( lib/macros.rco ))
"_helper" Macro
  [
    [ "from helper" println ] quote_ast
  ]
end

"say_ok" Macro
  [
    [ "_helper" macro_call ] quote_ast
  ]
end
```

```forth
(( main.rco ))
"lib/macros" import
"say_ok" macro_call
```

Unqualified calls prefer local macros. If exactly one imported module exports a
matching macro, that macro is used. If multiple imports export the same name,
the compiler fails and asks for a qualified call:

```forth
"lib/macros#say_ok" macro_call
"self#local_macro" macro_call
```

Package path imports use the same manifest and containment rules as ordinary
static imports. Lockfile-canonical macro module IDs are still stabilization
work, so do not treat the current `rco expand --json` module IDs as a permanent
package identity format.

## Inspection

`rco expand` prints expanded source without executing runtime code:

```powershell
rco expand app.rco
rco expand app.rco --json
```

The JSON form includes the expanded AST, macro tables, imports, traces, and
diagnostics. It is useful for debugging today, but the full cache/source-map
schema is still beta and may change before macro stabilization.

## Current Limits

- Macro declaration names must be string literals and cannot contain `#`.
- Runtime imports do not load macros.
- Macro bodies are fail-closed and can use only the compile-time helper surface.
- Expansion depth, same-macro recursion, evaluator steps, and generated AST
  nodes are bounded.
- `quote_items` declaration rows are supported only where a whole declaration
  item is valid.
- Stable `rco expand --json` source maps/cache metadata, package lockfile
  canonical macro module IDs, and public package examples remain stabilization
  work.
