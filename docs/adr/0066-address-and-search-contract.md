# ADR-0066: Address and search wire contract (Browse1)

- **Status:** Accepted
- **Date:** 2026-09-30
- **Deciders:** delegated technical choice ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md)) within the owner decisions of [ADR-0051](0051-address-and-search-entry.md)
- **Amends / Supersedes:** amends [ADR-0048](0048-browser-wire-contract.md) (the shell may open typed text) and [ADR-0051](0051-address-and-search-entry.md) (wire details)
- **Amended by / Superseded by:** —
- **Specification:** [§7.1](../specifications/arinn-specification.md#71-browser-chromechromium), [§11](../specifications/arinn-specification.md#11-state-ownership-and-persistence), [§13](../specifications/arinn-specification.md#13-security)

## Context

ADR-0051 decided the *Site or search* screen:
- the input rules;
- up to five recent entries, with *Clear*;
- a selectable search engine, default Google.

It left the wire contract to a `contract-change` task
([#108](https://github.com/LimeBeck/arinn/issues/108)). ADR-0048 lets the shell
open only web shortcuts. R-01 keeps engine names and URLs out of core code.

## Decision

- **Input rules** live in the protocol crate as `browse::resolve_input(text,
  template, engine)`, a pure function shared by the daemon and the tests.
    - Typed text is trimmed. Empty text, text over 2048 bytes or text with
      control characters is invalid.
    - `http://` and `https://` are used as typed, if the URL is valid for
      generic open (ADR-0048).
    - An IPv4 address, `localhost`, a `*.local` host or any host with a port
      gets `http://`.
    - A dotted host whose last label has two or more letters, with no spaces,
      gets `https://`.
    - Any other scheme is invalid.
    - Everything else is a search: the percent-encoded text replaces
      `{query}` in the engine's template.
- **Search engines are packaging data.** `/usr/share/arinn/search-engines.toml`
  lists `[[engine]]` entries with `id`, `title` and `template`, in the first
  XDG data root that has the file.
    - A template is an `https` URL with `{query}` exactly once
      (`browse::search_template`).
    - The daemon's own setting `search_engine` (provider `arinn`, ADR-0065) is
      an `enum` over the packaged ids. Its default is the first entry, which
      the package makes Google (ADR-0051).
- **`org.arinn.Browse1` on `/org/arinn/Daemon1`:**
    - `Resolve(text) → (kind, url, engine)`, where `kind` is `address`,
      `search` or `invalid`. This feeds the screen's intent line. It records
      nothing and opens nothing.
    - `Open(text, request_id) → Operation1`:
        - it resolves the text; invalid text is `InvalidArgument` at once;
        - it records the text as the newest recent entry, deduplicated and
          keeping the last five;
        - it opens the URL in the default BrowserProvider, with ADR-0050's
          focus-or-create and ADR-0055's launch of a closed browser.
    - `GetRecent() → as`, newest first, and `ClearRecent()`, which deletes all
      of them.
    - The signal `RecentChanged`.
- **Privacy.** Recent entries are stored in `state.sqlite3`, with the web
  shortcuts. Typed text, URLs and queries are never logged (spec §11, §13).
- **Scope.** The shell never builds URLs itself. The "already open" hint
  (IN-5) compares the resolved URL's origin with `Browser1` windows in the
  shell's C++ model, as for the Open services rows (DR-14).

The daemon implementation and the screen are separate tasks.

## Consequences

- The shell sends raw text, so the rules exist once.
- `Resolve` is called on every keystroke, so it must stay cheap: a pure
  function, no I/O beyond reading the configured engine.
- Adding or changing an engine is a packaging change, not a code change.

## Alternatives considered

- **Resolve in the shell's C++ layer.** Rejected: the rules would then live in
  two places, and recent entries and engines are daemon state anyway.
- **Engines as a daemon setting with a free template.** It would allow any
  template the user types on a TV. The packaged list with an enum setting
  keeps it simple; custom engines can come later.
- **Open through `WebShortcuts1`.** Rejected: shortcuts are user
  configuration, not arbitrary typed text.

## Verification

- Protocol tests (`crates/arinn-protocol/src/browse.rs`):
    - each ADR-0051 address rule;
    - other schemes and bad input are invalid;
    - searches, including Cyrillic and reserved characters, are
      percent-encoded;
    - template validation;
    - the `(sss)` signature.
- The daemon task adds private-bus tests for `Browse1`, and a live
  introspection comparison with `browse1.xml`.
