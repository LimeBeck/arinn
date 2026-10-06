# ADR-0089: Daemon provider supervisor without a systemd user manager

- **Status:** Accepted
- **Date:** 2026-10-06
- **Deciders:** delegated technical choice
- **Amends / Supersedes:** amends [ADR-0021](0021-plugin-runtime-ownership.md)
- **Amended by / Superseded by:** —
- **Specification:** [§9.10](../specifications/arinn-specification.md)

## Context

The target minimal HTPC image may run without a systemd user manager. In that environment `arinnd` could discover plugins but could not start provider processes, because supervision was delegated exclusively to transient systemd user services ([ADR-0021](0021-plugin-runtime-ownership.md)). Chrome and the Native Messaging host must remain outside provider supervision; only long-lived plugin binaries such as `arinn-plugin-chrome` and the desktop provider are supervised.

## Decision

Transient systemd user services remain the default whenever the session bus exposes a working `org.freedesktop.systemd1` user manager.

When no user manager is available, `arinnd` enables an in-process provider supervisor once at startup. A provider never runs under both runtimes in the same daemon instance.

The daemon supervisor:

- starts only the manifest process (absolute `executable` plus `args`, never a shell) with the same environment variables a transient unit would receive;
- restarts only on failure with the same budget as today (`RestartSec=2`, `StartLimitBurst=5` per `StartLimitIntervalSec=60`), then marks the provider `failed` until an explicit `Daemon1.Restart`;
- accepts `Register` only when the D-Bus sender’s PID is the main process the daemon launched for that provider; foreign same-user PIDs are refused and cgroups are not faked;
- does not supervise Chrome, the Native Messaging host, or applications launched by the desktop provider (session launcher isolation is unchanged).

The D-Bus contract and XML introspection are unchanged.

## Consequences

Minimal sessions without a user manager can run packaged browser and desktop providers. The daemon carries a bounded restart loop and PID tracking that must stay aligned with the systemd adapter semantics. Integration tests cover both supervision modes without requiring a real user manager on every runner.

## Alternatives considered

Requiring systemd user lingering or a portable supervisor such as `runit` on the minimal image was rejected: it adds packaging complexity without matching the existing transient-service contract on full desktops.

## Verification

Unit tests in `crates/arinnd/src/supervisor/` cover: daemon supervisor disabled when a session user manager exists; enabled only when absent; no dual runtime; absolute exec/argv without a shell; restart budget then `failed` until manual restart; `Register` authentication limited to daemon-launched PIDs. Owner merge is required for this architectural change.
