# ADR-0135: Client-rendered fixed provider text

- **Status:** Proposed
- **Date:** 2026-11-06
- **Deciders:** Owner authorized implementation of the refined proposal in [#846](https://github.com/LimeBeck/arinn/issues/846#issuecomment-6694979705), including fixed provider text and local overrides; technical transport proposed in [#719](https://github.com/LimeBeck/arinn/issues/719)
- **Amends / Supersedes:** supplements [ADR-0129](0129-provider-setting-translations.md); changes the fixed-message exception in specification §15
- **Amended by / Superseded by:** —
- **Specification:** §9, §15; R-01–R-04, R-09–R-10

## Context

The owner wants local, partial translations of an installed shell and third-party
providers without rebuilding either binary. Changing the interface language must
also change fixed provider labels without restarting a provider or playback.
Content, user names and free-form remote service errors remain as delivered.

ADR-0129 provides translations of settings schema fields only. Existing music
JSON rejects unknown fields and existing D-Bus structs have fixed signatures.
Adding message fields to those records would break old clients. Globally
matching strings would mistake a user playlist for a fixed section with the
same name.

## Decision

Providers optionally export `ProviderTexts1` at their existing provider path.
Clients discover it by probing through the daemon, not through capability-specific
feature flags. A bounded source catalogue describes messages, revisions, typed
arguments, Qt positional placeholders and numerus. Exact-locale QM catalogues
are opaque bytes; providers need no Qt runtime.

`Resolve` explicitly identifies an original snapshot field by scope, object id,
revision, field and observed fallback. The provider returns message references
only for fixed labels it owns. Unknown/stale requests are omitted. A client
applies the result only while the entire original identity and text still match.
Message revisions prevent reuse of a translation after a semantic change.

The source catalogue and supplied translations are immutable for the export.
Only the registered daemon in the current generation can call the SDK endpoint;
authorization is checked again after an asynchronous resolution. The daemon
passes data through without selecting a language. `Texts1` defines its additive
client routing contract; its runtime implementation follows separately.

The client renders in the chosen interface language. Lookup tries personal
overrides, an explicitly active external translation, the provider's supplied
translation and finally the existing plain text. Locale fallback happens inside
each source layer. Scoped translators prevent a provider dictionary from
overriding shell or another provider's text. Substitution is one-pass and treats
argument contents as data. A translation never changes ids, actions or content.

The complete wire and validation rules are in
[provider-texts.md](../plugin-api/provider-texts.md). This contract PR does not
implement daemon routing, the shell loader, provider adaptations or a central
translation service. The existing settings contract remains unchanged.

## Consequences

- Old providers, daemons and clients retain existing plain text behavior.
- A third party can translate a new provider using exported source data and
  TS/QM files without a change in the shell's code or the provider's binary.
- Snapshot-to-message associations require a bounded sidecar request. Clients
  cache only within the original provider generation and snapshot revision.
- Provider authors explicitly mark fixed fields; ordinary titles are never
  inferred to be translatable.
- QM files are trusted input to Qt, not a sandbox. Byte limits do not establish
  safety. External resources are scoped and checked by the later loader.

## Alternatives considered

- **Provider locale changes and restarts:** interrupts state and cannot serve
  different client languages. Rejected for this requirement.
- **Replace strings by their visible text:** confuses content and labels.
- **Append fields to existing tuples or strict JSON:** breaks old clients.
- **A new templating/plural engine:** unnecessary while the shell uses Qt
  Linguist. Source messages stay portable JSON; translations use Qt tooling.
- **Require a central translation platform:** unnecessary for local files.

## Verification

Protocol tests cover bounded parsing, duplicate keys, locales, placeholders,
numerus counts, typed arguments, stale revisions and response-to-request binding.
SDK private-bus tests cover export validation, exact-locale bytes, authorization,
registration changes during a request and the published provider XML.
Runtime and TV acceptance remain separate dependent work; this ADR does not mark
the complete #845/#846 behavior delivered.
