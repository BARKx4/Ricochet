# Chapter 31: Macros And Expansion

## What You Will Build

You will build a route macro lab. The example defines one expression macro
that splices route data into generated print code and one item-row macro that
generates a route-printer function. Then you will inspect the expanded source
with `rco expand`.

## Concepts

- String-named macro declarations and explicit macro calls.
- `quote_ast`, `ast_splice`, and `quote_items`.
- Expansion, source maps, cache metadata, and package macro lookup.
- Compile-time restrictions: macros transform parsed AST and cannot perform
  host effects.

## Words Introduced

Primary coverage: `"name" Macro`, `macro_call`, `quote_ast`, `ast_splice`,
`quote_items`, `rco expand`, and `rco expand --json`.

## Guided Example

Open `examples/learn/31-macros/route_macro/main.rco`.

The first macro is an expression macro. It receives three parsed operands:
`method`, `path`, and `action`. `quote_ast` turns the quoted block into AST for
the caller, and `$method ast_splice` inserts the caller's original parsed
operand:

```ricochet
"emit_route" Macro
  ( method path action -> expansion )
  [
    [
      "route " print
      $method ast_splice print
      " " print
      $path ast_splice print
      " -> " print
      $action ast_splice println
    ] quote_ast
  ]
end
```

The second macro emits a whole declaration item with `quote_items`. This is how
a macro can generate a function, class row, or another top-level item:

```ricochet
"install_route_printers" Macro
  [
    [
      [
        "Generated route table" println
        "GET / -> HomeController#index" println
        "POST /messages -> MessagesController#create" println
      ] "print_routes" function
    ] quote_items
  ]
end
```

Macro calls are explicit:

```ricochet
"install_route_printers" macro_call

print_routes drop
"PATCH" "/messages/:id" "MessagesController#update" "emit_route" macro_call
```

Run the example:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/31-macros/route_macro/main.rco
```

Expected output:

```text
Generated route table
GET / -> HomeController#index
POST /messages -> MessagesController#create
route PATCH /messages/:id -> MessagesController#update
[]
```

Now inspect the expanded source without running the program:

```powershell
cargo run -q -p ricochet_cli --bin rco -- expand examples/learn/31-macros/route_macro/main.rco
```

The plain expansion is readable Ricochet source:

```ricochet
print_routes function
  "Generated route table" println
  "GET / -> HomeController#index" println
  "POST /messages -> MessagesController#create" println
end

print_routes drop

"route " print "PATCH" print " " print "/messages/:id" print " -> " print "MessagesController#update" println
```

Use JSON expansion when you need tooling detail:

```powershell
cargo run -q -p ricochet_cli --bin rco -- expand examples/learn/31-macros/route_macro/main.rco --json
```

The JSON form includes `schema`, `sources`, `source_map`, `cache`,
`expanded_ast`, `macro_tables`, `trace`, `diagnostics`, and `output_hash`.
Those fields are for tools, reviews, and cache-aware build flows.

## Try It

Change the `PATCH` route call to use `DELETE`, then run `rco expand` again.
Only the generated expression line should change.

Add another function body to the `quote_items` macro and rerun the program. If
you try to place a `quote_items` macro in the middle of a larger expression,
Ricochet should fail with a diagnostic explaining that `quote_items` must own a
whole item. Use `quote_ast` for expression-position output.

To experiment with package-authored macros, inspect
`examples/showcase/package_macro_queue_report`. Its lockfile may need
`rco install` before `rco verify`; the important shape is the package import:

```ricochet
"queue_macros/macros" import
"install_queue_report" macro_call
```

## Common Mistakes

- Using macros before normal function or package factoring would be clearer.
- Expecting expansion output to be a byte-for-byte source rewrite.
- Calling a macro implicitly. Macro invocation is always explicit:
  `"name" macro_call`.
- Using `quote_items` where only expression output is valid.
- Forgetting `ast_splice`, which generates a runtime variable read instead of
  inserting the caller's operand.
- Expecting runtime `import_dynamic` to load macros. Macro lookup comes from
  static imports during compilation.

## Safety Notes

Macros run in a restricted compile-time evaluator. They should not perform host
effects and cannot use filesystem, network, process, database, clock,
randomness, task, or capability APIs. Keep generated output small enough to
review with `rco expand`.

## Production Notes

Production macros should be documented, deterministic, conservative, and backed
by expansion snapshots or tests. Prefer ordinary functions when all you need is
runtime reuse. Reach for macros when you need to generate repeated declarations
or source shapes that stay clearer after expansion than they would as
handwritten boilerplate.

For package macros, review `rco expand --json` trace and source-map metadata so
you can tell which package supplied each macro. Qualified calls such as
`"package/module#macro_name" macro_call` are useful when multiple imports expose
the same public macro name.

## Reference Links

- `docs/wiki/macros.md`
- `docs/feature-map.md`
- `examples/showcase/package_macro_queue_report`

## What You Know Now

You understand when compile-time generation is appropriate, how expression and
item-row macros differ, how to splice caller operands, how to inspect expanded
source, and why macro output should remain boring enough to review.
