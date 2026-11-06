<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Fixed provider text

The optional [ProviderTexts1](provider-texts1.xml) interface on
`/org/arinn/Provider1` supplies localization metadata without changing existing
records. [ADR-0135](../adr/0135-localized-provider-texts.md) explains the boundary.
The SDK exports it with `runtime::serve_texts` and `TextsHandler`.
The daemon-side [Texts1](texts1.xml) is specified here but not yet implemented.

## Discovery and lifetime

The daemon probes `GetCatalog(generation)` once per provider registration
generation. UnknownInterface/UnknownMethod means no fixed-text support; existing
plain strings remain usable. No existing `Features` list or required manifest
field changes. The deadline is bounded by the same three-second policy as
settings schema reads. Failed localization never makes content unavailable.

The catalogue, plugin identity and exact supplied locale set are immutable for
the export. SDK calls require the current registration's unique daemon owner
and generation. Absence is `Unavailable`, a different caller is
`PermissionDenied`, and a stale generation is `Conflict`. Resolution rechecks
after awaiting the handler. The daemon must also recheck provider owner,
generation and original snapshot before forwarding a response.

## Source catalogue

JSON uses camelCase field names and rejects unknown fields and duplicate map
keys. `Catalog` in `arinn_protocol::localization` is authoritative.

```json
{
  "formatVersion": 1,
  "pluginId": "third-party-radio",
  "sourceLocale": "en",
  "locales": ["ru"],
  "messages": {
    "connect.failed": {
      "revision": 1,
      "source": "Could not connect to %1",
      "arguments": {"service": {"position": 1, "type": "text"}},
      "comment": "A fixed connection error; service is content"
    },
    "stations.count": {
      "revision": 1,
      "source": "%n station(s)",
      "numerus": true
    }
  }
}
```

- `formatVersion` is 1 and `sourceLocale` is `en`. The daemon checks `pluginId`
  against the registered installation; a field cannot impersonate another one.
- Message keys and argument names are 1–128 ASCII alphanumeric, `.`, `_`, `-`.
  Positive message revisions change with semantics, types or argument roles.
- Arguments have unique contiguous positions starting at 1. Types are `text`,
  signed 64-bit `integer`, and finite `number` (a JSON number convertible to f64).
- Source positions `%1`–`%99` must exactly match declarations; at most 16 may be
  declared. `%L1` uses localized numeric formatting and cannot name text.
  `%n`/`%Ln` require numerus. Zero/leading-zero/out-of-range positions are invalid.
- `numerus` defaults to false; `arguments` and `comment` default to empty.
- Text allows line feeds and tabs but no other controls, and is at most 8 KiB
  per source/comment/text argument. The mandatory source is nonempty.
- The whole encoded catalogue is at most 240 KiB, with at most 1024 messages
  and 64 distinct exact locales.

Locales use canonical language[-Script][-REGION]: a two/three-letter lowercase
language, optional title-case four-letter script, optional uppercase two-letter
or three-digit region. Variants/extensions and process suffixes are not supported
in version 1. This is deliberately narrower than all BCP 47, and separate from
ADR-0129's underscore keys. No filesystem path is accepted as a locale.

## Translation files

`GetTranslation(generation, locale)` serves exactly one declared locale;
otherwise `NotFound`. Invalid locale syntax is `InvalidArgument`. There is no
implicit language fallback here. Client layer/locale ordering controls fallback.
The nonempty reply is an opaque QM file of at most 240 KiB. The SDK bounds bytes
but does not parse Qt files or claim that they are safe.
The aggregate supplied catalogue bytes per export are limited to 8 MiB.

Source export creates id-based TS messages with id `key@revision`, their English
source, comments, parameter roles and numerus metadata. Build with
`lrelease -idbased -nounfinished`. The translator is scoped to the registered
plugin id; a textual prefix is not a substitute for that scope. Empty or
unfinished required labels do not override another lookup layer. A missing
translation must be represented as absence, never a raw id shown to the user.

The final formatter uses Qt plural selection with an optional count in
`0..=2147483647`. Count is present exactly for numerus messages. Parameters are
inserted in one pass: a `%2` contained in parameter 1 is literal data, not a
second template to evaluate. Fallback is already complete plain text, so it
never uses the target locale's plural rules on the English source pattern.

## Snapshot field associations

`Resolve` takes a JSON array of at most 128 distinct requests and 240 KiB.
The provider handler must identify the actual fixed field in the requested
original snapshot. It omits content, unknown fields and expired revisions.

| Scope | Object id | Revision | Allowed field |
| --- | --- | --- | --- |
| `provider` | `self` | 0, metadata bound to generation | `name`, `description` |
| `music.entity` | Entity id in GetPage | Page.revision, the catalogue revision | `title` |
| `music.station` | State.station.id | State.revision | `title` |
| `download.job` | Job id in GetJobs | Download snapshot revision | `error_message` |
| `media.session` | Session id in GetSessions | Media snapshot revision | `title`, only a fixed fallback |

The SDK passes the authorized generation to `TextsHandler::resolve`.
`DownloadService::text_snapshot` and `MediaService::text_snapshot` expose an
atomic clone of the currently published generation, assigned revision and
records. They return `None` while unregistered, after owner loss or after stop.
A handler checks these authoritative values, not merely the request's revision.
Only the current publication needs to be retained; stale requests can be omitted.

Providers keep fixed-vs-content provenance in their model, protected by one
provider-owned lock shared by publishing and resolving. Hold this lock while
updating provenance and awaiting `replace`, and while capturing `text_snapshot`
and checking the corresponding provenance during resolution. This also fences a
fixed label becoming content with the same visible text, which would otherwise
leave the SDK snapshot revision unchanged. Unknown provenance means omission.

For a catalogue entry and the same station in playback, use their respective
scope/revision pair; they are not interchangeable. New scopes need a separately
reviewed additive contract. Settings keep ADR-0129. A future operation/error
scope must define its identity and retention before use; arbitrary messages do
not become translatable just because they have an error code.

```json
[{
  "scope": "download.job",
  "objectId": "job-1",
  "revision": 7,
  "field": "error_message",
  "fallbackText": "Could not connect to Radio"
}]
```

The reply is a JSON array with at most one result per request index:

```json
[{
  "requestIndex": 0,
  "fallbackText": "Could not connect to Radio",
  "message": {
    "messageKey": "connect.failed",
    "messageRevision": 1,
    "arguments": {"service": "Radio"}
  }
}]
```

The JSON reply is at most 240 KiB. These payload bounds leave 16 KiB for
identities and D-Bus framing under the existing 256 KiB message-body rule.
Indices must be in bounds and unique. Fallback
must exactly match the observed original request. Keys/revisions must exist in
the validated source catalogue; arguments match names/types exactly and count
matches numerus. Unknown properties, duplicate argument keys, nulls, arrays,
objects and booleans in place of typed arguments are rejected.

An omitted result preserves the original field. An invalid response preserves
the original fields and produces only a payload-free diagnostic. A client
applies a valid result only while provider generation, original scope, id,
revision, field and fallback still match. It keeps the reference and arguments
so a language switch can rerender without asking the provider again.

## Daemon routing and compatibility

`Texts1.GetCatalog(provider_id)` returns `(generation, catalog_json)`.
`GetTranslation(provider_id, generation, locale)` returns exact-locale bytes.
`Resolve(provider_id, generation, requests_json)` returns associations.
The daemon validates both sides and never exposes the provider's direct bus
owner or arbitrary file paths as a translation source. It does not choose the
client's language. Caches are discarded on provider/daemon owner changes.

Existing snapshots, actions and plain strings stay unchanged. An old client or
a provider without this interface continues to work. The provider SDK endpoint
and protocol are implemented in this contract change; daemon routing, shell
lookup, local overrides and provider adaptations remain dependent tasks.
No central repository, translation platform or change to CLA policy is needed.
