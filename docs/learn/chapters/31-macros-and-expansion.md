# Chapter 31: Macros And Expansion

## What You Will Build

This chapter will build a route macro lab.

## Concepts

- String-named macro declarations and explicit macro calls.
- `quote_ast`, `ast_splice`, and `quote_items`.
- Expansion, source maps, cache metadata, and package macro lookup.

## Words Introduced

Primary coverage: macro language core and `rco expand`.

## Guided Example

Planned example: `examples/learn/31-macros/route_macro`.

## Try It

Readers will expand a small macro and inspect generated source shape.

## Common Mistakes

- Using macros before normal function or package factoring would be clearer.
- Expecting expansion output to be a byte-for-byte source rewrite.

## Safety Notes

Examples will keep macro output readable and inspectable.

## Production Notes

Production macros should be documented, deterministic, and conservative.

## Reference Links

Links will point to macro and expansion references when drafted.

## What You Know Now

Readers will understand when compile-time generation is appropriate.
