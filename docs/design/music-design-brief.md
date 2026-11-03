# Music design brief

Status: brief merged in [#695](https://github.com/LimeBeck/arinn/pull/695), 2026-11-01; [visual prototype](prototypes/music/README.md) and local/Yandex backend foundations consolidated in [MR #710](https://github.com/LimeBeck/arinn/pull/710), task [#707](https://github.com/LimeBeck/arinn/issues/707), at the owner's request on 2026-11-02. Layout B is selected; full visual acceptance and service feasibility remain pending. Parent
[#677](https://github.com/LimeBeck/arinn/issues/677), discovery task
[#694](https://github.com/LimeBeck/arinn/issues/694).
The owner authorized implementation of local music and Yandex in this unified
MR; this brief does not establish completion or support for other candidates.
[Feasibility and proposed requirements](../architecture/music-discovery.md)
define the candidate-source limits. Existing [UI components](../architecture/ui-components.md)
remain the visual baseline; historical handoff assets are unchanged.

## Intended experience

Browse music and start playback inside Arinn using the remote. Reuse one screen
language across sources while preserving source attribution, supported actions
and honest availability. All five requested sources remain candidates; no
connected accounts, service artwork or playback are simulated as working support.

Proposed information architecture: a Music entry in global navigation, separate
from the existing film/series Library. Its initial view remembers the last source
and offers a source selector, Search, and supported Artists, Albums, Playlists and
Tracks facets. No recommendation feed, merged catalogue or cross-service queue
is assumed. The navigation entry and these defaults require owner review.

## Screens to design

| Screen | Content and actions | Remote/focus and states |
| --- | --- | --- |
| Music / source picker | Five candidates with connection/availability status; a source opens its catalogue or setup explanation | Restore last selected source; disconnected/blocked entries explain the next step rather than showing an empty catalogue |
| Source catalogue | Source title/attribution, search, supported facets; album/playlist artwork grid and track lists | Arrows move locally; paging preserves focused entity. Missing artwork uses an existing neutral placeholder |
| Search | Selected source, query field and result facets; keyboard through existing shell input | Back closes keyboard before leaving search. Retain query/scroll/focus on return; late replies do not move focus |
| Artist | Name, artwork when available, releases and tracks where supported | No fabricated biography/top tracks; unsupported facets stay absent |
| Album / playlist | Artwork, title, creator/source, explicit Play and ordered track rows | Opening detail never starts playback. Unavailable tracks stay readable with reasons; actions reflect capabilities |
| Now playing | Artwork, title/artist/album, source/device, confirmed transport state, seek and queue entry | A stable transport focus order; controls without capability are absent or disabled with an explanation |
| Queue | Current entry and ordered upcoming entries; supported play-next/add/remove/reorder actions | Menu opens an action sheet; Back restores the exact opener. Never offer mutations an upstream queue cannot confirm |
| Connection / settings | Connect, cancel, reconnect, disconnect; local-root selection and scan status | Never request a password/token in the ordinary music UI. Yandex uses the owner-requested CloudMus-style device-code flow; show verification link, user code, expiry and waiting/cancel/retry states. Other providers need their own qualified flow; QR support is not assumed |
| Home / Continue music card | Current artwork, track, artist, source/station and playing/paused status; separate Play/Pause icon | Opening the card returns to the same player/queue without restarting. The transport action preserves Home focus |

Illustrative structure for visual exploration, not a pixel specification:

```text
Music                                      [Source: Local] [Search]
[Artists] [Albums] [Playlists] [Tracks]

Album artwork / playlist grid, or ordered track list
Title                           Artist / album               Time
...

Now playing: Track / Artist        [Previous] [Pause] [Next] [Queue]
```

The compact player appears only with a real session. Selecting its artwork/title
opens Now playing; opening another source changes browsing only. If Play would
replace another source's session or queue, show the effect explicitly and offer
Cancel with safe initial focus. Never imply mixed-source queue support.

## Interaction rules to illustrate

Owner-directed refinement: use icons for Play/Pause, Previous/Next, seek and
Queue, including the compact player and Home card, rather than English transport
words. Keep accessible names, visible keyboard/remote focus and focused-control
labels; the Play/Pause icon and accessible action name change with confirmed
state. Like/dislike icons retain distinct selected states. Descriptive navigation,
station titles and recovery actions keep readable text.
Place player actions in one horizontal row, grouped by transport, queue/seek and
feedback/radio, while keeping the Play/Pause action prominent. Do not stack the
compact icon groups into separate rows at the 1080p/4K target sizes.
The owner requested several alternatives before selecting a final arrangement;
the [control-layout comparison](prototypes/music/control-layouts.md) proposes a
linear row, balanced groups with centered transport, and a minimal row with an
overflow sheet. These are review alternatives, not three permanent user settings.
The owner selected **B: balanced groups with centered transport** on 2026-11-01.
Use feedback on the left, Previous/Play-Pause/Next centered in the information
column, and Queue/Seek/Track radio on the right. The prototype now uses B by
default; A and C remain comparison references. This visual decision does not
qualify provider integrations or close the implementation tracker.

The proposed Home music card belongs to the existing Continue row and follows its
400×225 content-card geometry. Artwork, track and artist identify current music;
source and station distinguish My Wave from an album or ordinary playlist. A
separate icon action toggles Play/Pause, while opening the card returns to the
same session. This is a proposed Home projection, not a production Activity or
favorite-station contract. The prototype has playing and paused captures; adding
a station shortcut to Favorites remains a separate owner-reviewed scope choice.

- Back dismisses the topmost sheet/keyboard, then returns through detail to
  catalogue, restoring focused entity and scroll. At Music root it returns to
  Home under existing navigation rules. Home shows Home without destroying a
  supported live music session. A provider requiring a visible player must have
  a separately reviewed presentation; do not design hidden YouTube playback.
- Media keys act on the selected/active media target through daemon routing.
  Volume uses the existing system flow. Browsing a different source never
  silently changes the active target. Resolve concurrent sessions in contract
  review rather than introducing title/product-based priority.
- As lists change, preserve focus by entity identity; if it disappears, move to
  the nearest surviving item or the screen's recovery action. A scan update,
  artwork load or playback-state change must not steal focus.
- Show unsupported controls distinctly from temporary failures. Pending commands
  have visible progress and cannot be repeated accidentally; display confirmed
  playback/queue state rather than optimistic success.

## Personal selections and continuous radio

Owner-directed addition on 2026-11-01 after reviewing
[CloudMus](https://github.com/cloudmus/cloudmus): design provider-owned discovery
alongside ordinary music-library browsing. Add a For you source landing with
personal playlists and a distinct station treatment: My Wave, personal mixes and
radio from a track where the provider supports them. A daily playlist remains
finite unless its provider explicitly reports a continuous session.

Show the station name on Now playing and Queue. A continuous queue displays the
loaded upcoming window and the promise of more tracks, not a total track count
or duration for the station. Cover normal replenishment, loading, an empty buffer,
failure/retry, stopping, and replacement with Cancel initially focused. Current
playback survives a refill failure; late batches from a stopped/replaced station
must not reappear. Do not offer whole-queue shuffle/repeat or edit actions merely
because ordinary playlists have them.

Track menus expose Radio from this track only with the relevant capability.
Pin/unpin stations for quick access. Like and dislike are separate account
actions with distinct selected, pending and failure states; a dislike need not
cut off the current track, but may change upcoming recommendations. Display
service-confirmed state in production; the prototype only explores local fixture
transitions. Keep playback start, natural completion and explicit skip distinct
when preparing the future contract; service-specific feedback stays in providers.

The [prototype](prototypes/music/README.md) includes explicit synthetic Yandex
and YouTube radio review scenes. Normal source selection still shows unresolved
availability and never grants a connection or playback capability. The research
report's MU-11–MU-15 capture these proposed requirements. Production service
permission, account integration, persistent pins and feedback sync are untested.

## Required state sheets

Visual designs must cover loading, empty catalogue, no search results, missing
artwork, partial metadata, scanning, missing root/mount, unreadable track,
unsupported codec, account required, connection pending/cancelled, expired access,
subscription required, restricted/preview-only track, no playback device,
rate limit, offline service, provider restart and stale queue. Keep public
explanations actionable; no raw API errors, file URIs or credentials on screen.
Error recovery stays inside the current focus scope and does not erase results
from other sources or a live session.

## Visual deliverables and review

Produce editable 1080p and 4K layouts using the existing theme, typography,
spacing, focus and motion rules. Use synthetic artist/album/track names and
placeholder artwork. Include first use, local playback, service connection,
search-to-detail-to-play, queue edits, replacing a playing source and degraded
states. Provide a clickable remote-navigation prototype and a screen/state map.
Brand attribution and links must be checked per provider; a common layout cannot
remove required attribution or resolve service-policy restrictions.

Owner review should decide:

1. Music as a separate navigation entry and source-scoped search/queue for the
   first increment.
2. Local library as the first qualified playback source; connected candidates
   remain subject to the evidence/decisions in the discovery report.
3. Read-only album/playlist browsing first versus playlist authoring/import;
   queue mutations remain capability-dependent.
4. Song restart/resume defaults and whether queue persistence across restart is
   required in the first increment.

No design approval, pixel comparison, remote prototype test or real-desktop
qualification has been performed for this brief. After review, record decisions
in the living specification before contract and shell implementation; leave
unresolved provider routes explicitly blocked in #677.
