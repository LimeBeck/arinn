# ADR-0141: Temporary browser authorization and verified music access

- **Status:** Proposed
- **Date:** 2026-11-04
- **Deciders:** Owner requested browser sign-in and explicitly authorized CloudMus-compatible SDK registration with its embedded secret; maintainer reviews the contract/security change
- **Amends / Supersedes:** [ADR-0043](0043-web-shortcuts.md) and [ADR-0048](0048-browser-wire-contract.md), extension permission policy only
- **Amended by / Superseded by:** —
- **Specification:** §5.3.1, §7.1, R-01–R-10, §13

## Context

Task [#771](https://github.com/LimeBeck/arinn/issues/771) follows the merged
local/Yandex implementation #710. The owner's TV completed OAuth using the Arinn
registration but received Music API 403. CloudMus uses the SDK's Android
registration. On 2026-11-04 the owner explicitly chose compatibility, permitting
its bundled application secret. The earlier public-client-only constraint is
superseded; successful login alone must not produce a connected empty library.
Ordinary browsing can reuse windows and record recents, unsuitable for sign-in.

## Decision

Add a generic caller-scoped temporary browser lease through the daemon, protocol
and optional SDK handler. Keep product names out of core. Pin provider owner and
generation, mint internal lease identities, and retain uncertain mutations until
cleanup settles. Use fresh windows, isolate child tabs, exclude authorization
metadata from snapshots/history and revoke on caller/native-port/generation loss.
This proposed amendment replaces the earlier two-permission requirement with
exactly `tabs`, `nativeMessaging` and `storage`. The last permission is confined
to `storage.session`: opaque authorization handles and a fail-closed recovery
flag only. No authorization/browsing content or local/sync authorization storage
is permitted. Owner review/merge accepts this security-policy amendment; the
historical phase-3 pass is not evidence that the new permission was approved.
The [wire lifecycle](../plugin-api/browser-authorization.md) defines limits,
privacy recovery and the exceptional browser-session fail-closed latch.

The Yandex provider uses the SDK registration data pinned and attributed in
`compatibility.rs`, with its existing native Device Flow implementation. It opens
the verification page in the browser, polls the personal token and stores it in
Secret Service. There is no copied SDK implementation or Python dependency.
Custom client overrides remain possible but do not inherit another client's
secret. Credentials bind their issuing client; old/unknown registrations require
fresh sign-in. This is an owner-authorized compatibility choice, not official
service permission or a promise that the third-party registration remains valid.

Probe account status and authenticated liked tracks before `connected`, including
restore. A valid empty library succeeds. Forbidden becomes an explicit restricted
account; temporary API failure preserves credentials and reports unavailable.
Close the owned browser work before returning Home. No token reaches core or UI.

## Consequences

Chrome needs session-storage permission and two additional packaged assets.
Transient cleanup failure delays new sign-in rather than losing ownership.
Keyring availability and live service eligibility remain required. The owner
merges this security/contract change; this ADR remains proposed until review.

## Alternatives considered

- Own-registration Authorization Code + PKCE: solves secret distribution but
  does not grant Music API access. It is not the selected default.
- PKCE with the SDK registration: no qualified registered redirect URI exists in
  this task; do not infer that a third-party loopback redirect is accepted.
- Ordinary Browse1.Open: reuse/history violate temporary authorization isolation.

## Verification

Synthetic provider tests cover OAuth success followed by Music API denial,
issuer migration, successful browser handoff and failed cleanup retry. Private
D-Bus tests cover caller/lease scope, cancellation, late create after timeout and
cleanup retention. Extension tests cover placeholder/child privacy, restart,
retry, overflow and bounded proof. QML tests cover explicit denial and focus.
Full required CI/package checks and real-TV library/audio qualification must be
recorded separately. Synthetic passes do not establish live account access or
phase/Plugin API stability gates.
