<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Performance measurement harness

Issue [#150](https://github.com/LimeBeck/arinn/issues/150) prepares measurements
for G-5.4. The budgets come from [specification §6.5](../specifications/arinn-specification.md#65-performance-principles):
p95 input-to-focus ≤ 50 ms and p95 frame time ≤ 16.7 ms with a warm UI at 60 Hz.
This harness does not close G-5.4 or qualify the production shell on a TV.

## Run

Requires Python 3, CMake ≥ 3.24, Ninja, a C++20 compiler, Qt 6 ≥ 6.8 Quick and
DBus development packages, and `dbus-run-session`. Build artifacts are isolated;
the standalone CMake project also participates in the repository build, without
being installed as a production executable. CTest's `performance-harness` test
runs the 20-sample offscreen/private-bus smoke measurement using that executable.

```sh
python scripts/perf/run.py --output /tmp/arinn-performance.json
python scripts/perf/run.py --samples 20 --output /tmp/arinn-performance-smoke.json
```

The default is offscreen/software rendering, suitable for validating the harness
on any development machine. For an owner-run measurement on the target HTPC,
use a 60 Hz session with its normal renderer:

```sh
python scripts/perf/run.py --platform wayland --backend default --samples 1000 \
  --output /tmp/arinn-performance-target.json
```

`--build-dir` retains the Release build for another run; `--executable` uses an
already-built fixture (as CTest does). The fixture is a
windowed 1920×1080 Home surface. It registers no KWin script or global shortcut,
does not activate applications, and does not lock, suspend or shut down the
desktop. Native-display measurements are run by the owner. Reports belong in
local evidence storage; never commit host-specific logs or paths.

## Measurement boundaries

- Loads the actual `Ui.HomeView` and its production components with neutral
  populated fixtures, then warms them for two seconds. Moves focus between two
  cards at least 32 ms apart, waiting for each change before issuing another
  input. Each input must reach the expected card within three seconds; a missing
  change fails the run rather than silently reducing the sample count. Focus
  changes exceeding the 50 ms budget remain in the report.
- `input_to_focus_ms` uses a monotonic nanosecond clock from Qt key-event dispatch
  until `activeFocusItemChanged` identifies the expected target. It measures
  local handling, excluding physical input and compositor delivery and excluding
  the time until the visual focus treatment appears on screen.
- `frame_interval_ms` records consecutive `QQuickWindow::frameSwapped` timestamps
  while continuously requesting updates during focus movement. This measures
  presentation cadence, including scheduling and swap delays, not GPU execution
  time. It is a proxy for the frame budget, not proof of smooth production video.
- Two scenarios: warm Home and Home with a three-second asynchronous D-Bus read
  outstanding on a fresh private bus. The report counts focus samples while the
  read is pending and successful replies. This tests the components under an
  actual delayed IPC reply; production Backend/MediaCatalog/provider routing is
  not exercised and needs separate integration measurements.
- Navigation open and Home return measure local state/focus transitions after
  Menu and Home. They carry no numeric budget because §6.5 specifies none.
  They do not measure the global compositor Home action or animation completion.

The report contains raw samples, nearest-rank p50/p95, min/max, sample counts and
the two budget comparisons. No outliers are removed. Missing/negative/non-finite
samples fail reporting. A budget miss remains a valid measurement and does not
change the process exit status. Build, fixture or validation failure exits
nonzero and preserves an existing output file. Environment fields include Qt
version, platform, graphics API enum, requested backend/render loop, refresh rate
and dimensions; no hostnames, personal paths, URLs or application titles are
collected. Build type and CPU architecture are recorded too; use a Release build
for target measurements. `target_qualification` always remains `not_run`: an owner must review
target-device results alongside end-to-end input and production IPC evidence.

## Activity background (G-5.2)

G-5.2 needs its own owner-run report on the target device
([background contract](activity-background.md#budgets-and-verification-obligations)).
This harness does not capture windows. The daemon writes normalized journal
lines for each capture attempt: `background-ready` carries `duration_ms` from
the explicit Home request to the accepted frame, and `background-unavailable`
carries the reason (`deadline`, `session`, `window`, `adapter`, `shell` or
`cancelled`). Read them with `journalctl --user -u arinnd -o cat | grep
'"operation":"background-'`. Producer readback, shell copy, RSS and GPU
allocation figures come from the producer and shell integrations, which are
still pending.

## Verification

On 2026-09-30, Qt 6.11.2 offscreen/software runs passed the 20-sample CTest
smoke and a standalone Release run with 200 focus samples per scenario. All 200
delayed-scenario focus changes occurred while a read was pending, and three
delayed replies completed. These runs validate collection and reporting only.

`python -m unittest discover -s tests/ci -p test_performance_report.py` checks
percentiles, exact thresholds, invalid/missing samples, delayed-read evidence and
the distinction between budget comparisons and target acceptance. Run the smoke
command above to verify QML loading, real focus changes, frame collection, delayed
IPC and JSON output together. Native-display and target-HTPC measurements have
not been run by the agent.
