# Hosted Registry Protocol

This document specifies the Ricochet hosted package registry protocol target for
the Epic 8 implementation work. It does not replace the current local and
static registry behavior. Existing `rco publish PACKAGE --registry PATH`,
`rco registry rebuild PATH`, `rco registry check PATH`,
`rco search QUERY --registry-url URL`, and static
`ricochet-static-registry-v1` indexes remain supported.

Ricochet currently implements read-only hosted client operations for discovery,
search, metadata fetch, archive fetch, install, and lockfile verification.
Publish, yank, authentication, a real hosted server/reference implementation,
and mirror export remain future work.

## Protocol Identity

- Protocol name: Ricochet Hosted Registry Protocol.
- Protocol version string: `ricochet-hosted-registry-v1`.
- Registry base URLs must use `https://` outside local tests. Static mirrors
  may continue to use `file://` or `https://` index URLs.
- Clients should send `Accept` for the most specific media type they understand
  and treat unknown required fields as an unsupported protocol revision.

Canonical media types:

| Purpose | Media type |
| --- | --- |
| Registry discovery | `application/vnd.ricochet.registry.v1+json` |
| Search response | `application/vnd.ricochet.registry.search.v1+json` |
| Package metadata | `application/vnd.ricochet.registry.package.v1+json` |
| Publish metadata part | `application/vnd.ricochet.registry.publish.v1+json` |
| Error envelope | `application/vnd.ricochet.registry.error.v1+json` |
| Package archive | `application/vnd.ricochet.package.archive.v1+gzip` |
| Static export | `application/toml; profile="ricochet-static-registry-v1"` |

Discovery responses have this shape:

```json
{
  "protocol": "ricochet-hosted-registry-v1",
  "base_url": "https://registry.example"
}
```

The read client fetches `GET /v1` before hosted search or install operations.
`base_url` is the registry base used to resolve later endpoints and artifacts.
For beta/local fake registries the client also accepts discovery without
`base_url` and uses the requested base URL.

Package archives are gzip-compressed tar files with the same archive safety
rules as static registry archives: relative entries only, no `..`, no absolute
paths, no links, and bounded file counts and unpacked bytes.

## Package Identity

Hosted registries use the same package identity rules as the static registry:

- Unscoped names use ASCII letters, numbers, `_`, and `-`.
- Scoped names use `@scope/name`; both `scope` and `name` use ASCII letters,
  numbers, `_`, and `-`.
- Names are case-sensitive and must not be normalized by the registry.
- Dependency aliases are local project names. They do not change the hosted
  package identity recorded in metadata and lockfiles.
- Endpoint paths encode a package identity as one percent-encoded path segment:
  `@ricochet/forms` becomes `%40ricochet%2Fforms`.

Package versions use semver. A package version record is addressed by package
identity plus exact version.

## Version Metadata

Version records are immutable once published. A registry must reject every
attempt to publish an existing package/version pair, including attempts with
identical bytes. A yanked version stays present in metadata and artifacts but is
not available for new resolution.

Package metadata responses have this shape:

```json
{
  "protocol": "ricochet-hosted-registry-v1",
  "package": {
    "name": "@ricochet/forms",
    "latest": "0.1.0"
  },
  "versions": [
    {
      "version": "0.1.0",
      "published_at": "2026-06-21T00:00:00Z",
      "yanked": false,
      "archive": {
        "path": "artifacts/@ricochet/forms/0.1.0/forms-0.1.0.tar.gz",
        "integrity": "sha256:<64 lowercase hex>",
        "media_type": "application/vnd.ricochet.package.archive.v1+gzip"
      },
      "package_integrity": "sha256:<64 lowercase hex>",
      "provenance": {
        "attestation_path": "artifacts/@ricochet/forms/0.1.0/provenance.attestation",
        "attestation_integrity": "sha256:<64 lowercase hex>",
        "signature_path": "artifacts/@ricochet/forms/0.1.0/forms.sig",
        "signature_integrity": "sha256:<64 lowercase hex>",
        "signature_kind": "minisign"
      }
    }
  ]
}
```

Required fields for every version are `version`, `yanked`, `archive.path`,
`archive.integrity`, and `package_integrity`. Provenance and signature fields
are optional, but if `signature_kind` is present, `signature_path` and
`signature_integrity` must also be present. All integrity values use
`sha256:<64 hex chars>`.

Unknown signature kinds must be preserved and exposed as metadata, not treated
as verified signatures. A client may verify the signature artifact integrity and
then report `unknown_signature_kind` unless policy or user configuration
requires a known signature verifier.

## Endpoints

All JSON endpoints use UTF-8.

| Method and path | Purpose |
| --- | --- |
| `GET /v1` | Registry discovery and protocol version metadata. |
| `GET /v1/search?q=QUERY&limit=N&offset=N` | Search package names, aliases, summaries, and keywords. |
| `GET /v1/packages/{package}` | Fetch all version metadata for a package. |
| `GET /v1/packages/{package}/versions/{version}` | Fetch one immutable version record. |
| `PUT /v1/packages/{package}/versions/{version}` | Publish one new package version. |
| `POST /v1/packages/{package}/versions/{version}/yank` | Mark an existing version yanked without deleting artifacts. |
| `GET /{relative-artifact-path}` | Fetch archive, provenance, or signature artifacts referenced by metadata. |

Search responses exclude yanked versions from default "latest" selection.
Package metadata includes yanked versions so lockfile verification, audits, and
mirrors can see historical state.

The beta read client accepts compact search responses with this shape:

```json
{
  "protocol": "ricochet-hosted-registry-v1",
  "packages": [
    {
      "name": "@ricochet/forms",
      "latest": "0.1.0"
    }
  ]
}
```

For each result, `name` is a hosted package identity and `latest` is the latest
non-yanked semver version selected by the registry. The client validates the
protocol, package name, and version, then prints results in the same simple
`name version` style as static registry search. `results` is accepted as a
compatibility alias for `packages` during this beta slice.

Artifact paths in metadata are registry-relative paths, not absolute URLs.
Clients resolve them against the registry base URL discovered from `GET /v1`,
not against the metadata endpoint URL. For example, metadata fetched from
`https://registry.example/v1/packages/%40ricochet%2Fforms` with
`archive.path = "artifacts/@ricochet/forms/0.1.0/forms-0.1.0.tar.gz"` resolves
to `https://registry.example/artifacts/@ricochet/forms/0.1.0/forms-0.1.0.tar.gz`.
Clients must reject absolute URLs, leading slashes, backslashes, `.` or `..`
path segments, and any resolved URL outside the registry origin. Registries
should not require absolute archive URLs; relative paths keep metadata
mirrorable and allow static exports to preserve the existing index behavior.

## Publish

The publish endpoint uses `multipart/form-data`:

- `metadata`: JSON with media type
  `application/vnd.ricochet.registry.publish.v1+json`.
- `archive`: gzip-compressed package tarball.
- `provenance`: optional attestation bytes.
- `signature`: optional detached signature bytes.

Publish metadata includes package name, version, package tree integrity,
optional provenance and signature integrity fields, optional `signature_kind`,
summary fields for search, and optional diagnostic fields. The registry must
compute and verify archive integrity, safely unpack or otherwise validate the
archive, read `[package] name` and `version`, and recompute the package tree
`sha256:` before accepting the version.

Mutating requests use an `Idempotency-Key` header generated by the client. The
registry binds each key to the authenticated publisher, method, path, and body
digest for a bounded retention window. Replaying the same key with the same body
returns the original result; replaying the same key with different bytes returns
`409 Conflict` with code `idempotency_conflict`. Registries may also require a
fresh `Ricochet-Date` header and reject stale requests with
`401 Unauthorized` or `409 Conflict` depending on policy. This protects publish
and yank retries from duplicate side effects; bearer-token secrecy still relies
on TLS and credential hygiene.

Authentication uses bearer tokens resolved from secret references. Future CLI
configuration may store only references such as
`token = { secret_env = "RICOCHET_REGISTRY_TOKEN" }`; the resolved token is sent
as `Authorization: Bearer <token>`. Literal tokens must not be written to
`ricochet.toml`, `ricochet.lock`, generated reports, or command traces.

Publisher authorization is package-scoped. The registry must verify that the
authenticated publisher can publish the package identity and scope. The first
publish of a scoped package may claim the package only through registry policy;
later publishes require an authorized publisher for that package. A publish to
an existing version returns `409 Conflict` with code `version_exists`.

## Yank

Yanking marks a version unavailable without deleting metadata or artifacts. The
published version record remains immutable: archive paths, archive integrity,
package tree integrity, provenance, signature fields, publisher, and
`published_at` never change after publish. Yank state is an append-only
availability overlay associated with the version. Package metadata and
per-version responses may project the current overlay as `yanked = true` with
audit fields such as `yanked_at`, `yanked_by`, and `yank_event_id`, but that
projection is not a replacement of the original publish record.

Clients must not select yanked versions for new dependency resolution. Audit and
verification commands may fetch yanked metadata and artifacts for lockfile
forensics, but they must report the yanked state. Mirrors must preserve yanked
records. Clients may cache immutable publish fields and artifact bytes for long
periods, but package metadata and yanked-state overlays must use validators such
as `ETag` and short or `must-revalidate` cache lifetimes so new dependency
resolution sees yank changes.

## Verification Order

Hosted clients verify in this order:

1. Fetch metadata over TLS and validate the media type, protocol version,
   package identity, semver version, yanked state, relative artifact paths, and
   all `sha256:` field shapes.
2. Resolve and fetch the registry-relative archive path.
3. Hash the archive bytes and compare them with `archive.integrity`.
4. Extract the archive with static registry archive safety rules.
5. Confirm the extracted `[package] name` and `version` match metadata.
6. Recompute the unpacked package tree integrity and compare it with
   `package_integrity`.
7. Record the resolved version, registry URL, package identity, archive
   integrity, package tree integrity, provenance integrity, signature integrity,
   and signature kind in `ricochet.lock`.

Ordinary installs must reject same-version metadata or artifact changes that
conflict with an existing lock entry. A package cache with different tree
integrity must fail closed instead of being overwritten.

## Errors, Cache, And Retry

Error responses use this envelope:

```json
{
  "error": {
    "code": "version_exists",
    "message": "package @ricochet/forms 0.1.0 already exists",
    "details": {},
    "request_id": "01J..."
  }
}
```

Expected status codes:

| Status | Use |
| --- | --- |
| `400 Bad Request` | Invalid package identity, version, integrity, archive, or JSON. |
| `401 Unauthorized` | Missing or invalid bearer token. |
| `403 Forbidden` | Token is valid but not authorized for the package/scope. |
| `404 Not Found` | Package, version, or artifact is absent. |
| `409 Conflict` | Same-version publish attempt or lock-incompatible state. |
| `410 Gone` | Optional artifact-retention failure for a historical record. |
| `413 Payload Too Large` | Archive or metadata exceeds registry limits. |
| `415 Unsupported Media Type` | Unsupported request media type. |
| `429 Too Many Requests` | Rate limit; include `Retry-After`. |
| `5xx` | Server failure; include `Retry-After` when retry is useful. |

`GET` requests are retryable and should use `ETag` or `Last-Modified` caching.
Metadata for existing versions is immutable, so per-version responses may be
cached for long periods. Package-level metadata changes when new versions are
published or yanked, so it should have shorter freshness and validators. Clients
must never use cache freshness to ignore lockfile integrity mismatches.

Publish and yank requests are not retried automatically unless the client can
prove the previous attempt did not commit or can safely re-check the resulting
version state.

## Mirror And Static Fallback

Hosted metadata must be exportable to the current static registry format:

```toml
[registry]
format = "ricochet-static-registry-v1"

[packages]
"@ricochet/forms" = "packages/@ricochet/forms.toml"
```

Each hosted version maps to static package metadata fields:

```toml
[[versions]]
version = "0.1.0"
archive = "artifacts/@ricochet/forms/0.1.0/forms-0.1.0.tar.gz"
archive_integrity = "sha256:<64 hex>"
package_integrity = "sha256:<64 hex>"
yanked = false
provenance = "sha256:<64 hex>"
signature = "sha256:<64 hex>"
signature_kind = "minisign"
```

A future mirror command should fetch hosted package metadata and artifacts,
write registry-relative archive/provenance/signature paths, preserve yanked
records, and emit `ricochet-static-registry-v1` indexes that existing static
registry clients can search and install from. If a hosted registry is offline,
projects pinned to a static mirror should continue using the existing
`--registry-url file://.../index.toml` or `https://.../index.toml` behavior.

## Security Model

- Hosted registry traffic uses TLS. Plain `http://` is allowed only for local
  fake-server tests.
- Bearer tokens are resolved from secret references and kept out of
  `ricochet.toml`, `ricochet.lock`, reports, and logs.
- Publishers are authorized per package or scope.
- Publish and yank requests use idempotency keys bound to publisher, path, and
  request digest; same-key replays are idempotent only for identical bytes, and
  conflicting replays are rejected.
- Version records and artifacts are append-only; replacement uses a new semver
  version.
- Yanking is an auditable append-only availability overlay, not deletion or
  mutation of published integrity fields.
- Clients pin registry source, package identity, version, archive integrity,
  package tree integrity, provenance integrity, signature integrity, and
  signature kind in the lockfile.
- Rollback protection is client and server enforced: servers retain historical
  metadata, package-level metadata exposes every non-deleted version state, and
  clients reject locked versions whose hosted metadata no longer matches the
  lock.
- Mirrors preserve yanked records and relative artifact paths so a mirror cannot
  silently substitute an absolute archive URL.

## Implementation Sequence

Later Epic 8 slices should implement the protocol in this order:

1. Hosted read client: discovery, search, package metadata fetch, archive fetch,
   verification order, lockfile invariants, and HTTPS enforcement. Implemented.
2. Publish and yank client: secret-reference bearer tokens, package/scoped
   authorization errors, duplicate-version rejection, provenance/signature
   upload, and yanking.
3. Hosted server/reference implementation for operational smoke tests beyond
   the local fake-server client tests.
4. Mirror command that exports hosted metadata and artifacts to
   `ricochet-static-registry-v1`.
5. Hosted same-version replacement tests against a real server/reference
   implementation and documentation updates for operational deployment.
