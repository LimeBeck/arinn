## Summary

<!-- What does this change and why? One topic per pull request. -->

## Task

<!-- Closes #N (Agent task issue). The change stays inside the zone named in the issue. -->

## Requirements

<!-- IDs this implements or affects: AC-xx, G-x.y, R-xx, OQ-xx, ADR-NNNN. Write "none" for pure maintenance. -->

## Testing

<!-- Commands you ran and their result. Operator tests (KWin, lock screen) only if the platform layer changed; attach evidence. Write "not run" with a reason where applicable. -->

## Documentation

- [ ] Specification text updated or existing requirements referenced; no version bump outside a maintainer release
- [ ] ADR added for an architectural decision — or not needed
- [ ] `ROADMAP.md` updated for gate or phase changes — or not needed
- [ ] Technical documents in `docs/architecture` / `docs/plugin-api` updated — or not needed
- [ ] Changelog fragment added under `changes/<category>/` — or the pull request is labelled `skip-changelog`

## Checklist

- [ ] Applicable checks from `CONTRIBUTING.md` pass locally; failures and unrun checks are recorded above
- [ ] New files have SPDX coverage through headers or existing REUSE annotations
- [ ] No product-specific logic in the core or shell (R-01, AC-12)
- [ ] No real user URLs, tab titles, tokens, credentials or personal paths in logs, fixtures or commits; synthetic fixtures and public documentation links are allowed
- [ ] Owner-gated changes carry `needs-owner` and are left for the owner to merge
