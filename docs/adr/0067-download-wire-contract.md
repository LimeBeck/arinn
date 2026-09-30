# ADR-0067: Download wire contract

- **Status:** Accepted
- **Date:** 2026-09-30
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within [ADR-0025](0025-downloads-mvp.md)
- **Amends / Supersedes:** amends [ADR-0025](0025-downloads-mvp.md) (wire details)
- **Amended by / Superseded by:** —
- **Specification:** [§5.4](../specifications/arinn-specification.md#54-downloads), [§7.5](../specifications/arinn-specification.md#75-downloads-and-transmission), [§9](../specifications/arinn-specification.md), [§13](../specifications/arinn-specification.md#13-security)

## Context

ADR-0025 and the [API review](../plugin-api/mvp-extensions.md) accepted the
`download` capability and the job model: fields, states, actions, and
separate download and import states. The XML, the types and the SDK service
were left for this task ([#142](https://github.com/LimeBeck/arinn/issues/142)).
The daemon routing and the library import are
[#144](https://github.com/LimeBeck/arinn/issues/144). The Transmission
provider is [#143](https://github.com/LimeBeck/arinn/issues/143).

## Decision

- **Two interfaces, as for media.**
    - Providers export `org.arinn.DownloadProvider1` on `/org/arinn/Provider1`.
    - The daemon exports `org.arinn.Downloads1` on the provider's daemon path.
    - Types are in `arinn-protocol::download`. The SDK service is
      `DownloadService`, with `DownloadHandler::control(action, id)`.
- **Names.** The review's `GetSnapshot` becomes `GetJobs(token)`, and the
  daemon method is `GetDownloads(token)`. This matches `GetSessions` and
  `GetLibrary`.
- **Job** `(ssstbttxbdasssa(sss))`:
    - `id`, `title`;
    - `state`, a token. The known states are `queued`, `downloading`,
      `paused`, `verifying`, `completed` and `error`; the shell shows any
      other token as unsupported;
    - `completed_bytes`; `has_total` and `total_bytes`; `rate_bytes_per_second`;
    - `eta_seconds`, where −1 is unknown;
    - `has_progress` and `progress`, 0..1;
    - `actions`, from `pause`, `resume`, `start-now` and `retry`;
    - `error_code` and `error_message`;
    - `outputs`.
- **Job rules:**
    - without a value, `total_bytes` and `progress` are 0, so an unknown size
      is never a percentage;
    - `completed_bytes` never exceeds a known total;
    - `error_code` is an `Error1` name exactly when the state is `error`, and
      the message is empty otherwise;
    - at most 8 unique actions and 64 unique outputs.
- **Output** `(id, title, file_uri)`:
    - the id is the stable output identity. The library's completion ledger
      keys on (provider, job id, output id), and the daemon passes the output
      id as the `LibraryImport1` completion id;
    - `file_uri` is a local `file:///` URI or empty. The provider owns the
      mapping from backend paths to local mounts, as its own setting
      (ADR-0065). With no mapping, the URI is empty and the import is
      `unavailable`. Nothing scans an arbitrary path.
- **Download** `(ssstbttxbdasssa(ss)sa(sss))` is what the shell sees:
    - the job's fields, with outputs reduced to `(id, title)`, so a file URI
      never reaches the shell;
    - `import_state`: `none` (nothing to import yet), `pending`, `imported`,
      `unavailable` or `unsupported`. The daemon owns it;
    - `library_refs`, which are non-empty only when `imported`.
- **Commands.**
    - A command the job does not list is `Unsupported` and is not attempted.
    - With the backend `unavailable`, cached rows stay readable and commands
      are `Unavailable`.
    - After an ambiguous backend timeout, the provider returns `Timeout` and
      refreshes the job. It never repeats the mutation.
    - Daemon commands take a request id and return `Operation1`.
- **Signals.** The provider emits `StateChanged(generation, revision)` only
  when normalized data changes. The shell listens for `Daemon1.StateChanged`
  with the capability `download`.
- **Scope.** There is no method to add, delete or move data. `Features` is
  empty in version 1 and reserves the name for a future `AddSource`.

## Consequences

- The daemon can route `download` (#144) and the Transmission adapter can be
  written against the SDK (#143), in parallel.
- File URIs are daemon-only data, like `MediaSource` URIs.
- The import state being daemon-owned means the shell's toast rule ("Added to
  library" only after the import) needs nothing from providers.

## Alternatives considered

- **One job type with URIs for both sides.** Rejected: the shell must never
  receive file paths (R-01, §13), and filtering by convention is easy to break.
- **Import state in the provider's job.** Rejected: the provider does not know
  the library's result; the review makes download and import states independent.
- **The daemon maps paths.** Rejected: the mapping depends on the backend
  (remote Transmission, containers), so the provider owns it as a setting.

## Verification

- `arinn-protocol` tests:
    - signatures and the round trip;
    - unknown sizes, progress bounds and NaN;
    - the error code only with `error`;
    - unique actions and outputs;
    - local file URIs only;
    - the shell view has no URI in its encoded page;
    - import states, and refs only when imported.
- `arinn-plugin-sdk/tests/download_service.rs` on a private bus:
    - registration as `download`;
    - reads during `unavailable`, and commands then refused;
    - `Unsupported`, `NotFound`, `InvalidArgument`, `Conflict` and
      `PermissionDenied`;
    - the handler gets only allowed commands;
    - live introspection matches `download-provider1.xml`.
- `downloads1.xml` is compared with the daemon export in #144.
