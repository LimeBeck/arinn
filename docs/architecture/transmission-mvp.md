# Transmission in the MVP

Part of [ADR-0025](../adr/0025-downloads-mvp.md), answer P-03. The core receives only DownloadProvider entities and operations; Transmission stays an external service.

`arinn-plugin-transmission` connects to a configurable RPC endpoint; the default is loopback `http://127.0.0.1:9091/transmission/rpc`. The provider never enables RPC on an external interface and never changes the configuration of an existing server without a separate installation step. A remote endpoint supports HTTPS and a credential reference; passwords, headers and session tokens never reach logs.

On connect the provider negotiates the supported RPC version and capabilities; old and new RPC naming conventions are adapter details. The versions actually covered by integration tests are recorded in diagnostics. Compatibility with any Transmission is never assumed from the service name alone.

The HTTP 409 session-id challenge is handled as the [RPC specification](https://github.com/transmission/transmission/blob/main/docs/rpc-spec.md) describes, by repeating the rejected request with the new token. That is not a retry of an executed command. After an ambiguous network timeout a mutation is not repeated automatically: the job state is refreshed first.

The backend is polled every 1 s while Downloads is open or transfers are active, and every 10 s when idle; reconnect uses 1/2/4/8/16/30 s with jitter. The provider publishes a new snapshot revision only when normalized data changes. When RPC is lost, cached rows are explicitly unavailable/stale and commands are disabled.

After completion, media goes through the generic DownloadProvider → `LibraryImport1` flow. Arinn never moves or deletes data automatically and never interrupts seeding for indexing. The configured download destination must be inside the library roots or have an explicit mapping; without it the download stays completed and import is unavailable.

The MVP shows and controls jobs added through the regular Transmission client. Adding jobs from Arinn, tracker settings and torrent statistics are not part of the current design. Acceptance includes one real job with several media files and non-media output, disconnect/reconnect, duplicate completion delivery and no duplicates in Library/New.

[API review and job model](../plugin-api/mvp-extensions.md).
