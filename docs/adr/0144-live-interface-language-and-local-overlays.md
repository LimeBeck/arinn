# ADR-0144: Live interface language and local translation overlays

- **Status:** Proposed
- **Date:** 2026-11-06
- **Deciders:** Owner requested implementation in [#845](https://github.com/LimeBeck/arinn/issues/845) and the refined [#846 proposal](https://github.com/LimeBeck/arinn/issues/846#issuecomment-6694979705); technical layout proposed here
- **Amends / Supersedes:** [ADR-0112](0112-shell-preferences-as-core-settings.md) (language preference), supplements [ADR-0129](0129-provider-setting-translations.md) (client-selected locale and personal setting text)
- **Amended by / Superseded by:** —
- **Specification:** §5.6, §11, §15; R-01–R-05, R-09–R-10

## Context

Following the process locale makes a translation impossible to choose from a TV
remote. Editing an installed application must not require a source checkout,
rebuilding Arinn, a provider restart or admission to a central translation service.
Settings already have a daemon-owned persistence path and a compatible translation
table. The additive fixed-provider-text contract is a separate increment
([ADR-0135](0135-localized-provider-texts.md)).

## Decision

`arinn.ui_language` is an Appearance enum persisted in `config.toml`. `system`
(the default) follows the system locale; `en`, `ru` and discovered local locales
are selectable without a completeness threshold. Values use canonical
language[-Script][-REGION] tags, as in ADR-0135. A valid saved choice survives
temporary removal of its files. Invalid hand edits read as `system` with the
existing warning policy; an invalid setting operation is rejected.

The daemon discovers choices on a blocking worker and never interprets QM files.
The shell loads bounded local QM catalogues from the XDG data locations before
built-in resources. Personal data directories have precedence over system data
directories; each source tries the exact locale and compatible parents. A script
fallback cannot turn Traditional Chinese into Simplified Chinese or Latin Serbian
into Cyrillic Serbian. English code strings remain the ultimate fallback.

`LocalizationManager` owns the bytes and translators, sets the formatting locale,
retranslates the existing QML engine and notifies C++ presentation models. It does
not recreate the engine, navigate, restart a process or mutate content. A confirmed
preference cache under the shell's XDG cache directory avoids starting in another
language while the daemon answers. It is derived, disposable and never writes
back to configuration. The daemon's confirmed preference always supersedes it.
The requested translation tag remains separate from formatting `QLocale`, so
Qt aliases or unsupported languages cannot silently change dictionary identity.

Local overrides live under `arinn/translations/overrides/<locale>/`: shell
catalogues in `shell/arinn.qm` and `shell/music.qm`, provider settings in
`providers/<plugin-directory>/settings.json` and fixed provider messages in
`providers/<plugin-directory>/messages.qm`. Separate lower-priority flat shell
and provider resource paths accept manually shared community files. A settings
overlay names the stable plugin id, locale and expected English source for every
field; an outdated field is skipped individually. Instance ids and arbitrary
visible-text matching cannot select a provider overlay.

The language picker includes *Reload translations*, a local action that does not
call `SetValue`. A corrupt replacement of a currently active QM keeps the previous
in-memory catalogues and reports failure; removing a file intentionally restores
fallback. Files can be exchanged manually. No registry, hosted service, CLA change
or application update is required for a personal translation.

The concrete paths, limits, JSON envelope and edit/compile/reload instructions are
in [Shell translations](../architecture/ui-translations.md). Versioned external
bundle import remains an optional extension; the installed CLI exports editable
sources, checks partial files and atomically compiles QMs without a checkout.
General provider messages use private plugin dictionaries and snapshot-bound
references from ADR-0135. Presentation ownership retains valid mixed facet
revisions while bounding caches, and changing language rerenders references
without repeating Resolve or reading content pages.

## Consequences

- Partial translations are useful immediately. Missing entries fall back per
  string, rather than hiding an entire language.
- Language changes preserve current playback, operations and editor state.
  C++ models must republish translated presentation without fetching provider data.
- Selected language and formatting follow the same locale. A bare language that
  matches the system language preserves its regional formatting; an explicit
  territory chooses that territory's formatting. `system` restores the system.
- Local data is trusted user input for Qt, subject to size/path bounds. These
  checks do not turn Qt's binary translation reader into a security sandbox.
- Generic data roots provide one installation location shared by the daemon's
  discovery and the shell loader; cache files are never translation sources.

## Alternatives considered

- Changing `LANG` and restarting: cannot satisfy live switching.
- A second shell-owned configuration preference: would disagree with Settings1.
- Requiring a hosted translation platform or a completeness threshold: prevents
  the requested small personal changes.
- Translating provider content by matching strings: violates the content boundary.

## Verification

Rust tests cover persistence, invalid edits, bounded discovery, unavailable saved
languages and the Settings1 operation path. Qt tests cover live QML bindings,
unchanged editor data, locale formatting, layered partial catalogues, corrupt
reload recovery, missing strings, the derived cache, stable plugin identity,
source guards and setting retranslation without provider calls. A Qt Quick test
checks that Reload is keyboard-accessible and sends no setting mutation.
Actual screen tests retain nav, tab, season and episode focus across translated
delegate replacement, including focus from hover without changing selection.
Private-bus and real-QM tests additionally cover fixed/content separation,
provider identity changes, retained music facets, single-pass arguments, plural
selection, cache saturation and ownership cleanup. These automated checks do not
replace the separate real-TV acceptance procedure.
