# Metadata in named season folders

Local series NFO and artwork implement specification §7.4 / AC-15 and
[ADR-0056](../adr/0056-local-library-scan-metadata-and-resume.md).
Named season folders such as `Show TV2`, `Show S02` and `Show 2nd Season`
keep name-based series grouping across siblings
([ADR-0085](../adr/0085-anime-tv-season-markers.md),
[ADR-0109](../adr/0109-season-only-snn-markers.md),
[ADR-0128](../adr/0128-configurable-library-recognition.md)).
That identity must not prevent an unambiguous local sidecar from being read
([#746](https://github.com/LimeBeck/arinn/issues/746)).

The library keeps the existing series key and content identifiers. A directory
series continues to use its directory for `tvshow.nfo`, poster and fanart.
For a name-grouped series, a containing folder is usable only when all known
episodes share that one non-root folder and it contains no catalogue media
belonging to another group. The existing configured season parser must also
identify that folder as the same named series. A generic `Season 2` folder
does not qualify, even if only one of its shows has been imported so far.
No single sibling season folder is selected when
episodes span several folders; a shared library root is never a fallback.
This does not introduce online enrichment or a new source-precedence policy.

The series row's existing `rel_path` field records the resolved local artwork
location in the already supported `dir:` form, independently of `series_key`.
The asset provider still validates the published asset id and reads beneath the
configured root. No public path, new schema, IPC method or content id is added.
An ambiguous fallback clears its former series-wide sidecar metadata instead of
publishing an artwork id that can no longer be resolved. Missing series keep
the existing offline-description behavior.

Isolated catalogue tests cover localized NFO title/year/plot, both artwork reads,
rescan stability, content ids, discovery times and resume state. Adding another
named season must preserve one grouped series without choosing either season's
sidecars. Root-level and mixed-folder fixtures prevent cross-series metadata.
These are filesystem/provider fixtures, not real-desktop operator qualification.
