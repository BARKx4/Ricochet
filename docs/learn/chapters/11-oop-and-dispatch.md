# Chapter 11: OOP and Dispatch

## Why this chapter matters

Maps are excellent for flexible data, but sometimes data has behavior. A contact can format itself. A model can validate itself. A controller can respond to a route. Ricochet’s dynamic OOP vocabulary gives you those shapes while keeping postfix reading order.

The key beginner question is: should this be a map or a class? Use a map for a simple payload. Use a class when named behavior, generated accessors, inheritance, or framework integration makes the code clearer.

## What you will build

You will build a contact-book model using Ricochet’s dynamic OOP vocabulary.

## Concepts in plain English

A class describes a kind of object. A field stores data. An accessor is a generated word for reading or writing a field. A method is behavior attached to an object. Dispatch means Ricochet chooses the method implementation for the receiver you send the message to.

## Words introduced

Primary words: `Subclass`, `Field`, `Accessor`, `Table`, `new`, `self`, `Method`, `send`, and generated `field.get` / `field.set` selectors.

## Guided example

Run the OOP example:

```bash
rco run examples/learn/11-oop/main.rco
```

The class body uses capitalized declaration words:

```rco
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

Generated accessors stay postfix. Put the value below the receiver for a setter:

```rco
"Ada Lovelace" $contact name.set contact set
"ada@example.com" $contact email.set contact set
```

Read with a selector after the receiver:

```rco
$contact card println
$contact name.get println
```

When a method name is data, use `send`:

```rco
$contact "card" send println
```

## How to read the code

Inside a method, `self` is the current receiver. `self name.get` reads the current object’s `name` field. The string pieces are then concatenated into a display card.

A setter returns an updated object. That is why examples often assign the result back to the binding:

```rco
"Ada Lovelace" $contact name.set contact set
```

Read it as: value, receiver, setter, then update the `contact` binding with the returned object.

## Try it

Add a `label` method that returns only the name:

```rco
[
  self name.get
] "label" Method
```

Then call it:

```rco
$contact label println
```

## Check your understanding

- Selector reads put the receiver first: `$contact name.get`.
- Selector writes put the value before the receiver: `"Ada" $contact name.set`.
- Use `send` when the method name is data.
- Prefer a map if you only need a flexible record with no behavior.

## Common mistakes

- Treating selectors as dot syntax.
- Forgetting that OOP declarations are still postfix-shaped.
- Reaching for a class before a map has stopped being clear.
- Forgetting to assign the result of a setter when the surrounding code expects the binding to change.

## What you know now

You can read a Ricochet class, create objects, use generated accessors, and decide when behavior belongs with data.
