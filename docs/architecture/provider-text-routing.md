<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Fixed provider text routing

The daemon implements the additive `org.arinn.Texts1` interface at its root,
following [ADR-0135](../adr/0135-localized-provider-texts.md) and the existing
[wire contract](../plugin-api/provider-texts.md). This guide describes the daemon
part of the integrated localization implementation in #863 (#845/#846/#859).
The shell's private rendering, local overlays and Local music/Yandex Music
adapters are also implemented; see [UI translations](ui-translations.md) for
their architecture and installed-copy workflow. Routing preserves existing
content records and leaves the language choice to the client.

## Catalogue and resource lifetime

Every request resolves the instance id to its registered stable plugin id,
unique bus owner and generation. An unregistered instance is unavailable;
an expected generation that differs is a conflict. Calls go to that unique
owner's standard provider path. The identity is checked after awaited work;
changing an owner, installation or generation invalidates the response.

`GetCatalog` probes the optional `ProviderTexts1`. Unknown interface/method
means unsupported, so existing plain text stays usable. The typed catalogue
parser checks schema, source locale, message revisions, parameters, duplicate
keys and bounds, including exact agreement with the registered plugin id.
Concurrent reads share one initialization per retained registration entry.
Absent support is remembered; transient failures and invalid data can retry.

`GetTranslation` accepts one canonical, explicitly declared locale. Undeclared
locales return NotFound; fallback belongs to the client. Bytes are opaque here:
the daemon has no Qt dependency and does not claim that a QM is safe. It checks
the 240 KiB per-file bound, immutable size/digest per locale and the 8 MiB
aggregate export budget as resources are requested. The client must additionally
check self-containment and keep each plugin's dictionary in its own lookup scope.

All endpoint work, including locking and response validation, has one three-second
deadline. Sixteen in-flight calls are admitted; excess localization calls fail
without queuing provider commands or making content unavailable. The registry
retains at most 64 provider entries in least-recently-used order. Eviction permits
a later probe again; it never reuses another generation's metadata. Each source
catalogue retains the existing 240 KiB/1024-message limits. No QM bytes are cached
by the daemon, only up to 64 locale size/digest records per retained entry.

## Snapshot associations

`Resolve` parses and bounds the original request list before calling a provider.
The daemon forwards only fields it can bind to an observed original publication:

| Scope | Binding |
| --- | --- |
| `provider` | Generation-bound provider metadata. The name must also agree with the current ProviderInfo; the optional description is declared by the provider because the current ProviderInfo has no description field |
| `music.entity` | A retained validated music page with the same Page.revision, local entity id and title; independent materialized facets may have different valid revisions |
| `music.station` | The current cached music state revision, station id and title |
| `media.session` | The current media snapshot revision, local session id and title |
| `download.job` | The current projected download revision, local job id and error text, mapped to the original job publication revision |

The page observer retains at most 512 entity fields and 256 KiB of ids/titles
per provider, bound to `(Page.revision, entity id)`. A newer observed version
of the same id supersedes its old proof, including a provenance-only change;
it does not discard unrelated ids from other facets. The Music API retains
independent immutable materialization leases, so a newer page or
`State.catalogue_revision` alone does not expire an older observed page.
The provider remains authoritative about which original snapshots it still
recognizes and omits expired provenance. The observer bounds retained pages and
refuses to rebind an id to different text under the same revision. Eviction
omits localization for an unretained field; it does not remove content. Ordinary
titles are still candidates only: the provider must omit content from its
resolution response, even if its visible title equals a fixed label.

The fixed-text contract requires provider-declared metadata to stay immutable
within a generation. The existing daemon also supports a metadata-only manifest
reload without restarting that generation. This increment preserves that
behavior: a renamed ProviderInfo is checked by its current observed name and
does not inherit a reference for the old name. Clients must invalidate displayed
metadata references when that observed name changes, as well as on generation
changes. A different metadata lifecycle is outside this transport change.

Filtered request indices are mapped back to the client's original list. Results
must have valid unique indices, exact fallbacks and catalogue-compatible message
revisions/typed arguments. After the response, the daemon rechecks registration
identity and each original publication. A changed or expired publication drops
that association. The client must also check its own latest observed identity
and retain references, rather than translated strings, for live retranslation.

## Download projection

The daemon adds import state and library references to a provider's jobs, so the
public download revision can differ from the provider's job revision. The text
router accepts the public revision, resolves against the current original job
revision, and returns indices bound to the original public request.

A provider may advance only localization provenance while leaving all job fields
identical. When that happens, the daemon also touches its projected snapshot and
emits its ordinary revision change. Otherwise a client could keep an obsolete
fixed/content association indefinitely. An in-progress source update whose
projection is not yet published is omitted from resolution.

## Verification

Private-bus tests use a synthetic provider with a new opaque plugin id. They
cover catalogue identity, exact locale transport, legacy absence, malformed or
oversized data, filtered indices, content omission and owner/generation/snapshot
changes while awaiting a reply. Download tests cover revision mapping and an
unchanged plain projection under a new provenance revision. No real-desktop
operator test is required for this transport increment.
