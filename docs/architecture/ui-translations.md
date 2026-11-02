<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Shell translations

The shell interface is Russian through Qt translations
([specification §15](../specifications/arinn-specification.md#15-design-contract),
[#363](https://github.com/LimeBeck/arinn/issues/363)).

- **Source strings.** They are English and live in the code: `qsTr()` in QML, `tr()` and
  `QT_TRANSLATE_NOOP` in C++. Counted strings use Qt numerus forms (`%n`).
  The Russian wording uses a "label: number" form where it can, so no case
  agreement is needed.
- **Catalogue.** `apps/arinn-shell/i18n/arinn_ru.ts` is the reviewed catalogue.
  `qt_add_lrelease` compiles it, and the `.qm` file is built into the shell's
  resources under `:/i18n/`.
- **Loading.** At start the shell installs a `QTranslator` for the system locale
  (`QLocale()`). A Russian locale gets Russian; any other locale keeps English.
- **Provider text.** Titles and messages that come from providers are shown as
  delivered; translating them is the provider's concern. Provider setting texts
  (title, description, option titles, unit) can come with a translation table
  ([ADR-0129](../adr/0129-provider-setting-translations.md)): the Settings screen
  looks each text up by the full locale (`ru_RU`), then the language (`ru`),
  and otherwise shows the provider's English source text. The daemon's own
  setting texts are still translated from this catalogue
  ([ADR-0112](../adr/0112-shell-preferences-as-core-settings.md)).

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
