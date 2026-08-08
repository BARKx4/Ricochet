# Support

Ricochet 1.0.x is the feature-locked Long Term Support line. The canonical
1.x tool is `rco`.

Ricochet 2 is an incompatible product line under development on the
`ricochet-2` branch. Its canonical tool will be `ricochet`. A 2.x prerelease is
not covered by the 1.x LTS compatibility promise.

## Documentation

Start with the published documentation:

- [Learn Ricochet](https://barkx4.github.io/Ricochet/learn/)
- [Reference guides](https://barkx4.github.io/Ricochet/reference/guides/)
- [Published releases](https://github.com/BARKx4/Ricochet/releases)

## Questions and bug reports

Search [existing GitHub issues](https://github.com/BARKx4/Ricochet/issues)
before opening a new one. State whether the report is a question, bug, or
documentation problem; maintainers will apply the corresponding label.

For a question or reproducible defect, include:

- the Ricochet version or commit;
- operating system and architecture;
- installation method;
- the exact command or workflow;
- expected and observed behavior;
- complete relevant output;
- the smallest reproducible example.

Do not report suspected security vulnerabilities in a public issue. Follow
[SECURITY.md](SECURITY.md) instead.

## Supported builds

Official release artifacts are currently produced for:

- Windows x64;
- Linux x64, as a portable tarball and Debian package;
- macOS arm64 and x64.

Source checkouts are tested in CI on current Windows, Ubuntu, and macOS runners.
Other operating systems, architectures, package managers, and modified builds
are best-effort and are not guaranteed.

## Version policy

Ricochet 1.x accepts correctness, security, crash, data-loss,
supported-platform, packaging, installer, CI, and documentation fixes for
existing behavior. It does not accept new syntax, public words, commands,
packages, targets, or language features. Maintenance releases use `1.0.x`
patch versions; there is no planned `1.1.0` feature line.

Before reporting a 1.x release problem, reproduce it with `rco` on the
newest published stable release in the `1.0.x` line when possible.

Older release lines, release candidates, nightly artifacts, and downstream
modifications do not receive guaranteed maintenance or backports.

Ricochet 1.x remains supported through Ricochet 2.0.0 general availability and
for at least twelve months afterward. Any end-of-life date will be announced
at least ninety days in advance.

## Expectations

Support is provided on a best-effort basis. LTS defines maintenance scope and
duration; it does not provide a guaranteed response, resolution, or backport
timeline.
