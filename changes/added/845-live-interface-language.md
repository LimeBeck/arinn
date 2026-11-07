- Settings can switch the interface language without restarting Arinn. Partial local
Qt catalogues and source-checked provider setting translations can override the
installed copy and be reloaded from the language picker. Cached music room and
search headings and stored source-command errors also retranslate locally (#845,
#846).
- Forward provider source catalogues, exact-locale QM files and snapshot-bound
  fixed-text references through the daemon, including original download revision
  mapping and provenance-only publication changes (#859, #846).
- Export editable shell/provider catalogues and provider setting templates from
  installed Arinn with `arinnctl translations`, check partial translations and
  atomically compile them with Qt Linguist without a source checkout (#846).
- Render explicitly associated fixed provider messages through private per-plugin QM dictionaries, including cached music station/entity labels, media fallback titles and download errors, with one-pass typed formatting and live local reload; keep content unchanged and bound metadata/cache work.
