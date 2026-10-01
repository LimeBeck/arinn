# Web shortcut site icons

[ADR-0075](../adr/0075-web-shortcut-site-icons.md); task
[#224](https://github.com/LimeBeck/arinn/issues/224). Requirements: AC-03, AC-12;
R-01–R-05, R-09, R-10; specification §5.1, §7.1, §9.7, §11, §13, §18.

**Status:** production implementation in progress in
[#226](https://github.com/LimeBeck/arinn/issues/226). The provider, preset migration
and asynchronous shell binding are implemented on that branch. Daemon routing
integration is pending shared-file coordination; end-to-end completion and real
KDE/TV qualification are not claimed.

## Ownership and optional transport

The daemon owns shortcut configuration. Only the browser provider performs site
network requests; neither daemon nor shell downloads website resources. This
path does not inspect tabs, page contents in an authenticated browser, browsing
history or browser cookies, and does not start a browser. The extension keeps
its existing permissions. Installed-application icons keep ADR-0059 unchanged.

Two optional interfaces preserve existing WebShortcuts1 and BrowserProvider1:

- `org.arinn.WebShortcutIcons1` at the daemon root provides
  `Get(epoch: s, revision: t, id: s) -> (mime: s, data: ay)`, one output struct.
  Epoch is the current non-nil daemon UUID; revision is the positive revision
  from WebShortcuts1.List; id is the existing bounded opaque shortcut ID.
- `org.arinn.WebIconProvider1` at the registered browser provider path provides
  `GetIcon(generation: s, origin: s) -> (mime: s, data: ay)`, one output struct.
  Generation is the current registration UUID. Origin is a canonical public
  HTTPS origin ending in `/`, without path, credentials, query, fragment or
  nondefault port. ASCII DNS names use lowercase/IDNA form. DNS/IP safety is a
  runtime check, not a property guaranteed by syntactic validation.

The daemon discovers provider support per unique owner through introspection,
then pins calls to that owner. Select the first registered, runtime-ready browser provider in the same registry
ordering used by ordinary default-browser selection, without filtering out a
disconnected browser backend. There is no new default-browser configuration.
Do not probe successive providers for a successful download or choose by product
name. Missing optional support on the selected provider keeps the monogram; it
does not trigger a second provider or browser launch. The provider-owned network
service can work while the browser itself is closed.

Provider calls authenticate the registered daemon unique name and current
registration generation before work and recheck that context before returning.
The daemon checks the epoch, shortcut list revision, entry presence and default
provider registration before work, releases shared locks during I/O, and checks
again after the reply. An edited/removed shortcut, changed selected provider or
owner/generation change discards the pending result. No requested URL is accepted
from the shell: the daemon derives the origin from the stored shortcut URL.

An explicit packaged icon retains precedence. Empty `WebShortcut.icon` allows
optional automatic site discovery; it remains empty in persisted user state.
No downloaded filename, URL or browser asset ID is written into that field.
Missing optional support, errors or rejected data produce the existing monogram.

Errors: `InvalidArgument` for malformed inputs; `Conflict` for stale epoch,
revision or registration; `NotFound` for a missing shortcut or no usable icon;
`Unsupported` for absent optional support or an ineligible URL; `Unavailable`
for network/provider failure or exhausted concurrency; `PermissionDenied` for
an unauthorized provider caller. An invalid provider image is `Internal`.

The result uses the existing `(say)` image envelope but a tighter policy:
**PNG only, at most 128 KiB**, decoded dimensions at most 256×256. No arbitrary
asset lookup is exposed. The provider fully decodes and normalizes the image;
the daemon checks size/type and the shell also bounds dimensions before decode.
The body stays below the ordinary 256 KiB IPC limit. Empty bytes are not success.

## Network and image policy

- Fetch only the root of a stored HTTPS origin on port 443. Local HTTP shortcuts
  remain valid launch targets but receive no automatic network icon. Strip user
  paths, queries and fragments before the provider call.
- Resolve every request host, including each redirect/candidate, under the total
  deadline. Reject all non-public addresses: loopback, private, link-local,
  multicast, unspecified, reserved and IPv4-mapped bypasses. Reject mixed DNS
  answers containing a forbidden address. Pin the approved address for the
  connection while preserving the original hostname for TLS/SNI verification;
  a second uncontrolled DNS lookup is forbidden.
- HTTPS only on redirects and candidates; at most three redirects per resource,
  eight HTTP requests total per discovery (redirects count). Revalidate the
  destination every time. No automatic proxy, cookies, authentication, client
  certificate, Referer, browser profile or ambient credential use. Do not send
  shortcut paths or titles to a favicon aggregator.
- Five-second total discovery deadline, including DNS, response bodies, candidate
  processing and decoding. Two concurrent discoveries per provider, coalesced by
  origin; no unbounded waiting queue. Cancellation discards results and temporary files; worker permits stay held
  until the work actually stops so timed-out decodes cannot accumulate. Failures do not retry on the UI thread or mutate shortcut state.
- Read at most 512 KiB of HTML, 128 KiB of manifest JSON and 1 MiB per candidate
  image. Enforce streaming limits without trusting Content-Length, and disable
  transparent decompression (or charge decompressed bytes to these limits).
- Parse HTML as inert metadata, never execute scripts or fetch subresources.
  Consider `rel=icon`, `apple-touch-icon`, one linked web manifest's icons and
  `/favicon.ico`. Resolve relative references against the final document URL;
  ignore HTML base elements. Inspect at most 32 declarations and attempt at most
  four distinct image candidates, preferring declared raster sizes near 256 px.
  Manifest and apple-touch icons improve TV-scale results but failure is harmless.
- Accept PNG, JPEG, WebP and ICO input only through bounded raster decoders. Reject
  SVG, HTML, malformed/truncated images, animation and disguised content. Before
  allocation, cap input dimensions at 2048×2048 and four million pixels, including
  ICO frame selection. Normalize one static image to at most 256×256 PNG and
  128 KiB output; discard metadata. No runtime image bytes enter the repository.

## Local cache and shell rendering

Use the provider's private XDG cache directory `arinn/web-icons`, independent of
browser profiles. At most 64 positive origin entries and 8 MiB including metadata;
positive TTL seven days, bounded in-memory negative TTL ten minutes (64 entries).
Keys are hashes of canonical origins; no plaintext URL/title/history index.
Validate permissions, refuse symlinks, use atomic replacement, and bound directory
scans and startup cleanup. Evict expired then oldest entries. Reject corrupt,
oversized or expired data before serving; cache loss is harmless. A restart must
not evade disk bounds. Network requests never occur merely to prune the cache.

The shell uses an asynchronous image provider. Its private lookup binds unique
daemon owner, epoch, list revision and shortcut ID; QML receives only an opaque
`image://` token in the real web tile's `iconSource`. Recheck context before
installing results; owner changes, edits and removal invalidate pending work.
Bound the shell cache to 64 entries/8 MiB and at most four in-flight requests
with no unbounded waiting queue. The daemon caps concurrent icon calls at four
and applies a six-second deadline; shell calls have a seven-second deadline. No requests occur synchronously
on focus or D-pad navigation. Failed/missing images leave the monogram visible.
Explicit packaged icons use an allowlisted packaged resolver, never an arbitrary
file/network URL. Installed app theme/local icons keep their separate resolver.

## Packaged preset correction

Change the packaged Kinopoisk URL to `https://hd.kinopoisk.ru`. Ship a generic,
versioned data migration describing the exact prior `(title, url, icon)` tuple
and replacement URL. Apply once transactionally only to an existing seeded state
and exact matches, preserving ID, position, title and icon and advancing revision
only on change. Never create an absent row or reseed deleted defaults; skip any
entry with changed title, URL or icon. Product names remain packaging data.

Historical state did not store per-row seed provenance. An exact clone of an
old default cannot be distinguished retrospectively from that default; no fuzzy
or host-only matching is allowed. Record migration completion even if no row
matches, so later user edits are not silently rewritten. New seeds use the
corrected data from the start.

## Verification and handoff

Contract fixtures verify optional absence, signatures, caller and lifetime
checks, malformed arguments and bounded output. They do not prove downloads or
rendering. The dependent production change must test DNS pinning and forbidden
redirects, total timeout and all budgets, malformed/oversized images, manifest
and touch icons, cache eviction/expiry/corruption/restart, monogram fallback,
current model-to-image wiring, and unchanged installed-application icons.

Migration tests must cover fresh state, unmodified old default, edited title,
URL or icon, removed default, id/order preservation and repeated startup. Required
checks and an independent review precede readiness; real KDE/TV checks require
separate owner authorization. No acceptance criterion or phase closes here.

## Implementation details

`plugins/chrome/src/icons/` owns the optional endpoint, inert metadata parsing,
DNS-pinned HTTPS client, bounded raster conversion and cache. It authenticates
through the SDK registration watch, independently of extension connectivity.
Each blocking DNS/parser/decoder/cache worker retains the discovery semaphore
permit until it actually finishes, including after the outer deadline expires.
At most four subscribers wait for one coalesced origin. Losing all subscribers
cancels discovery; cache replacement checks cancellation and its deadline.

Cache files are named with SHA-256 origin digests and contain only an eight-byte
creation timestamp followed by normalized PNG pixels. Directory-relative,
no-follow file operations retain a directory descriptor across reads and atomic
replacement. Startup and later pruning inspect at most 256 directory entries;
unexpected directory contents disable caching. Cache errors leave downloading
and the monogram fallback independent of persistence.

The shell keeps private lookup records in `WebIconReferences`; the actual
`Backend::readShortcuts` model exposes only their opaque image-provider tokens.
`asset_images.cpp` uses a separate four-worker pool with immediate refusal when
busy, a seven-second bus timeout and dimension checks before PNG decoding.
Its bounded decoded-image cache is separate from media artwork and installed-app
icons. No site artwork is currently packaged: the explicit packaged-icon
allowlist is empty, so unknown nonempty references retain their monogram and
never become filesystem or network paths.

The packaged TOML contains versioned `[[migration]]` records with an ID, old
`title`, `url`, `icon`, and `replacement_url`. SQLite schema 6 adds
`web_preset_migrations`; startup applies this data transactionally before the
existing one-time seeding operation. Fresh databases record completion without
matching rows and then seed the current defaults. Existing seeded databases
only update exact tuples. A batch advances the shortcut revision once when any
URL changes; IDs, ordering and all other fields remain intact.
