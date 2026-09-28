# Changelog fragments

Every pull request that changes something a user, plugin author or contributor can notice adds **one fragment file** here instead of editing `CHANGELOG.md`. Separate files mean parallel branches never conflict on the changelog ([ADR-0034](../docs/adr/0034-parallel-development.md)).

- Put the file into the directory of its [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) category: `added/`, `changed/`, `deprecated/`, `removed/`, `fixed/` or `security/`.
- Name it after the issue or topic: `changes/fixed/42-provider-timeout.md`. Names must be unique; the issue number makes them so.
- Write one or more Markdown bullets (`- …`) in English. Links are relative to the **repository root**, because the text is moved into `CHANGELOG.md` as is.
- A change can have fragments in several categories.
- Changes nobody notices (refactoring, CI tweaks, typo fixes) need no fragment: label the pull request `skip-changelog`.

At release time the maintainer runs `python scripts/release.py collect X.Y.Z`, which moves all fragments into a `## [X.Y.Z]` section of `CHANGELOG.md` and deletes them. `python scripts/release.py unreleased` previews the pending notes.
