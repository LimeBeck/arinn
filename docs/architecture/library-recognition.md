# Library season and episode recognition rules

The library provider reads seasons and episodes from folder and file names. Its
built-in markers are `SxxEyy`, `NxNN`, `TVn`, `Snn` and `Season N`
([ADR-0056](../adr/0056-local-library-scan-metadata-and-resume.md),
[ADR-0085](../adr/0085-anime-tv-season-markers.md),
[ADR-0109](../adr/0109-season-only-snn-markers.md)). Further markers are
recognition rules in configuration
([ADR-0128](../adr/0128-configurable-library-recognition.md),
[#658](https://github.com/LimeBeck/arinn/issues/658)). Adding a rule needs no
rebuild. The code is `plugins/library/src/recognition.rs`.

## Configuration

The rules are the `recognition` table of the provider's settings in
`config.toml` ([ADR-0064](../adr/0064-provider-settings-in-config.md)):

```toml
[providers.library.settings.recognition]
season = ["Staffel {n}", "Part {n}"]   # season markers, tried in this order
episode = ["Folge {n}"]                 # episode numbers in a season folder
disable = ["ordinal-season"]            # default rules switched off, by id
```

All keys are optional, and each list holds at most 32 entries. Any other key
makes the table invalid.

## Template grammar

- **Slot.** A template has exactly one number slot:
  - `{n}`: 1 to 4 digits, a value from 1 to 9999 (`07` is 7);
  - `{n:ord}`: an English ordinal whose suffix fits the number: `1st`, `2nd`,
    `3rd`, `4th`, `11th`, `12th`, `13th`, `21st`, `22nd`. `2th` is not a
    match.
- **Size.** A template has 1 to 4 words and at most 64 bytes.
- **Text around the slot.** Text may surround the slot inside one word:
  - `S{n}` matches `S3`, and `{n}x` matches `2x`. Names are cleaned before matching (a `.` becomes a space), so write `Vol {n}` for `Vol.2`;
  - a digit directly next to `{n}` is rejected (`S1{n}`);
  - so is a letter or digit directly after `{n:ord}`.
- **A letter.** The literal text must contain at least one letter, so a bare
  `{n}` (which would match any number) is rejected.
- **Matching.** The name is first cleaned as the parser always does: `.` and
  `_` become spaces, `[...]` groups are dropped, and a year is taken out.
  Template words are then compared with whole words of the name, ignoring
  case in any script. `Season {n}` never matches inside `Seasonal 2`.

## Where rules apply, and precedence

The built-in markers come first, and a configured rule is tried only where
none of them matches.

| Name | Built-in first | Then |
| --- | --- | --- |
| Folder that is only a marker (`2nd Season/`) | `TVn`, `Snn`, `Season N` | the first season rule whose match is the whole name |
| `<Title> <marker>` folder (`Show 2nd Season/`) | `TVn`, `Snn` | the first season rule whose match ends the name; the text before it is the series title |
| File name (`Show 2nd Season - 01.mkv`) | `SxxEyy`, `NxNN`, `TVn`, `Snn` | the first season rule, at its leftmost match, with an optional episode number right after it (`- 01`, `E01`, `#01`) |
| Episode number in a season folder | `SxxEyy` and the number after a season marker | the first episode rule, then the loose number of ADR-0085 |

Season rules are ordered with the user's rules first, as written (`season:1`,
`season:2`, …), then the enabled defaults. Episode rules are ordered the same
way (`episode:1`, …).

## Defaults

| Id | Kind | Template | Example |
| --- | --- | --- | --- |
| `ordinal-season` | season | `{n:ord} Season` | `Show 2nd Season/Show 2nd Season - 01.mkv` is season 2, episode 1 |

Numbered and ordinal film titles stay films, because a season rule needs its
season word: `The 2nd Coming (2001)`, `Film 2 (2019)`, `Fifth Season Rising`.

## Invalid rules

Invalid rules do not stop the provider:
- the defaults apply, and the library folders and switches still load;
- one warning at start, `{"operation":"recognition.rules","error":"InvalidArgument","detail":...}`,
  names the rule and the reason (for example
  `season:1 "{n}": needs a letter outside the slot, or it matches any number`);
- no path is logged.

## Trying rules first: the dry run

```sh
cat > rules.toml <<'TOML'
season = ["Staffel {n}"]
episode = ["Folge {n}"]
TOML
arinn-plugin-library --check-recognition --rules rules.toml \
  "Show 2nd Season/Show 2nd Season - 01.mkv" \
  "Show Staffel 2/Show Folge 03.mkv"
```

- **Names.** Each name is a path relative to a library folder. Only the names
  are read, no media.
- **Rules.** Without `--rules`, the rules come from `ARINN_PROVIDER_SETTINGS`,
  or the defaults apply.
- **Output.** JSON lines on stdout:
  - the active rules, with their ids, first;
  - then, for each name, `kind`, `title`, `series`, `season` and `episode`;
  - an `ambiguous` list when several season rules read different numbers from
    one folder or file name. The other fields show which reading applies.
- **Exit codes.** 1 with the reason for invalid rules; 2 for wrong arguments.

## Applying a change

1. **Try the rules.** Run the dry run on representative names.
2. **Edit `config.toml`.** Put the rules in `[providers.library.settings.recognition]`.
3. **Restart LibraryProvider only.** Rules are read at provider start. A
   `SIGHUP` rescan does not reread them. Restart the provider, or reload the
   daemon's provider registry. The shell and the system keep running.
4. **Check the result.** The start scan reclassifies the catalogue. Every file
   keeps its content id, so the following survive:
   - resume and watched state;
   - artwork references;
   - the first-discovery time used by Home's New row.

   A film that becomes an episode is not duplicated. No file is renamed,
   moved or linked.

Live reload and editing the rules on the Settings screen are not part of this
increment. The table is not in the provider's `ProviderSettings1` schema,
because its value kinds cannot express a rule list.
