# ADR-0126: Verified application identity behind a media proxy

- **Status:** Accepted
- **Date:** 2026-11-01
- **Deciders:** delegated technical choice under ADR-0037, task #663; implements the existing owner-approved single browser/media card and ADR-0117 fallback
- **Amends / Supersedes:** ADR-0096 process-evidence producer semantics; complements ADR-0117
- **Amended by / Superseded by:** —
- **Specification:** §5.1, §7.1, §7.3, §11, §13; AC-09, AC-16; R-01–R-05

## Context

A browser can expose media only through a delegated endpoint. The MPRIS endpoint
then belongs to a helper process while BrowserProcess1 identifies the browser.
The current reference producer reports the helper's process, so equality cannot
hold even while exactly one tab is audible. ADR-0117 already verifies the proxy's
direct browser-parent identity for source precedence, but fallback-only endpoints
cannot currently use that evidence for the existing strict join.

The owner reported duplicate Continue cards. Read-only inspection confirmed a
single delegated endpoint, matching kernel parent/proxy claim and native-host
browser parent, and no association. No process IDs, media payloads or paths are
retained. This contract prerequisite changes no runtime code or UI projection.

## Decision

MediaProcess1 reports the verified **application process controlling the media**.
For direct endpoints this is the endpoint owner's process, as before. For a
recognized delegated endpoint the provider may instead report its represented
application process only when all of these hold:

- the endpoint's unique D-Bus owner is pinned and current;
- the process ID of that owner is read from D-Bus, not parsed from a name;
- the provider recognizes a documented delegation mechanism;
- the mechanism's application-process claim is positive and agrees with the
  endpoint process's direct kernel parent;
- the evidence belongs to the currently published session and remains valid at
  the read boundary; a changed owner, generation or process invalidates it.

Do not traverse arbitrary process ancestry. A provider must not infer delegation
from titles, URLs, matching duration/position or arbitrary numeric metadata.
Unverified endpoints retain direct-owner evidence or return 0 (unknown); an
unpublished session is NotFound. Registration/caller checks still precede argument
validation, and only the registered daemon owner reads process evidence.

The reference MPRIS producer recognizes the Plasma Browser Integration mechanism
already described by ADR-0117: its documented name family identifies the protocol,
not a process ID; `kde:pid` must match the direct kernel parent of the pinned bus
owner. Use the verified parent for a published fallback proxy even when no native
MPRIS endpoint exists. Native-over-proxy precedence stays unchanged. Suppression
must clear the proxy's published snapshot so a pending/failed subsequent read
cannot resurrect old media metadata.

Changes to effective process evidence must advance the provider's snapshot
revision even when visible session fields do not change. Revalidate evidence on
read; a stale snapshot must never return another owner's identity. Unknown or
lost evidence cannot invent a link.

No method, signature, feature name, authorization or limit changes. Existing direct
producers and consumers remain compatible. The daemon remains generic and keeps
all ADR-0096 uniqueness, audible-tab, provider-set and post-await snapshot checks.
In particular, this does not establish links from a paused cold start: a link is
created only with one audible tab and then survives pause under the held-link
rules. No process identity reaches QML, logs, telemetry or persistent storage.

## Consequences

A verified proxy-only browser can participate in the existing association join.
Unknown or ambiguous cases remain separate rather than guessing which tab owns
media. Browser-internal placeholder tabs in the Home summary require a separate
presentation fix. This contract does not qualify real desktop behaviour.

## Alternatives considered

- Treating every child process as browser-owned media would merge unrelated apps.
- Matching page URLs or titles is not identity evidence.
- Disabling the delegated endpoint loses the supported fallback.
- Weakening the audible-tab or unique-session rules changes owner-approved policy
  and does not repair the mismatched process evidence.

## Verification

Implementation must cover proxy-only delegation and strict joins, unverified
claims, owner changes, process-evidence revision changes, suppression followed by
failed reads, native precedence and fallback, multiple endpoints, and caller/
generation/session fencing on a private bus. This documentation change claims no
runtime regression test or real-desktop/operator qualification.
