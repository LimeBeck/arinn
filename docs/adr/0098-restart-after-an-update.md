# ADR-0098: Restart Arinn after an update

- **Status:** Accepted
- **Date:** 2026-10-08
- **Deciders:** Owner (the restart mechanism, chosen in chat on 2026-10-08 for [#357](https://github.com/LimeBeck/arinn/issues/357)); delegated technical details
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §6 (Close Arinn, Home states), §8.2, §8.6

## Context

When the shell meets a daemon of another build ([ADR-0097](0097-build-identity.md)),
Home says "Arinn was updated" and offers Restart. The issue asked for Restart
"through the existing close/restart path", but none exists:

- Close Arinn closes the shell window, so the shell exits with status 0.
  `arinn-shell.service` uses `Restart=on-failure`, so systemd does not start it
  again.
- The stale process is often the daemon ([#355](https://github.com/LimeBeck/arinn/issues/355)):
  after an upgrade the new shell meets the old daemon. That daemon predates any
  method that could restart it.
- The shell talks only to the daemon (spec §8.2), and platform code stays in the
  platform layer (R-rules). The shell cannot call systemd itself.

## Decision

- **Exit status.** Restart makes the shell exit with status 75
  (`BuildIdentity::RestartExitStatus`). The shell asks nothing else and starts
  no process.
- **Platform launcher.** `arinn-shell.service` runs
  `arinn-shell-launch --stopped` as `ExecStopPost`. When the shell exited with
  75 (`EXIT_CODE=exited`, `EXIT_STATUS=75`), it runs
  `systemctl --user --no-block try-restart arinnd.service`. `try-restart`
  starts nothing that was not running, and `--no-block` keeps the stop of one
  unit from waiting on another. Any other stop leaves the daemon alone.
- **Unit.** `RestartForceExitStatus=75` restarts the shell after `RestartSec`,
  whatever the `Restart=` policy is. The daemon restarts its providers on
  start.
- **Without systemd.** A shell started by hand exits with 75 and is not
  restarted. That is the same as Close Arinn.

## Consequences

- One action restarts both processes from the installed binaries, including
  an old daemon that has no `BuildInfo`.
- The shell keeps its §8.2 boundary. The systemd call lives in a platform
  script, next to the transport selection that script already does.
- A restart interrupts managed playback, as a daemon restart does today.
- The shell and the unit must agree on 75. A package test checks the unit, the
  launcher and the shell constant together.

## Alternatives considered

- **Close the shell and tell the user to start Arinn again.** Rejected: the
  owner asked for a Restart action, and a TV has no easy way to start it again.
- **Restart only the shell.** Rejected: it does not help when the daemon is the
  stale process, which is the #355 case.
- **A daemon method that restarts Arinn.** Rejected: an old daemon, the one
  that most needs restarting, would not have it.
- **The launcher waits for the shell instead of `exec`.** Rejected: the
  launcher would have to forward signals and would stay in the process tree.
  `ExecStopPost` runs after the shell exits with no change to how it starts.

## Verification

- `tests/package/test_shell_launcher.py`: with a stub `systemctl`, status 75
  restarts `arinnd.service`, and every other stop does not. The unit, the
  launcher and the shell agree on 75.
- Shell tests: `CatalogTest::daemonOfAnotherBuildAsksForRestart`
  (Home state on every owner change) and `tst_home.qml`
  `test_updated_state_offers_restart` (the action).
- Not tested on a real session: the full restart under a running user
  manager is an operator test that needs owner permission.
