# Proposals to reduce feature delivery time

Prepared for the project owner on 2026-10-05 against `485a412`; refreshed on
2026-10-06 against master `1a2dd1f` after the supplementary architecture review. Coordination: [task #336](https://github.com/LimeBeck/arinn/issues/336).

The recommendation is to finish existing user scenarios, reduce unstable test
feedback and repeated build work, and share the shell's transport lifecycle.
These are proposals, not adopted rules or implementation claims. Process changes
need an owner decision under [ADR-0037](../adr/0037-agent-autonomy-and-owner-gated-decisions.md).
Existing [contribution rules](../../CONTRIBUTING.md), reviews, contract ordering,
operator permission and acceptance criteria remain in force.

## Evidence and limits

The initial history sample contained 185 commits from 2026-09-27 through 2026-10-05.
That establishes activity, not feature delivery speed. No baseline of coding,
review, CI queue or operator waiting time has been measured. The expected effects
below are hypotheses to test, not percentage speedup claims.

- The original Favorites example exposed a gap between merged contract/daemon
  work and an unfinished shell. [#347](https://github.com/LimeBeck/arinn/pull/347)
  now binds durable favorites and tile actions in the shell; it is merged. Keep
  this as historical scenario-delivery evidence, not an outstanding contract or
  shell dependency. Real-device acceptance and any follow-up interaction defects
  remain separate from that implementation merge.
- [CONTRIBUTING](../../CONTRIBUTING.md#parallel-work) serializes all contract
  changes and starts dependent implementation after merge. This protects shared
  boundaries but also queues unrelated contracts behind each other.
- Browser, media and library C++ catalogs separately manage owner changes,
  generations, paging, refreshes and pending commands. Library and media listen
  to all daemon `StateChanged` notifications; browser also polls every second.
  This repeats lifecycle code and broadens refresh work. Runtime cost is unmeasured.
- [CI](../../.github/workflows/ci.yml) installs the Arch toolchain and builds the
  Rust workspace in the integration job. Rust and package jobs have explicit
  Cargo caches; the integration job has no explicit equivalent cache.
- [#257](https://github.com/LimeBeck/arinn/issues/257),
  [#275](https://github.com/LimeBeck/arinn/issues/275) and
  [#317](https://github.com/LimeBeck/arinn/issues/317) already track unstable tests.
  Their causes must be established individually; resource contention is not a
  proven common explanation.
- The [Plugin API overview](../plugin-api/README.md) still describes download
  XML/routing as outstanding, despite the checked-in XML and daemon routing.
  Passing documentation guards does not establish semantic freshness.

Coordination was rechecked on 2026-10-06. Downloads
[#310](https://github.com/LimeBeck/arinn/pull/310), navigation
[#334](https://github.com/LimeBeck/arinn/pull/334) and Favorites #347 are merged.
Background [#149](https://github.com/LimeBeck/arinn/issues/149) and its separate
contract [#348 / #349](https://github.com/LimeBeck/arinn/pull/349) are active;
coordinate any shared shell/Core files before implementation. A proposal does
not reserve those zones or create competing implementation claims.

### Correctness findings from the supplementary review

The [review comment](https://github.com/LimeBeck/arinn/pull/338#issuecomment-5984720757)
records its original `2ad6a29` baseline and Qt 6.8.2 temporary probes. Both failures
were independently reproduced against `1a2dd1f` with isolated offscreen Qt 6.11.2
probes on 2026-10-06. These are outstanding findings, not fixes delivered here.

| Priority | Failure | Proposed bounded fix and regression |
| --- | --- | --- |
| P1 | Settings row identity concatenates `providerId + "-" + settingId`: `(foo-bar, baz)` and `(foo, bar-baz)` collide. Opening the second string editor resolves the first provider. | Generate an unambiguous presentation key from the structured identity in C++; retain provider/setting fields separately. Verify editor selection, emitted save target and focus after reordering. Changing the delimiter without escaping is insufficient. |
| P2 | Settings `integerDraft`/`integerWas` are QML `int`, truncating a valid `2147483648` draft to `-2147483648` although the contract accepts i64. | Keep exact decimal editing text and parse/validate qint64 in C++. Cover INT32_MAX+1, signed i64 boundaries, unchanged save and exact wire serialization. A double-only replacement does not preserve the whole i64 range. |

The current `integerValue()` Number/integer check does not fix the narrower QML
draft property. Neither temporary probe is a committed regression suite; the
follow-up implementation must add one. No Settings/runtime edits are authorized
by accepting this proposal document; create scoped tasks after overlap checks.

### Integration and repeated compilation evidence

Within the latest 100 reachable master commits at `1a2dd1f`, counting non-merge
`git diff-tree --no-commit-id --name-only -r` changes gives:

| Shared file | Commits touching it |
| --- | ---: |
| `apps/arinn-shell/Main.qml` | 26 |
| `crates/arinnd/src/runtime.rs` | 14 |
| `apps/arinn-shell/CMakeLists.txt` | 14 |

The original review's `2ad6a29` sample was 22/20/14 respectively. Keep each sample
with its baseline; neither establishes a measured causal cost or lead-time gain.
Shared-file activity motivates smaller routing/lifecycle seams and explicit
integration ownership rather than a general Core rewrite.

The current shell CMake source lists mention `backend.cpp` four times, browser,
media and library models six times each, and settings/assets seven times each.
This is source-list repetition, not a measured compile-time multiplier. Extracting
an `arinn-shell-models` static/object library is a concrete build-work candidate;
verify registration/resource/static-retention behavior and benchmark it before
claiming a speedup.

## Recommended order

| Priority | Proposed action | Expected effect | Completion evidence |
| --- | --- | --- | --- |
| First | Correct reproduced Settings identity and i64 editing failures | Avoid wrong-target writes and value corruption | Committed editor/save identity and exact-i64 regressions fail before the fixes |
| First | Finish one existing user scenario across its dependent tasks | Shorter wait between infrastructure and usable behavior | Scenario demonstrated through production shell and daemon; operator evidence listed separately |
| First | Preserve failure diagnostics and fix existing unstable tests | Fewer repeated qualification runs and unrelated investigations | Regression fails before its fix; repeated identical-head runs retain logs and reproduce no known failure |
| First | Cache integration builds and reuse dependency setup | Shorter time to actionable CI results | Compare cold/warm job timings and cache hits; preserve required checks |
| Next | Share shell connection and operation lifecycle | Less repeated recovery code in every catalog | Two catalogs use one helper with unchanged public APIs and recovery tests |
| Next | Correct contract implementation gaps and status documentation | Fewer integration surprises and duplicate tasks | Multi-capability/default-selection regressions and accurate implementation records |
| After a baseline | Pilot narrower contract serialization and local qualification | Less waiting between independent tasks | Owner-approved policy, bounded pilot and measured feature lead time |

The first correctness, scenario, test-diagnostic and build-work actions can be prepared under the current rules. A change that
touches governance or product behavior still follows the owner boundary, and all
implementation tasks must check overlap before editing.

## Finish user scenarios across existing tasks

Use one coordination issue to describe the visible result and link each scoped
implementation task. Each implementation issue keeps its own branch, zone and PR.
Name one coordinator responsible for checking the complete scenario, outstanding
dependencies and remaining acceptance evidence. A coordination issue is not an
extra implementation branch and authorizes no edits to another task's zone.

The delivered Favorites chain illustrates the coordination goal:
pin an application or shortcut, restart the shell, see the saved Home order,
unpin without deleting the target, and retain an unavailable target. Empty saved
Favorites must stay empty under the existing contract. This example adds no
acceptance criterion and does not claim TV qualification.

Proposed owner choice: pilot at most two user scenarios in implementation at once,
with a separate lane for regressions that prevent their use. Before opening an
additional scenario, finish or explicitly block one of those two. Keep other
accepted MVP work in the backlog; changing product scope is a separate decision.
The alternative is to retain unrestricted scheduling and only add coordination.
Measure scenario lead time and blocked days to see whether the limit helps.

## Stabilize feedback before reducing checks

Reuse #257, #275 and #317 rather than opening another umbrella flaky-test task.
Retain the first failing CTest output, per-test logs, exact head, dependency
versions and execution settings before a rerun. A passing rerun remains a
separate result. Use synthetic data and private buses; do not retain tokens,
personal paths, real URLs or media in diagnostics.

Investigate ordering, focus readiness, resource overlap and fixture lifetime
separately. Replace unexplained sleeps with an observed readiness condition
where appropriate; bound the wait and keep the behavior assertion. Do not
increase deadlines or weaken assertions just to obtain green runs. Qualification
must not overlap a different checkout's build/resource generation in the same
directory. Record serial/parallel test settings when comparing failures.

For a fix, demonstrate its regression on the unfixed implementation and preserve
all required checks. A repeated-run sample provides confidence, not proof that
all flakes have disappeared. Track the fraction of unchanged-head reruns and
the time spent on failures outside the feature's zone.

## Reduce CI setup and compilation work

First measure queued time, dependency installation, compilation and test execution
separately for recent code PRs. Check runner availability before adding capacity:
a cache cannot shorten time when no runner is online. New hardware, repository
variables or secret/settings changes require the owner.

Add an integration-job Cargo cache keyed by OS/architecture, Rust toolchain,
`Cargo.lock` and relevant build inputs. Keep it separate from the Rust-container
and release-package caches; do not reuse compiled artifacts across incompatible
toolchains. Build required examples and consume artifacts from the same checked-out
revision. Treat cache misses as normal and validate both a cold and a warm run.

Evaluate a prebuilt Arch dependency image if installation is a material share
of job time. Record its digest and tool versions and keep the nightly's current
Arch dependency coverage. An image refresh must run the same Rust, Qt and
private-bus checks. Any change to rolling-dependency policy needs owner review.
Use an existing image registry only if its access is already configured; record
new registry/settings requirements as dependencies.

Keep the current full master/package pipeline and exact-commit snapshot checks
required by [ADR-0086](../adr/0086-automatic-master-prereleases.md). The merged
[retention policy](../adr/0088-single-master-snapshot-retention.md) keeps only the
newest verified master snapshot release; it does not reduce build coverage. Avoid
optimizations that cancel a required master run or reuse a different revision's
package. A pilot succeeds if comparable warm jobs take less time without changing
the checks or hiding cold-cache failures.

## Share shell lifecycle in small increments

Start with connection ownership and `Operation1` tracking, because several
catalogs repeat them. A connection helper can expose the current daemon owner
and an epoch; an operation helper can subscribe before reading, handle completion
before the method reply, normalize errors and discard late replies after an
owner change. Mutating commands must never retry automatically.

Migrate two catalogs in separate scoped PRs before attempting a common snapshot
loader. Keep capability-specific record validation and presentation in each
catalog. If the pilot reduces duplicated code and preserves test clarity, then
share paging, generation/revision fences and dirty-refresh coalescing. Do not
replace typed capability APIs with a generic JSON transport.

Filter `StateChanged` by provider/capability where relevant. Keep a documented
fallback refresh for missed notifications, and measure requests during idle,
provider updates and owner replacement before changing polling intervals.
Tests must cover a notification during a read, torn pages, stale replies,
provider disappearance and daemon restart. Recheck current claims and PRs before migration; #233/#329 implementation
is merged, while #149 background work reserves shared integration paths.

## Remove hidden integration constraints

The existing specification allows several capabilities in one provider and an
explicit default when several providers serve a capability. The current daemon
rejects manifests with more than one capability and `open_url()` selects the
first ready browser provider in map order. `media_opener()` does read the
configured media default, then falls back to another usable provider; qualify
that fallback against §9.2 instead of claiming media ignores its default.
Fixing confirmed gaps implements existing
requirements; it must not silently change the specification to fit the code.

Separate provider lifecycle state from state per capability, including revisions,
caches and exported endpoints. Add a private-bus multi-capability fixture and a
two-browser/default-selection regression for explicit default, no default and
an unavailable selected default. Add equivalent media-selection cases against
the existing requirements before extracting a common resolver. Inspect whether default selection needs
additional public API before editing: any protocol/SDK addition remains a separate
contract-change task. Do not create a new contract until that need is established.

Also verify that a media-position notification does not trigger
`Library1.GetLibrary`: the library model's broad StateChanged subscription and
the daemon's one-second refresh need request-count/snapshot-byte measurements.
Narrow invalidation must preserve a bounded missed-notification recovery path.

Also move synchronous config reads and writes in async settings paths to blocking
tasks. The current settings lock spans provider Apply and its timeout; preserve
whole-file consistency and rollback semantics when changing serialization. A
mechanical move of I/O is safer than removing the lock without a transaction
design. Check concurrent settings changes and failed Apply without real services.

Correct stale technical status text in a dedicated documentation task. Keep
requirements and implementation evidence distinct, link evidence rather than
copying it, and leave phase completion to the owner. No general Core rewrite is
needed before these targeted improvements.

## Process options for an owner decision

| Decision | Recommended pilot | Alternative | Required safeguard |
| --- | --- | --- | --- |
| Contract serialization | Permit two independent contract tasks with explicit nonoverlapping interfaces and a shared-file integration owner | Keep the global single-contract queue | Same-interface/type changes remain serialized; implementation uses only a merged contract; reserve ADRs and revalidate shared files |
| Local qualification | Require focused local regression/area checks and one complete required CI run on the final integrated head | Keep the full local list plus required CI for every code PR | Initially exclude contracts, platform, packaging and CI; preserve owner/operator gates and approving review; fail open to full checks for uncertain scope |
| Work in progress | Limit the pilot to two user scenarios and keep urgent regressions separate | Keep current scheduling | Preserve accepted scope; report blocked dependencies instead of bypassing task zones |

The proposed local-check pilot removes duplicated full local qualification,
not final CI coverage. It is incompatible with the current full-local-check rule
until the owner accepts a process change. Any automated path classifier needs
tests for shared dependencies, unknown paths and a full-run fallback.

If selected, prepare a separately scoped Proposed ADR and synchronized changes
to CONTRIBUTING, AGENTS and the specification as applicable. The owner decides
and merges those changes. Keep the existing approving-review requirement; a
timeout alone never permits a merge.

## Measure a bounded pilot

Collect an initial sample of at least 20 recent code PRs and all completed user
scenarios in that period. Record PR reviewable time, first human/agent review,
merge time, CI queue and job durations, reruns and blocked dependencies.
For each scenario, measure from its first scoped implementation claim to the
demonstrated production result; record pending operator acceptance separately.
Do not substitute commit counts for these measurements.

Run a proposed two-week pilot, keeping medians, upper-tail durations and sample
counts for comparable task sizes. Aim for a 25% reduction in median scenario lead
time and warm CI duration, with no increase in escaped regressions, hidden failures
or missing acceptance evidence. These are proposed targets, not measured results.
If the sample is too small or task mix changes, report that limitation and extend
the comparison rather than claiming a speedup.

Start with existing-scenario completion, test diagnostics and integration caching.
Evaluate shared shell lifecycle next; use measured waiting time to decide whether
contract concurrency or local-check policy is worth changing.

## Verification provenance for this document update

The two current offscreen Settings probes fail at their intended assertions:
wrong provider `foo-bar` instead of `foo`, and draft `-2147483648` instead of
`2147483648`. Setup/cleanup pass. This confirms the findings only; neither a full
Settings suite nor the proposed fixes are claimed to pass. No operator tests,
full runtime builds, throughput pilot or measured speedup were run for this
Markdown-only update. Historical test/environment limitations in the review
comment remain historical evidence rather than current qualification results.
