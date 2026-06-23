# Chapter 00: Orientation

## What You Will Build

This opening chapter orients you to Ricochet, the `rco` toolchain, the
reference site, and the examples directory. There is no code to type yet; the
goal is to know where the learning path starts and where exact lookup material
lives.

## Concepts

- Ricochet is a postfix, stack-oriented language with a complete local
  toolchain.
- The manual teaches scripts first, then local apps, MVC apps, packages,
  tooling, and packaging.
- Tutorials, reference pages, wiki notes, and examples each have a different
  job.

## Words Introduced

This chapter introduces the `rco` command and documentation layout rather than
language words.

## Guided Example

There is no guided example in this chapter.

## Try It

Open these files in the repo:

- `docs/learn/index.md` for the manual table of contents.
- `docs/learn/manual-map.md` for chapter status.
- `docs/reference/index.html` for the static reference site.
- `docs/reference/guides/index.html` for focused reference guides.
- `examples/learn/README.md` for runnable manual examples.

If you already have `rco` installed, this command shows the toolchain entry
point:

```powershell
rco --help
```

## Common Mistakes

- Treating the manual as a complete command reference. The manual is the guided
  course; the reference is the catalog.
- Skipping the postfix model before reading MVC or package chapters. Ricochet
  stays postfix in scripts, classes, controllers, templates, and packages.
- Reading capability chapters before safety boundaries. Filesystem, network,
  process, PTY, TUI, and GUI words are powerful host effects.

## What You Know Now

You know where the manual starts, how the learning path is organized, and
where to look for deeper reference material. The next chapter starts with the
smallest possible program.
