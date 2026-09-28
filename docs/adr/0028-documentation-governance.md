# ADR-0028: Documentation governance, living specification and English docs

- **Status:** Accepted
- **Date:** 2026-09-27
- **Deciders:** project owner
- **Amends / Supersedes:** changes the status of ADR-0001–ADR-0019 from Proposed to Accepted
- **Amended by / Superseded by:** [ADR-0036](0036-parallel-development.md) — specification version bumped at release; reserved ADR numbers
- **Specification:** [§1](../specifications/arinn-specification.md)

## Context

Until now the developer specification v0.3 stayed frozen, clarifications lived next to it, and accepted ADRs were edited in place. A consistency review found the result hard to follow: banners pointed to an incomplete ADR range, the review map and acceptance list were stale, ADR-0023 and ADR-0026 contradicted themselves after in-place edits, and accepted ADRs refined ADRs that were still only Proposed. Documentation was in Russian, which limits outside contributors.

## Decision

1. **One living specification.** [`docs/specifications/arinn-specification.md`](../specifications/arinn-specification.md) (v0.4 and later) is the only source of current requirements. It consolidates v0.3 and every accepted ADR. v0.3 is kept unchanged as a historical source.
2. **ADR lifecycle.** Statuses are Proposed, Accepted, Rejected, Deprecated and Superseded. After acceptance an ADR is immutable except for typo fixes and the "Amended by / Superseded by" header field. A change to an accepted decision is a new ADR that amends or supersedes the old one. A pull request that accepts an ADR also updates the specification. New ADRs use [the template](template.md).
3. **What needs an ADR:** component boundaries, the Plugin API and its capabilities, IPC and persistence formats, platform mechanisms, security posture, licensing and project process. Implementation details inside one component do not.
4. **Roadmap.** [`ROADMAP.md`](../../ROADMAP.md) is the only place for phase status. Each phase has exit criteria referencing requirement IDs and links to evidence. A phase is done only when its exit criteria hold and the documentation is synchronized.
5. **Requirement IDs.** AC-xx (acceptance criteria), R-xx (architecture rules), G-x.y (phase gates), P-xx (product decisions), DR-xx (design reconciliation), OQ-xx (open questions). Code, tests and pull requests reference them.
6. **Language.** Living documentation, code comments, commit messages and pull requests are in English. Historical sources stay as delivered: developer specification v0.3, the design handoff and its archive, the brand guide. A short `README.ru.md` is kept for Russian-speaking users. Issues may be written in any language.
7. **Baseline acceptance.** ADR-0001–ADR-0019 are accepted as of this ADR with the amendments recorded in their headers.

### Errata fixed during migration

- The ambient auto-start condition is "an external Activity **or** playing media" (the product table); the interaction document said "and".
- ADR-0023 claimed to change v0.3 §48; its Back rule matches §48 and only adds the "no Activity" case.
- Update banners referring to "ADR-0020–0025" were removed; the specification replaces them.
- `architecture/review-map.md` and `architecture/product-decisions.md` were folded into the specification and the roadmap.
- The in-place clarifications of ADR-0023 and ADR-0026 were moved into ADR-0030 and ADR-0031.

The Russian texts before the migration remain in Git history (commit `2e679c0`).

## Consequences

Readers get current requirements from one document and history from ADRs. Every requirement change touches the specification, so pull requests show the requirement diff. Maintaining the specification is extra work in each architectural pull request.

## Alternatives considered

Keeping v0.3 frozen with clarifications next to it caused the drift described above. Bilingual living documentation would double the maintenance and drift again.

## Verification

`scripts/check-docs.py` in CI checks links, ADR headers and acceptance criteria coverage in the roadmap.
