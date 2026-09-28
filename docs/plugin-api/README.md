# Plugin API

API major 1 is implemented for the production daemon and the `application`
capability, but **has not been released as a stable Plugin API**. Requirements:
[specification §9](../specifications/arinn-specification.md),
[ADR-0020](../adr/0020-dbus-contract-baseline.md),
[ADR-0035](../adr/0035-phase1-application-wire-contract.md).

## Wire contracts

| Interface | Endpoint and contract |
| --- | --- |
| `org.arinn.Daemon1` | `org.arinn.Daemon1`, `/org/arinn/Daemon1`; [daemon1.xml](daemon1.xml) |
| `org.arinn.Application1` | Daemon-owned `/org/arinn/Daemon1/providers/p_<uuid_hex>`; [application1.xml](application1.xml) |
| `org.arinn.Provider1` | Provider unique bus owner, `/org/arinn/Provider1`; [provider1.xml](provider1.xml) |
| `org.arinn.Operation1` | Daemon-owned operation path returned by a command; [operation1.xml](operation1.xml) |

The private-bus suite compares these files to live introspection. Struct field
order is defined in `arinn-protocol::production` and `application`; methods returning
one struct use a one-element tuple to preserve the D-Bus body signature for QtDBus.
The old [phase0.xml](phase0.xml) is historical evidence, not an active interface.

### Platform window contract (production implementation pending)

`org.arinn.PlatformWindows1` is specified for the daemon service/root path by
[platform-windows1.xml](platform-windows1.xml) and
[ADR-0039](../adr/0039-platform-window-contract.md). It defines paged live windows,
platform generations and targeted activation/graceful-close operations for
registered application providers. Exact field meanings, authorization, lifetime
and failure rules are in the [platform contract](../architecture/platform-windows.md).
The SDK exports `platform::WindowClient`; its private-bus fixture checks XML and
client behaviour. The production daemon does **not** export this interface yet.
This is not a seventh capability or a shell-to-compositor API.

### Web shortcut contract (daemon implementation pending)

`org.arinn.WebShortcuts1` is specified for the daemon service/root path by
[web-shortcuts1.xml](web-shortcuts1.xml) and typed in `arinn-protocol::web`
([ADR-0043](../adr/0043-web-shortcuts.md)). Web shortcuts are Arinn-owned state,
not a capability; the shell manages them from the TV.

- `List` returns the complete ordered list `(revision, [(id, title, url, icon)])`.
  At most 64 shortcuts; titles are single-line, non-empty and at most 256 bytes;
  URLs are absolute `https`, or `http` for a local-network host judged without
  DNS (private, loopback, link-local and unique-local IP literals, single-label
  names, `.local`, `.lan`, `.home.arpa`, `.internal`;
  [ADR-0045](../adr/0045-local-http-web-shortcuts.md)), without user information,
  whitespace or control characters and at most 2048 bytes; `icon` is empty or an opaque packaged icon
  reference. The largest valid list fits one message, so there is no paging.
- `Add(title, url, icon, request_id)` returns the new ID and is deduplicated by
  caller and request ID. `Update(id, revision, …)` and `Remove(id, revision)`
  take the list revision the caller last read and fail with `Conflict` when it
  is stale, `NotFound` for an unknown ID and `InvalidArgument` for invalid input.
- `Open(id, request_id)` returns an `org.arinn.Operation1` object. The daemon
  opens the URL through the default BrowserProvider; with none available the
  operation fails with `Unavailable`.
- `Changed(revision)` follows every committed change; subscribe before `List`.

Shortcut URLs are user configuration: they never appear in logs, diagnostics or
`arinnctl diagnostics` output. The protocol test checks the XML against a live
fixture export on a private bus.

The production daemon exports it ([task #56](https://github.com/LimeBeck/arinn/issues/56)).
Shortcuts and the list revision are stored in `state.sqlite3` (schema 3), so a
stale revision stays stale across daemon restarts. On start the daemon seeds the
packaged default set from the first `arinn/web-shortcuts.toml` in the XDG data
home and data directories, once per state database; invalid entries are skipped
and a removed default is never re-added. A full list rejects `Add` with
`InvalidArgument`. Until a BrowserProvider runtime contract exists (Phase 3),
`Open` admits an operation that fails with `Unavailable`.

### Production application and registry semantics

`Providers(token)` returns an immutable registry page with a snapshot ID. Start
with an empty token and follow `next_token`; tokens are caller-bound and expire
in 30 seconds. Each provider record identifies its capability endpoint, generation,
revision, runtime/backend status, source manifest and safe diagnostic timestamps.
Clients derive available capabilities from ready provider records.

Each application record is `(id, title, can_launch, group)`. `group` is chosen
by the provider: `online`, `games`, `media` or `other` (spec §5.5). It is a short
lowercase token on the wire; consumers display an unknown value as `other`, so
providers can add groups without breaking older shells.

`GetApplications(token)` combines installed applications and running instances
under one immutable generation/revision. Pages have at most 200 total entries and
256 KiB of encoded body. The complete catalogue is bounded to 100,000 entries and
16 MiB; at most 32 concurrent leases are retained by each store. Consumers install
the signal match before the first read and publish only a complete, validated
snapshot. `Conflict` on a token requires a new read from the beginning. A late
subscriber can first observe any positive revision, not necessarily revision 1.

A public application command takes generation, opaque local ID and request UUID,
then returns an operation object. Subscribe to `Changed` before `Get`; completion
can precede the admission reply. Deduplication uses caller unique name and request
UUID; changed content gives `Conflict`. Read/registration budget is 3 seconds,
controls 5 seconds, launch 30 seconds. Commands are never retried automatically.
The private provider command reply completes the daemon's operation; providers do
not create a second public operation tree. Cancellation is currently unsupported.

`Status`, `HomeItems`, `Home` and `Back` support the minimal shell/platform bridge.
`HomeItems` is a bounded projection of activatable running instances (up to five),
not a complete catalogue. `Status.generation` contains the daemon epoch;
provider generations and revisions come from provider/application pages.

## SDK and fixtures

`arinn-plugin-sdk::runtime` exports `ApplicationService`, `ApplicationHandler` and
`run`. The daemon supplies provider/plugin IDs, version, token file and private
provider data/state directories in its launch environment. SDK methods authenticate
the daemon's unique owner. Owner changes trigger re-registration and a fresh snapshot;
`replace` publishes validated content atomically and emits a revision notification.

`arinn-plugin-demo --fixture <name>` supports `populated`, `empty`, `delayed`,
`invalid-id`, `crash`, `reconnect`, `revision-race`, `unavailable` and `large`.
The [production contract harness](../../tests/integration/phase1.py) runs those
fixtures through manifest reloads on a private bus. See the
[implementation record](../architecture/phase-1-progress.md) for evidence and limits.

Known capabilities are `application`, `browser`, `media`, `library`, `action` and
`download`. Only application has a runtime contract in Phase 1; the others are
explicitly incompatible until implemented. [MVP extensions](mvp-extensions.md)
require their own typed XML and contract fixtures.

## Licensing

Protocol types, SDK and interface XML use `MIT OR Apache-2.0`; providers may use any
license ([ADR-0029](../adr/0029-licensing.md)).

## Internal platform transport

[KWinBridge1](kwin-bridge1.xml) and [its lifecycle contract](../architecture/kwin-transport.md)
define trusted adapter inventory/control transport. This is not a provider capability.
SDK registration contexts expose the registered connection for authenticated platform
access. Production daemon/KDE implementation remains in issue #20.
