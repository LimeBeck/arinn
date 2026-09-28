# ADR-0045: Plain HTTP web shortcuts for local-network hosts

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** project owner, explicit request in the Claude Code session to allow `http` for local-network sites; the locality rule under delegated authority ([ADR-0037](0037-agent-autonomy-and-owner-gated-decisions.md))
- **Amends / Supersedes:** amends [ADR-0043](0043-web-shortcuts.md) (accepted URL schemes only)
- **Amended by / Superseded by:** —
- **Specification:** [§5.1](../specifications/arinn-specification.md#51-home)

## Context

ADR-0043 accepts only `https` URLs for web shortcuts. On an HTPC many useful
services run on the home network without TLS: a media server web UI, a NAS, a
router or a torrent client, typically at `http://192.168.x.y:port` or a local
name such as `http://nas.local`. The owner asked to allow `http` for them.

## Decision

A web shortcut URL is valid when it is `https` for any host, or `http` for a
**local-network host**. Locality is decided from the URL alone, without DNS,
because a name can resolve anywhere and change later:

- IPv4 literals in private (10/8, 172.16/12, 192.168/16), loopback (127/8) or
  link-local (169.254/16) ranges;
- IPv6 literals that are loopback (`::1`), unique-local (`fc00::/7`) or
  link-local (`fe80::/10`);
- single-label host names (`nas`, `jellyfin`);
- names in the local-use zones `.local` (mDNS), `.lan`, `.home.arpa`
  (RFC 8375) and `.internal`.

Everything else with `http` is rejected with `InvalidArgument`. All other
ADR-0043 rules are unchanged: no user information, whitespace or control
characters, bounded length, and no shortcut URL in logs or diagnostics.
`arinn-protocol::web::local_host` implements the rule for the daemon and clients.

## Consequences

Local services can be shortcuts without certificates. A local name could still
resolve to a public address on a misconfigured network; this is accepted, since
the user created the shortcut and it opens in the normal browser with its own
warnings. Public sites keep requiring `https`.

## Alternatives considered

- **Allow `http` for every host:** rejected; public sites have `https`, and plain
  `http` to the Internet invites mistyped or downgraded shortcuts.
- **Resolve the name and check the address:** rejected; it depends on the network
  at validation time and does not bind what the browser later connects to.

## Verification

`web::tests` accepts private, loopback, link-local and unique-local literals,
single-label names and the local-use zones over `http`, and rejects public
IPv4/IPv6 literals, numeric shorthand such as `10.1`, look-alike names such as
`local.example.org`, credentials and non-HTTP schemes.
