# Chapter 15: Async And Tasks

> Part III: Host Capabilities and Local App Surfaces

## Why this chapter matters

Async work lets a program wait for several things without turning every wait into a blocking pause. The important beginner model is that a task is a value you can start, wait on, and reason about.

## What you will build

You will build a parallel-checks script with first-class task handles.

## Concepts in plain English

A task is a handle for work that may finish later. You start work, keep the handle, and join or inspect the result when you need it.

The chapter uses these concepts:

* Spawning blocks as tasks.
* Awaiting one task or many tasks.
* Inspecting task state.
* Releasing retained completed handles.

## Vocabulary and commands

Primary coverage: `spawn`, `await`, `await_all`, `release_task`, `tasks`,
`task_status`, `id`, `info`, `pending?`, `running?`, `completed?`, and
`failed?`.

## Guided example

Open `examples/learn/15-async/parallel-checks.rco` and run:

```
rco run examples/learn/15-async/parallel-checks.rco
```

Spawn runs a block on a background worker and returns a task handle:

```
[ 40 2 + ] spawn answer var
$answer id println
$answer task_status println
```

Await returns the task result:

```
$answer await println
$answer completed? println
```

Completed and failed handles remain retained until you release them:

```
$answer release_task println
```

For several tasks, store the handles in a collection and await them together:

```
handles array
$handles [ 20 2 + ] spawn push! drop
$handles [ 30 4 + ] spawn push! drop
$handles await_all results var
$results 0 at println
$results 1 at println
```

## How to read the example

Trace one representative line from left to right. Identify the values placed on the stack, the word that consumes them, and the result or binding that carries the work forward.

## Try it

Add a task that returns a string:

```
$handles [ "ready" ] spawn push! drop
```

Then inspect a handle before and after awaiting:

```
$answer info inspect println drop
```

## Check your understanding

- What new value shape, command, or host boundary did this chapter introduce?
- Which line is most important to trace with a stack diagram?
- Where would you add a binding to make the example easier to read?

## Common mistakes

* Forgetting to release retained task handles when examples retain them.
* Treating failure inside a task as invisible. A failed handle keeps failed
  status and rethrows when awaited.
* Assuming awaited handles disappear. They remain inspectable until released.
* Hiding too much work inside a task block before you can test it normally.

## What you know now

You know how Ricochet models first-class asynchronous work: task handles are
values, waiting is explicit, and cleanup is explicit too.

## Next step

Continue to [Chapter 16: Capabilities And Sandboxing](16-capabilities-and-sandboxing.md).
