# Plugin API

Plugin API v1 is **not yet released as stable**. The architecture and semantics are chosen; the Rust SDK, D-Bus XML for every capability and the contract suite are produced during implementation (Phase 1 onwards, see the [roadmap](../../ROADMAP.md)).

- Requirements summary: [specification §9](../specifications/arinn-specification.md).
- [ADR-0020](../adr/0020-dbus-contract-baseline.md) and the [implementation baseline](../architecture/implementation-baseline.md): transport, identity, state, operations, limits.
- [Plugin runtime](../architecture/plugin-runtime.md): manifest, registration, supervision.
- [Media and download extensions](mvp-extensions.md): separate API review for the extended MVP.

Capabilities: `application`, `browser`, `media`, `library`, `action`, `download`. The last one was added by an explicit MVP extension ([ADR-0025](../adr/0025-downloads-mvp.md)), not taken from specification v0.3.

Before a capability is implemented, its versioned introspection XML with a typed model and contract fixtures is fixed. Phase 0 needed only the registry, an application/demo snapshot, operations and state lifecycle.

The executed spike keeps the actual [introspection XML](phase0.xml) of `org.arinn.Spike1`. Rust types are in `crates/arinn-protocol`; Qt decoding is checked by an integration test. Names and model are experimental: a singleton provider, a reduced snapshot and a temporary KWin bridge. This is not a replacement for the production XML. When zbus returns a struct, a one-element tuple `(Snapshot,)` is used so the wire body contains exactly one struct matching the XML and QtDBus.

## Licensing

The protocol crate, the future SDK and the interface XML are licensed `MIT OR Apache-2.0`, so plugins may use any license ([ADR-0029](../adr/0029-licensing.md)).
