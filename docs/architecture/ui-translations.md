<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Shell translations

The shell interface uses Qt translations with English source strings, a bundled
Russian catalogue and local partial overlays
([specification §15](../specifications/arinn-specification.md#15-design-contract),
[#363](https://github.com/LimeBeck/arinn/issues/363)).

- **Source strings.** They are English and live in the code: `qsTr()` in QML, `tr()` and
  `QT_TRANSLATE_NOOP` in C++. Counted strings use Qt numerus forms (`%n`).
  The Russian wording uses a "label: number" form where it can, so no case
  agreement is needed.
- **Catalogue.** `apps/arinn-shell/i18n/arinn_ru.ts` is the reviewed catalogue.
  `qt_add_lrelease` compiles it, and the `.qm` file is built into the shell's
  resources under `:/i18n/`.
- **Loading.** Settings › Appearance › Language chooses *As in the system*,
  English, Russian or a locale discovered in the XDG data directories.
  `LocalizationManager` loads shell and music catalogues, changes `QLocale`,
  calls `QQmlEngine::retranslate()` and notifies existing C++ models. It keeps the
  engine, current screens, playback and operations alive. Formatting follows the
  selected locale; a bare language matching the system preserves its territory.
  A derived `ui-language.json` in the shell's XDG cache directory supplies the last
  daemon-confirmed choice during startup. It never writes a preference to the daemon.
  The requested translation tag is separate from the formatting `QLocale`:
  local dictionaries retain that identity even for languages/scripts unknown to Qt.
- **Codes, units and languages.** No wire token reaches the screen as is:
    - an error code (`Internal`, `NotFound`, an `org.arinn.Error1` name) or a
      failed operation state becomes translated text on each screen, and an
      unknown code reads as a generic failure;
    - a failed download without a provider message shows the shell's text
      for its normalized code (a provider's `error_message` is shown as
      delivered, §15);
    - sizes, rates and durations take their unit from the catalogue and
      their number from `QLocale` (a decimal comma under `ru_RU`);
    - a track's language tag becomes a language name: the common languages
      come from the catalogue, another language Qt knows gets its own name,
      and an unknown tag stays as sent.
- **Provider text.** Content and free-form remote messages stay as delivered.
  General fixed provider text uses the additive sidecar in
  [ADR-0135](../adr/0135-localized-provider-texts.md); client adoption is tracked
  separately in #846. Provider setting texts
  (title, description, option titles, unit) can come with a translation table
  ([ADR-0129](../adr/0129-provider-setting-translations.md)): the Settings screen
  looks each text up in a personal overlay, then in the provider's table by
  full locale (`ru_RU`), compatible script parent and language (`ru`),
  and otherwise shows the provider's English source text. The daemon's own
  setting texts are still translated from this catalogue
  ([ADR-0112](../adr/0112-shell-preferences-as-core-settings.md)).

## Customize an installed copy

No Arinn checkout or provider rebuild is needed. The writable root is
`$XDG_DATA_HOME/arinn/translations`, normally
`~/.local/share/arinn/translations`. System roots are
`<entry in XDG_DATA_DIRS>/arinn/translations` (normally `/usr/local/share` and
`/usr/share`). Earlier roots win, including a personal language parent over a
more specific system catalogue. Locale parents are tried inside each root;
fallback never crosses writing scripts. Within a root, structured overrides
precede flat files. Missing strings keep falling back through lower sources to
the built-in catalogue for that language, then English.

For the main interface, create an ordinary Qt TS catalogue using the English
context/source pairs. Keep the same contexts and `%1`, `%L1`, `%n` placeholders;
only include strings you want to change. Qt Linguist can edit it. For example:

```xml
<?xml version="1.0" encoding="utf-8"?>
<TS version="2.1" language="de" sourcelanguage="en">
  <context><name>HomeView</name>
    <message><source>Continue</source><translation>Weiter</translation></message>
  </context>
</TS>
```

Compile with the Qt 6 Linguist `lrelease` executable (some distributions name it
`lrelease6`):

```sh
mkdir -p ~/.local/share/arinn/translations/overrides/de/shell
lrelease -nounfinished my-arinn_de.ts -qm ~/.local/share/arinn/translations/overrides/de/shell/arinn.qm
```

Choose German in Appearance › Language and select *Reload translations* in that
picker after editing. A custom language appears after reopening/refreshing
Settings; no minimum percentage applies. A flat `arinn_de.qm` under the root also
works. Music has the separate `music.qm` / `music_ru.qm` domain. Use canonical
directory names such as `pt-BR`, `sr-Latn` and `zh-Hant-TW`; flat file names replace
hyphens with underscores. A file is at most 1 MiB; loaded external shell catalogues
total at most 8 MiB. Symlinks outside the translation root are ignored. QM files
must be self-contained: dependency blocks that load other files are rejected.

Replace a compiled file atomically when possible. Reload reads owned bytes rather
than retaining a mapping of an editable file. If a previously active catalogue
becomes invalid, the last good in-memory set stays active and the UI reports the
failure. Deleting the override and reloading restores lower layers. These are
trusted local translation resources, not executable plugins or a Qt sandbox.

### Provider settings

Find the provider's stable `pluginId` in its manifest / daemon provider listing;
the configured instance name is a different identity. Put this UTF-8 JSON in
`overrides/de/providers/<pluginId>/settings.json`:

```json
{
  "formatVersion": 1,
  "pluginId": "example.provider",
  "locale": "de",
  "settings": {
    "scan_interval": {
      "title": {"source": "Scan interval", "translation": "Prüfintervall"},
      "description": {"source": "Time between scans", "translation": ""},
      "unit": {"source": "s", "translation": "s"}
    },
    "decoding": {
      "options": {
        "auto": {"source": "Automatic", "translation": "Automatisch"}
      }
    }
  }
}
```

Portable ASCII plugin ids (`A-Z`, `a-z`, digits, `_`, `-`, `.`, at most 128
characters, except `.` and `..`) use that readable directory. Existing plugin ids
are opaque UTF-8, so other ids use `providers/by-id/<sha256-of-UTF8-plugin-id>/`.
The lowercase hex digest is the same for every instance. The JSON still contains
the full original `pluginId`; it is never replaced by the digest. A provider's
generation from the daemon listing must match the settings snapshot before an
overlay applies.

The setting ids and source strings must match the current English schema. Each
field is independent: a changed source skips that field, while other translations
still apply. Only a description can intentionally be empty. The file is at most
256 KiB; a translated field is at most 8 KiB and permits line breaks/tabs, not
other control characters. No provider code, ids, enum values, paths or content are
changed. Reload/reselection republishes existing settings locally. The legacy
ADR-0129 table remains the next fallback, so older providers work as before.

An optional `overrides/<locale>/language.json` (at most 4 KiB) can supply a name:

```json
{"formatVersion":1,"locale":"de","nativeName":"Deutsch"}
```

The name is at most 128 UTF-8 bytes without control characters. Qt's native language
name or the locale tag is used otherwise. A saved locale stays selected if its
files temporarily disappear; missing text falls back to English. Discovery is
bounded to 16 roots, 512 entries per directory and 64 language choices; system,
English, Russian and the saved choice are retained. Share these files/directories
by any ordinary channel. Versioned bundle import/selection and source export
tools belong to the next #846 increment; no central service is required.

## Adding or changing strings

1. Change the string in the code.
2. Run `cmake --build build --target arinn_lupdate` to refresh `arinn_ru.ts`.
3. Add the Russian wording to `apps/arinn-shell/i18n/fill_ru.py` and run it. A
   context entry overrides a word that reads differently on one screen.
4. Commit both files.

The `translations-complete` test runs `lupdate` over the sources into a copy of
the catalogue. It fails when a string has no translation or a stale one
remains. The `translations` test loads the catalogue for a Russian locale and
checks representative strings and numerus forms.
