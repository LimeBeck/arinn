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
pair `arinn`, `web-shortcut`, and their daemon-owned shortcut ID. A pinned music
station uses its music provider ID, `music-station`, and its pinnable station ID
(#811); its title and artwork live in `MusicPins1` (see below). Browser tab,
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

Holding Menu or Select for 650 ms, or a pointer press-and-hold, opens
the tile action sheet on Applications or Home favorites (spec §6.2; a short Menu
keeps toggling navigation, or opens the shortcut editor on a web shortcut). The
sheet is the right-hand sheet of [ui-running.md](ui-running.md). Pin/unpin sends the captured revision and the
complete ordered identity list. Only one write is in flight; no mutation is
retried, errors are normalized without remote payloads, and every outcome is
reconciled with List. A refresh action is available after conflicts or uncertain
replies. Removing a Home favorite closes its sheet and restores the nearest
surviving control. A uniquely associated terminable application Activity offers
Close through Application1; the shell's close flow asks first, with Cancel
focused by default ([ui-running.md](ui-running.md)), without calling external
applications directly. Transient read failures schedule
a fresh bounded read on the next status poll; writes never retry. Select
autorepeat releases cannot activate a tile or cancel its hold timer.

## Verification

The contract tests cover wire signatures and round trips, ordering, empty lists,
duplicates, invalid namespaces, transient capabilities and count/body bounds.
Implementation requires persistence/restart, concurrent-write conflict, no-op,
unavailable-target retention and admission tests, live XML introspection, and
private-bus/offscreen shell tests for mutation, Home filtering and focus restoration.
Contract preparation alone closes no gate and runs no real-desktop operator tests.

## Pinned music stations

[ADR-0143](../adr/0143-music-station-pins.md), [MusicPins1 XML](../plugin-api/music-pins1.xml).
`org.arinn.MusicPins1` sits next to `Favorites1` at the daemon root. It keeps
what Home shows for a pinned station while its provider is away: provider ID,
station ID, title (at most 256 bytes, no control characters) and an opaque
artwork ID or empty.

- `List() → (revision, [(provider_id, station_id, title, artwork)])` returns
  the pinned stations in favorites order, at the favorites revision.
- `Pin(revision, pin) → revision` stores the presentation and appends the
  `music-station` target in one transaction. For a station already pinned it
  refreshes the title and artwork in place; an identical pin returns the same
  revision without a signal. A full favorites list gives `Unavailable`.
- `Unpin(revision, provider_id, station_id) → revision` removes the target and
  its record; the other favorites keep their order. An unknown pin is
  `NotFound`.
- Both share the favorites serializer and revision: a stale revision is
  `Conflict`. Both announce through `Favorites1.Changed` after commit.

`Favorites1.Set` keeps the two consistent. A `music-station` target may be
reordered or removed; removing it drops its record in the same transaction. It
may be added only while `MusicPins1` holds its record (`NotFound` otherwise), so
a station enters the favorites only through `Pin`. SQLite schema 9 adds the
`music_pins` table. The daemon never calls a music provider for a pin; starting
the station and the provider's own `pin` action are the shell's (#804).

### Shell binding (#804)

- **Reading.** The shell accepts `music-station` targets in `Favorites1.List`.
  When the list holds any, it reads `MusicPins1.List` and gives the station's
  Favourites row the pinned title. The whole reply must hold
  `MusicPinList`'s invariants (valid IDs, titles and artwork IDs, no duplicate
  station, at most 1024 pins, a nonzero revision), or none of it is shown. A
  list of another favorites revision has favorites read again. That, and a
  transient `List` failure, which keeps the titles already read, is retried on
  a later status poll, at most five times in a row. An older daemon without
  the interface is not asked again. While the source is available, the stored
  artwork ID is shown through the source's current asset-image route
  (`MusicCatalog.artworkUrl`). Without a record, or against an older
  daemon, the row is the usual removable "Unavailable favorite".
- **Writing.** `Backend.pinMusicStation(provider, station, title, artwork)` and
  `unpinMusicStation(provider, station)` call `Pin` and `Unpin` at the current
  favorites revision. Like `setFavorite`, they allow one write at a time,
  never retry, and reconcile through `List`. The title is cleaned before
  `Pin`: control characters become spaces and the text is cut to 256 UTF-8
  bytes on a character boundary. Removing the row through the shelf's action
  sheet is an ordinary `Favorites1.Set`.
- **Availability.** Home marks a pin unavailable, with its reason, while its
  music source is not connected: the provider is not running, or the music
  room reports its account as anything but connected or scanning (signed out,
  expired, restricted, unavailable, still connecting). The room polls its
  sources while Home shows a pin, as it does on the music screen, so this
  follows an account that changes while Home is open. It is also unavailable
  after the source answered `radio` for it with NotFound or Unavailable
  (`MusicCatalog.stationResolved`; `docs/plugin-api/music.md`) until that
  provider restarts with a new generation. Selecting such a pin opens the
  action sheet; it never starts another station.
- **Starting.** Selecting an available pin starts the station the way a station
  card in the music room does: the replace confirmation, then *Now playing*.
- **The provider's own `pin`.** Where a provider offers it, Arinn mirrors a pin
  or an unpin to it for the station that plays now
  (`MusicCatalog.syncProviderPin`). It mirrors only a stored change:
  `Backend.musicStationPinChanged` reports the stations a favorites list read
  pins or unpins compared with the previous read of the same daemon
  connection, never a request that may still fail. A mirror that meets another
  music command in flight waits for it; one whose station stopped playing is
  dropped. The Home shelf still comes from Arinn's store.
- **Pinning from the player.** For a station the provider marks `pinnable`,
  and whose title is not empty once control characters are removed (Home
  stores the title), *Now playing*'s *More* sheet offers *Pin to Home* or
  *Unpin from Home*
  (`MusicView.stationPinRequested`, then `Backend.pinMusicStation` or
  `unpinMusicStation`). The row waits while a favorites write is in flight.
  Any other station keeps the provider's own *Pin station* row.

