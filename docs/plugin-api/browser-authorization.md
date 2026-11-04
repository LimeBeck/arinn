# Temporary browser authorization (pre-stable)

[ADR-0141](../adr/0141-temporary-browser-authorization.md) introduces
[BrowserAuth1](browser-auth1.xml) at `/org/arinn/Daemon1` for provider-hosted
browser confirmation. This route does not record Arinn browsing recents. It carries
HTTPS verification pages and opaque UUID request identities, never tokens or
application secrets. No shell-supplied JavaScript or plugin UI is involved.

Isolation here covers Arinn recents, extension storage and exported tab snapshots.
The regular browser profile may retain navigation history, cookies and site data
under its own/user policy. This route neither reads nor clears that browser-owned
data and does not create an incognito session. It requests no history permission.
Closing the temporary window is not a claim that a navigation was removed from
the browser profile. The initial Device Flow verification address contains no
personal access token or device code; downstream login navigation remains subject
to browser policy.

`Open(url, request_id) → operation` admits at most one lease per calling unique
bus owner and sixteen overall. Replaying its payload is idempotent; a different
payload conflicts. Admission mints a separate provider-private UUID and pins the
browser provider's unique owner and registration generation before dispatch.
The provider's `OpenTemporaryUrl(generation, internal_id, url) → window_id` creates
a fresh owned window without normal origin/window reuse. Unsupported providers
fail explicitly. Normal Browse1.Open remains unchanged.

`Close(request_id)` revokes before awaiting the owned Open, then reconciles
`CloseTemporaryWindow(generation, internal_id)` against the same provider and
generation. A timeout is an uncertain mutation, not proof that no window exists.
Failed cleanup retains its lease for retry and prevents another launch by that
caller. Caller loss revokes and settles the lease; provider generation loss does
not retarget old cleanup onto a new registration. SDK reset revokes outstanding
work before a new registration may accept launches. Once public Open is admitted, provider Close may precede provider Open dispatch;
bounded provider cancellation tombstones stop that queued late mutation.
Public Close for an identity never admitted by this caller is an idempotent
no-op, not prospective cancellation. Callers first obtain the Open operation
before requesting Close.

The Chrome implementation persists opaque window/tab handles in session storage,
never authorization URLs/codes/tokens. A fresh unfocused placeholder establishes
ownership before navigation/focus. Native-port loss revokes its leases. Startup
reconciles persisted windows, orphan placeholders and descendant tabs; ownership
uncertainty suppresses snapshots until recovery. Child tabs remain private and
close with their parent lease. Live child admission and retry proof are bounded;
exhausting emergency cleanup proof disables launches and snapshots for the rest
of that browser session, including worker restarts. Restarting the browser clears
that session latch. Cleanup remains permitted. This exceptional fail-closed state
must not be represented as successful sign-in.

The provider decides when service authorization completes. It closes its lease
and waits for cleanup before requesting Home. Browser completion does not imply
service API access; providers verify their service separately. OAuth response
query/fragment fields and stale callback titles are redacted defensively before
ordinary snapshots can cross the native messaging boundary.
