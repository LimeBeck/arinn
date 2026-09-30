# Transmission in the MVP

Part of [ADR-0025](../adr/0025-downloads-mvp.md), answer P-03. The core receives only DownloadProvider entities and operations; Transmission stays an external service.

`arinn-plugin-transmission` connects to a configurable RPC endpoint; the default is loopback `http://127.0.0.1:9091/transmission/rpc`. The provider never enables RPC on an external interface and never changes the configuration of an existing server without a separate installation step. A remote endpoint supports HTTPS and a credential reference; passwords, headers and session tokens never reach logs.

On connect the provider negotiates the supported RPC version and capabilities; old and new RPC naming conventions are adapter details. The versions actually covered by integration tests are recorded in diagnostics. Compatibility with any Transmission is never assumed from the service name alone.

The HTTP 409 session-id challenge is handled as the [RPC specification](https://github.com/transmission/transmission/blob/main/docs/rpc-spec.md) describes, by repeating the rejected request with the new token. That is not a retry of an executed command. After an ambiguous network timeout a mutation is not repeated automatically: the job state is refreshed first.

The backend is polled every 1 s while a transfer is queued, downloading, verifying or seeding, and every 10 s when idle. The provider cannot see whether the Downloads section is open; a command schedules an immediate read. Reconnect uses 1/2/4/8/16/30 s with jitter. The provider publishes a new snapshot revision only when normalized data changes. When RPC is lost, cached rows stay readable with the backend `unavailable` and commands are refused.

After completion, media goes through the generic DownloadProvider → `LibraryImport1` flow. Arinn never moves or deletes data automatically and never interrupts seeding for indexing. The configured download destination must be inside the library roots or have an explicit mapping; without it the download stays completed and import is unavailable.

The MVP shows and controls jobs added through the regular Transmission client. Adding jobs from Arinn, tracker settings and torrent statistics are not part of the current design. Acceptance includes one real job with several media files and non-media output, disconnect/reconnect, duplicate completion delivery and no duplicates in Library/New.

[API review and job model](../plugin-api/mvp-extensions.md).

## Adapter

`arinn-plugin-transmission` exports `DownloadProvider1` and `ProviderSettings1`. It does not add, delete or move data, and it does not change the server configuration. The Arch package lists `transmission-cli` as optional. With no running service the process stays up and the backend is `unavailable`.

Integration tests cover RPC version 17 (integer `status`, the 409 session challenge, and HTTP basic authorization) against a fake server. The provider reads `rpc-version` from `session-get` and accepts 15 or newer. An older or missing version leaves the backend unavailable. The negotiated version is logged as an integer. The endpoint, the session id, credentials, paths and titles are not logged.

| Transmission `status` | Job state |
| --- | --- |
| 3 | `queued` |
| 4 | `downloading` |
| 0, while data remains and the torrent has not failed | `paused` |
| 1 and 2 | `verifying` (`eta_seconds` is −1) |
| 5, 6, or 0 when the selected data is complete | `completed` |
| local error (`error` 3), or tracker error (`error` 2) while stopped | `error` |

Peers, seeds, ratio, trackers and the backend error string are not copied into the job. A failed job carries a fixed safe message.

| Action | RPC | When |
| --- | --- | --- |
| `pause` | `torrent-stop` | queued, downloading, verifying, seeding |
| `resume` | `torrent-start` | paused, or completed and stopped |
| `start-now` | `torrent-start-now` | queued, paused, or completed and stopped |
| `retry` | `torrent-verify`, then `torrent-start` | `error` only |

`retry` never calls `torrent-add`. A timed-out mutation is not sent again; the following poll is a read.

### Settings

| Id | Kind | Meaning |
| --- | --- | --- |
| `endpoint` | string | RPC URL. The default is `http://127.0.0.1:9091/transmission/rpc`. HTTP or HTTPS, with no user or password in the URL. |
| `credential_ref` | string | Secret-storage label. Empty means no authorization. The value is not a password. |
| `path_map` | string | `backend>local` pairs separated by `;`. Both sides are absolute paths. The longest matching backend prefix is replaced. |

The secret service is `org.freedesktop.secrets`. Item attributes are `arinn=transmission` and `credential=<credential_ref>`. The secret text is `username:password`. A locked item is not unlocked here, so the provider raises no prompt. A missing service or item leaves the backend unavailable.

### Outputs

A file whose `bytesCompleted` has reached `length` is an output. The id is the file index, a hyphen, and the first 16 hexadecimal digits of the SHA-256 of the torrent-relative path. At most 64 outputs are published, in index order. `file_uri` is the mapped `file:///` URI, or empty when no prefix matches.
