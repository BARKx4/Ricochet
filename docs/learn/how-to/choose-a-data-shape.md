# How to Choose a Data Shape

Ricochet gives you several ways to represent information. Choose the simplest shape that explains the problem.

| Need | Shape |
| --- | --- |
| A temporary value with a role | Binding |
| Ordered items | Array or list |
| Named fields | Map |
| Unique membership | Set |
| Behavior attached to data | Class and methods |
| Success/failure boundary | Result |

## Bindings first

If a value is important enough to explain, name it:

```rco
"Ada" name var
$name println
```

## Maps for flexible records

```rco
profile map
$profile "name" "Ada" put! drop
$profile "name" at println
```

## Classes for behavior

Use a class when the data has methods, invariants, or belongs to an MVC model. Use a map when you just need a flexible payload.
