# ADR-0138: Launched applications go full screen

- **Status:** Accepted
- **Date:** 2026-11-04
- **Deciders:** Owner, answer to the TV design review [#756](https://github.com/LimeBeck/arinn/issues/756) finding A2 on 2026-11-04 (Claude session). The platform command and the window choice are delegated technical choices ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)).
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §7.2

## Context

On the owner's TV a desktop application started from Applications opened as an ordinary desktop window, with its title bar and menus, on top of an unrelated browser window. Two sources disagreed:
- The design handoff (*Activity*) says the external application goes full screen after the launch overlay.
- Specification §7.2 said nothing about it.

Before this decision, the daemon only asked the KWin adapter to hide Home after a launch (`dismiss`). Only the managed media player (ADR-0074) and managed browser windows (ADR-0050) were full screen.

## Decision

- Arinn makes the window of a launched application full screen. Owner decision on 2026-11-04.
- **Exceptions:**
  - Managed browser windows keep following ADR-0050.
  - A browser opened as an application (the provider's Online group, for example *Open … with tabs*) opens as an ordinary window with its own UI (§7.1, `phase-3-runbook.md` AC03-5).
- **Daemon.** After a successful `launch`:
  - it queues the platform command `launched` for an application outside the Online group;
  - for an Online-group application it queues `dismiss`, as before.

  The group comes from the provider's catalog, which the daemon already holds. No contract changes.
- **KWin adapter.** `launched` hides Home, as `dismiss` does. It then sets `fullScreen` on the launched window and activates that window. The launched window is:
  - the active window if it is an ordinary application window;
  - otherwise the newest ordinary application window that was mapped in the last 120 s.

  The managed media player and windows that existed before the adapter started are never chosen. With no such window nothing becomes full screen.
- The window's own decision stands: an application that leaves full screen is not forced back.

## Consequences

- Applications fill the TV, as in the handoff. Desktop applications that expect a normal window show their menus inside a full-screen surface. The user leaves through Home, as from any Activity.
- The X11 path has no adapter command for this yet. The rule is implemented for the KDE adapter only.
- Tests:
  - `tests/integration/kwin_adapter.test.cjs`: the launched window becomes full screen; existing windows are untouched.
  - `platform/kde/window_tests.rs`: `launched` for an application and `dismiss` for a browser.

## Alternatives considered

- Only applications flagged in the catalog: rejected by the owner ("always full screen").
- Leave windows as they open: rejected by the owner.
- A provider-supplied full-screen flag: needs a contract change and gives no benefit while the rule is "always".

## Verification

- The tests above.
- A real-desktop check of a launch on the owner's TV is an operator test that needs the owner's permission. Accepting this ADR does not mean it has passed.
