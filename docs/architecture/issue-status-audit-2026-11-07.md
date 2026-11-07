<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Open-issue status audit — 2026-11-07

Requested in [#866](https://github.com/LimeBeck/arinn/issues/866). This is a dated
implementation/coordination audit, not a replacement for the specification,
roadmap gates, issue reproductions or newer agent claims.

## Evidence boundary

- Baseline: `master` / `611e067`; 58 open issues and five open PRs (#861–#865), excluding the new audit task.
- Read issue bodies and available discussion, merged implementation records and current source.
- Current public captures: [showcase provenance](../design/showcase/README.md). The focused `hearth-light` CTest passed on Qt 6.12.0; this is offscreen evidence only.
- GitHub repository metadata reports public visibility. Recent snapshot assets omit `.deb`; the tagged `v0.2.0` release contains it.
- During the audit, #862 merged as `602a1fe` and was integrated into the documentation branch. New PRs #867 (source-off follow-up) and #868 (daemon text routing, #859) opened. The rows retain the initial snapshot for reproducibility.
- Active PRs and comments are time-sensitive: the references below describe the snapshot, not a perpetual status feed.
- Owner evidence added during the audit: on 2026-11-07 the owner confirmed successful music listening on the TV. The source/account/build was not stated; preserve the successful use without extending it to all providers or edge cases. Music parent issues #677/#707 and interface-sound task #727 record this distinction.
- No agent-run real desktop, TV, remote, account or listening test was run. No phase was marked Done, owner decision inferred, release published or active development claim taken over. Only the explicit owner acceptance below updates existing roadmap evidence.

Four issues are closed: completed implementation increments #719/#723 and, after
explicit owner confirmation, the TV interface review #756 and music-card bug #834.
In total 27 issue bodies receive current status/evidence; the original request
and discussion remain available. Other open issues retain their state and scope.
Owner decisions and real-device qualification remain open even when code is merged.

## Issue-by-issue disposition

| Issue | Disposition | Evidence and remaining work |
| --- | --- | --- |
| [#64](https://github.com/LimeBeck/arinn/issues/64) | update current status; keep open | Remote mapping editor is still unimplemented. Its #683 prerequisite already merged as #704; it should no longer wait for that MR. ADR-0132 remains reserved here. Coordinate Settings overlap with active PR #863, and preserve the owner decisions for per-device/gamepad scope. |
| [#74](https://github.com/LimeBeck/arinn/issues/74) | update current status; keep open | Repository visibility is now public (GitHub metadata checked 2026-11-07); the original private/pre-public wording is historical. Remaining security-channel, contributor/process and informed telemetry-consent items stay owner-gated. v0.2.0 is already tagged; the remaining release decision concerns future publication/acceptance. The owner explicitly deferred the remaining installation checklist checks on 2026-11-01; that is not a test pass. |
| [#85](https://github.com/LimeBeck/arinn/issues/85) | retain open | Voice control is post-MVP. No implementation or acceptance completion is established. |
| [#148](https://github.com/LimeBeck/arinn/issues/148) | retain open | Real suspend/wake, lock/unlock and provider reconnect qualification remains pending under AC-19. No operator action was run during this audit. |
| [#149](https://github.com/LimeBeck/arinn/issues/149) | update current status; keep open | The owner comment “7 — OK” on 2026-11-01 records a qualitative pass of item 7. It supplies no build/session or numeric samples. Preserve that positive evidence, while keeping quantitative G-5.2 and related G-5.4/G-5.5 qualification open; no new operator test was run. |
| [#186](https://github.com/LimeBeck/arinn/issues/186) | update current status; keep open | XFCE/X11 is an MVP target under P-09/ADR-0080. Producer/runtime work is implemented; real XFCE/TV acceptance and the outstanding #149 measurements remain. Optional KDE/X11 and generic other-window-manager setups are separate. |
| [#198](https://github.com/LimeBeck/arinn/issues/198) | update current status; keep open | Android TV / Android Automotive OS (AAOS, not projected Android Auto) planning was delivered by #675. No Android app, emulator or device implementation is claimed; retain the future-platform work. |
| [#242](https://github.com/LimeBeck/arinn/issues/242) | retain open | Offline folder/season/artwork/NFO and recognition work #630/#637/#632/#654/#679 is delivered as already recorded in the issue. Online enrichment, vendor and precedence decisions remain owner-gated; keep open. |
| [#244](https://github.com/LimeBeck/arinn/issues/244) | retain open | Conditional libmpv fallback remains an owner decision, not started work or a requirement selected by this audit. |
| [#317](https://github.com/LimeBeck/arinn/issues/317) | retain open | Intermittent CI/test qualification remains open. Passing runs elsewhere do not establish a regression fix. |
| [#362](https://github.com/LimeBeck/arinn/issues/362) | retain open | Per-card browser frames require a separate contract/design increment. Baseline activity capture under #149 is not this feature. |
| [#370](https://github.com/LimeBeck/arinn/issues/370) | retain open | Symbol upload/packaging #563/#594 is implemented. Exact-package symbolication of a real matching event remains unproven; existing QML event reports alone do not qualify native/Rust symbolication. |
| [#433](https://github.com/LimeBeck/arinn/issues/433) | retain open | Nested-compositor spike evidence exists; the production architecture choice remains with the owner. It is not a duplicate of the conditional libmpv fallback #244. |
| [#435](https://github.com/LimeBeck/arinn/issues/435) | retain open | Steam integration beyond opening its launcher is post-MVP. No Arinn Steam game catalogue is implemented. |
| [#438](https://github.com/LimeBeck/arinn/issues/438) | update current status; keep open | Dedicated-session implementation #476/#501/#510/#542 and direct fallback #690 are merged. Real login → Switch to desktop → Return to Arinn qualification and the logout-cancellation owner question remain. Keep G-5.8 open. |
| [#440](https://github.com/LimeBeck/arinn/issues/440) | retain open | Notification support is explicitly post-MVP; no implementation or acceptance completion is established. |
| [#506](https://github.com/LimeBeck/arinn/issues/506) | retain open | No-frame/minimized-window lifecycle behavior remains an owner-gated choice and implementation task; background producer code alone does not decide it. |
| [#507](https://github.com/LimeBeck/arinn/issues/507) | update current status; keep open | Partial operator evidence exists: the owner-reported manual XFCE/X11 run on `c9c9d3b`, DISPLAY=:3, passed Home, Meta+F11, Escape and Meta+F12. That session lacked a systemd user manager and had an empty catalogue. It does not qualify KWin, managed video, hardware remotes or the full matrix. Keep the remaining qualification open. |
| [#508](https://github.com/LimeBeck/arinn/issues/508) | retain open | Handoff/launch outcome behavior still needs an owner decision. Review follow-ups around #511 do not settle this product choice. |
| [#519](https://github.com/LimeBeck/arinn/issues/519) | retain open | Implementation #521/#541 is merged; real power-key behavior, Plasma-version/XFCE cases and concurrent-session qualification remain. Optional cache proposals are not silently added as acceptance requirements. |
| [#606](https://github.com/LimeBeck/arinn/issues/606) | retain open | Desktop catalogue refresh on read_dir failure remains open. No merged fix/evidence was found to justify closing it. |
| [#612](https://github.com/LimeBeck/arinn/issues/612) | retain open | Release rendering still needs the SPDX-fragment cleanup and owner-reviewed changelog correction. No direct CHANGELOG edit or release is part of #866. |
| [#615](https://github.com/LimeBeck/arinn/issues/615) | update current status; keep open | Roadmap status refresh #624 is already merged. README, public screenshots/site and issue reconciliation are the bounded #866 work. Specification drift, CHANGELOG cleanup and disposition of Proposed ADRs remain here; no owner-gated document is changed by this audit. |
| [#617](https://github.com/LimeBeck/arinn/issues/617) | retain open | Architecture checker coverage still skips daemon runtime submodules through its direct-parent condition. No implementation is established by the existing comment; keep open. |
| [#677](https://github.com/LimeBeck/arinn/issues/677) | update current status; keep open | The local/Yandex native subset is implemented by merged #710, with subsequent Hearth Music fixes. Spotify, SoundCloud and YouTube Music are not implemented or qualified. Live account/audio checks and the broader provider-scope decision remain; keep the parent open. |
| [#685](https://github.com/LimeBeck/arinn/issues/685) | retain open | Provider enable/disable management remains open. Turning a source off in the Music room (#851) is a music preference, not disabling or stopping its provider process. |
| [#686](https://github.com/LimeBeck/arinn/issues/686) | retain open | Telemetry Settings consent/control implementation remains pending and owner-gated. A default-off development opt-in and this documentation refresh are not public informed consent. |
| [#692](https://github.com/LimeBeck/arinn/issues/692) | update current status; keep open | Still open: recent snapshot releases expose Arch/debug packages, source and checksums, but no .deb. Tagged v0.2.0 has a Debian asset; that does not implement Debian snapshots or qualify an install on the owner Debian 13 stand. |
| [#697](https://github.com/LimeBeck/arinn/issues/697) | retain open | The xfconf test depends on whether xfconfd is present on its bus. A clean-bus green run is not evidence that the reported stand/environment behavior has been corrected; keep open. |
| [#702](https://github.com/LimeBeck/arinn/issues/702) | update current status; keep open | The test stderr hook is already gated behind test/test-support in the #701 implementation. Provider-id prefixes on supervised stderr remain the outstanding work; keep this issue open for that item. |
| [#705](https://github.com/LimeBeck/arinn/issues/705) | retain open | Lock-provider probe duplication remains an open implementation follow-up. Do not infer completion from unrelated session changes. |
| [#706](https://github.com/LimeBeck/arinn/issues/706) | retain open | Lock-provider late-export behavior remains a separate open race/qualification task. No merged completion evidence was found. |
| [#707](https://github.com/LimeBeck/arinn/issues/707) | update current status; keep open | The unified local/Yandex native implementation MR #710 is merged, not pending. Music contracts, SDK/daemon/shell integration and package rules are in source. Remaining work is owner design acceptance, installed audio/codec behavior, live deployment-client/account/stream qualification and related follow-ups #677/#771. Keep this qualification parent open. |
| [#715](https://github.com/LimeBeck/arinn/issues/715) | retain open | Downloads row-hover deferred-focus failure remains open. The separate destroyed-view focus fix in #732 does not prove this row-hover path safe. |
| [#716](https://github.com/LimeBeck/arinn/issues/716) | retain open | Canvas/artwork race remains open: the drawing path still depends on the visible image dimensions across an asynchronous source change. No merged regression fix was found. |
| [#717](https://github.com/LimeBeck/arinn/issues/717) | retain open | Sentry-derived focus failure remains open. The proposed Main/player focus cause is not a verified fix; retain the event/reproduction evidence. |
| [#719](https://github.com/LimeBeck/arinn/issues/719) | close completed increment | Completed the refined contract-only scope recorded in the claim: #853 merged (`06f3ad2`), with protocol, ProviderTexts1 SDK and Texts1 routing contract. The old manifest-name proposal is historical, not the shipped design. Daemon consumers remain #859, shell/local overlays #845 / PR #863, and provider adapters/tools #846. ADR-0135 remains Proposed; closing this contract increment does not claim the entire interface is translated. |
| [#723](https://github.com/LimeBeck/arinn/issues/723) | close completed increment | Completed implementation: #732 merged the floor/pool/ceiling, period setting and light measurement; night tokens were delivered separately by #722. The recorded daytime maximum is approximately OKLab L 0.465, below 0.48. This audit reran hearth-light successfully and refreshed current day screenshots. Closing this implementation task does not pass G-5.1 or a TV/design gate. |
| [#726](https://github.com/LimeBeck/arinn/issues/726) | retain open | Ignition/Kindle/Ambient implementation and visual acceptance still need to be reconciled with the current Hearth/Playfair direction. Do not close this animation/design task merely because fonts and light have landed. |
| [#727](https://github.com/LimeBeck/arinn/issues/727) | update current status; keep open | Implementation merged in #730 (`cae3e0c`): nine generated WAVs, QSoundEffect, media guard, repeat handling, settings level and the owner-selected restart-required unavailable state. Remaining work is listening/output-device qualification on the owner setup. Prior tests explicitly skipped audio-device cases; no listening check was run by this audit. |
| [#733](https://github.com/LimeBeck/arinn/issues/733) | retain open | The Qt 6.8.2 media-key abort remains open. The reported Qt.Key_MediaPause keyClick is still in the Home test; a passing Qt 6.12 run does not prove the minimum-Qt case fixed. |
| [#736](https://github.com/LimeBeck/arinn/issues/736) | update current status; keep open | Current bundled fonts are Onest and Playfair Display, not Ysabeau. Arch already installs the OFL text; its license array still omits OFL-1.1. Debian copyright still lacks the font-specific stanzas/text. Keep open for the owner-reviewed licensing/package correction. |
| [#737](https://github.com/LimeBeck/arinn/issues/737) | update current status; keep open | PR package validation policy remains open and owner-gated by ADR-0102. The historical missing Qt Multimedia build dependency is already present in current Debian metadata; that example is not a remaining dependency fix. No CI policy change is made by this audit. |
| [#738](https://github.com/LimeBeck/arinn/issues/738) | retain open | Still open: Debian runtime SVG image-plugin availability needs an explicit dependency/clean-install check; libqt6svg6 alone is not the Qt SVG image plugin package. |
| [#756](https://github.com/LimeBeck/arinn/issues/756) | closed after owner acceptance | The original TV findings remain the historical reproduction on `0a50207`. Implementation has advanced: graphical folders #780, Music fixes #797/#798, the #810 error fix, Hearth Music work #800–#805, Home-row work #837, room #848, artwork/restore #849 and source-off #851 have landed. M4 (music preventing Ambient) was explicitly dropped by the owner. These are implementation references, not a blanket re-acceptance of A1–D or the 3 m rim check; the owner subsequently accepted the current TV interface and explicitly requested closure; #756 is closed on that later report. |
| [#771](https://github.com/LimeBeck/arinn/issues/771) | retain open | Browser-assisted Yandex sign-in and Music API implementation exists in source. Deployment-client registration, real account/stream behavior and authenticated audio qualification remain open; mocked tests and package checks do not qualify a live account. |
| [#782](https://github.com/LimeBeck/arinn/issues/782) | retain open | Open follow-up; no merged completion evidence was found. Preserve its current scope and reproduction rather than closing it from unrelated UI changes. |
| [#831](https://github.com/LimeBeck/arinn/issues/831) | update current status; keep open | Active PR #864 implements explicit media-key semantics and next/previous handling. It is still open at the snapshot. Real remote event traces/TV verification remain pending; #827 is already merged. |
| [#832](https://github.com/LimeBeck/arinn/issues/832) | update current status; keep open | The attempted logical-size × integer-DPR correction in PR #839 was rejected/closed because fractional Wayland scaling needs native output-mode information. No accepted fix is merged; native-mode sourcing and a hardware retest remain. |
| [#834](https://github.com/LimeBeck/arinn/issues/834) | closed after owner verification | Latest investigation did not reproduce the fault on master `28cfc1b`; no fix was pushed and the claim was released. The owner subsequently confirmed the card updates on the TV; #834 is closed on that verification. The earlier #827 dependency is merged, not an open MR. |
| [#845](https://github.com/LimeBeck/arinn/issues/845) | update current status; keep open | Active implementation now has draft PR #863. Contract #853 is merged; shell language switching/local overlays are not yet merged. Preserve the agent claim and needs-owner boundary; coordinate MusicCatalog overlap with PR #861. |
| [#846](https://github.com/LimeBeck/arinn/issues/846) | update current status; keep open | The owner-refined local-translation workflow is still open. Contract/SDK #853 is merged; daemon consumer work is #859 and shell/local-overlay work has draft PR #863 under #845. Provider adapters and export/check/compile tooling remain separate unfinished work. This does not accept Proposed ADR-0135 or claim live switching is already on master. |
| [#854](https://github.com/LimeBeck/arinn/issues/854) | update current status; keep open | Music pin UI #836 is merged. Provider-only pin handling, wording and error-caption follow-ups remain open. PR #861 addresses the provider mirror race and is still open at the snapshot; it does not complete all items here. |
| [#855](https://github.com/LimeBeck/arinn/issues/855) | update current status; keep open | Partially delivered: #849 fixed stale restore after a root change. Persistent scan-failure retry/backoff and test-order isolation remain open. PR #862 addresses a further stale-refresh/pending-key race and is still open at the snapshot; do not treat it as merged. |
| [#856](https://github.com/LimeBeck/arinn/issues/856) | retain open | Intermittent test qualification remains open. A green unrelated run does not establish that the reported race is fixed. |
| [#857](https://github.com/LimeBeck/arinn/issues/857) | retain open | Open implementation follow-up. No merged fix or acceptance evidence was found in the audited default branch; preserve the original reproduction and scope. |
| [#858](https://github.com/LimeBeck/arinn/issues/858) | retain open | Open implementation follow-up. No merged fix or acceptance evidence was found in the audited default branch; preserve the original reproduction and scope. |
| [#859](https://github.com/LimeBeck/arinn/issues/859) | retain open | Daemon Texts1 routing is claimed on `codex/859-texts-routing`. Contract #853 is merged; daemon integration is still pending. Preserve the active claim. |

## Owner acceptance and remaining checks

[Owner report, 2026-11-07](https://github.com/LimeBeck/arinn/issues/866#issuecomment-6704400452): music listening succeeds; the current TV
interface is accepted and its review should close; the Home music card updates;
Quick menu, video and Downloads work. #756 and #834 are closed on that evidence.
These are qualitative owner results, with no exact build, source/account or new
per-scenario trace supplied. Existing roadmap evidence records that distinction.
Do not ask the owner to repeat those accepted normal-use checks.

The same report says **interface sounds are absent** and **measurements were not
performed**. #727 is a confirmed TV problem, not a passed listening check.

| Remaining item | Scope | Tracker |
| --- | --- | --- |
| Silent interface feedback | Investigate the setting, startup output availability and QSoundEffect status; music itself plays | #727 |
| Power and recovery | Suspend/wake, lock-screen safety, Ambient, provider reconnect and one action per physical power-key press | #148, #519 |
| Dedicated-session round trip | Arinn login → Switch to desktop → Return to Arinn, including confirmation/cancellation | #438, G-5.8 |
| External/on-screen keyboard | Remote entry in text flows and global keyboard input in managed Chrome; Quick menu acceptance does not assert keyboard results | G-5.6/G-5.9 |
| Device measurements | Quantitative Activity background/performance report; earlier qualitative item-7 pass remains recorded | #149, G-5.2/G-5.4 |
| Extra platform/failure matrix | XFCE/X11 acceptance; daemon-loss/KGlobalAccel release, another graphical session and late power-manager startup | #186, #507, #519, G-5.5 |
| Native diagnostics | Match a real allowlisted event to the exact package's uploaded symbols within existing consent boundaries | #370 |
| Provider-specific music edges | Unspecified successful music use does not identify a Yandex account/client or verify auth expiry/reconnect and every codec | #707, #771, #782 |

The music-key fix #831 / PR #864 still needs to land before its post-fix TV
check. Do not request installation/upgrade/uninstall checks again: the owner
accepted/deferred that checklist item in #74. Debian snapshot installation awaits
#692's asset. Future Android/post-MVP work and unresolved product decisions are
not ready-to-run TV checks. No operator action is authorized by this list itself.

## Follow-up use

Use the current issue body and open PRs before claiming work; this audit reserves
only documentation/capture paths under #866. In particular, #845/#859 and
PRs #861–#865 retain their owners and review requirements. Proposed ADRs and
the wider specification/changelog drift stay under #615 and its owner review.
