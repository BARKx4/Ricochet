# Chapter 07: Strings, JSON, and Regex

## Why this chapter matters

Most useful programs cross text boundaries: command output, config files, logs, HTTP payloads, form fields, templates, and package metadata. This chapter teaches a practical order: use direct string words first, JSON words at structured boundaries, and regex only when pattern matching is truly the right tool.

## What you will build

You will build a small log-cleaning workflow that cleans text, moves through JSON, and uses a regex for the one part direct string words do not express.

## Concepts in plain English

A string is text. JSON is a text format for structured data. A regex is a compact pattern language for matching text. Regex is powerful, but direct words such as `trim`, `split`, `contains?`, `starts_with?`, and `replace` are usually easier to read.

## Words introduced

Primary words: `trim`, `trim_start`, `trim_end`, `blank?`, `slice`, `index_of`, `last_index_of`, `repeat`, `lines`, `chars`, `split`, `replace`, `contains?`, `starts_with?`, `ends_with?`, `uppercase`, `lowercase`, `concat`, `to_string`, `json_encode`, `json_decode`, `regex`, `matches?`, `regex_find`, `regex_replace`, and `captures`.

## Guided example

Run the log cleaner:

```bash
rco run examples/learn/07-strings-json-and-regex/log-cleaner.rco
```

The first half uses direct string words:

```rco
"  warn: user=ada  " trim cleaned var
$cleaned uppercase println
$cleaned "user=" contains? println
$cleaned ":" split count println
```

The JSON section turns a Ricochet map into a JSON string and back:

```rco
payload map
$payload "ok" true put! drop
$payload "name" "Ada" put! drop
$payload json_encode json var
$json json_decode value "name" at println
```

Regex work follows the same result habit:

```rco
"\\d+" regex value digits var
"ticket-42" $digits regex_find "text" at println
$digits "ticket-42" "#" regex_replace println
```

## How to read the code

`"  warn: user=ada  " trim cleaned var` starts with a raw string, trims whitespace, and names the cleaned result. The later lines read `$cleaned` instead of repeating the cleanup pipeline.

`json_decode` returns a `Result`. The example unwraps with `value` because the JSON was produced by `json_encode` immediately before. When JSON comes from a file, request, or user input, check `ok?` before unwrapping.

## Try it

Try these direct string words before reaching for regex:

```rco
"Ricochet" 0 4 slice println
"Ada\nGrace" lines count println
"abc" chars "," join println
"warning" "warn" starts_with? println
```

Then intentionally decode bad JSON and inspect the error:

```rco
"not json" json_decode error "kind" at println
```

## Check your understanding

- Use `trim` to clean edges before comparing text.
- Use `split` and `lines` for simple structure.
- Use `json_decode` at data boundaries and handle its `Result`.
- Use regex when direct string words cannot express the pattern clearly.

## Common mistakes

- Treating JSON parse failure as an exception instead of a result path.
- Using regex where direct string words are simpler.
- Forgetting to escape backslashes in regex strings.
- Assuming `concat` is the only way to join text. Collections can use `join`.

## What you know now

You can process text, encode and decode structured payloads, and keep regex work explicit and result-checked.
