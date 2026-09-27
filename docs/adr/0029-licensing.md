# ADR-0029: GPL-3.0-or-later core, permissive protocol, contributor license agreement

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** project owner
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§18](../specifications/arinn-specification.md)

## Context

The repository had no license. The owner wants forks of Arinn to stay open, third parties to be able to write plugins under any license, and to keep the option of selling a commercial edition. Rust plugins statically link the protocol crate and the future SDK, so a copyleft SDK would force every plugin to be GPL.

## Decision

| Part | License |
| --- | --- |
| `arinnd`, `arinn-shell`, `arinnctl`, platform adapters, Arinn's own plugins, Chrome extension, scripts and tests | `GPL-3.0-or-later` |
| `crates/arinn-protocol`, the future `arinn-plugin-sdk`, D-Bus interface XML, example manifests | `MIT OR Apache-2.0` |
| Documentation and design materials | `CC-BY-SA-4.0` |
| Generated files and trivial configuration | `CC0-1.0` |
| Third-party service logos in the design prototype | not licensed; trademarks of their owners |

- Every file carries an SPDX header or a `REUSE.toml` annotation; license texts live in `LICENSES/`; `reuse lint` runs in CI.
- Contributors sign the [Contributor License Agreement](../../CLA.md) once, through the CLA Assistant workflow on pull requests. The CLA gives the maintainer the right to relicense contributions, which keeps a dual-licensing option (GPL plus a commercial license) open.
- Dependencies must be compatible with GPL-3.0-or-later, and dependencies of `arinn-protocol` with `MIT OR Apache-2.0`; `cargo deny check licenses` enforces this. Qt is used under LGPL-3.0 with dynamic linking.

## Consequences

Forks of the core stay open. Plugins talk to the core over D-Bus and may use any license. Contributors must sign a CLA, which some people dislike. Relicensing is possible only for code covered by the CLA or written by the owner. This is not legal advice; commercial plans should be reviewed by a lawyer, including the copyright status of AI-assisted code.

## Alternatives considered

- MIT for everything: simplest, but allows closed forks by anyone.
- GPL with DCO only: no relicensing without every author's consent.
- GPL for the SDK: forces every Rust plugin to be GPL.

## Verification

CI: `reuse lint`, `cargo deny check licenses`, CLA check on pull requests.
