# Persistent Home favorites

[Specification §§5.1, 5.5 and 11](../specifications/arinn-specification.md),
[ADR-0084](../adr/0084-favorites-contract.md), [Favorites1 XML](../plugin-api/favorites1.xml).

`org.arinn.Favorites1` is a daemon-root interface at `/org/arinn/Daemon1` on
`org.arinn.Daemon1`. It is Arinn-owned state, not a provider capability.

`List() → (revision, [(provider_id, capability, local_id)])` returns one complete
ordered set, at most 1024 entries and 256 KiB. Revision is nonzero and persists
with the list. Empty means the user selected no favorites; clients must not
replace it with the installed application catalog. No defaults are inferred from
application names, provider identity or catalog position.

Targets reuse `EntityRef`. Applications use their stable opaque provider ID,
`application`, and provider-declared application ID. Shortcuts use the reserved
pair `arinn`, `web-shortcut`, and their daemon-owned shortcut ID. Browser tab,
window, media session and generation IDs cannot be pinned. Entries contain no
titles, URLs, icons or payloads. Duplicate identities and unsupported kinds are
`InvalidArgument`; Provider IDs and local IDs follow the existing opaque ID bounds.

`Set(revision, entries) → revision` replaces the whole ordered set in one SQLite
transaction. A stale revision returns `Conflict` without writing. Newly added
application targets must occur in a complete current provider catalog; newly
added shortcuts must exist. Missing targets are `NotFound`, unavailable catalogs
are `Unavailable`. Existing targets may be retained, reordered or removed even
when their provider or shortcut disappears. Catalog validation does not turn an
ephemeral target into a stable identity. Identical lists succeed without a revision
change. A changed list increments the durable revision and emits `Changed` after
commit. Revision overflow fails with `Conflict` and leaves the list intact.

Clients subscribe before reading, validate complete bounded snapshots, and reject
nonregressing-revision violations within one daemon owner. A new daemon owner
resets the transport fence, never the saved list. Mutations capture the revision,
serialize locally, do not retry automatically, and reconcile through `List` after
success or an uncertain reply. A conflict keeps the current selection and asks
for refresh. A notification received during a read must schedule another read.

Home joins pinned identities with current daemon-mediated application and shortcut
models in saved order. A missing target remains visible as unavailable with a
neutral presentation; only the user removes it. The shell exposes pin/unpin on
Applications and Home; unpinning never deletes the application or shortcut.
Only focus, pending state and drafts are shell-local. Persist no shell copy and
never emit identities, titles or URLs in logs/diagnostics.

## Verification

The contract tests cover wire signatures and round trips, ordering, empty lists,
duplicates, invalid namespaces, transient capabilities and count/body bounds.
Implementation requires persistence/restart, concurrent-write conflict, no-op,
unavailable-target retention and admission tests, live XML introspection, and
private-bus/offscreen shell tests for mutation, Home filtering and focus restoration.
Contract preparation alone closes no gate and runs no real-desktop operator tests.
