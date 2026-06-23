# Chapter 14: Date, Time, And Duration

> Part III: Host Capabilities and Local App Surfaces

## Why this chapter matters

Dates and durations are common in reminders, logs, packages, sessions, and release workflows. This chapter teaches time as data before you use it inside larger applications.

## What you will build

You will build a reminder calculation that parses a timestamp, adds a
duration, formats the result, and compares calendar dates.

## Concepts in plain English

A timestamp names a moment. A duration names an amount of time. Date formatting turns time data into text for people or protocols.

The chapter uses these concepts:

* UTC timestamps and RFC3339 strings.
* Date parts, duration units, and date arithmetic.
* Duration values expressed in milliseconds.
* Result-based handling for invalid dates.

## Vocabulary and commands

Primary coverage: `now`, `timestamp_now`, `timestamp_parse`,
`timestamp_format`, `timestamp_format_pattern`, `timestamp_parts`,
`timestamp_from_parts`, `timestamp_add`, `timestamp_diff`,
`date_from_timestamp`, `date_to_timestamp`, `date_parse`, `date_format`,
`date_add_days`, `date_diff_days`, `duration_millis`, `duration_seconds`,
`duration_minutes`, `duration_hours`, `duration_days`, `duration_weeks`, and
`duration_parts`.

## Guided example

Open `examples/learn/14-date-time-and-duration/reminder.rco` and run:

```
rco run examples/learn/14-date-time-and-duration/reminder.rco
```

Timestamps parse from RFC3339 strings and normalize to Ricochet’s UTC
millisecond boundary:

```
"2026-06-18T13:14:15.250Z" timestamp_parse value startedAt var
$startedAt timestamp_format value println
$startedAt timestamp_parts value "year" at println
```

Duration unit words return millisecond durations as results:

```
$startedAt 2 duration_hours value timestamp_add value later var
$startedAt $later timestamp_diff value println
```

Dates are calendar values. Use date words for day arithmetic instead of adding
raw milliseconds:

```
"2026-02-28" date_parse value date var
$date 1 date_add_days value nextDate var
$nextDate "%Y-%m-%d" date_format value println
$date $nextDate date_diff_days value println
```

## How to read the example

Trace one representative line from left to right. Identify the values placed on the stack, the word that consumes them, and the result or binding that carries the work forward.

## Try it

Change the example to use your own timestamp. Then try a custom format:

```
$startedAt "%Y-%m-%d %H:%M:%S" timestamp_format_pattern value println
```

Try an invalid date and inspect the error:

```
"2026-02-30" date_parse error "kind" at println
```

## Check your understanding

- What new value shape, command, or host boundary did this chapter introduce?
- Which line is most important to trace with a stack diagram?
- Where would you add a binding to make the example easier to read?

## Common mistakes

* Assuming local time when a value is UTC.
* Adding days as raw milliseconds when calendar date arithmetic is clearer.
* Ignoring invalid-date results.
* Forgetting to unwrap `Result` values before indexing maps.

## What you know now

You know how Ricochet represents timestamps, dates, and durations, and you can
keep invalid time input in the same explicit `Result` flow used by conversions
and host APIs.

## Next step

Continue to [Chapter 15: Async And Tasks](15-async-and-tasks.md).
