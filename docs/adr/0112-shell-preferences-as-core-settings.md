# ADR-0112: The shell's preferences are core settings of the daemon

- **Status:** Accepted
- **Date:** 2026-10-21
- **Deciders:** Owner (option A on [#389](https://github.com/LimeBeck/arinn/issues/389), 2026-10-07: a daemon-owned core settings source persisted in `config.toml`; the ambient timeout as an Appearance setting with a 10-minute default, 2026-10-21 on [#478](https://github.com/LimeBeck/arinn/pull/478)); delegated technical choice for the ids, kinds and the shell reader
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** §5.6, §5.7, §11, §14.3, §15

## Context

Spec §15 says the light field "can be disabled", and handoff 01 puts the
switch at Settings › Appearance › Light field. Spec §5.7 starts ambient after
an idle timeout of 10 minutes by default, and the owner made that timeout a
setting next to Light field. Until now only providers and the daemon's own
browser settings had a home in `Settings1`. No shell presentation preference
was stored anywhere ([#389](https://github.com/LimeBeck/arinn/issues/389)).

[ADR-0065](0065-provider-settings-contract.md) already reserves the provider
id `arinn` for the daemon's own settings, and the daemon serves
`browser_application` and `search_engine` under it from `config.toml`.

## Decision

The shell's presentation preferences are settings of the reserved `arinn`
source in `Settings1`. They are stored in `config.toml`, shown by the existing
Settings screen and reachable by any `Settings1` client (§14.3). The wire
contract does not change: these are two more entries in the daemon's own
schema.

| Id | Category | Kind | Values | Default | `config.toml` |
| --- | --- | --- | --- | --- | --- |
| `light_field` | `appearance` | `boolean` | `true`, `false` | `true` | `light_field` |
| `ambient_timeout` | `appearance` | `enum` | `"5"`, `"10"`, `"15"`, `"30"`, `"60"` (minutes) | `"10"` | `ambient_timeout` (an integer of minutes) |

- **Ambient timeout.** The value is an `enum` of minutes rather than an
  `integer`, so the remote reaches each choice in one press. There is no
  "never" value: the owner set only the default.
- **Hand-edited values.** An invalid hand-edited `light_field` or
  `ambient_timeout` in `config.toml` (a wrong type, or a timeout that is not
  offered) reads as the default and never makes the file invalid. The daemon
  writes one warning line per key and process, with the key name and no value
  (§14.1). Every other key keeps its strict rules. Owner decision on 2026-10-22
  ([#488](https://github.com/LimeBeck/arinn/pull/488)). `SetValue` still accepts
  only the offered values.
- **Texts.** The daemon sends English source texts, as before. The shell shows
  the titles, descriptions and option titles of the `arinn` source translated
  (its `SettingsCatalog` context); provider texts are shown as the provider
  sends them.
- **Shell reader.** `SettingsCatalog` reads the two values from the `arinn`
  source it already loads and exposes `lightField` and `ambientTimeoutMinutes`.
  Until the daemon answers it uses the defaults; while the daemon is away it
  keeps the last values it read. A value outside the table above is ignored.
  The same ids from another provider do not count.
- **Use.** `Main.qml` binds `lightField` to Home's light field. It binds
  `ambientTimeoutMinutes` to the ambient idle timer once that timer has landed
  ([#307](https://github.com/LimeBeck/arinn/issues/307)). A change applies at
  once, through `SettingsChanged`.

## Consequences

- Shell preferences live in the same place as every other setting: one file,
  one Settings screen, one wire contract. Adding the next one is a schema entry
  and a `config.toml` field.
- An older daemon does not list the two settings. The shell then keeps the
  defaults: the light field on and 10 minutes.
- A `config.toml` written by this version has two more keys. An older daemon
  rejects unknown keys, as it already does for other additions.

## Alternatives considered

- **A shell-local preference store** (option B on #389): rejected by the owner.
  It splits persistence and bypasses the CLI.
- **`ambient_timeout` as an integer with `min` 5 and `max` 60:** valid under
  ADR-0071, but the stepper moves one minute per press, which is slow with a
  remote. The schema has no `step` constraint.
- **A "never" choice:** not part of the owner's answer. It can be added later as
  another enum value without a wire change.

## Verification

- `crates/arinnd/src/runtime/settings_tests.rs`
  (`the_shell_preferences_are_core_settings`): the schema validates, the
  defaults are listed, the values are stored and read back, and an unoffered
  timeout is `InvalidArgument` with nothing stored.
- `crates/arinnd/tests/storage.rs` (`shell_preferences_default_and_validate`,
  `invalid_hand_edited_preferences_fall_back_to_defaults`): an older file loads
  with the defaults; invalid hand-edited preferences read as the defaults and
  are reported once, while other keys stay strict.
- `crates/arinnd/src/runtime/settings_tests.rs`
  (`an_invalid_hand_edited_timeout_reads_as_the_default`): the daemon still
  serves its settings, with the default timeout.
- `apps/arinn-shell/settings_catalog_test.cpp`
  (`shellPreferencesFromTheCoreSource`): defaults before the daemon answers,
  values from `arinn` only, an unoffered value ignored, and a change through
  `SettingsChanged`.
- `tests/ui/tst_home.qml`: the light field goes dark when the setting is off.
