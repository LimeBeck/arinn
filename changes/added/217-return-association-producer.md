- Export the optional Return/window association from the production daemon and
  report exact saved-window evidence from the KDE adapter (ADR-0073, #217).
  Serialize Home/inventory invalidation and reject stale browser links. Shell
  consumption and real KDE qualification remain separate follow-ups.
- Recover from transient owner lookups and unacknowledged association reports
  without exposing unverified identity, and keep bus round-trips outside the
  provider/inventory locks (PR #218 review follow-up).
- Fence delayed Home authorization so an obsolete owner cannot overwrite a newer
  Home commit, including one with identical public association content.
