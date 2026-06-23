# Chapter 11: OOP And Dispatch

## What You Will Build

You will build a contact-book model using Ricochet's dynamic OOP vocabulary.

## Concepts

- Class bodies and postfix declarations.
- Fields, accessors, methods, inheritance, and overrides.
- Generated selectors such as `email.get` and `email.set`.

## Words Introduced

Primary coverage: `Subclass`, `Field`, `Accessor`, `Table`, `new`, `self`,
`Method`, `send`, and generated `field.get` / `field.set` selectors.

## Guided Example

Open `examples/learn/11-oop/main.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/11-oop/main.rco
```

The class body uses capitalized declaration words:

```ricochet
Contact Object Subclass
  "name" Accessor
  "email" Accessor

  [
    self name.get
    " <" concat
    self email.get concat
    ">" concat
  ] "card" Method
end
```

Generated accessors stay postfix. Put the value below the receiver for a
setter:

```ricochet
"Ada Lovelace" $contact name.set contact set
"ada@example.com" $contact email.set contact set
```

Read with a selector after the receiver:

```ricochet
$contact card println
$contact name.get println
```

When a method name is data, use `send`:

```ricochet
$contact "card" send println
```

## Try It

Add a `label` method that returns only the name:

```ricochet
[
  self name.get
] "label" Method
```

Then call it:

```ricochet
$contact label println
```

## Common Mistakes

- Writing receiver-first pseudo-object calls instead of postfix selectors.
- Reversing setter order. Use `"value" receiver field.set`.
- Using class declarations when a map would be simpler.
- Forgetting that MVC model declarations reuse this vocabulary but add
  database table mapping.

## What You Know Now

You understand the OOP vocabulary reused by MVC model declarations: classes
are declared in postfix style, methods receive `self`, and generated accessors
are ordinary selectors.
