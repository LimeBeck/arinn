- Request Home when the shell starts in an unlocked session, waiting for daemon
  and platform readiness so a successful autostart does not leave Arinn hidden.
  The startup request is not replayed after later daemon reconnects or unlocks.
