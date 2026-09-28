<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Power and ambient overlay

Fixture presentation for [specification §5.7](../specifications/arinn-specification.md#57-system-ui-power-and-ambient)
(P-04). This is not the platform power path and does not meet AC-19.

`apps/arinn-shell/ui/screens/power/PowerMenu.qml` is a power-scope overlay.
Tests import that directory from the filesystem (`tests/ui/tst_power.qml`).
The existing `ui-components` CTest loads every `tests/ui/tst_*.qml` file, so
this suite needs no CMake change. It is not wired into `HomeView` or
`Main.qml`. `HomeView` still only emits `powerRequested()`.

The overlay imports `ActionButton`, `Surface`, `StatusPane`, `UiText` and
`Theme` from `apps/arinn-shell/ui`. It does not modify those components.

## What the fixture does

| Action | Behaviour |
| --- | --- |
| Sleep | Emits `sleepRequested()`. Does not call systemd, logind or suspend, and does not open the ambient screen. |
| Restart, Shut down | Open a confirmation. Cancel has the default focus. The confirm control is destructive. Accepting emits `restartRequested()` or `shutdownRequested()` and leaves the confirmation open. |
| Closing applications | `closingApplications`, an explicit list of strings or `{title}` records. Empty means no running applications. This file does not read a live registry. |
| Refusal | `refusalMessage` shows `StatusPane` kind `error`. The overlay never sets that property and has no timer that reports success. |
| Ambient | `startAmbient()` and the Ambient row emit `ambientRequested()` and show a full-screen state. There is no idle timer. |
| Wake | The first key or pointer event while ambient is armed is accepted, emits `wakeRequested()`, and does not activate Sleep, Restart, Shut down or Ambient again. |
| Back, Home | Back closes a confirmation onto its opener. Home closes a confirmation without a destructive intent and emits `homeRequested()`. Focus stays inside the overlay. |

Local focus is trapped in this overlay (power above the screen): arrows and Tab
do not leave it. The confirmation is a step inside the same scope. Back on that
step matches the power-confirmation row of specification §6.3.

Session lock, OS suspend and the idle ambient timer are out of scope. Lock and
sleep observation stay in the [session lifecycle](session-lifecycle.md) guard,
which does not execute power commands. Production must later supply the closing
list from the live registry and must report a real platform refusal through
`refusalMessage` ([activity and interaction](activity-and-interaction.md)).

## Verification

```sh
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
  build/apps/arinn-shell/arinn-ui-test -input tests/ui PowerMenu
```

Operator tests were not run. This fixture does not lock the screen or suspend
the machine.
