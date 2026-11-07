- Settings can switch the interface language without restarting Arinn. Partial local
Qt catalogues and source-checked provider setting translations can override the
installed copy and be reloaded from the language picker. Cached music room and
search headings and stored source-command errors also retranslate locally (#845,
#846).
- Forward provider source catalogues, exact-locale QM files and snapshot-bound
  fixed-text references through the daemon, including original download revision
  mapping and provenance-only publication changes (#859, #846).
