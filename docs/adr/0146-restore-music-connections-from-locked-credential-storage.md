# ADR-0146: Restore music connections from locked credential storage

- **Status:** Proposed
- **Date:** 2026-11-13
- **Deciders:** Maintainer review for [#906](https://github.com/LimeBeck/arinn/issues/906)
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§5.3.1](../specifications/arinn-specification.md#531-native-music-owner-requested-extension)

## Context

Desktop autologin can leave the system credential store locked. The music
provider cannot restore the approved grant at startup in that session. It
needs a way to request the desktop's unlock dialog from the TV interface.

## Decision

A provider with locked credentials advertises the account action
`unlock`. The shell shows it in Music Sources and in the standalone
catalogue's account controls. The command opens the system prompt only on
user activation and acknowledges starting recovery without waiting for user
input. That prompt handles the password and dismissal.

The provider requests Secret Service unlock for the approved grant and
configured application-secret item when present. It binds the prompt to
the selected service's D-Bus owner, waits for completion with a deadline,
then rereads the credentials and verifies music access. Dismissing the
prompt preserves the approval and lets the user retry.

## Consequences

The recovery works with the existing desktop login and credential-store
configuration. Shell, daemon and provider need a coordinated update for
the added action in the pre-stable music contract.

## Alternatives considered

Asking for provider sign-in again would create an unnecessary new grant.
Displaying the unlock prompt on every startup would interrupt other
activities whenever the credential store remained locked.

## Verification

The protocol tests cover the sessionless command. The provider fixtures
cover targeted Secret Service unlocking, dismissal, and saved-grant recovery
with Music API verification. The QML fixture covers remote-control activation
from Sources. Live desktop qualification is tracked separately.
