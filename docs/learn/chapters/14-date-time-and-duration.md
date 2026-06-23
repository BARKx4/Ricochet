# Chapter 14: Date, Time, And Duration

## What You Will Build

You will build a reminder calculation that parses a timestamp, adds a
duration, formats the result, and compares calendar dates.

## Concepts

- UTC timestamps and RFC3339 strings.
- Date parts, duration units, and date arithmetic.
- Duration values expressed in milliseconds.
- Result-based handling for invalid dates.

## Words Introduced

Primary coverage: `now`, `timestamp_now`, `timestamp_parse`,
`timestamp_format`, `timestamp_format_pattern`, `timestamp_parts`,
`timestamp_from_parts`, `timestamp_add`, `timestamp_diff`,
`date_from_timestamp`, `date_to_timestamp`, `date_parse`, `date_format`,
`date_add_days`, `date_diff_days`, `duration_millis`, `duration_seconds`,
`duration_minutes`, `duration_hours`, `duration_days`, `duration_weeks`, and
`duration_parts`.

## Guided Example

Open `examples/learn/14-date-time-and-duration/reminder.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/14-date-time-and-duration/reminder.rco
```

Timestamps parse from RFC3339 strings and normalize to Ricochet's UTC
millisecond boundary:

```ricochet
"2026-06-18T13:14:15.250Z" timestamp_parse value startedAt var
$startedAt timestamp_format value println
$startedAt timestamp_parts value "year" at println
```

Duration unit words return millisecond durations as results:

```ricochet
$startedAt 2 duration_hours value timestamp_add value later var
$startedAt $later timestamp_diff value println
```

Dates are calendar values. Use date words for day arithmetic instead of adding
raw milliseconds:

```ricochet
"2026-02-28" date_parse value date var
$date 1 date_add_days value nextDate var
$nextDate "%Y-%m-%d" date_format value println
$date $nextDate date_diff_days value println
```

## Try It

Change the example to use your own timestamp. Then try a custom format:

```ricochet
$startedAt "%Y-%m-%d %H:%M:%S" timestamp_format_pattern value println
```

Try an invalid date and inspect the error:

```ricochet
"2026-02-30" date_parse error "kind" at println
```

## Common Mistakes

- Assuming local time when a value is UTC.
- Adding days as raw milliseconds when calendar date arithmetic is clearer.
- Ignoring invalid-date results.
- Forgetting to unwrap `Result` values before indexing maps.

## What You Know Now

You know how Ricochet represents timestamps, dates, and durations, and you can
keep invalid time input in the same explicit `Result` flow used by conversions
and host APIs.
