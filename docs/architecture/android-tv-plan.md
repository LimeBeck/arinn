# Android TV and Automotive OS: post-MVP specification and PoC plan

Status: planning only, owner-requested documentation for [#198](https://github.com/LimeBeck/arinn/issues/198).
No Android build, emulator run or device test has been performed for this document.
The [current specification](../specifications/arinn-specification.md) remains the
source of current requirements. The criteria below qualify future experiments;
they do not extend Linux MVP acceptance or commit a delivery date.

## Scope and decisions still pending

The proposed product is a TV-first home screen with application tiles and
D-pad/OK/Back and touch navigation, preserving Arinn's visual language and plugin model.
The owner also requested an AAOS head-unit variant, qualified separately below.
Execution follows the Linux MVP, including its current KDE/Wayland and XFCE/X11
scope. #186 is related portability work, not an Android prerequisite.

Two operating modes must be evaluated independently:

| Mode | Registration / setup to investigate | Expected boundary |
| --- | --- | --- |
| Ordinary TV app | `MAIN` with `CATEGORY_LEANBACK_LAUNCHER`; TV manifest, banner and remote-only input requirements | Listed by the existing TV launcher; does not own Home |
| Default Home | `MAIN`, `CATEGORY_HOME` and `CATEGORY_DEFAULT`; check `ROLE_HOME` availability and supported user selection flow | Opt-in only, per device/firmware; Android owns Home delivery |

Check role availability before offering a request; cancellation or denial keeps
the previous launcher. Being a Leanback launch target does not establish Home
eligibility. Where Home replacement is unavailable, retain ordinary TV-app mode
and document the limitation without repeated prompts or privileged workarounds.

Production UI (shared Qt/QML versus native Android host), minimum/target API,
ABIs, distribution/signing, provider trust policy and transport are open decisions.
The recommendation is to compare both UI options using the same small fixture
before deciding. Binder/AIDL with Android Services is a candidate, not an accepted
replacement for D-Bus. A reviewed specification/ADR decision must precede
production implementation; any shared protocol/SDK change is a separate coordinated
`contract-change` task. This plan selects no new architecture and reserves no ADR.

## Proposed interaction contract to validate

| Trigger | Proposed observable result / experiment |
| --- | --- |
| Launch Arinn from the existing launcher | Show Home and a visible focused tile; all primary actions work without touch or mouse |
| D-pad / OK | Deterministic focus movement and one activation; preserve focus by opaque identity after refresh, with a nearby fallback if removed |
| Back inside Arinn | Dismiss modal first, then leave nested navigation and restore opener focus; record root-Back behavior separately for app and Home modes before approving it |
| Launch installed TV app | Provider resolves a visible enabled launch target using Android APIs; foreground user action launches it or returns a bounded normalized failure |
| Home while another app runs | In Home mode, Android may bring Arinn forward; in ordinary app mode the selected system launcher handles it |
| Back from another app | Android and that application's task stack determine the result; Arinn does not force a return or claim exact resume |
| Resume Arinn | Restore its own navigation/focus when possible; refresh capabilities and snapshots before enabling actions |
| Arinn process recreated | Reconstruct its own safe state; discard stale operations and generation-bound references; tolerate lost grants/providers |

Exact restoration of arbitrary foreign windows, Linux global overlay behavior,
background activity launches, global key interception and unrestricted closing of
other apps are not promised. No root, accessibility interception, hidden API or
OEM privilege is a standard requirement. Silent capture, protected-media bypass,
arbitrary executable plugin downloads, phone/tablet UI and a remote companion
are outside this scope. Desktop Native Messaging/Chrome and external Linux mpv
need separate adaptation work; neither is automatically portable.

## Touch input adaptation

Owner addition: touch is a required input path for the planned Android interface
where the device exposes a touchscreen. D-pad navigation remains fully usable on
devices without touch; touch hardware must not become an installation requirement
for the TV app. This adds input adaptation to the TV-first presentation, without
qualifying a separate phone/tablet product or changing Linux MVP requirements.

| Interaction | Proposed behavior / verification criterion |
| --- | --- |
| Tap | Activate the tapped enabled control once on release; no preliminary focus tap, hover or double-tap required; cancelled gestures activate nothing |
| Drag / swipe | Scroll vertical lists and horizontal rails naturally, including fling and bounds; crossing the drag threshold cancels tile activation; nested rails must not trap vertical navigation |
| Secondary actions | Provide visible tappable controls for tile actions, menus and dismissal; long press may be a shortcut, never the only route |
| Back / navigation | Provide reachable on-screen Back where needed and honor supported Android system Back, including gesture Back; use the same modal/nested navigation semantics and do not intercept the system Home gesture |
| Input switching | Touch reaches the item under the finger; the next D-pad action restores a visible, valid focus anchored to the last interacted item or a deterministic nearby fallback without activating it |
| Touch targets / feedback | Use at least 48 x 48 Android dp hit targets with adequate separation, pressed/disabled feedback and no hover-only information; verify density conversion in Qt and native candidates rather than treating QML pixels as dp |
| Text input | A tap focuses the field and opens the selected Android-compatible input method; text, selection, keyboard dismissal and window insets must remain usable; do not show two keyboards at once |
| Lifecycle / interruption | Pointer cancellation, leaving the app, opening a modal or process recreation must not leave a pressed state, cause a deferred launch or replay a gesture |

Stage B must run a touch-only pass of every primary action as well as the
remote-only pass. Include taps near target edges, scroll versus activation,
nested scrolling, long-press cancellation, modal dismissal, software keyboard
occlusion, switching repeatedly between touch and D-pad, and focus after a
touched item disappears. Compare Qt/QML and native candidates on these same cases.
Record viewport, density, input source, gesture thresholds and results.
Use a touch-capable emulator configuration and, only with separate owner
permission, representative touch hardware. Mouse clicks alone do not qualify
touch or multi-pointer cancellation. Touch support remains **not tested** until
evidence is attached; unsupported input sources must be stated per target.

## Architecture constraints and portability seams

The existing [architecture rules](../specifications/arinn-specification.md#20-architecture-rules)
and [runtime semantics](implementation-baseline.md) constrain the experiments.

| Boundary | Reuse / isolate | Evidence required |
| --- | --- | --- |
| Presentation (R-03, R-04) | Candidate QML design components, navigation and presentation models; remove QtDBus assumptions behind a host binding | Gallery runs without providers or direct third-party app calls; providers supply data/actions only |
| Domain (R-01, R-09) | Portable Rust routing/state logic after separating zbus, systemd and KDE dependencies | Routing uses capabilities and opaque IDs, never product/package-name switches |
| Platform (R-05) | Android host handles lifecycle/input/platform binding | No Linux session bus, systemd or KWin requirement in Android package; Linux bindings unchanged |
| Providers (R-02) | Separate service processes or installed provider APKs | Provider crash leaves shell alive; no integration library loaded into core |
| Discovery and trust | Package/service discovery replaces Linux manifest/supervision assumptions | Verify Binder caller UID/package ownership and selected signing/permission policy; reject unauthorized callers and provider identity substitution |
| Protocol (R-10) | Preserve semantic invariants, not Linux wire encoding | Negotiated features, snapshots/revisions, generations, bounded async operations, normalized errors, death/reconnect and stale-reply rejection tested |

The application provider must evaluate package visibility and TV launch intent
resolution, package install/remove/disable updates and missing launch handlers.
Do not request broad package visibility without a documented distribution-policy
review. MediaSession access requires an explicit permission/access analysis;
absence or revocation is an unavailable capability, not permission to bypass it.
Later library work must account for document URIs, persisted grants, scoped storage
and revocation. No local playback/library provider starts before the launcher and
runtime approach is validated.

## Staged experiments and exit evidence

Each stage records commit, APK hash, toolchain (Qt/SDK/NDK/JDK), API/ABI, image or
model/firmware, mode, setup steps, expected/actual result and sanitized logs.
Use synthetic apps/data; never attach personal titles, accounts or credentials.
Mark each case pass, fail, blocked or not run, with a reason and evidence link.
An emulator result never qualifies an untested physical device.

| Stage | Work and verification criteria | Exit / next dependency |
| --- | --- | --- |
| A: feasibility | Select a versioned Android TV emulator image and representative owner-approved physical TV/Google TV device. Record HOME resolution, role availability, selection/cancel/deny, app-mode fallback and recovery path. Compare shared Qt/QML and native host using identical navigation fixture | Publish observations and unresolved restrictions; select no production UI without the comparison and owner review |
| B: presentation | Package the isolated QML gallery without Linux transport. Exercise every focusable control with D-pad/OK/Back and the touch-only/input-switching cases above, focus fallback, 1080p/4K scaling, repeated background/foreground, activity recreation and process recreation | Reproducible APK/build steps and case results; measure cold/warm launch, input-to-frame latency, frame pacing and memory on stated hardware; owner approves budgets before performance acceptance |
| C: runtime | Bind one application provider and a demo provider through the candidate transport. Discover/launch a synthetic TV app; inject unauthorized caller, unsupported feature, timeout, provider death, reconnect and delayed old-generation reply | Shell survives provider death; actions complete or fail within recorded finite limits, stale replies cannot mutate current state, fresh snapshots restore operation; review transport/trust ADR before production implementation |
| D: Home | After rollback rehearsal, explicitly opt in on a supported device; test Home from another app, launch/return, repeated Home, cancel setup, restart/reboot and shell process death; restore previous Home | Device-specific behavior and limitations recorded; previous launcher is reachable and restored through supported settings; otherwise only ordinary-app mode is eligible |
| E: decision | Consolidate support matrix and UI/runtime comparison, outstanding failures, permissions, distribution constraints and proposed follow-up zones | Owner approves bounded implementation scope and reviewed ADRs; create separate tasks only for approved scope, with explicit dependencies and criteria |

For stage C record exact timeout, payload/page bounds, retry/backoff and recovery
limits in the experiment report before running it. Test cancellation, duplicate
completion, revision gaps and core/shell death as well as provider death. Feature
absence must disable only that capability; reconnect must not silently replay a
launch. These are semantic checks, not approval of a new public wire contract.

## Device safety and support matrix

Real-device/operator tests require separate explicit owner permission. This PR
provides none. Before changing default Home, record the previous launcher, verify
its supported settings entry and a usable way to reach it independently of Arinn,
and rehearse restoration. Keep the previous launcher installed/enabled. Stop on
an inaccessible recovery path; do not disable system packages or require shell,
root or OEM-only commands as the consumer rollback procedure. Record uninstallation
and restoration behavior on each approved firmware.

The matrix starts unqualified; fill actual image/model, API and firmware during A.

| Target | Ordinary app | Home selection / denial | Home and app return | Process death / reconnect | Restore previous launcher | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| Android TV emulator, image TBD | Not run | Not run | Not run | Not run | Not run | None |
| Representative physical TV/Google TV, model/firmware TBD | Not run | Not run | Not run | Not run | Not run | None; separate permission required |

A failed Home case qualifies only for ordinary-app fallback if that mode passes;
report unsupported firmware explicitly. Do not generalize a vendor or Android
version from one passing device. The documentation PR is complete when this plan,
its specification/backlog links and changelog pass documentation checks. #198
remains open for the experiments, decisions and subsequent implementation.

## Android Automotive OS head-unit variant

Owner clarification: also plan execution directly on an Android Automotive OS
(AAOS) head unit. This is a separate qualification target alongside Android TV;
it is not phone-projected Android Auto. A generic Android aftermarket head unit
is not automatically AAOS: record the actual OS, API, automotive features,
OEM firmware and distribution channel before assigning support.

| Mode | Feasibility question | Fallback / limitation |
| --- | --- | --- |
| Ordinary installed AAOS app | Which supported app category, host/UI model, installation channel and parked/driving availability permit Arinn's intended behavior on this firmware? | Qualify only the allowed subset; an installable APK alone is not evidence of usable or distributable support |
| Default Home on AAOS | Does the OEM expose a supported user-selectable Home role or launcher choice to an ordinary app? | Keep the OEM launcher when unavailable; OEM/system-app integration is a separate decision, not a standard prerequisite |

Do not assume a TV manifest, Leanback intent, QML surface or Car App Library
template can be reused unchanged. Compare the native automotive host and shared
presentation against supported categories, host requirements and OEM restrictions.
Reuse portable domain logic and provider semantics where appropriate; isolate
automotive lifecycle, input and platform APIs. No CAN/vehicle-control integration,
privileged car permissions or control of safety-critical functions is included.

The proposed initial qualification is parked use. Determine allowed behavior
while moving from platform/OEM UX restrictions and the approved application
category before enabling any moving-use surface. Respect restrictions and their
changes at runtime, including loss of eligibility; do not bypass restrictions
or keep video, a keyboard or unrestricted browsing visible when prohibited.
Unknown restriction state must not enable an unrestricted surface. Car APIs
and their permissions/availability require an explicit feasibility check rather
than assuming access from an ordinary APK.

Extend stages A–E with an AAOS track:
- **A:** select an AAOS emulator image and representative owner-approved head unit;
  record category/distribution eligibility, Home availability and firmware limits.
- **B:** run the touch cases above and available rotary controller/Select/Back
  input, focus restoration, display density/insets, day/night appearance and
  permitted keyboard behavior. Do not require D-pad hardware on the head unit.
- **C:** test application discovery/launch against the permitted automotive
  surface and caller trust, plus disconnect/reconnect, lifecycle and restriction
  changes; never assume all installed apps are launchable.
- **D:** test parked launch/return, ignition-related shutdown/resume and process
  recreation, then restore OEM Home if replacement is supported. Simulate driving
  restrictions in the emulator; no real-road test is authorized by this plan.
- **E:** publish separate TV and AAOS support decisions and bounded follow-up
  tasks. A TV pass does not qualify AAOS, or vice versa.

Real-head-unit/operator tests still require separate explicit owner permission;
plan any approved physical checks on a stationary vehicle or bench. Record safe
access back to the OEM interface before changing Home. If the recovery path or
required category is unavailable, record Home/app mode as unsupported or blocked
rather than requiring root or disabling OEM packages.

| Additional target | App/category eligibility | Home selection / recovery | Touch / rotary | Parked / driving restrictions | Shutdown / resume | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| AAOS emulator, image/API TBD | Not run | Not run | Not run | Not run | Not run | None |
| AAOS head unit, model/OEM firmware TBD | Not run | Not run | Not run | Not run | Not run | None; separate permission required |

No automotive APK, emulator run, head-unit test or distribution approval has been
performed. Platform/category selection and driving-use scope remain owner-reviewed
decisions before implementation.

## Primary references

- [Android Automotive OS](https://source.android.com/docs/automotive)
- [Build apps for cars: platform and category guidance](https://developer.android.com/training/cars)
- [Driver distraction and UX restrictions](https://source.android.com/docs/automotive/driver_distraction/consume)
- [Qt for Android](https://doc.qt.io/qt-6/android.html)
- [TV app registration](https://developer.android.com/training/tv/get-started/create)
- [HOME role](https://developer.android.com/reference/android/app/role/RoleManager#ROLE_HOME)
- [Home key](https://developer.android.com/reference/android/view/KeyEvent#KEYCODE_HOME)
- [Background activity restrictions](https://developer.android.com/guide/components/activities/secure-bal)
- [AIDL IPC](https://developer.android.com/develop/background-work/services/aidl)
- [Package visibility](https://developer.android.com/training/package-visibility)
- [Storage Access Framework](https://developer.android.com/guide/topics/providers/document-provider)

These references guide future verification; linking them is not evidence of a
successful build, role request or device test.
