# ADR-0065: Provider settings contract (ProviderSettings1, Settings1)

- **Status:** Accepted
- **Date:** 2026-09-30
- **Deciders:** project owner for the scope: providers declare settings, the daemon stores and passes them, and the shell edits them (2026-09-30, [#145](https://github.com/LimeBeck/arinn/issues/145)); the wire design is a delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** amends [ADR-0064](0064-provider-settings-in-config.md) (adds the schema and live changes)
- **Amended by / Superseded by:** —
- **Specification:** [§5.6](../specifications/arinn-specification.md#56-settings), [§9](../specifications/arinn-specification.md#9-plugin-api-v1), [§11](../specifications/arinn-specification.md#11-state-ownership-and-persistence)

## Context

Spec §5.6 asks for provider settings with a typed schema:
- fields: id, title, description, type, current value, constraints, enabled;
- types: boolean, enum, string, integer, path reference;
- and no plugin-supplied UI.

[ADR-0064](0064-provider-settings-in-config.md) stores values in `config.toml`
and passes them to the provider at start. The owner asked for the rest:
providers declare their settings, the shell shows and changes them, and a
change reaches the provider.

## Decision

- **Setting** `(id, category, title, description, kind, value, constraints,
  enabled)`, signature `(sssssssb)`:
    - `kind` is `boolean`, `integer`, `string`, `enum` or `path`;
    - `category` is one of the design's Settings categories: `media`,
      `playback`, `sound`, `network`, `applications`, `appearance`, `system`;
    - `value` and `constraints` are JSON, the same representation as the
      stored values and `ARINN_PROVIDER_SETTINGS`;
    - in a provider schema `value` is the default; from the daemon it is the
      current value.
- **Constraints per kind**, with unknown keys rejected and the default
  required to satisfy them:
    - integer: `min`, `max`;
    - string: `max_length` (at most 4096, default 1024), and no control
      characters;
    - enum: `options` (1–64 unique `{value, title}`);
    - path: `directory`, `multiple` and `max_items` (at most 64). A path is
      absolute, local (not `//`), with no empty, `.` or `..` segment. A list of
      paths, such as the library roots, is a `path` with `multiple`, so no
      sixth type is needed.
- **Limits:** at most 128 settings, and each JSON field at most 16 KiB.
- **Provider side.** Any provider may export the optional
  `org.arinn.ProviderSettings1` at `/org/arinn/Provider1`:
    - `GetSchema(generation)` returns the schema;
    - `Apply(generation, values)` receives the complete effective values as
      one JSON object. The provider answers `Unsupported` if they take effect
      only at its next start, and `InvalidArgument` if it rejects them;
    - only the registered daemon may call, in the current generation;
    - the SDK validates the schema at export, and checks every value against
      it before the handler runs;
    - at start, `startup_settings` turns `ARINN_PROVIDER_SETTINGS` into the
      effective values: valid stored values, defaults for the rest, unknown
      ids dropped.
- **Daemon side.** `org.arinn.Settings1` on `/org/arinn/Daemon1`:
    - `ListProviders()` returns (id, name) of the providers with settings;
    - `GetSettings(provider_id)` returns (generation, settings with current
      values);
    - `SetValue(provider_id, generation, setting_id, value, request_id)` is an
      operation: validate against the schema, store in `config.toml`, then
      `Apply` to the running provider;
    - the signal `SettingsChanged(provider_id)`;
    - the provider id `arinn` is reserved for the daemon's own settings (the
      search engine of ADR-0051, `browser_application` of ADR-0055).
- **No UI from providers (R-04).** The shell renders typed controls for the
  kinds.

The daemon implementation and the Settings screen are separate tasks.

## Consequences

- The library provider can declare its roots as a `path` setting with
  `multiple`, and receive changes without a restart.
- Settings are not secrets. Credentials stay in the system secret storage
  (spec §11).
- JSON inside D-Bus strings keeps one representation from the provider
  through the daemon and `config.toml` to the shell, at the cost of parsing
  on each side.

## Alternatives considered

- **D-Bus variants (`v`) for values.** They are typed on the wire, but path
  lists and nested constraints become awkward. The values would also need a
  second mapping to TOML and to the environment variable.
- **Settings in the manifest.** The schema would be static and could not
  depend on the provider's environment. Registration would also have to parse
  it.
- **A sixth type for lists.** Not needed: `multiple` on `path` covers the only
  list in the MVP. The spec's type list stays as it is.

## Verification

- Protocol tests (`crates/arinn-protocol/src/settings/tests.rs`):
    - the `(sssssssb)` round trip;
    - defaults of every kind validate;
    - values are checked against the kind and the constraints;
    - bad shapes are rejected: unknown kind or category, foreign or unknown
      constraint keys, an inverted range, an empty or duplicate enum,
      `max_items` without `multiple`, a default breaking its constraints, and
      duplicate ids;
    - effective values merge stored values and defaults;
    - path rules.
- SDK private-bus test (`settings_service.rs`):
    - the schema is served;
    - a partial `Apply` is completed with defaults;
    - bad values, unknown ids and non-object input are `InvalidArgument`, and
      nothing is applied;
    - a stale generation is `Conflict`;
    - another caller is `PermissionDenied`;
    - the live interface matches `provider-settings1.xml`;
    - startup values come from the stored JSON.
