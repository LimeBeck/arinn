# ADR-0034: Provider data directories independent of registry state

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** project owner, explicit confirmation of the hash-based option for OQ-06
- **Amends / Supersedes:** amends [ADR-0022](0022-state-and-interaction-baseline.md), provider data-directory identity only
- **Amended by / Superseded by:** —
- **Specification:** [§11](../specifications/arinn-specification.md#11-state-ownership-and-persistence)

## Context

The implementation baseline names provider-owned data directories using a UUID stored
in Arinn's `state.sqlite3`. Resetting that database loses the mapping even when the
provider's data remains intact. OQ-06 asks how to preserve access independently of
Arinn's registry state.

## Decision

Provider-owned data and state directories use a deterministic SHA-256 digest of the
exact UTF-8 bytes of the validated `provider_id`. The directory component is the full
64-character lowercase hexadecimal digest, without truncation, case folding or Unicode
normalization. Hash the provider instance ID, not the plugin package ID, executable,
version, generation or registry UUID.

The directories are `$XDG_DATA_HOME/arinn/providers/<digest>/` and
`$XDG_STATE_HOME/arinn/providers/<digest>/`, with the usual XDG defaults. The raw opaque
ID is never interpolated into a filesystem path. The digest does not require the
registry database and remains stable across daemon restarts, upgrades and database
recreation when the provider ID is unchanged.

Registry UUIDs continue to identify D-Bus proxy paths and systemd units; this decision
changes only provider-owned persistent directory naming. Providers still own their
state. Hashing is path naming, not encryption or an authorization boundary.

## Consequences

Re-discovering the same provider instance recovers its directory location after an
Arinn-state reset. A different provider ID has a different namespace; changing an ID
requires explicit data migration and must not silently adopt another provider's data.
This decision does not restore provider instance configuration lost with Arinn state.

Production directory provisioning is not implemented yet. No on-disk migration or
verification of reset recovery is claimed by accepting this decision.

## Alternatives considered

Registry UUIDs were rejected because locating provider data would depend on the
survival of Arinn's database. A truncated SHA-256 digest was mentioned in OQ-06;
the full digest is used to avoid an unnecessary truncation convention.

## Verification

During Phase 1 persistence implementation, test fixed hash vectors, opaque and Unicode
IDs, separate provider instances, and directory stability after recreating the registry.
An integration test must preserve provider-owned data across an Arinn database reset.
These tests are pending; no phase gate is closed by this ADR.
