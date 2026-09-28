# ADR-0037: Agent autonomy and owner-gated decisions

- **Status:** Accepted
- **Date:** 2026-09-28
- **Deciders:** project owner
- **Amends / Supersedes:** amends [ADR-0036](0036-parallel-development.md) (who creates tasks, who merges)
- **Amended by / Superseded by:** —
- **Specification:** [§1.2, §17.1](../specifications/arinn-specification.md)

## Context

ADR-0036 split work into issues and zones but left the owner as the bottleneck: agents were told to ask the owner to create or approve issues, labels had to be created by hand, and every pull request waited for the owner to merge it. The owner wants agents to work on their own and to be involved only in decisions that are really theirs.

## Decision

Agents act autonomously by default. They stop and ask only for the **owner-gated** decisions listed below.

### Agents do on their own

1. **Plan work.** Create Agent task issues for the current and the next roadmap phase from its scope and exit criteria, split large items, set zones and dependencies, and reserve ADR numbers. Before creating one, search open issues and pull requests so that no two tasks cover the same work or overlapping zones.
2. **Claim work.** Label the issue `agent:<name>`, comment "claimed" with the branch name, then work. A claim with no commit, comment or pull request activity for **48 hours** is stale: another agent may take it over after commenting on the issue.
3. **Make technical decisions** inside the accepted specification and ADRs, including new technical ADRs accepted by delegated authority (as ADR-0020–ADR-0022 were), when they change neither product behaviour, scope, licensing nor project process.
4. **Drive their pull requests to done.** Resolve merge conflicts, fix CI failures (including ones inherited from `master`, by porting the fix), answer and resolve review threads, update gate rows in `ROADMAP.md` with evidence, and open follow-up issues for problems found along the way.
5. **Merge their own pull requests** with squash merge, then delete the branch and close the issue, when all of the following hold:
   - every CI check, the changelog check and the CLA check are green on a branch that is up to date with `master`;
   - there are no unresolved review threads and no `needs-owner` label;
   - the pull request touches no owner-gated area below.
6. **Keep labels in sync.** Labels are declared in `.github/labels.yml` and created by the *Labels* workflow; nobody creates them by hand.

### Owner-gated

The agent opens the pull request or issue, adds the label `needs-owner`, writes the question with its recommendation and the consequences of each option, and **continues with other work** instead of waiting. The owner decides:

| Area | Examples |
| --- | --- |
| Open questions and product scope | answering OQ-xx; adding, removing or rewording acceptance criteria; changing the MVP; specification sections on product behaviour |
| Phase completion | marking a phase Done |
| Non-technical ADRs | product, licensing, security posture, project process and governance (including this ADR and ADR-0036) |
| Licensing | `LICENSE`, `LICENSES/`, `REUSE.toml` policy, `CLA.md`, dependency license policy in `deny.toml` |
| Releases | version bumps, tags, GitHub releases |
| Real hardware and desktop | operator tests on a live KDE session, tests on the target HTPC or TV |
| Repository administration | settings, branch protection, visibility, secrets, force pushes or history rewrites on `master` |
| Rules for agents | `AGENTS.md`, `CONTRIBUTING.md`, `CODEOWNERS`, workflows under `.github/workflows` that decide merging, releasing or the CLA |

Pull requests touching these areas are merged by the owner.

## Consequences

The owner is involved in decisions and releases, not in routine task management and merging. More work happens without a human review of the diff, so CI, the architecture guards, the documentation checks and the regression-test rule carry more weight; a problem that slips through is fixed forward by a new task. A stale-claim rule prevents abandoned tasks from blocking progress.

## Alternatives considered

- Owner approves every task and merges every pull request (ADR-0036 as written): safe but makes the owner the bottleneck.
- Agents also decide open questions and process: faster, but takes product and legal decisions away from the owner.

## Verification

Owner-gated pull requests carry `needs-owner` and are merged by the owner. Agent merges show green required checks on an up-to-date branch. The *Labels* workflow keeps `.github/labels.yml` and the repository labels identical.
