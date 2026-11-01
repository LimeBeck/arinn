# ADR-0125: Optional lazy library descriptions

- **Status:** Accepted
- **Date:** 2026-11-01
- **Deciders:** delegated technical choice under ADR-0037, task #653; owner requested local NFO descriptions and a single combined PR
- **Amends / Supersedes:** extends ADR-0054
- **Amended by / Superseded by:** —
- **Specification:** §5.3, §7.4, §11, §13, §15; AC-02, AC-12, AC-15

## Context

Library details can render descriptions, but Item and LibraryPage have no plot
field. Changing their signatures would break existing providers. Importing plots
for every item on every catalogue poll would add avoidable traffic and memory.
The owner selected an offline NFO increment of #242, with contract, provider and
shell integration in one PR. Online services remain outside this decision.

## Decision

Add optional feature `LibraryDescriptions1` and provider interface
`org.arinn.LibraryDescriptions1.GetDescription(generation, revision, id)`. The
same method is exposed on daemon `Library1`. It returns
`(generation, revision, id, text)`, signature `(stss)`. Text is plain UTF-8, at most
8192 bytes, without NUL; empty means no known description. Generation/revision
are the base catalogue pair. Unknown items return NotFound; a different pair
returns Conflict. Missing support returns Unsupported. Existing Item and page
signatures are unchanged. Paths and playback locations are not added to the API.

The SDK enables the extension with `.descriptions()` and publishes a bounded
map in `LibraryMetadata` through `replace_with_metadata`. Description, additions,
hierarchy and catalogue updates are atomic. All map keys must reference current
items; ID/text bytes total at most 16 MiB. Description-only edits advance the
catalogue revision; identical publication stays silent. Legacy replacement
methods clear descriptions to prevent stale metadata. Only the registered daemon
owner may read the provider interface. Re-registration binds the same published
metadata to the new catalogue generation.

The daemon reads one requested description under a 500 ms deadline and validates
the response bounds, identity and pair. It rechecks the provider owner and
catalogue pair after the await. A failure affects only that optional lookup, never
the base catalogue or playback. No library-wide description polling/cache is added.

The local provider reads `<plot>` from the video's matching `.nfo`, and from
`tvshow.nfo` for a series. Existing bounded local-NFO reads reject DTDs/entities
and sidecars resolving outside the root. XML text, numeric/named entities and
CDATA become plain text; unknown entities are retained literally. Description
text is bounded at a UTF-8 boundary. Missing/invalid/empty plots clear the known
value on the next readable scan. Unavailable roots preserve previous metadata.
Descriptions use a provider-owned SQLite table keyed by existing item IDs;
unchanged descriptions do not write updates. Optional publication is capped at
16 MiB in deterministic ID order, leaving the base catalogue usable even when
not every description fits. Metadata edits do not alter content IDs, discovery
times or resume rows.

The shell lazily requests the opened detail entry and keeps only its description.
Requests are asynchronous and selected-entry, daemon-epoch and revision fenced.
Catalogue changes invalidate the selected description; navigation ignores late
responses. The existing three-line detail treatment is retained. Qt renders
plain text, never provider-supplied rich text. No provider paths or online calls
are introduced into QML.

## Consequences

Old providers retain the same UI fallback. Local plots already stored in NFO can
be displayed without a metadata-service account. Empty or unavailable optional
text does not block navigation, artwork, episode scrolling or playback.
The broader #242 remains open for separately approved enrichment work.

## Alternatives considered

- Extending Item breaks existing wire consumers.
- Bulk description imports duplicate potentially large data during polling.
- Reading NFO in the shell crosses the provider boundary and leaks file paths.
- Fetching from an online service requires decisions not part of this increment.

## Verification

Protocol limits/round trips; SDK authorization and atomic revision changes;
daemon optional/stale/malformed/timeout/race cases; provider NFO refresh and
identity/resume persistence; Qt lazy reads, stale replies and plain-text display.
Results and independent review are recorded on the PR, not assumed by this ADR.
