# ADR-0083: Anime `TVn` season markers in library names

- **Status:** Accepted
- **Date:** 2026-10-04
- **Deciders:** owner request ([#311](https://github.com/LimeBeck/arinn/issues/311)); delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) for the exact patterns
- **Amends / Supersedes:** [ADR-0056](0056-local-library-scan-metadata-and-resume.md) (*Names*)
- **Amended by / Superseded by:** —
- **Specification:** [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv)

## Context

ADR-0056 recognises an episode only by `SxxEyy`, `NxNN` or a `Season N`
folder. Anime releases usually mark the season as `TV1`, `TV2`, `TV-4` and
number episodes absolutely (` - 05`, `[07]`). Such files were catalogued as
films. The owner keeps three layouts:

```text
Show/[Group] Show TV2 - 05 [1080p].mkv     TV in the file name
Show TV1/01.mkv                             TV in the folder name
Show/TV1/01.mkv                             series folder with TV sub-folders
```

## Decision

The library provider (`plugins/library/src/parse.rs`) adds these rules. All
ADR-0056 rules keep their precedence: `SxxEyy` and `NxNN` win.

- **Marker.** `TV` plus an optional space or `-` (after `.` and `_` became
  spaces) plus 1–2 digits, case-insensitive. It must stand alone: `MTV2` and
  `TV 2000` are not markers. A `[...]` group that holds only a marker
  (`[TV-2]`) or an episode number of up to three digits with an optional
  revision (`[07]`, `[07v2]`) is opened before cleaning. Other groups are
  still dropped.
- **Marker in a file name.** It sets the season. An episode number right
  after it (`05`, `- 05`, `E05`, `EP 05`, `Episode 5`, `#05`, optional `v2`)
  sets the episode. The series title follows ADR-0056: the series folder,
  otherwise the text before the marker. The episode title is the text after
  the number.
- **Folders.** A folder that is only a marker (`TV1`) is a season folder like
  `Season N`; the series is the folder above it. A folder `<Title> TVn` (an
  optional year allowed) is season `n` of the series `<Title>`. Its key is the
  normalized title, so `Show TV1/` and `Show TV2/` are one series. That series
  has no folder of its own, so no `tvshow.nfo` or folder artwork applies to it.
- **Episode number inside a season folder.** This covers `Season N` and the
  `TV` forms. A name without a marker takes its episode from, in order:
  - the whole name (`01`);
  - ` - 05`;
  - `E05`, `EP05` or `#05`;
  - a trailing number of up to three digits (`Show 05`).

  Without any of these the episode number stays unset, as before.

## Consequences

- Anime in these layouts appears under Series with seasons and episode
  numbers instead of as many films.
- A file inside a `Season N` folder can now gain an episode number it lacked
  (`Season 1/Show - 03.mkv` becomes episode 3). Its item keeps the same UUID:
  identity is the path or fingerprint, not the parsed fields.
- A film whose name has a standalone `TVn` token becomes an episode. A `.nfo`
  still does not change the kind.
- Absolute numbering without any season marker (`[Group] Title - 05.mkv` in a
  plain folder) stays a film. That needs a separate rule.

## Alternatives considered

- **Configure the kind per library root.** Rejected for now: it is a product
  setting (owner-gated) and does not tell seasons apart.
- **Treat every ` - NN` name as an episode.** Rejected: film titles such as
  `Title - 2` would turn into episodes.

## Verification

- `plugins/library/src/parse.rs` unit tests:
  - `tv_marker_in_the_file_name_is_a_season`;
  - `tv_folders_are_seasons_of_one_series`;
  - `tv_lookalikes_stay_films`.
- The `plugins/library/tests/catalogue.rs` scan test
  `tv_season_folders_form_one_series`.
- Disabling the `<Title> TVn` folder rule fails the folder test.
