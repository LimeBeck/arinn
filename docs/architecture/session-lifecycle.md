# Lock, sleep and graphical session shutdown

The shell listens to `org.freedesktop.ScreenSaver.ActiveChanged` on the session bus and `org.freedesktop.login1.Manager.PrepareForSleep` on the system bus. On lock or before sleep the window is hidden regardless of the daemon state; the Activity context is kept. After wake the shell first re-checks the lock state; an old `GetActive` reply cannot override a newer signal. If ScreenSaver is unavailable, the state counts as locked until confirmed otherwise.

The Phase 0 transient units have `PartOf=graphical-session.target` and `After=graphical-session.target`: stopping the graphical session stops the daemon, demo and shell. Stopping the whole desktop target was not tested.

## Verified

A private-bus Qt Test uses real D-Bus subscriptions to fake ScreenSaver and login1 services: lock/unlock, sleep/wake, unlock during sleep, ScreenSaver owner disappearing and returning. The full IPC/input suite passes. On an unlocked KDE session `blocked=false`, 20 Chrome Home/Return cycles and the actual `PartOf`/`After` properties of all three units were checked. This verifies the reaction to events, not physical suspend or the compositor lock screen.

## Deferred by the project owner

On 2026-09-27 the project owner chose to run the real checks later. Real lock, suspend/wake and logout were not executed. These acceptance items stay open (Phase 5, AC-19); this is not a missing product decision.

For an operator-approved lock/unlock check there is `tests/integration/kwin_lock.py`: open Home over a disposable Activity first, then run the test. It **locks the real screen** and waits up to 180 s for a manual unlock. It checks the `org.arinn.SpikeSession1.blocked` property and that the return target is kept. The operator visually confirms the shell is not shown over the lock screen. Suspend is checked separately through the regular KDE menu without bypassing inhibitors; the test suite never suspends or logs out automatically.

A complete MVP power action still needs inhibitor/refusal reporting and provider recovery after wake. `SessionGuard` does not implement the suspend command itself.
