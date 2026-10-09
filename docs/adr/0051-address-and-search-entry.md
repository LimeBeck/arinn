# ADR-0051: Address and search entry for free browsing

- **Status:** Accepted
- **Date:** 2026-09-29
- **Deciders:** project owner, explicit choices V-1b, T-1b, V-2b and V-3b and the search engine policy in the session of 2026-09-29 ([issue #106](https://github.com/LimeBeck/arinn/issues/106), [issue #107](https://github.com/LimeBeck/arinn/issues/107)); technical details under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** amends [ADR-0043](0043-web-shortcuts.md) (web shortcuts are no longer the only stored URLs), [ADR-0048](0048-browser-wire-contract.md) (the shell may open a typed address or search through the daemon) and [ADR-0050](0050-window-per-browser-activity.md) (the generic browser entry)
- **Amended by / Superseded by:** [ADR-0066](0066-address-and-search-contract.md) — the `Browse1` wire contract and packaged search engines; [ADR-0103](0103-on-screen-keyboard.md) — the on-screen keyboard in the MVP and its suggestion row
- **Specification:** [§5.1](../specifications/arinn-specification.md#51-home), [§7.1](../specifications/arinn-specification.md#71-browser-chromechromium), [§11](../specifications/arinn-specification.md#11-state-ownership-and-persistence)

## Context

[ADR-0050](0050-window-per-browser-activity.md) made every browser activity a
managed fullscreen window, but kept the *Browser* tile as an ordinary Chrome
window with tabs and an address bar, the only place the browser UI appears on
the TV. The owner asked for free browsing to go through Arinn as well
([#106](https://github.com/LimeBeck/arinn/issues/106)). The design handoff of
2026-09-29 answers the brief with an address and search screen (IN-1…9), a
renamed tile (T-1), an entry from *Open services* (OS-8) and paired variants
for the owner decisions (V-1…3). [ADR-0048](0048-browser-wire-contract.md) gives
the shell no generic open-URL method, and [ADR-0043](0043-web-shortcuts.md)
says web shortcuts are the only URLs Arinn stores.

## Decision

- **Entry screen.** The Home tile *Site or search* (T-1b, Russian UI label in the handoff) opens a separate
  screen where the user types an address or a search query on a hardware
  keyboard. Enter opens the result as a managed window with all ADR-0050 rules:
  an already open origin is focused, not duplicated. *Open services* offers the
  same screen through a button (OS-8).
- **Input rules.** `http` and `https` are used as typed. An IPv4 address,
  `localhost`, a `*.local` host or any host with a port gets `http://`. A dotted
  host with a TLD of two or more letters and no spaces gets `https://`. Any
  other scheme is rejected. Everything else is a search. The rules live in the
  daemon or the shell's C++ layer, never in QML business logic (R-01, R-05).
- **Search engine.** Search opens the results page of the configured engine,
  which is a URL template in the configuration, selectable in Settings. The
  default is Google. No engine name or URL appears in core code or QML business
  logic (R-01).
- **Shortcut suggestions.** While typing, up to three matching web shortcuts are
  offered. They are user configuration, not history.
- **Recent entries (V-2b).** Arinn keeps up to five recent entries (addresses or
  queries the user confirmed with Enter) in its state, shows them when the field
  is empty, and offers *Clear*, which deletes them all. Like web shortcuts, they
  MUST NOT appear in logs, diagnostics or crash reports. Browser tab titles,
  URLs and browsing history are still not archived.
- **Ordinary browser window (V-1b).** The entry screen keeps a secondary action
  that opens an ordinary browser window with tabs. ADR-0050 does not manage that
  window.
- **Save as shortcut (V-3b).** A row in *Open services* can be saved as a web
  shortcut through the existing web shortcut API. The ADR-0043 and ADR-0045
  validation applies.
- **Contract.** Opening a typed address or search needs a daemon method in
  place of the web-shortcut-only path of ADR-0048. It is defined in a separate
  `contract-change` task. Until then the screen is not implemented.
- **Scope.** This is not part of the MVP acceptance criteria until the roadmap
  assigns it to a phase.

## Consequences

- The browser UI no longer appears on the TV by default. It remains reachable
  for sites that need it, such as account sign-in.
- Arinn stores a small, user-clearable list of recent entries. The persistence
  schema and the privacy rules gain one more item.
- The shell needs one more contract method and a settings entry for the search
  engine.
- Without a hardware keyboard only shortcuts can be opened from this screen
  (IN-8). An on-screen keyboard is post-MVP.

## Alternatives considered

- **Remove the ordinary browser window (V-1a).** Rejected by the owner: some
  sites and sign-ins need it.
- **No recent entries (V-2a).** Rejected by the owner in favour of a short,
  clearable list.
- **No saving from Open services (V-3a).** Rejected by the owner.
- **One fixed search engine.** Rejected by the owner: the engine is selectable,
  default Google.
- **Search suggestions from the engine.** Not considered: they need network
  requests from Arinn.

## Verification

- The contract-change task defines the open method and its validation tests.
- The implementation adds unit tests for the input rules, persistence tests for
  recent entries including *Clear*, and log checks that entries never reach
  logs.
- Qt Quick Test covers the screen states against the handoff (IN-1…9).
  Accepting this ADR does not mean these checks have passed.
