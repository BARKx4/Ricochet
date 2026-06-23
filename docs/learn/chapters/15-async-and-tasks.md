# Chapter 15: Async And Tasks

## What You Will Build

You will build a parallel-checks script with first-class task handles.

## Concepts

- Spawning blocks as tasks.
- Awaiting one task or many tasks.
- Inspecting task state.
- Releasing retained completed handles.

## Words Introduced

Primary coverage: `spawn`, `await`, `await_all`, `release_task`, `tasks`,
`task_status`, `id`, `info`, `pending?`, `running?`, `completed?`, and
`failed?`.

## Guided Example

Open `examples/learn/15-async/parallel-checks.rco` and run:

```powershell
cargo run -q -p ricochet_cli --bin rco -- run examples/learn/15-async/parallel-checks.rco
```

Spawn runs a block on a background worker and returns a task handle:

```ricochet
[ 40 2 + ] spawn answer var
$answer id println
$answer task_status println
```

Await returns the task result:

```ricochet
$answer await println
$answer completed? println
```

Completed and failed handles remain retained until you release them:

```ricochet
$answer release_task println
```

For several tasks, store the handles in a collection and await them together:

```ricochet
handles array
$handles [ 20 2 + ] spawn push! drop
$handles [ 30 4 + ] spawn push! drop
$handles await_all results var
$results 0 at println
$results 1 at println
```

## Try It

Add a task that returns a string:

```ricochet
$handles [ "ready" ] spawn push! drop
```

Then inspect a handle before and after awaiting:

```ricochet
$answer info inspect println drop
```

## Common Mistakes

- Forgetting to release retained task handles when examples retain them.
- Treating failure inside a task as invisible. A failed handle keeps failed
  status and rethrows when awaited.
- Assuming awaited handles disappear. They remain inspectable until released.
- Hiding too much work inside a task block before you can test it normally.

## What You Know Now

You know how Ricochet models first-class asynchronous work: task handles are
values, waiting is explicit, and cleanup is explicit too.
