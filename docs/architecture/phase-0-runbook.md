# Phase 0 runbook

The live launcher retains its historical name but now starts the production Phase 1
runtime through manifest discovery. Historical Phase 0 evidence describes the old
wire API. For current isolated tests, build `cargo build --locked --examples` and run
`dbus-run-session -- python tests/integration/phase1.py`; see the
[Phase 1 record](phase-1-progress.md). The live launcher and operator tests still
require explicit permission and a disposable KDE session.

An experimental vertical slice: Rust daemon, a separate demo provider, a Qt Quick shell, a CLI and a temporary KWin script. The current launcher uses `org.arinn.Daemon1`; the Plugin API is still pre-stable. The UI checks IPC and focus, not the design handoff.

## Build

Requirements: Rust with edition 2024, CMake ≥ 3.24, Ninja, C++20, Qt 6 Quick/Quick Controls/DBus/Test ≥ 6.6, Python 3, Node.js (adapter regression test), `dbus-run-session`. For the live session: KDE Plasma 6 on Wayland, a systemd user session and `qdbus6`. Verified versions are in the [report](phase-0-report.md).

From the repository root:

```sh
cargo build --locked
cargo build --locked --examples
cmake -S . -B build -G Ninja
cmake --build build -j4
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all --check
node --test tests/integration/kwin_adapter.test.cjs
dbus-run-session -- python tests/integration/phase0.py
```

The private-bus suite runs Qt Test automatically with offscreen/software rendering. It checks reading a real typed snapshot through QtDBus and local focus during a slow action. It does not touch the live desktop.

## Live session

```sh
python scripts/phase0-session.py start
target/debug/arinnctl status
target/debug/arinnctl --json snapshot demo
```

Meta+F12 opens Home on the monitor of the foreground Activity; Esc or Backspace returns there. Without an Activity, Back opens the navigation prototype. Arrows and Enter work locally in QML. The CLI also provides `home`, `back` and `activate`.

The launcher starts `arinn-phase0-daemon` and `arinn-phase0-shell` with bounded restart-on-failure, `PartOf=` and `After=graphical-session.target`. The daemon discovers the demo manifest and creates `arinn-provider-<registry_uuid>.service`. `.phase0/` (mode 0700) holds isolated manifests/config/state; private registration tokens live under `$XDG_RUNTIME_DIR/arinn`. The KWin script is loaded only for this session; no permanent autostart is installed.

```sh
python scripts/phase0-session.py stop
```

Stop returns to the Activity if it still exists, unloads the script and stops the launcher units and provider units recorded in the isolated registry. No monitor layout or scale changes are needed. KDE may keep an inactive entry for the temporary shortcut after unloading.

## Operator checks

Open a test application, then run `python tests/integration/kwin_cycles.py`. It runs 20 cycles through KGlobalAccel and checks the UUID and output of the returned window. Do not switch or close windows during the check. This is not a test of a physical keyboard or the G60S Pro, and not an input-to-photon measurement.

`tests/integration/kwin_focus.py RESOURCE_CLASS --fullscreen --output OUTPUT_NAME` is a helper for an explicitly chosen test window. It changes fullscreen and placement of that window only, so use it on a disposable fixture, not a real working window.

When the daemon restarts, the shell, provider and KWin bridge reconnect automatically. To check on a disposable Activity: `python tests/integration/kwin_recovery.py`. It deliberately kills only the transient `arinn-phase0-daemon` and checks that Home/Return survive.

`tests/integration/kwin_runtime.py` checks manual/automatic provider restart and argv reload against the real systemd manager. It uses only the isolated demo manifest, restores it and records local evidence in `.phase0/`.

Further operator tests: [Activity background](background-experiment.md) (`kwin_background.py`, `background_preview.py`) and [lock/unlock](session-lifecycle.md) (`kwin_lock.py` — **locks the real screen**).
