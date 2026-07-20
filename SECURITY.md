# Security Policy

## Supported versions

Ricochet 1.x is stable software. Security fixes are developed on the current
supported line and released forward in a patch or minor release when
appropriate. Older release lines, release candidates, nightly builds, and
source snapshots do not receive guaranteed backports.

Use the newest published stable release when evaluating or reporting an issue.

## Reporting a vulnerability

Please do not disclose suspected vulnerabilities in a public GitHub issue.

Use [GitHub's private vulnerability reporting
form](https://github.com/BARKx4/Ricochet/security/advisories/new).

Include, when available:

- the affected Ricochet version or commit;
- operating system, architecture, and installation method;
- the affected component;
- prerequisites and a minimal reproduction;
- the expected security boundary and observed behavior;
- potential impact and any known mitigation.

Use synthetic data and credentials. Do not include real secrets, personal data,
or access tokens.

## Security boundaries

Ricochet's language runtime, CLI, first-party packages and editor integration,
official release artifacts, and update metadata are in scope.

The CLI uses its trusted capability profile by default for local scripts.
Untrusted code should be run with the sandboxed profile and only the
capabilities it needs. A bypass of documented filesystem, network, process,
package, session, or artifact-verification controls is in scope. Behavior
explicitly permitted by a granted capability is not, by itself, a
vulnerability.

If an issue originates in a third-party dependency, report it upstream as well.
Please still report it privately here when Ricochet's use of that dependency
makes the issue exploitable.

## Response expectations

Reports are handled on a best-effort basis. The project does not promise a
particular acknowledgement, remediation, or disclosure timeline. Follow-up and
coordinated disclosure will use the private advisory thread.
