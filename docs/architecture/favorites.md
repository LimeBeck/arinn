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

## Daemon implementation

`arinnd` exports the interface through its production object server. SQLite schema
7 preserves existing ordered favorite rows and adds one revision singleton. The
revision uses decimal text to retain the complete wire `u64` range; overflow at
`u64::MAX` rejects a changed list. Revision and entries are read in one snapshot
and replaced with an immediate transaction. Shortcut admission shares that
transaction. New applications are admitted against the complete validated Core
cache and provider health, while existing missing entries remain editable.

The export serializes mutations and notifications, runs SQLite work in blocking
tasks, and publishes `Changed` only after commit. There was no production
Favorites writer before this export; schema-six compatibility retains the
ordered stable application selections supported by the internal storage writer.
The shell reads Favorites1 before resolving Home tiles and stores no local copy.
Its asynchronous owner/ticket fence rejects stale, regressing, duplicate or
oversized replies. Changed subscriptions are installed before the first List;
a notification during a read starts a newer read and invalidates the older reply.
Application rows join stable provider/application identities; shortcuts join the
reserved shortcut identity. Empty saved state never substitutes the catalog.
Missing or unlaunchable saved targets remain focusable for removal with neutral
presentation and no disclosure of opaque IDs.

Holding Select for 650 ms or a pointer press-and-hold opens the tile action sheet
on Applications or Home favorites. Pin/unpin sends the captured revision and the
complete ordered identity list. Only one write is in flight; no mutation is
retried, errors are normalized without remote payloads, and every outcome is
reconciled with List. A refresh action is available after conflicts or uncertain
replies. Removing a Home favorite closes its sheet and restores the nearest
surviving control. A uniquely associated terminable application Activity offers
Close through Application1 after a confirmation with Cancel focused by default,
without calling external applications directly. Transient read failures schedule
a fresh bounded read on the next status poll; writes never retry. Select
autorepeat releases cannot activate a tile or cancel its hold timer.

## Verification

The contract tests cover wire signatures and round trips, ordering, empty lists,
duplicates, invalid namespaces, transient capabilities and count/body bounds.
Implementation requires persistence/restart, concurrent-write conflict, no-op,
unavailable-target retention and admission tests, live XML introspection, and
private-bus/offscreen shell tests for mutation, Home filtering and focus restoration.
Contract preparation alone closes no gate and runs no real-desktop operator tests.
