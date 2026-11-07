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
  [ADR-0135](../adr/0135-localized-provider-texts.md). A private client cache
  renders only explicitly associated fields; an omitted association keeps
  plaintext even when it equals a known fixed label. Provider setting texts
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

### Export, check and compile without a checkout

The installed `arinnctl` embeds the shell's editable source snapshot. Export a
template for either domain, edit it in Qt Linguist, then compile it separately:

```sh
arinnctl translations export-shell de arinn
arinnctl translations export-shell de music
arinnctl translations check ~/.local/share/arinn/translations/overrides/de/shell/arinn.ts
arinnctl translations compile ~/.local/share/arinn/translations/overrides/de/shell/arinn.ts
```

Exports default to the personal XDG override directory. An optional final output
path chooses another destination. Export never overwrites an existing editable
file; export an updated source snapshot to another path and merge your work in
Qt Linguist. Every exported translation starts unfinished, including plural
messages. There is no coverage threshold.

For a registered provider, use its instance id from `arinnctl providers`:

```sh
arinnctl translations export-provider radio de
arinnctl translations export-settings radio de
```

The provider-message command requires the optional `ProviderTexts1` contract. It
exports id-based TS messages as `key@revision`, with source text, argument roles
and numerus metadata, including languages the provider does not supply. Settings
export uses the existing Settings1 schema and does not copy current values or
credentials. It writes a source-checked JSON template whose `translation: null`
fields remain inactive; replace a null with a string to translate that field.
An empty description string intentionally hides that description. Settings JSON
is used directly and needs no compilation.

Provider TS messages use an empty context name. Keep it empty: current Qt
selects id-based output from that context, while older Qt additionally requires
the `-idbased` option that `compile` supplies. A context/source provider QM
cannot serve the versioned message IDs.

Both provider exports derive the stable plugin id and safe directory from the
daemon, and recheck registration identity after reading. They never start or
restart a provider. A legacy provider without fixed-message support can still
use settings overlays. A failed export leaves existing translations alone.

`check` validates TS structure, lookup keys, completed labels and Qt placeholders,
or the settings JSON envelope and text bounds. It reports finished/unfinished
counts. `compile` requires Qt 6 Linguist's `lrelease`, excludes unfinished
messages and selects id-based compilation for provider sources. It checks the
compiled resource's bounds/self-containment and replaces the destination `.qm`
atomically. Failed checking/compilation preserves the previous `.qm`. File-only
commands work while the daemon is stopped; runtime loading never runs a compiler.

After compilation or a JSON edit, select *Reload translations* in the language
picker. Source files remain beside compiled files for later corrections and
sharing.

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

### Fixed provider messages and sharing

Compiled provider messages go in
`overrides/<locale>/providers/<plugin-directory>/messages.qm`. They use a private
translator bound to the daemon's registered plugin identity. They cannot replace
shell strings or another plugin's messages. The source catalogue controls message
IDs/revisions and typed arguments. Missing, empty, malformed or obsolete entries
fall through to lower layers, then the already complete snapshot plaintext.
Qt selects plurals before a single formatting pass; argument text is never
interpreted as another template. Media session titles and download errors are
eligible only when the provider associates that exact field; music uses separate
catalogue-entry and current-station identities. Content is omitted by providers.

A provider package and a translation package are independent. A provider can
ship its own exact-locale QMs through Texts1; an external translation needs only
data files under the XDG translation root. To install community files beside
personal corrections, use the lower-priority paths within that root:

```text
arinn_de.qm
music_de.qm
providers/<plugin-directory>/messages_de.qm
providers/<plugin-directory>/settings_de.json
```

These settings JSON files use the same source-checked envelope. Personal
`overrides/de/...` entries, including a compatible language parent, win over
community files in the same root. A missing string can still come from the
community file or provider package. Copy a partial TS/JSON/QM set by any ordinary
channel, retain editable sources, and use Reload translations after updating it.
A provider-only language is discovered from either resource layout, including
hashed plugin directories; optional `overrides/<locale>/language.json` supplies
its native name and can also announce a language before its resources arrive.
No provider executable, manifest change,
central registry or application rebuild is involved in installing a translation.

The client pins daemon owner, provider generation and original scope/id/revision/
field/fallback. It retains typed references across language switches, so changing
language does not repeat Resolve or read content pages. Metadata calls have a
three-second deadline, at most four are active, and batches hold at most 128
fields/240 KiB. The presentation cache admits at most 64 provider identities,
512 associations and 4 MiB of encoded references, plus 8 MiB of QM bytes. Excess
fields preserve plaintext. Models retain the fields of each current presentation
in at most 512 separate channels; different valid facet revisions coexist. Replacing
a presentation retires fields no view retains, while its own repaint notices
do not evict the same working set. Registry discovery removes departed
identities, and language changes retire obsolete dictionaries before loading
new personal layers. A normal provider status change preserves unchanged
identities. Last-good local message dictionaries survive a corrupt replacement;
deleting one deliberately restores fallback.

The bundled Local music and Yandex Music providers export their source names
and source-owned station labels. They always publish English fixed fallbacks;
their process locale no longer chooses the label. Russian TS sources and the
corresponding compiled private QMs live in each provider's `i18n/` directory,
and the QM bytes are embedded in its existing executable. Recompile a bundled
TS with `arinnctl translations compile <path/to/ru.ts>`; provider compilation
itself needs no Qt dependency. Tests load the actual embedded resources and
check the source messages. Yandex's stored pin titles retain their delivered
content when provenance is unknown, even if a title resembles a fixed label;
a live rotor session supplies explicit evidence for its fixed station label.
Settings source names use the same provider lookup, with the existing known
names as compatibility fallback. A presentation-only settings refresh does not
advance pending preference writes.

An optional `overrides/<locale>/language.json` (at most 4 KiB) can supply a name:

```json
{"formatVersion":1,"locale":"de","nativeName":"Deutsch"}
```

The name is at most 128 UTF-8 bytes without control characters. Qt's native language
name or the locale tag is used otherwise. A saved locale stays selected if its
files temporarily disappear; missing text falls back to English. Discovery is
bounded to 16 roots, 512 entries per directory and 64 language choices; system,
English, Russian and the saved choice are retained. Share these files/directories
by any ordinary channel. Versioned bundle import/selection is an optional later
extension; local source export and compilation use the commands above. No central
service is required.

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
