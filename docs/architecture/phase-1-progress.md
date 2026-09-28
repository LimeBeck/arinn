# Phase 1 implementation record

Started 2026-09-28 from `master` at `48d40c0`. Phase status remains **In progress**
in [ROADMAP.md](../../ROADMAP.md): local and CI implementation evidence does not
close the remaining maintainer release gate. Branch protection is now verified.

## Implemented

- Deterministic XDG manifest discovery, overrides, duplicate/error handling, offline
  diagnostics and live reload: [registry](../../crates/arinnd/src/registry.rs),
  [runtime](../../crates/arinnd/src/runtime.rs).
- Production `org.arinn.Daemon1`, per-provider `Application1` proxies and `Operation1`
  objects. Versioned [XML and client semantics](../plugin-api/README.md) replace the
  spike interfaces in executable code; historical spike XML remains as evidence.
- Systemd transient services, UID/PID/unit/token authentication, fresh generations,
  owner-loss invalidation, daemon restart attachment and bounded restart policy.
  The production supervisor only uses systemd D-Bus; the private test manager is
  a separate example binary and is never selected by production configuration.
- Permissive Rust SDK with registration/re-registration, typed application models,
  atomic snapshots, immutable caller-bound pages and validation on both boundaries.
- Async operations, request deduplication, per-provider admission limits, deadlines,
  late-reply rejection, five-minute retention and safe structured completion logs.
- Versioned TOML config and transactional SQLite state, stable registry UUIDs,
  restart counters, ordered favorites and bounded launch recents. Provider-owned
  directories use SHA-256 of the exact provider ID ([ADR-0034](../adr/0034-provider-data-directory-identity.md));
  deleting the registry database does not relocate those directories.
- SDK demo fixtures: populated, empty, delayed, invalid ID, crash, reconnect,
  revision race, unavailable backend and a large catalogue.
- CLI read commands, paging, reload/restart, application commands, human/JSON output
  and documented exit codes. Minimal Qt shell receives data and operations solely
  from the daemon. KDE-specific code remains in `platform/kde`.
- CI builds the contract helpers and runs the production private-bus harness;
  packaging installs the demo manifest and uses the same test supervisor for smoke tests.

Only `application` is implemented in this phase. Other known capability names are
recognized but reported as unsupported until their versioned contracts are delivered.
Settings/favorites UI, desktop application launching, full product UI and later
capability providers retain their roadmap assignments.

## Verification and limits

Passed locally on 2026-09-28:

- Rust formatting, clippy with warnings denied, 27 unit/process tests, workspace
  and contract-helper builds.
- CMake/Ninja Qt build, complete private-bus contract suite and offscreen Qt tests.
- KWin adapter JavaScript tests, architecture guards, documentation/version checks
  and `git diff --check`.
- REUSE (217/217 files) and `cargo deny check licenses`.
- Package smoke test using the built binaries staged under a temporary prefix:
  daemon, supervised demo, CLI and offscreen shell communicate successfully.
  This is not a `makepkg` or clean Arch install/uninstall result.

The [private-bus harness](../../tests/integration/phase1.py) runs actual daemon,
SDK demo, CLI and Qt binaries against a test-only systemd manager. It exercises
contract introspection, paging, malformed IDs, stale generations, duplicate
requests, unsupported cancellation, queue exhaustion, delayed replies, backend
unavailability, crash/reconnect, daemon recovery, manifest metadata changes and
remove/reinstall, and recovery when the manager was unavailable at startup. Storage tests cover hash vectors, registry reset, migration
rollback, atomic config writes, symlink rejection and restart-counter persistence.

Real systemd/KDE operator tests passed after explicit owner authorization on
2026-09-28: 20/20 Home/Back cycles over Chrome, daemon crash recovery with Home
hidden and visible, manual provider restart, automatic crash restart, restart counters
and argv reload with the original fixture restored. See the sanitized
[live evidence](phase-1-live-evidence.json). Cycle times measure the whole scripted
Home/Back cycle, not physical input latency. Fullscreen games, other monitors,
lock/suspend/logout and a physical remote were not tested in this run.

### GitHub CI qualification

Verified on 2026-09-28: [CI run 36383019061](https://github.com/LimeBeck/arinn/actions/runs/36383019061)
passed on `master` commit `f4e9b688f0b02306384988fe67124d4c56877de0`.
All six jobs completed successfully:

| Check | Evidence |
| --- | --- |
| Rust formatting, clippy and unit/process tests | [Rust job](https://github.com/LimeBeck/arinn/actions/runs/36383019061/job/108802512207) |
| Qt build, KWin JavaScript tests and private-bus integration suite | [Qt/integration job](https://github.com/LimeBeck/arinn/actions/runs/36383019061/job/108802512300) |
| REUSE and dependency licenses | [Licensing job](https://github.com/LimeBeck/arinn/actions/runs/36383019061/job/108802512261) |
| Documentation, ADR, architecture and release consistency | [Documentation job](https://github.com/LimeBeck/arinn/actions/runs/36383019061/job/108802512134) |
| Arch package build and checks with makepkg | [Package build job](https://github.com/LimeBeck/arinn/actions/runs/36383019061/job/108802512246) |
| Clean Arch installation, smoke test and uninstall | [Package installation job](https://github.com/LimeBeck/arinn/actions/runs/36383019061/job/108804414465) |

The package installation job's installation, smoke-test and uninstall steps each
passed. This supplies the previously missing clean-container package evidence;
it does not verify autostart or session behaviour on the target HTPC, or close
Phase 5 acceptance. These are inspected CI results, not newly rerun local tests.

The original [Phase 1 PR #8](https://github.com/LimeBeck/arinn/pull/8) had a failed
licensing job. The successful run above qualifies the later integrated `master`,
not that earlier PR run. The PR's [CLA check](https://github.com/LimeBeck/arinn/actions/runs/36382270639/job/108800304208)
passed; a successful CLA check does not prove that branch protection requires it.
The subsequent [shell boundary review](phase-1-shell-review.md) passes AC-12
for the reviewed production shell. The owner resolved the G-1.4 wording conflict
in [ADR-0041](../adr/0041-independent-session-lifecycle-guard.md): application/provider integration remains daemon-only,
while session observation stays independent. The review supports that clarified
boundary. Platform transport extraction was merged in
[PR #32](https://github.com/LimeBeck/arinn/pull/32), closing
[task #29](https://github.com/LimeBeck/arinn/issues/29).

### Release recovery regression

Release qualification exposed a provider re-registration stall under unrelated
D-Bus name churn. [PR #46](https://github.com/LimeBeck/arinn/pull/46) filters the
SDK ownership subscription to the daemon name. The regression creates 128 unrelated
name acquisition/release pairs before a replacement daemon replies: it failed on
the old SDK with a timeout and passed with the fix. A temporary private-bus stress
harness completed 300 consecutive daemon restarts after the fix. The complete
[PR CI](https://github.com/LimeBeck/arinn/actions/runs/36412698903), including clean
Arch package installation, passed on the integrated branch. This resolves
[issue #34](https://github.com/LimeBeck/arinn/issues/34) before publication.

### Remaining maintainer gate

The earlier branch-protection API request returned a plan restriction. A fresh
check on 2026-09-28 instead reported an unprotected branch. Following the owner's
request to close Phase 1, protection was configured and verified by a separate
GET of `repos/LimeBeck/arinn/branches/master/protection`:

- Pull requests and an up-to-date branch are required, including for administrators.
- Required checks: `Rust (fmt, clippy, test)`,
  `Qt shell and private-bus integration (Arch Linux)`,
  `Licensing (REUSE, cargo-deny)`, `Documentation consistency`,
  `Changelog fragment`, `Package / Build Arch package`,
  `Package / Install and smoke-test package`, and `cla`.
- Checks are bound to the GitHub Actions app (15368).
- Force pushes and branch deletion are disabled; code-owner approval is not required.

The `v0.1.0` release is still unpublished. On commit
`cdc11c4dd3783cbe24faf4b3b8cb1fd3ad6c2088`,
`python scripts/release.py check v0.1.0` failed because the workspace version
is `0.0.1`, there is no collected release section, and changelog fragments remain.
The owner subsequently delegated preparation and publication of `v0.1.0`.
The release preparation updates the workspace/package version, generates the
lockfile with Cargo, collects the changelog and advances specification 0.6.
`python scripts/release.py check v0.1.0` now passes on the release branch.
CI and publication through the release pipeline remain required, as described
in [CONTRIBUTING.md](../../CONTRIBUTING.md#releases).

No release tag was created and no release was published during this verification.
Phase 1 remains **In progress** until publication evidence closes G-1.6.
Remaining work is tracked in [task #41](https://github.com/LimeBeck/arinn/issues/41).

## Reproduce the isolated checks

```sh
cargo build --locked
cargo build --locked --examples
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cmake -S . -B build -G Ninja
cmake --build build
dbus-run-session -- python tests/integration/phase1.py
node --test tests/integration/kwin_adapter.test.cjs
python scripts/check-docs.py
python scripts/check-architecture.py
python scripts/release.py check
reuse lint
cargo deny check licenses
```

`phase0.py` remains a compatibility entry point for the same private-bus harness.
`--update-xml` deliberately refreshes the four XML files; ordinary test runs compare
against them and fail on drift. Review signature changes before committing XML.

The live launcher `scripts/phase0-session.py` retains its name for compatibility.
It now creates an isolated manifest/config/state tree in `.phase0`, starts the
production daemon and lets it supervise the demo. Run it only with explicit
operator authorization; it changes a real KDE session.
