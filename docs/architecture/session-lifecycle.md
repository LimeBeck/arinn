# Lock, sleep and graphical session shutdown

The platform session adapter listens to `org.freedesktop.ScreenSaver.ActiveChanged` on the session bus and `org.freedesktop.login1.Manager.PrepareForSleep` on the system bus. On lock or before sleep the window is hidden regardless of the daemon state; the Activity context is kept. After wake the shell first re-checks the lock state; an old `GetActive` reply cannot override a newer signal. If ScreenSaver is unavailable, the state counts as locked until confirmed otherwise.

The Phase 0 transient units have `PartOf=graphical-session.target` and `After=graphical-session.target`: stopping the graphical session stops the daemon, demo and shell. Stopping the whole desktop target was not tested. The packaged session units use the same binding and are additionally `PartOf=arinn-session.target` ([ADR-0042](../adr/0042-opt-in-session-autostart.md)); logout with them is an operator test pending AC-01.

## Boundary and implementation status

The owner approved daemon-independent observation in [ADR-0041](../adr/0041-independent-session-lifecycle-guard.md) on
2026-09-28. G-1.4 applies to application/provider integration through Daemon1;
this observer is a narrow session-lifecycle exception. It does not replace the
system lock screen, control applications or execute suspend/power commands.
Platform-specific transport belongs in the platform layer under R-05. The
implementation now lives in
[`platform/session`](../../platform/session/sessionlifecycle.h) ([task #29](https://github.com/LimeBeck/arinn/issues/29)).
The shell constructs the platform observer through `createSessionLifecycle` and
consumes only its `blocked` property and `changed` signal. The public Qt interface
contains no native bus details; `DbusSessionLifecycle` owns ScreenSaver/login1
subscriptions, asynchronous queries and stale-reply epochs. The existing
`org.arinn.Session1.blocked` shell export and QML visibility binding are preserved.
The observer executes independently of the application's daemon transport.

## Verified

The production shell constructs `StartupHome` after loading its window. It waits
for the independent lifecycle guard to confirm an unlocked session, then sends
the existing `Daemon1.Home` command asynchronously. Unavailable daemon/platform
responses retry once per second with at most one outstanding request. A successful
acceptance ends the startup intent for that shell process; subsequent state
refreshes, daemon reconnects and lock/unlock never replay it. Visibility continues
to follow daemon state and the lifecycle guard, without a synthetic Home result.
This implements the startup Home requirement of AC-01; actual login/TV acceptance
remains an operator test. `shell-startup-home` launches the production executable
on private session/system buses with an offscreen display and synthetic daemon
and ScreenSaver services to test startup, delayed platform readiness, locked and
unknown initial states, and absence of later replay.

The `session-lifecycle` CTest runs on a dedicated private bus and uses real D-Bus subscriptions to fake ScreenSaver and login1 services: lock/unlock, sleep/wake, unlock during sleep, ScreenSaver owner disappearing and returning. It also covers absent/unknown lock state, stale unlocked replies after lock,
owner loss or sleep/wake, and production window hiding with Daemon1 absent or
owning its name without replying. Wake keeps the window hidden until a fresh
lock query completes. These fixtures never connect to the real system bus.
The IPC/input suite is verified separately against the demo provider.

Historical Phase 0 evidence (not rerun for the extraction): on an unlocked KDE session `blocked=false`, 20 Chrome Home/Return cycles and the actual `PartOf`/`After` properties of all three units were checked. This verifies the reaction to events, not physical suspend or the compositor lock screen.

## Deferred by the project owner

On 2026-09-27 the project owner chose to run the real checks later. Real lock, suspend/wake and logout were not executed. These acceptance items stay open (Phase 5, AC-19); this is not a missing product decision.

For an operator-approved lock/unlock check there is `tests/integration/kwin_lock.py`: open Home over a disposable Activity first, then run the test. It **locks the real screen** and waits up to 180 s for a manual unlock. It checks the `org.arinn.Session1.blocked` property and that the return target is kept. The operator visually confirms the shell is not shown over the lock screen. Suspend is checked separately through the regular KDE menu without bypassing inhibitors; the test suite never suspends or logs out automatically.

A complete MVP power action still needs inhibitor/refusal reporting and provider recovery after wake. `SessionLifecycle` does not implement the suspend command itself.
