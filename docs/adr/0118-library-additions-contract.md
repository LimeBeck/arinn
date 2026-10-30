# ADR-0118: Optional provider-owned library discovery times

- **Status:** Accepted
- **Date:** 2026-10-29
- **Deciders:** delegated technical choice, task [#582](https://github.com/LimeBeck/arinn/issues/582); the owner explicitly requested recently added library films and series on Home in chat on 2026-10-29
- **Amends / Supersedes:** extends [ADR-0054](0054-media-and-library-wire-contract.md)
- **Amended by / Superseded by:** —
- **Specification:** §5.1, §7.4, R-04, R-05, §13

## Context

Home's fixture row has no production source. The library contract has neither
provider-owned discovery times nor a reliable ordering for recent additions.
File modification times, titles, release years and the shell's first observation
cannot establish when a provider added content. Existing item signatures must
remain compatible with providers that do not supply this optional metadata.

## Decision

Add optional `org.arinn.LibraryAdditions1` on a library provider's standard
path. `Features` advertises `LibraryAdditions1`. Its read-only
`GetAdditions(generation, revision)` returns `(generation, revision, a(id,
added_at_us))`, signature `(sta(st))`. The revision is the **catalogue** revision,
not an independent metadata revision. A mismatched request returns `Conflict`.
The existing catalogue `StateChanged` announces additions changes.

At most 64 unique, available, top-level `film` or `series` items are returned.
Each timestamp is a positive Unix microsecond value no greater than `i64::MAX`.
The provider selects its latest known entries; equal timestamps use ascending
opaque item IDs. Unknown discovery times may be omitted. A film's time is its
first discovery; a series' time is its newest episode's first discovery.
Rescans, metadata edits, playback and temporary unavailability do not reset these
times. Provider persistence and the product projection land separately.

The SDK's `.additions()` extension enables the interface.
`replace_with_additions` validates and publishes metadata and the matching
catalogue atomically. A metadata-only change advances the catalogue revision;
an unchanged replacement stays silent. The existing `replace` publishes an
empty additions list, preventing stale references after a catalogue replacement.
All provider reads require the current registered daemon owner.

The daemon imports the optional metadata with a 500 ms deadline after reading
the catalogue. It validates the generation, revision, bounds, uniqueness and
item references. Missing, unsupported, stale or malformed metadata leaves the
base catalogue usable and makes additions unavailable. The shell-facing
`Library1.GetAdditions(generation, revision)` returns the validated snapshot,
`Unsupported` without the feature, `Conflict` for a stale request and
`Unavailable` without validated metadata. No file URI or filesystem path crosses
this API. Provider departure clears the cached additions.

## Consequences

Old providers and Item/LibraryPage wire signatures are unchanged. Consumers can
merge provider times without inferring content identity or reading files. The
64-entry bound is a recent projection limit, never a library capacity limit.
The Home feature still requires provider persistence and a shell binding; this
prerequisite does not claim the visible row is implemented.

## Alternatives considered

- Changing Item's signature would break existing providers.
- Shell-owned first-seen times depend on when the shell runs and duplicate
  provider-owned catalogue persistence.
- File modification times change during copies and metadata updates.

## Verification

Protocol validation and round trips, SDK private-bus authorization and atomic
revision changes, daemon optional/stale/malformed metadata routing and exported
XML checks. Required repository checks and an independent agent review are
recorded on the pull request; this decision is not a claim that they passed.
