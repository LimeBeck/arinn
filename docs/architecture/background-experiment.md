# Activity background: runtime experiment

2026-09-27, KWin 6.7.5 / Qt 6.11.2 / Mesa, Intel Iris Xe. Feasibility of the graphics path was verified; automatic integration into Home is not done yet. Decision: [ADR-0027](../adr/0027-activity-background-snapshot.md).

## What was checked

The operator script `kwin_background.py` creates a temporary KWin QML script. It selects exactly the foreground window UUID verified before the run and creates an invisible, non-focusable window with `WindowThumbnail`. It captures raw, near, far and raw-later frames, then unloads the script. Only a separate Chrome fixture window with a local HTML page and a time counter was used.

`ActivityBackground.qml` applies scale .95/.90, Qt MultiEffect blur with maximum 36 and near factor 8/36, opacity .6/.32 over black and scrim .3/.6. This implements the handoff parameters technically; an exact match of the blur kernel and colour management with the HTML reference is not claimed yet.

`background_preview.py` reads the raw PNG in a **separate Qt process** and renders the same component. So an external shell can use the last-frame path without moving user UI into KWin. The raw and raw-later pixels differ in the counter area, so the source was not a static fake image. The images were checked visually.

- [Near](evidence/background-near.png)
- [Far](evidence/background-far.png)
- [The same snapshot in a separate Qt renderer](evidence/background-external.png)

## Measurements and limits

In the mixed-DPI environment a logical 1920×1080 request produced a physical 3360×1890 PNG in a KWin context with DPR 1.75: binding to an output does not guarantee the offscreen export size. Four consecutive grab/save calls took about 257–367 ms each, including GPU readback and PNG encoding. Last run: raw 296 ms, near 367 ms, far 260 ms, raw-later 264 ms. This is neither input latency nor a GPU frame-time benchmark.

Before capture is connected to Home: limit the physical frame size, take PNG encoding off the compositor's critical path, define a deadline and fallback, and measure frame time and memory on the target TV. Live texture sharing between processes and DRM/protected content were not checked. The experiment has no full capture lifecycle for lock/return/restart; it is run explicitly by an operator on a disposable Activity.

The ScreenShot2 API returned `NoAuthorized` even with a temporarily declared desktop permission. KWin protection was not disabled and the temporary permission entry was removed. This path is not declared working.

## Reproduction

Requires a running Phase 0, a disposable foreground Activity, QtQuick.Effects and `qml6`. From the repository root:

```sh
python tests/integration/kwin_background.py
python tests/integration/background_preview.py
```

Frames are stored in `.phase0/background-probe/` (private, ignored by Git) and never sent anywhere. Repository evidence contains only images of the experiment's own HTML page, no user windows. Do not run this operator test during lock or suspend; the session guard is not connected to the capture pipeline yet.

API sources: [KWin QML effects](https://develop.kde.org/docs/plasma/kwineffect/), [WindowThumbnail implementation](https://invent.kde.org/plasma/kwin/-/blob/Plasma/6.7/src/scripting/windowthumbnailitem.cpp), [QML workspace windows](https://invent.kde.org/plasma/kwin/-/blob/Plasma/6.7/src/scripting/workspace_wrapper.h).

## Production foundation for #149

The transport prerequisite is merged: [ADR-0087](../adr/0087-bounded-activity-background-frames.md)
and the [bounded frame contract](activity-background.md) replace the prototype's
PNG/file handoff. The following implementation foundations can be built separately:

- `platform/background`: move-only sealed memfd ownership, physical-size admission,
  exact-size/seal validation before copying, close-on-exec duplication and positional
  reads independent of the shared descriptor offset. It accepts only tightly packed
  straight-alpha RGBA8888; callers perform colour conversion before publication.
- `platform/kde/background`: conversion of a bounded Qt grab image from premultiplied
  pixels to straight-alpha RGBA8888, with explicit tagged-colour conversion to sRGB
  and row packing. Physical dimensions and input storage are checked before conversion;
  DPR metadata does not expand admission. Untagged Qt grabs are interpreted as sRGB.
  `encodeGrab` is synchronous and must be called on the producer worker, never inside
  the compositor callback. This library does not capture a window or publish on D-Bus.
- `apps/arinn-shell/background`: a consumer admission/lifetime gate. Ready pixels
  require exact owner/epoch, Return/output and authoritative window evidence, a safe
  session guard and live provider/producer/shell. Revision floors reject stale or
  contradictory metadata. Evidence invalidation retires the current request, so
  unlock or Home recovery cannot resurrect it. An invalidated worker retains the
  single occupied slot until completion; late/duplicate callbacks cannot install
  pixels or free a replacement's slot. Invalidated pixel storage is released.

Focused builds (Linux, C++20, CMake ≥ 3.24; the encoder also requires Qt ≥ 6.8):

```sh
cmake -S apps/arinn-shell/background -B build/background-consumer
cmake --build build/background-consumer
ctest --test-dir build/background-consumer --output-on-failure
cmake -S platform/kde/background -B build/background-encoder
cmake --build build/background-encoder
ctest --test-dir build/background-encoder --output-on-failure
```

The standalone builds also run the shared descriptor tests. Synthetic tests cover
malformed extents, incomplete seals, writable mappings, non-frame descriptors,
truncated/extra bytes, maximum admitted pixels, FD ownership/offsets, alpha/colour
conversion, padded rows, revision/epoch conflicts, wrong echoed identities,
Return/lock/unknown-guard/provider/owner loss and recovery, cancelled workers and
replacement lifetimes. These are foundation tests, not private-bus runtime tests
or production capture evidence.

### KDE producer

The producer runs inside KWin, as ADR-0087 requires: it needs no helper
process and no new bus authority.

- **Script.** `platform/kde/background.qml` is a packaged declarative KWin script
  that `arinn-kwin-adapter` loads as `arinn-background` after the adapter script
  and unloads first. If KWin refuses it, Home keeps the normal background, and
  the unit does not fail.
- **Module.** The script imports the `org.arinn.background` QML module
  (`platform/kde/background`, plugin `libarinn-kwin-background.so`). The
  packages install it into the distribution's Qt QML directory.
- **Producer.** `BackgroundProducer` follows `Background1`, pinned to the daemon
  owner. For a pending attempt with a resolved context, it exposes the request
  and its target.
- **Capture.** The script looks up the window whose `internalId` equals the
  target, the exact Return identity. A missing, minimized or deleted window
  cancels the request and is never replaced by another window. The script shows
  only that window in a `WindowThumbnail` inside an invisible, non-focusable,
  input-transparent stage on the window's output, sized to the window's aspect
  ratio within 1920×1080. After the first rendered frame it calls `capture()`.
  The producer holds the only reference to the one outstanding grab result and
  releases it as soon as the image is taken or the request ends.
- **Encoding and publication.**
  - The grab is requested at physical bounds for the window's DPR.
  - The grab callback only keeps a reference. On a one-thread worker the image
    is scaled into 1920×1080 (input storage ≤ 64 MiB) and converted to sealed
    straight-alpha RGBA by `encodeGrab`.
  - The frame is published with `BackgroundCapture1.Publish` from KWin's own
    session connection, the `org.kde.KWin` owner the daemon authenticates.
- **Limits.**
  - Each request is captured at most once, and at most one grab or encoding
    runs at a time: a request that arrives while an earlier image is still
    being encoded is cancelled as `busy` instead of queueing a second image.
  - A superseded or invalidated request drops its result, and the descriptor is
    closed.
  - An encoding failure, a rejected publication or the local 1500 ms guard ends
    the attempt; the daemon's deadline stays authoritative.
- **Diagnostics.** Lines named `background-capture`, with outcome `published`,
  `rejected`, `deadline`, `window`, `encode` or `busy` and `duration_ms` from the
  request, are the producer's G-5.2 hook. They contain no identifiers.

`producer_test.cpp` drives the producer against a fake daemon on a private bus,
with a plain Qt Quick item in place of `WindowThumbnail`. It covers publication
from the producer's own connection with a sealed frame of exact size and
colour, a single capture per request, the deadline and missing-window
cancellation, superseded and invalidated requests, rejected publication and
daemon owner loss. `encode_test.cpp` covers scaling a DPR 1.75 grab into the
bounds. The KWin script itself and the real `WindowThumbnail` capture are not
exercised by any automated test. They need the owner's KDE run.

Because KWin keeps a loaded QML plugin, an upgraded plugin takes effect at the
next KWin start, that is the next login; restarting `arinn-kwin.service`
reloads only the script.

### Shell consumer

The shell build registers `apps/arinn-shell/background`, so its pixel and
frame-gate tests run under CTest. `ActivityBackgroundSource`
(`apps/arinn-shell/activity_background.{h,cpp}`, QML type in `Arinn.Shell`)
reads `Background1` for Home:

- **Owner-pinned and asynchronous.** It subscribes to `Changed` before the first
  `Get`, pinned to the daemon owner that `ReturnAssociation` resolved. It asks
  again only on `Changed` or when its evidence actually changes, never on
  `ReturnAssociation`'s unchanged once-a-second poll, and an admitted snapshot
  is read before a pending re-read. A `Get` reply that spans any evidence
  change (a lock, a hidden Home or a lost provider, even if restored by then)
  is discarded and asked again. A daemon
  without the interface leaves the normal background and is not asked again
  until its owner changes.
- **Evidence.** The gate's evidence is the shell's own: Home visibility, target
  and output from `Status`, the Return window from `ReturnAssociation1`,
  platform liveness, and the session guard bound from `sessionGuard.blocked`.
  A repeated Home clears the Return association until the adapter reports it
  again. The last window of the same target is kept meanwhile, because the
  daemon invalidates the frame itself when that window really goes away.
- **Lock and lost evidence.** A blocked guard clears the frame synchronously,
  before any daemon reply. Restoration never brings a frame back: the gate
  retires the request on every evidence change. That includes a request first
  seen while blocked, while Home was hidden or while the provider was down.
  The only exemption is the Return window resolving after the snapshot while
  everything else is live and unchanged.
- **Inventory changes.** The shell keeps the resolved Return window while the
  association is re-read. The daemon is the authority for removal: it
  checks the token against the committed inventory atomically and publishes
  the invalidation with `Changed`. Clearing on every `WindowsChanged` would
  retire the frame whenever any unrelated window opened or closed.
- **Reading.** `ReadFrame` runs only for a ready snapshot that the evidence
  accepts. The descriptor is duplicated and validated (seals, exact size)
  before a positional copy on a one-thread pool. That worker also builds the
  `QImage`, so the GUI thread only installs it. A failed read is not retried
  for the same revision, so a persistent failure cannot loop.
- **Rendering.** The copy becomes a `QImage` served by the `arinn-activity`
  image provider under a per-frame serial; a released frame's URL resolves to
  nothing. The provider holds at most one frame (≤ 8 MiB) beside the gate's
  copy, and MultiEffect's blur textures follow the full-screen item.
- **Home.** `HomeView` draws `ui/ActivityBackground.qml` behind its content:
  near while the Activity's own card has focus (the current-service card for a
  Return Activity verified as a browser service), otherwise far, and no layer
  outside the Home screen. While it shows, the ambient light field stays off.
  Focus never waits for the frame.

The rendering component moved from `apps/arinn-shell/ActivityBackground.qml` to
`apps/arinn-shell/ui/ActivityBackground.qml`; the operator probe and preview
copy it from there. Tests: `activity_background_test.cpp` (fake daemon on a
private bus: ready frame, repeated Home, unavailable and remap, lock without
IPC, invalid echo and unsealed descriptors, unsupported and pending daemons),
the frame-gate tests, and `HomeNavigation::test_activity_background_near_and_far`.

### Remaining integration and qualification

The runtime is split across [#490](https://github.com/LimeBeck/arinn/pull/490)
(daemon export), [#491](https://github.com/LimeBeck/arinn/pull/491) (shell consumer
and Home), [#492](https://github.com/LimeBeck/arinn/pull/492) (KDE producer) and
[#493](https://github.com/LimeBeck/arinn/pull/493) (X11 producer). The sections
above describe what each delivers. Until the daemon export and a producer are
both present, Home keeps its normal background.

Before marking #149/G-5.2 complete, run runtime/private-bus lifecycle tests and
obtain the owner-run device report specified by the contract: capture/publication
and focus latency, CPU/GPU allocations and RSS across near/far and repeated
invalidation, media frame times, visual colour/kernel comparison and both-platform
lock/Return/protected-content behaviour. No measurement report, GPU/RSS gate,
real-desktop qualification or phase completion is claimed by these foundations.
