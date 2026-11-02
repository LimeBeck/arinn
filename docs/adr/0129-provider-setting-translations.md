# ADR-0129: Translations for provider setting texts

- **Status:** Accepted
- **Date:** 2026-11-01
- **Deciders:** Owner, for the scope: provider setting texts are shown in the
  user's language and providers ship the translations with their schema
  (2026-11-01, Claude session, [#683](https://github.com/LimeBeck/arinn/issues/683)).
  The wire design is a delegated technical choice
  ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)).
- **Amends / Supersedes:** amends [ADR-0065](0065-provider-settings-contract.md)
  (adds translations) and [ADR-0112](0112-shell-preferences-as-core-settings.md)
  (provider texts are no longer shown only as sent)
- **Amended by / Superseded by:** —
- **Specification:** [§5.6](../specifications/arinn-specification.md#56-settings),
  [§9](../specifications/arinn-specification.md#9-plugin-api-v1),
  [§15](../specifications/arinn-specification.md#15-design-contract)

## Context

The shell interface is Russian through Qt translations, and English is the
source and fallback (§15, [#363](https://github.com/LimeBeck/arinn/issues/363)).
The daemon's own settings are translated by the shell's catalogue
(ADR-0112). A provider's setting texts, however, are shown as the provider
sends them. These texts are the title, the description, the enum option
titles and the integer `unit`. A Russian household therefore sees "Library
folders", "Video decoding" and "Download service address" in an otherwise
Russian Settings screen.

The shell cannot translate these strings itself. It would have to know each
provider's texts, which R-01 forbids, and a third-party provider would stay
untranslated. Only the provider knows its texts. The daemon does not know the
shell's language, and the language can differ between clients (§14.3).

## Decision

A provider may ship translations of its setting texts next to its schema. The
daemon passes them through unchanged, and each client picks the language it
shows.

- **Table.** The translations are one JSON object, in the same JSON style as
  values and constraints (ADR-0065):

  ```json
  {"de": {"roots": {"title": "Mediathek-Ordner",
                    "description": "Ordner mit Filmen und Serien"},
          "hardware_decoding": {"options": {"auto": "Automatisch"}}},
   "pt_BR": {"roots": {"title": "Pastas da biblioteca"}}}
  ```

    - A **locale key** is a language (`ru`) or a language and a territory
      (`pt_BR`): two or three lowercase letters, then optionally `_` and two
      uppercase letters. This is the form of Qt's `QLocale::name()`.
    - A **setting entry** may hold `title`, `description` and `unit`, and
      `options`, a map from an enum value to its title. Every field is
      optional.
    - **Rules:**
        - a setting id must be in the schema;
        - an option value must be one of that enum's options;
        - `unit` is allowed only on an integer setting whose constraints
          have a `unit`;
        - texts follow the same rules as the schema texts, and a unit the
          same rules as `unit`;
        - unknown keys are rejected;
        - at most 64 locales;
        - at most 256 KiB of JSON.
    - The source texts in the schema stay English. An absent table or entry
      means the source text is shown.
- **Lookup.** A client looks up each text by the full locale (`ru_RU`), then
  by its language (`ru`), then falls back to the source text. The lookup is
  per field: a translated title with an untranslated description shows the
  English description.
- **Provider side.** `org.arinn.ProviderSettings1` gains
  `GetTranslations(generation) → s`. Callers and generations are restricted
  as for `GetSchema`. A provider that does not implement it has no
  translations. In the SDK, `SettingsHandler::translations()` returns
  `settings::Translations`, and an empty table by default. `serve_settings`
  validates the table against the schema at export.
- **Daemon side.** `org.arinn.Settings1` gains
  `GetTranslations(provider_id) → (generation s, translations s)`. The
  daemon fetches the table once per registration generation, with the
  schema and within the schema's deadline, so the two calls together never
  take longer than the schema call alone may. It returns `{}` for:
    - a table that arrives after that deadline;
    - a provider without the method;
    - a table that fails validation;
    - the reserved `arinn` source, whose texts the shell still translates
      from its catalogue (ADR-0112).

  The daemon never chooses a language.
- **Compatibility.** Both methods are additions. An older provider, daemon
  or client keeps working and shows the source texts.

## Consequences

- In-repo providers (library, mpv, Transmission) can ship Russian texts.
  Third-party providers can ship any language without a shell change.
- The shell makes one more call per provider when it loads Settings.
- Error messages from a provider (for example a rejected value) are still
  shown as delivered: this ADR covers schema texts only.
- §5.6, §15 and `ui-translations.md` no longer say that provider setting
  texts are shown only as delivered.

## Alternatives considered

- **The daemon passes the locale to the provider, which returns a
  translated schema.** The daemon would have to know the client's language.
  It would also have to cache one schema per language, and refetch the schema
  when the language changes. Rejected by the owner.
- **The shell translates the in-repo providers' texts by id.** No contract
  change, but the shell would name concrete providers (R-01), and
  third-party providers would stay untranslated. Rejected by the owner.
- **Translations inside each setting's `constraints`.** That would avoid a
  new method, but it mixes display text with validation rules and breaks the
  "unknown constraint keys are rejected" rule for older daemons.

## Verification

- **Protocol tests** (`crates/arinn-protocol/src/settings/tests.rs`):
    - a valid table round-trips;
    - each rule above rejects bad input: an unknown setting id, an unknown
      option value, `unit` on a setting without a unit, a bad locale key, an
      unknown field, a control character, and too many locales or bytes;
    - the lookup goes from the full locale to the language to the source
      text.
- **SDK private-bus test** (`settings_service.rs`):
    - the table is served;
    - an invalid table makes `serve_settings` fail;
    - a provider without translations serves `{}`;
    - callers and generations are checked as for `GetSchema`;
    - the live interface matches `provider-settings1.xml`.
- **Daemon test:**
    - `Settings1.GetTranslations` passes the provider's table through;
    - it answers `{}` for a provider without the method and for `arinn`;
    - a provider that answers the schema late and never the translations
      is answered within the schema's deadline, with its schema and `{}`;
    - the export matches `settings1.xml`.
