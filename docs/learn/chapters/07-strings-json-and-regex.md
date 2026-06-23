# Chapter 07: Strings, JSON, And Regex

## What You Will Build

You will build a small log-cleaning workflow that cleans text, moves through
JSON, and uses a regex for the one part direct string words do not express.

## Concepts

- Practical string cleanup and search.
- JSON encode/decode patterns.
- Regex matching, captures, and replacement.
- Choosing direct string words before regex.

## Words Introduced

Primary coverage: `trim`, `trim_start`, `trim_end`, `blank?`, `slice`,
`index_of`, `last_index_of`, `repeat`, `lines`, `chars`, `split`, `replace`,
`contains?`, `starts_with?`, `ends_with?`, `uppercase`, `lowercase`, `concat`,
`to_string`, `json_encode`, `json_decode`, `regex`, `matches?`, `regex_find`,
`regex_replace`, and `captures`.

## Guided Example

Open `examples/learn/07-strings-json-and-regex/log-cleaner.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/07-strings-json-and-regex/log-cleaner.rco
```

The first half uses direct string words:

```ricochet
"  warn: user=ada  " trim cleaned var
$cleaned uppercase println
$cleaned "user=" contains? println
$cleaned ":" split count println
```

The JSON section turns a Ricochet map into a JSON string and back:

```ricochet
payload map
$payload "ok" true put! drop
$payload "name" "Ada" put! drop
$payload json_encode json var
$json json_decode value "name" at println
```

`json_decode` returns a `Result`, so this example unwraps with `value` only
because the JSON was produced by `json_encode` in the line above.

Regex work follows the same result habit:

```ricochet
"\\d+" regex value digits var
"ticket-42" $digits regex_find "text" at println
$digits "ticket-42" "#" regex_replace println
```

## Try It

Try these direct string words before reaching for regex:

```ricochet
"Ricochet" 0 4 slice println
"Ada\nGrace" lines count println
"abc" chars "," join println
"warning" "warn" starts_with? println
```

Then intentionally decode bad JSON and inspect the error:

```ricochet
"not json" json_decode error "kind" at println
```

## Common Mistakes

- Treating JSON parse failure as an exception instead of a result path.
- Using regex where direct string words are simpler.
- Forgetting to escape backslashes in regex strings.
- Assuming `concat` is the only way to join text. Collections can use `join`.

## What You Know Now

You can process text, encode and decode structured payloads, and keep regex
work explicit and result-checked.
