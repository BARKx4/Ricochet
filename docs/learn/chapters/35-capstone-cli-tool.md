# Chapter 35: Capstone CLI Tool

> Part VI: Capstone Applications

## Why this chapter matters

The CLI capstone combines data, results, files, tests, and reporting into a complete command-line tool.

## What you will build

You will build a complete command-line worklog reporter. It reads a sample
JSON worklog, summarizes minutes and status counts, reports upcoming due items,
prints each matching entry, and includes `rco test` coverage for the reusable
report functions.

## Concepts in plain English

A capstone is a complete project that proves several earlier skills work together.

The chapter uses these concepts:

* Strings, collections, results, local files, config, tests, linting, and packaging in one application.
* Small feature additions with matching tests.
* Cleanup-free self-check commands.
* Keeping reusable logic in a small local module while the command stays easy
  to run.
* Treating a CLI’s input file and optional arguments as explicit boundaries.

## Vocabulary and commands

This chapter consolidates words taught earlier rather than introducing a new primary word family.

## Guided example

Open `examples/learn/35-capstone-cli/worklog`. The project has four important
pieces:

```
data/entries.json
lib/report.rco
main.rco
WorklogTest.rco
```

The data file is ordinary JSON:

```
{
  "date": "2026-06-21",
  "due": "2026-06-24",
  "project": "Learn Ricochet",
  "task": "Write CLI capstone",
  "minutes": 80,
  "status": "open",
  "tags": ["docs", "cli"]
}
```

`lib/report.rco` holds the reusable work:

```
( path -> Result ) worklog_load_entries function
  path var
  [ json_decode ] $path fs_read_text and_then
end

( entries status -> Array ) worklog_filter_status function
  status var
  entries var

  $status "all" = if
    $entries
  else
    [ "status" at $status = ] $entries select
  end
end
```

The command imports the local module:

```
"lib/report" import

"examples/learn/35-capstone-cli/worklog/data/entries.json" inputPath var
"all" filterStatus var
```

It accepts two optional arguments: input path and status filter.

```
args count 0 > if
  args 0 at inputPath set
end

args count 1 > if
  args 1 at filterStatus set
end
```

Then it keeps file and JSON failures inside a `Result` boundary:

```
$inputPath worklog_load_entries entriesResult var

$entriesResult ok? if
  $entriesResult value entries var
  $entries $filterStatus worklog_filter_status filtered var
  "2026-06-22" date_parse value today var
  $filtered $today worklog_summary summary var
else
  "load failed:" print
  $entriesResult error "kind" at print
  ": " print
  $entriesResult error "message" at println
end
```

Run the capstone:

```
rco run examples/learn/35-capstone-cli/worklog/main.rco
```

Expected output:

```
Worklog CLI
source:examples/learn/35-capstone-cli/worklog/data/entries.json
filter:all
entries:4
minutes:250
done:2
open:2
due soon:2
items:
- 2026-06-17 [done] Learn Ricochet: Draft package import lab (70m)
- 2026-06-18 [done] Learn Ricochet: Validate debugger chapter (55m)
- 2026-06-21 [open] Learn Ricochet: Write CLI capstone (80m)
- 2026-06-22 [open] Release Prep: Review package metadata (45m)
[]
```

Filter to open items:

```
rco run examples/learn/35-capstone-cli/worklog/main.rco examples/learn/35-capstone-cli/worklog/data/entries.json open
```

Run the tests:

```
rco test examples/learn/35-capstone-cli/worklog
```

Expected output:

```
PASS WorklogTest.testDueSoonSummary
PASS WorklogTest.testSummarizesEntries
2 tests, 0 failed
```

`WorklogTest.rco` imports the same report module and checks totals, status
counts, filtering, due-soon logic, and summary map fields.

## How to read the example

Read the capstone from the outside in. Start with the user command or app surface, identify the data boundary, follow the core transformation, and then check tests and packaging. The point is integration, not new syntax.

## Try it

Add a `"blocked"` entry to `data/entries.json`, then add a new assertion to
`WorklogTest.rco`:

```
$entries "blocked" worklog_count_status
1 assert_equals
```

Add a new report field to `worklog_summary`:

```
$summary "blocked" $entries "blocked" worklog_count_status put! drop
```

Then print it from `main.rco`:

```
"blocked:" print
$summary "blocked" at println
```

Run the command, tests, and lint:

```
rco run examples/learn/35-capstone-cli/worklog/main.rco
rco test examples/learn/35-capstone-cli/worklog
rco lint examples/learn/35-capstone-cli/worklog
```

## Check your understanding

- What earlier chapters does this capstone combine?
- What is the user-facing entry point?
- What data boundary does the app use?
- Which tests or checks prove the capstone still works?

## Common mistakes

* Expanding scope before the core CLI workflow is stable.
* Skipping tests once the app becomes useful.
* Using `var` repeatedly inside a loop when you meant to update one loop
  binding. Declare once with `nil entry var`, then use `entry set` inside the
  loop.
* Reading CLI args without checking `args count`.
* Unwrapping `fs_read_text` or `json_decode` before deciding what failure
  should mean to the user.
* Hiding file writes in a capstone command. This example is read-only by
  default so it leaves no cleanup burden.

## Safety notes

The capstone reads a sample JSON file and prints a report. It does not
write, move, or delete files. If you extend it into an editor or importer,
prefer `workspace_resolve`, containment checks, explicit overwrite behavior,
and a confirmation step before destructive operations.

## Production guidance

Production CLI tools should keep parsing, validation, reporting, and host
effects separated. That makes tests small and keeps command behavior
inspectable. Prefer status-specific functions over one large command body, and
add a test when a new status, field, or date policy becomes user-visible.

For distributable CLIs, package the final command only after the run, test,
lint, and format loop is clean.

## Reference links

* `docs/learn/chapters/07-strings-json-and-regex.md`
* `docs/learn/chapters/08-collections.md`
* `docs/learn/chapters/09-results-and-errors.md`
* `docs/learn/chapters/12-testing-linting-and-formatting.md`
* `docs/learn/chapters/17-files-workspaces-env-and-secrets.md`
* `docs/reference/guides/language-runtime.html`
* `docs/reference/guides/host-capabilities.html`

## What you know now

You know how to assemble the language core into a complete CLI: read a bounded
input, decode structured data, keep failures explicit, summarize collections,
accept optional arguments, print a useful report, and protect the behavior with
tests.

## Next step

Continue to [Chapter 36: Capstone TUI Dashboard](36-capstone-tui-dashboard.md).
