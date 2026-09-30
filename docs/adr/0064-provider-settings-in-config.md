# ADR-0064: Provider settings in config.toml and their delivery

- **Status:** Accepted
- **Date:** 2026-09-30
- **Deciders:** project owner for the location (settings stay in `config.toml`, 2026-09-30, [#153](https://github.com/LimeBeck/arinn/pull/153)); the delivery mechanism is a delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§9.9](../specifications/arinn-specification.md#99-manifest-and-discovery), [§11](../specifications/arinn-specification.md#11-state-ownership-and-persistence)

## Context

Spec §11 puts provider configuration in `config.toml`. The library provider
(#153) needs its roots and kept them in a separate file. The owner decided
that they belong in `config.toml`. Two things were missing:
- `InstanceConfig` rejected any key but `plugin_id` and `enabled`;
- providers had no way to receive settings
  ([#165](https://github.com/LimeBeck/arinn/issues/165)).

The owner also wants a proper interface later: providers declare their
settings, the shell shows them, and changes reach the provider. That is
[#145](https://github.com/LimeBeck/arinn/issues/145). This decision is its
storage and transport base.

## Decision

- **Storage.** `[providers.<id>]` in `config.toml` may carry a `settings` table.
  The daemon does not interpret it and keeps it across saves.
- **Validation.** Settings must have a JSON form: no TOML datetimes. Their JSON
  is at most 64 KiB. Otherwise the configuration is invalid.
- **Delivery.** Each provider start sets `ARINN_PROVIDER_SETTINGS` to the
  settings as compact JSON, or `{}`, next to the existing variables. Settings
  are read at every start: a change takes effect at the provider's next start,
  restart or registry reload.
- **Privacy.** Settings are configuration, not secrets: credentials still go to
  the system secret storage (§11). Providers must not log settings that contain
  paths.

## Consequences

- The library provider reads its roots from `ARINN_PROVIDER_SETTINGS`, and no
  separate configuration file is needed.
- Nothing edits settings yet apart from `config.toml` itself. #145 adds the
  typed schema, a daemon interface for reading and writing values, live change
  notification to the running provider, and the Settings screen, on top of
  this storage.
- Environment variables are visible to the provider's own processes. That is
  acceptable for non-secret settings.

## Alternatives considered

- **A per-provider file next to the data directory.** Rejected by the owner.
  It would split the configuration across files.
- **Let providers read `config.toml` themselves.** Rejected: providers would
  depend on the daemon's file format and could see other providers' settings.
- **A D-Bus method `GetSettings` on the daemon.** It is better for live changes,
  and belongs to #145's contract. The environment variable needs no contract
  change and works before registration.

## Verification

- `crates/arinnd/tests/storage.rs`:
    - settings round trip through save and load;
    - an old configuration without settings stays valid;
    - the JSON form is exact;
    - datetimes and oversized settings are rejected, and a configuration with
      them does not validate.
- `provider_env_tests` in `runtime.rs`: the environment carries the settings
  JSON, and `{}` without settings.
