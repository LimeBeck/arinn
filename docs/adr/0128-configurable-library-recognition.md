# ADR-0128: Configurable season and episode recognition rules in the library

- **Status:** Accepted
- **Date:** 2026-11-01
- **Deciders:** owner decision of 2026-11-01 in [#658](https://github.com/LimeBeck/arinn/issues/658) (recognition markers move into configuration, applied by a restart of only the library provider, with a dry run); delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) for the grammar, limits and precedence
- **Amends / Supersedes:** — (adds markers beside those of [ADR-0085](0085-anime-tv-season-markers.md) and [ADR-0109](0109-season-only-snn-markers.md); their rules are unchanged)
- **Amended by / Superseded by:** —
- **Specification:** [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv)

## Context

The library provider recognises seasons and episodes in folder and file names
with markers built into the parser: `SxxEyy`, `NxNN`, `TVn`, `Snn` and
`Season N` (ADR-0056, ADR-0085, ADR-0109). Every new naming form has needed a
code change and a new package. #658 reports an ordinal form, `2nd Season`, in
both the folder and the file name; such an episode was catalogued as a film.
The names below are synthetic:

```text
[Fixture] Synthetic Show 2nd Season/
    [Fixture] Synthetic Show 2nd Season - 01.mkv
```

The owner asked to move the markers into configuration, so that adding a
pattern needs no rebuild, with built-in defaults, user additions and
overrides, a dry run before a rule reaches the catalogue, a usable fallback
for invalid rules, and no renaming, moving or symlinking of media.

## Decision

- **Rules.** A rule is a template matched word by word against a cleaned
  name: `.` and `_` become spaces and `[...]` groups are dropped, as the
  parser already does.
  - **Slot.** A template has exactly one number slot: `{n}` (1–4 digits, 1 to
    9999) or `{n:ord}`, an English ordinal whose suffix must fit the number
    (`1st`, `2nd`, `3rd`, `11th`).
  - **Words.** It has 1–4 words and at most 64 bytes. Text may surround the
    slot inside a word (`S{n}`, `{n}x`), and the literal text must contain
    at least one letter, so a bare `{n}` is rejected.
  - **Matching.** Matching ignores case (Unicode) and covers whole words only.
- **Two lists.** Season rules give a season marker:
  - a folder that is only the marker is a season folder;
  - `<Title> <marker>` at the end of a folder name names season *n* of
    `<Title>`;
  - in a file name the marker is followed by an optional episode number.

  Episode rules give the episode number in a season folder.
- **Defaults and precedence.**
  - **Built-in markers first.** They keep their meaning and are tried first:
    `SxxEyy`, `NxNN`, `TVn`, `Snn` and `Season N`.
  - **Season rules after them.** The user's season rules come first, in the
    order written, then the enabled default rules. The first rule that
    matches wins, at its leftmost match. For `<Title> <marker>` it is the
    first rule whose match ends the folder name.
  - **Episode rules before the loose number.** In a season folder, episode
    rules are tried before the loose episode number of ADR-0085.
  - **The default.** The only default rule is `ordinal-season`,
    `{n:ord} Season`, so #658's names need no configuration. A default is
    disabled by its id.
- **Configuration.** The rules live in the provider's settings table
  (ADR-0064), which the daemon passes through and keeps when the Settings
  screen saves other keys:

  ```toml
  [providers.library.settings.recognition]
  season = ["Staffel {n}"]
  episode = ["Folge {n}"]
  disable = ["ordinal-season"]
  ```

  Each list has at most 32 entries. An unknown key, or a `disable` id that
  names no default rule, makes the rules invalid.
- **Fallback.** Invalid rules do not stop the provider. The defaults apply,
  the roots and other settings still load, and one warning at start names
  the rule and the reason, never a path.
- **Applying a change.** Rules are read at provider start (ADR-0064). Changed
  rules take effect when LibraryProvider restarts and its start scan runs;
  the shell and the system keep running. The scan keeps each file's content
  id, so resume and watched state, artwork references and discovery times
  survive a reclassification. No duplicate is left, and nothing on disk is
  touched.
- **Dry run.** `arinn-plugin-library --check-recognition [--rules FILE] NAME...`
  reads sample names relative to a library folder and prints JSON lines:
  - the active rules first;
  - then, for each name, its kind, title, series, season and episode;
  - an `ambiguous` list when several season rules read different numbers
    from one folder or file name.

  `FILE` is TOML with the table's keys. Without it, the rules come from
  `ARINN_PROVIDER_SETTINGS`, or the defaults apply. Invalid rules exit with
  code 1 and the reason. The dry run touches no media and no catalogue.

## Consequences

- A new naming form is a configuration line plus a provider restart, tried
  first with the dry run; no package rebuild is needed. Changing the grammar
  itself (new slot kinds, regular expressions) still needs code.
- The built-in markers cannot be disabled. A user rule cannot override them;
  it can only add forms they do not cover. This keeps every existing library
  reading as before.
- Live reload without a provider restart, and editing the rules from the
  Settings screen, are later improvements. The table is not part of the
  provider's `ProviderSettings1` schema: its kinds cannot express a rule list.
- `docs/architecture/library-recognition.md` is the technical contract for
  the grammar, limits, defaults and steps.

## Alternatives considered

- **Regular expressions in configuration.** Rejected: unbounded matching
  cost, easy false positives on film titles, and errors that a TV user cannot
  read. Templates cover the reported forms with finite, word-bounded
  matching.
- **Only a new built-in ordinal marker.** It fixes #658, but the next form
  would again need a release. The owner asked for configuration.
- **User rules ahead of the built-in markers.** Rejected for the first
  increment: one rule could change how existing libraries are read. The
  precedence above only adds readings.
- **Keeping the last valid rules in the catalogue database.** Not needed: the
  defaults are a usable fallback, and the warning names the rejected rule.

## Verification

- `plugins/library/src/recognition.rs`:
  - the ordinal default matches whole words, with the right suffixes;
  - user rules come first, defaults can be disabled, and ambiguity is visible;
  - invalid rules are rejected with a reason;
  - numbers are bounded.
- `plugins/library/src/parse.rs`:
  - `an_ordinal_season_is_an_episode_with_the_default_rules`: the #658
    reproduction and its control;
  - `numbered_film_titles_are_not_seasons`;
  - `configured_rules_change_recognition`: a custom rule without a rebuild,
    and disabling the default.
- `plugins/library/src/store.rs`: invalid rules fall back to the defaults with
  a reason that contains no path, and the roots still load.
- `plugins/library/tests/catalogue.rs`,
  `changed_rules_reclassify_without_losing_identity_or_progress`: a film
  becomes an episode and keeps its id, resume position and discovery time,
  with no duplicate.
- `plugins/library/src/check.rs`: the dry run reports kind, series, season and
  episode, and reports invalid and ambiguous rules and usage errors.
