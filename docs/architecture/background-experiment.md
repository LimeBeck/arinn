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
