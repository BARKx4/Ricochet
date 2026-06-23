# How to Install and Run Ricochet

The guide assumes you can run `rco` from a terminal.

## Check your install

```bash
rco --help
rco repl
```

Inside the REPL, try:

```rco
"hello" println
```

Exit the REPL with the command your shell displays for quitting, usually `Ctrl+D` or a quit command.

## Run a file

Create `hello.rco`:

```rco
"Hello, Ricochet!" println
```

Run it:

```bash
rco run hello.rco
```

## When you are working from the language source tree

The main guide keeps source-checkout commands out of the beginner path. If you are changing the Ricochet implementation itself, install the CLI once from the repository and then continue using ordinary `rco` commands.
