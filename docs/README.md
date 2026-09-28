# Documentation

| Document | What it answers |
| --- | --- |
| [Specification](specifications/arinn-specification.md) | What Arinn must do — current requirements, acceptance criteria, open questions |
| [Roadmap](../ROADMAP.md) | What is being built when, and what is done |
| [ADRs](adr/README.md) | Why decisions were made |
| [Contributing](../CONTRIBUTING.md) | How to work on Arinn |

## Technical contracts

| Document | Scope |
| --- | --- |
| [Implementation baseline](architecture/implementation-baseline.md) | Stack, D-Bus names, identifiers, snapshots, operations, errors, limits, storage, logging, CLI, tests |
| [Plugin runtime](architecture/plugin-runtime.md) | Manifests, discovery, systemd units, registration, Chrome bridge, desktop and media edge cases |
| [Activity and interaction](architecture/activity-and-interaction.md) | Activity model, focus, pending operations, keys, settings |
| [Media MVP](architecture/media-mvp.md) | Local library, mpv, OSD, resume ownership |
| [Transmission MVP](architecture/transmission-mvp.md) | Transmission adapter, polling, import into the library |
| [Session lifecycle](architecture/session-lifecycle.md) | Lock, sleep and session shutdown |
| [Plugin API](plugin-api/README.md) | Production XML, SDK semantics, [media and download extensions](plugin-api/mvp-extensions.md) |

## Phase 1 progress

[Implementation record](architecture/phase-1-progress.md): completed foundations, verification and remaining runtime work.

## Phase 0 evidence

[Gate matrix](architecture/phase-0-gate.md) · [Report](architecture/phase-0-report.md) · [Runbook](architecture/phase-0-runbook.md) · [Activity background experiment](architecture/background-experiment.md) · [Evidence files](architecture/evidence/)

## Design

[Design overview](design/README.md) · [Design handoff](design/handoff/README.md) · [Integration review](design/integration-review.md) · [Brand](design/brand/README.md)

## Historical sources

Never edited; superseded where the specification says so.

- [Developer Specification v0.3](specifications/developer-specification-v0.3.md) (Russian)
- [Design handoff](design/handoff/README.md) and its [archive](design/archive/2026-09-27-before-brand/README.md)
- [Brand Guide v0.1](design/brand/brand-guide-v0.1.txt) (Russian)
