# How Postfix Reads

Postfix means the operation comes after the values it uses.

| Style | Example | How to read it |
| --- | --- | --- |
| Infix | `1 + 2` | Add 1 and 2. |
| Function call | `add(1, 2)` | Call `add` with 1 and 2. |
| Ricochet postfix | `1 2 +` | Put 1 down, put 2 down, then add them. |

Ricochet source is a sequence of tokens separated by whitespace. Some tokens are values, such as numbers and strings. Some tokens are words, such as `+`, `println`, `map`, or `json_decode`. A word consumes the values it needs from the top of the stack and leaves its result.

## A tiny trace

Read this from left to right:

```rco
4 3 * 2 + println
```

| Step | Token | Stack after the token |
| --- | --- | --- |
| 1 | `4` | `[4]` |
| 2 | `3` | `[4, 3]` |
| 3 | `*` | `[12]` |
| 4 | `2` | `[12, 2]` |
| 5 | `+` | `[14]` |
| 6 | `println` | `[]` |

The important habit is not memorizing tables. The habit is asking, “What values does this word need, and what does it leave?”

## Why the order can feel good

Postfix makes pipelines compact. The next word often uses the result of the previous word without you wrapping expressions in parentheses:

```rco
"  ada@example.com  " trim lowercase println
```

That reads as: start with a string, trim it, lowercase it, print it.

## A beginner rule

Keep the stack shallow. When you would have to remember a value for more than a line or two, give it a name:

```rco
4 price var
3 quantity var
$price $quantity * subtotal var
$subtotal 2 + total var
```

Names are not a failure of stack thinking. Names are how you keep stack thinking readable.
