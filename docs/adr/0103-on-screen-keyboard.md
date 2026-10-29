# ADR-0103: On-screen keyboard in the MVP

- **Status:** Accepted
- **Date:** 2026-10-09
- **Deciders:** project owner, answers to DR-39 (a), DR-40 (a) and DR-41 (b) on 2026-10-09 ([#418](https://github.com/LimeBeck/arinn/issues/418), [#420](https://github.com/LimeBeck/arinn/issues/420)); technical details under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** amends [ADR-0051](0051-address-and-search-entry.md) (the on-screen keyboard is no longer post-MVP; suggestions on the address and search screen)
- **Amended by / Superseded by:** amended by [ADR-0105](0105-global-on-screen-keyboard-platform-text-delivery.md) (the keyboard also types into external applications)
- **Specification:** [§5.1](../specifications/arinn-specification.md#51-home), [§6.2](../specifications/arinn-specification.md#62-keys), [§7.1](../specifications/arinn-specification.md#71-browser-chromechromium)

## Context

The MVP expected a hardware keyboard for text input, such as the keyboard of a
keyboard-compatible remote, and called an on-screen keyboard post-MVP
([ADR-0051](0051-address-and-search-entry.md), specification §5.1). Without a
keyboard, the address and search screen (IN-8) and a `string` setting (STR-2)
are dead ends, and web shortcuts can be managed only through the CLI. The
design handoff of 2026-10-09 ([#417](https://github.com/LimeBeck/arinn/issues/417))
adds an on-screen keyboard for every text field (KB-1…8). The
[integration review](../design/integration-review.md) left three questions to
the owner: whether the keyboard enters the MVP (DR-39), how its suggestions
relate to the §7.1 list (DR-40), and whether library search (KB-6) is in scope
(DR-41).

## Decision

- **Scope.** The on-screen keyboard is part of the MVP. Every text field in
  the shell can be filled with the remote alone (arrows, OK, Back): the address
  and search field, the web shortcut address and name, a `string` setting and
  the folder picker path. A hardware keyboard keeps working as before and never
  opens the panel.
- **Design.** The shell follows the handoff section *On-screen keyboard*
  (variant 1a, KB-1…5, KB-7, KB-8). The panel opens only through the keyboard
  button in the field. Back hides the panel before the screen's own Back.
- **Suggestions on the address and search screen.** The keyboard's suggestion
  row replaces the list of ADR-0051. It holds up to four entries: recent
  entries and open services that match the text. Web shortcuts are no longer
  suggested; their tiles stay on Home. The recent-entry rules of ADR-0051 stay:
  up to five stored entries, *Clear* deletes them all, and none of them appear
  in logs or diagnostics. The keyboard row has no *Clear*. While the panel is
  hidden and the field is empty, the screen keeps the list of recent entries
  with *Clear* (handoff V-2b), reached with the remote after Back hides the
  panel. This placement is a delegated reconciliation: the handoff draws
  *Clear* only in V-2b.
- **Library search.** KB-6 is not part of the MVP. The specification gains no
  library search, and `Library1` gains no query method.
- **Architecture.** The keyboard is one shell component used by every field.
  Layouts, input rules and suggestion matching live in the shell's C++ layer or
  the daemon, not in QML business logic (R-01, R-05). It is not a platform input
  method, so it works the same on KDE/Wayland and X11/EWMH. Per-field memory
  (language, last key) never stores the typed text.

## Consequences

- IN-8 and STR-2 are no longer dead ends. The CLI stays a fallback for web
  shortcuts, not the only way without a keyboard.
- The Phase 5 exit criteria gain the keyboard (G-5.6), with AC-02 and AC-18
  extended to it.
- The address and search screen drops its shortcut suggestions. The existing
  implementation ([UI browser](../architecture/ui-browser.md)) changes in the
  keyboard task ([#418](https://github.com/LimeBeck/arinn/issues/418)).
- Open-service suggestions read the open tabs from `Browser1.GetTabs` and match
  them by origin, as the already-open hint does. No contract changes.
- KB-5 suggestions need the folder listing method of the folder picker (DR-27).

## Alternatives considered

- **Keep the keyboard post-MVP (DR-39 b).** Rejected by the owner: the remote is
  the primary input, and the text screens would stay dead ends without it.
- **Combine recent entries, web shortcuts and open services (DR-40 b).**
  Rejected by the owner in favour of the handoff's suggestion row.
- **Library search with a new `Library1` query (DR-41 a).** Not part of the MVP;
  it can return as its own scope and `contract-change` task.
- **A platform input method (Maliit, the Plasma virtual keyboard).** Not chosen:
  it differs per desktop and cannot follow the handoff.

## Verification

- G-5.6 in [ROADMAP.md](../../ROADMAP.md): Qt UI tests of navigation, layers,
  limits, Back/Home/Menu with the panel open and a hardware keyboard typing
  while it is open; the AC-02 arrows-only walk fills each field kind with the
  panel; a visual comparison with `kb-*.png`.
- An operator check with the remote on the target HTPC.
