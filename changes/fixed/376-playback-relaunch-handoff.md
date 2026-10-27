- Fence old Release failures when selecting a new media session, ignore stale Home snapshots during presentation, and bound the wait for Qt activation on initial playback and explicit Return without requesting focus ([#376](https://github.com/LimeBeck/arinn/issues/376)).

- Retain the playback overlay when authenticated native shell focus corroborates a missing Qt activation event, and retry unresolved window reports within the existing deadline without accepting changed window identities.

- Wait for the exact new media session when resuming a stopped title from Home, including Back before its catalog row arrives; keep Home as the return destination.

- Keep the native shell alpha channel across Home and playback cycles, with opaque shell backgrounds drawn separately from the transparent video overlay.
