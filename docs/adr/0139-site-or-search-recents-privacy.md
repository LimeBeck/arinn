# ADR-0139: Site or search recents keep no query and no loopback address

- **Status:** Accepted
- **Date:** 2026-11-04
- **Deciders:** Owner, an answer to the TV design review [#756](https://github.com/LimeBeck/arinn/issues/756) C28 on 2026-11-04 (Claude session). The rule's implementation details are delegated technical choices ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)).
- **Amends / Supersedes:** amends [ADR-0066](0066-address-and-search-contract.md) (what `Browse1.Open` records as a recent entry)
- **Amended by / Superseded by:** —
- **Specification:** [§7.1](../specifications/arinn-specification.md#71-browser-chromechromium), [§11](../specifications/arinn-specification.md#11-state-ownership-and-persistence), [§13](../specifications/arinn-specification.md#13-security)

## Context

ADR-0066 records the typed text of every `Browse1.Open` as a recent entry, up to five, in `state.sqlite3`. On the owner's TV the *Site or search* screen listed a loopback address with a `secret=` query parameter ([#756](https://github.com/LimeBeck/arinn/issues/756) C28):
- a full URL keeps whatever a page put in its query or fragment, such as tokens, session ids and search terms of the site;
- loopback addresses are local services, callbacks and development pages, not places to return to from the sofa.

Specification §13 keeps secrets out of storage and logs.

## Decision

The daemon reduces the typed text before it records it (`arinnd::storage::browse_recent_entry`):

- **Address** (the text resolves to `address`, ADR-0066):
  - the entry is the typed text up to its first `?` or `#`, trimmed, so `example.org/watch?v=1#t` is kept as `example.org/watch`;
  - an address whose host is loopback is not recorded at all: `localhost`, `*.localhost`, `127.0.0.0/8`, `0.0.0.0`, `::1` and `::`.
- **Search:** the text is kept as typed. A search is not an address and has no query of a site.
- **Invalid text** is not recorded, as before.
- Text that may not be kept leaves the list unchanged and emits no `RecentChanged`.
- **Migration:** schema 8 of `state.sqlite3` rewrites the entries stored before this rule by the same function. It goes oldest first, so an entry that two old entries reduce to keeps the newer position. Entries that may not be kept are deleted.
- The wire contract does not change: `Open`, `GetRecent`, `ClearRecent` and `RecentChanged` keep their signatures and meaning, and `Open` still opens the full address.

## Consequences

- A recent entry that had a query reopens the page without it. That is the price of not storing it.
- A LAN host such as `192.168.1.20:8096` is still recorded; only loopback hosts are dropped.
- A daemon from before this ADR cannot open a schema-8 state database (it refuses unknown schema versions, as for every earlier migration).

## Alternatives considered

- **Keep full URLs and only skip loopback.** Rejected by the owner: a query or fragment can carry a token on any host.
- **Record nothing for addresses with a query.** Rejected: the page itself is still a useful recent entry.
- **Reduce in the shell.** Rejected: the daemon owns the recents and the input rules (ADR-0066), and the rule must also clean existing entries.

## Verification

- `crates/arinnd/tests/storage.rs`:
  - `browse_recents_drop_queries_and_loopback_addresses` covers new entries and the schema 7 → 8 migration;
  - `browse_recent_entry_rule` covers the reduction rule.
- `crates/arinnd/src/runtime/browse_tests.rs` (`open_records_recent_entries_newest_first`) covers the same rule through `Browse1.Open`.
- Accepting this ADR is not a TV check.
