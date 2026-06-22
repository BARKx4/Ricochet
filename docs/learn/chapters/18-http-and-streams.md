# Chapter 18: HTTP And Streams

## What You Will Build

This chapter will build a small API client.

## Concepts

- Simple GET and POST requests.
- Structured request maps, headers, JSON, timeouts, and bearer helpers.
- Retained response streams, bounded reads, cancellation, and release.

## Words Introduced

Primary coverage: HTTP and HTTP stream system words.

## Guided Example

Planned example: `examples/learn/18-http-streams/api-client.rco`.

## Try It

Readers will perform a bounded request and handle success or failure through results.

## Common Mistakes

- Forgetting capability flags for outbound HTTP.
- Leaving retained streams unreleased.

## Safety Notes

The chapter will avoid surprising external side effects and will make network permissions explicit.

## Production Notes

Production clients should set timeouts, bound stream reads, and handle redirects according to Ricochet's HTTP behavior.

## Reference Links

Links will point to HTTP and stream references when drafted.

## What You Know Now

Readers will know how to make outbound HTTP requests and clean up retained streams.
