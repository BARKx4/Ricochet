# Concept: Bindings vs. Stack Juggling

Stack motion is useful, but readable Ricochet does not try to keep everything anonymous forever. A binding gives a value a name.

```rco
4 price var
3 quantity var
$price $quantity * subtotal var
```

Use a binding when the value has a role in the problem: `price`, `quantity`, `subtotal`, `profile`, `settings`, `request`, `response`.

## A practical rule

If you need to use a value more than once, or several lines later, name it. If you only need to transform it immediately, leave it on the stack.

```rco
"  Ada  " trim uppercase println       (( immediate pipeline ))
"Ada Lovelace" name var               (( named domain value ))
```

## Stack motion is not bad

`dup`, `drop`, `swap`, and `over` are good tools. The mistake is using them to hide a missing name. Clear Ricochet code alternates between fluent postfix pipelines and named values at important boundaries.
