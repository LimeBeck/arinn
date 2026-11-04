# ADR-0140: Visual decisions from the TV design review

- **Status:** Accepted
- **Date:** 2026-11-04
- **Deciders:** Owner, answers to the open points of the TV design review [#756](https://github.com/LimeBeck/arinn/issues/756) on 2026-11-04 (Claude session). The implementation details are delegated technical choices ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)).
- **Amends / Supersedes:** amends [ADR-0136](0136-hearth-light-visual-direction.md) (the Hearth light brief) and [ADR-0027](0027-activity-background-snapshot.md) (the light over an Activity background)
- **Amended by / Superseded by:** —
- **Specification:** §5.1, §15

## Context

The review of the running shell on the owner's TV ([#756](https://github.com/LimeBeck/arinn/issues/756)) found:
- two places where the [Hearth light brief](../design/briefs/hearth-light/README.md) contradicts itself or an earlier decision;
- two places where the handoff's own values produce a visible defect on a real screen.

The brief and the handoff are historical sources and are not edited. The owner answered each point. This ADR records the answers so the implementation tasks ([#757](https://github.com/LimeBeck/arinn/issues/757), [#761](https://github.com/LimeBeck/arinn/issues/761), [#763](https://github.com/LimeBeck/arinn/issues/763)) have one source.

## Decision

1. **The floor over an Activity.** While an Activity background is shown behind Home, the light field stays on. The floor takes the Activity's identity hue instead of the warm hearth hue, as the brief's *Home* section says. Two existing rules still apply:
   - *Light field* off turns the floor off here too;
   - the pool still follows focus.

   This replaces the earlier rule that switched the light field off over an Activity background ([ADR-0027](0027-activity-background-snapshot.md), `ui-components.md`).
2. **The near state keeps the application's controls under the shell.** When the Activity card has focus (near: scale .95, blur 8, brightness .6), two dark gradient bands lie over the Activity: one along the top 180 px and one along the bottom 140 px. Otherwise the application's own top bar and bottom controls read through Home's header and lowest row. The far state is unchanged.
3. **Library tabs.** A focused tab is a white pill with ink text. The selected tab is marked by an Ember dot under its label. This follows the brief's *Library* section; its *Shape roles* table, which puts tabs in the container role, does not apply to them.
4. **On-screen keyboard context rows.** The address and path context rows ([KB-1](../design/handoff/README.md), KB-5) do not repeat a character that the letter rows already have. `:`, `.` and `-` leave the context rows; the multi-character keys (`https://`, `www.`, `.ru`, `.com`, `.local`, `/`, `~/`, `..` and the mount prefixes) stay.

## Consequences

- `HomeView` keeps the light field lit over an Activity background and passes the Activity's identity colour to the floor (`AmbientGlow.floorTint`). `ActivityBackground` draws the near bands.
- `ui-components.md` and `activity-background.md` describe the new rules; tests cover the floor tint and the bands.
- The Library tabs and the keyboard rows change in their own tasks ([#761](https://github.com/LimeBeck/arinn/issues/761), [#763](https://github.com/LimeBeck/arinn/issues/763)).
- The bands are dark on purpose and hide part of the receded application near the edges, the price of a readable header.

## Alternatives considered

- Keep the light field off over an Activity (ADR-0027): rejected by the owner.
- Always use the far state, without near: rejected by the owner; the near state tells which card returns to the application.
- Container role for Library tabs (brief, *Shape roles*): rejected by the owner.
- Keep the duplicated keys as drawn in the handoff: rejected by the owner.

## Verification

- `tests/ui/tst_home.qml` (`test_floor_and_bands_over_an_activity`) checks the floor tint and the near bands.
- The Library and keyboard tasks carry their own tests.
- A TV check with the owner after deployment. Accepting this ADR does not mean that check has passed.
