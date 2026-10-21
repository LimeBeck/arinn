# ADR-0109: Season-only `Snn` markers in library names

- **Status:** Accepted
- **Date:** 2026-10-21
- **Deciders:** owner decision of 2026-10-06 in [#361](https://github.com/LimeBeck/arinn/issues/361) (a standalone `Snn` is a season marker by the same rules as `TVn`); delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) for the exact pattern
- **Amends / Supersedes:** [ADR-0085](0085-anime-tv-season-markers.md) (*Marker*, *Folders*)
- **Amended by / Superseded by:** —
- **Specification:** [§7.4](../specifications/arinn-specification.md#74-local-library-and-mpv)

## Context

ADR-0085 marks anime seasons with `TVn`. Many release folders carry the season
only as a standalone `S01` at the end of the folder name, with no `E`:

```text
Show Name [WEB-DL 1080p] S01/
    Short - 01 [1080p AVC] [RUS MVO+SUB] [ENG SUB].mkv
    Short - 02 [...].mkv
```

The library knew `SxxEyy`, `NxNN`, `Season N` and `TVn`, so this folder was
not a season folder, the files' ` - 01` numbers were never read, and every
episode was catalogued as a film. The issue was reported by the owner on
2026-10-06; the names above are synthetic.

## Decision

The library provider (`plugins/library/src/parse.rs`, `find_season_marker`)
treats a standalone `Snn` as a season marker wherever ADR-0085 accepts `TVn`.

- **Marker.** `S` immediately followed by 1–2 digits, case-insensitive
  (`S1`, `S01`, `s2`). No letter or digit, in any script, may touch it on
  either side, as for `TVn`. Unlike `TVn` no space or `-` is allowed between
  `S` and the digits: a lone `S` is common in titles (`Plan S 2`). A `[...]`
  group that holds only the marker (`[S02]`) is opened before cleaning.
- **Precedence.** The marker counts only when the name has no `SxxEyy` or
  `NxNN` marker; those keep precedence. `S01E02` itself is not a `Snn`
  marker, because the boundary after the digits fails. `MS01`, `S100` and
  `S01x` are not markers either.
- **Folders.** A folder that is only `Snn` is a season folder like
  `Season N` and `TVn`; the series is the folder above it. A folder
  `<Title> … Snn` is season `n` of `<Title>`: `[...]` groups and a year are
  dropped from the title, and the series key is the normalized title, so
  `<Title> S01/`, `<Title> S02/` and `<Title> TV3/` form one series.
- **File names.** `Show S01 - 05.mkv` is season 1, episode 5, like
  `Show TV1 - 05.mkv`; the episode number right after the marker follows
  ADR-0085.
- **Episode number inside such a folder.** The loose ADR-0085 rules apply:
  the whole name, ` - 05`, `E05`/`EP05`/`#05`, a trailing number of up to
  three digits.

All other ADR-0085 rules are unchanged.

## Consequences

- Release folders marked only `Snn` appear under Series with seasons and
  episode numbers instead of as many films.
- A film whose name or folder has a standalone `S1`–`S99` token becomes an
  episode, as a standalone `TVn` already did. A `.nfo` still does not change
  the kind. Existing items keep their UUIDs: identity is the path or
  fingerprint, not the parsed fields.
- Like `<Title> TVn`, a `<Title> Snn` series has no folder of its own, so no
  `tvshow.nfo` or folder artwork applies to it.

## Alternatives considered

- **Allow `S 01` and `S-01`, as `TV 1` and `TV-1`.** Rejected: a single
  letter followed by a number is common in titles, and the owner report has
  only the compact form.
- **Accept `Snn` only in folder names.** Rejected: the owner decision applies
  the `TVn` rules, which include file names.

## Verification

- `plugins/library/src/parse.rs` unit tests:
  - `season_only_folder_is_a_season_of_one_series`;
  - `season_only_marker_in_the_file_name_is_a_season`;
  - `season_only_lookalikes_stay_films`.
- The `plugins/library/tests/catalogue.rs` scan test
  `season_only_folders_form_one_series`.
- Disabling the `S` prefix fails the folder, file-name and scan tests;
  disabling the `SxxEyy`/`NxNN` precedence check fails the lookalike test.
