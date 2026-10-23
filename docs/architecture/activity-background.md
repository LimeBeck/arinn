# Bounded Activity background transport

Contract prerequisite [#348](https://github.com/LimeBeck/arinn/issues/348) for
[#149](https://github.com/LimeBeck/arinn/issues/149), implementing existing
specification §12.3 / G-5.2 and [ADR-0027](../adr/0027-activity-background-snapshot.md).
Technical transport choice: [ADR-0087](../adr/0087-bounded-activity-background-frames.md).
**Status:** the daemon export is implemented (see [Daemon implementation](#daemon-implementation));
the KDE and X11 producers and the shell consumer are not yet connected, so no
automatic capture runs. No target-device budget or operator gate is passed.

## Optional interfaces and identities

Both optional interfaces live at `org.arinn.Daemon1`, `/org/arinn/Daemon1`.
They add no provider capability and alter no existing method or record.
[Background1](../plugin-api/background1.xml) has `Get`, `ReadFrame` and `Changed`;
[BackgroundCapture1](../plugin-api/background-capture1.xml) has `Publish` and `Cancel`.

`Context = (epoch, request_id, target, output, (window_generation, window_token))`.
Epoch is the current non-nil daemon UUID; request ID is a fresh non-nil UUID for
one eligible capture attempt. Target is the exact opaque Return identity,
not a provider tab/session ID. Output is the retained Home output. Both are
nonempty ordinary strings bounded to 8 KiB with no NUL. The window reference
uses the committed PlatformWindows1 generation and non-reused token; its bounds
and identity rules are those of ReturnAssociation1. All fields compare exactly;
never derive a match through a title, executable, URL or token spelling.

`Get -> (epoch, revision, context: [Context], state)` is one atomic snapshot,
signature `(sta(ssss(ss))s)`. Context cardinality is zero or one. Revision is
positive and monotonic within one daemon owner/epoch; same-revision contents
must agree. States are `idle`, `pending`, `ready`, `unavailable`. Idle has no
context; ready has exactly one. Pending may have none while waiting for the exact
Return/window association, within the same finite deadline. Unavailable may
retain the attempted context for correlation, but never usable pixels. A present
context's epoch must equal the snapshot epoch. Revision exhaustion requires a
fresh daemon epoch before further publication; never wrap or reset in place.

`Changed(epoch, revision)` follows an atomic commit, including invalidation.
It is metadata only, never a presentation request or pixel transport. Clients
subscribe before reading, pin calls to the unique daemon owner, and reject
replies older than notifications or installed state. Owner loss, a mismatching
epoch, invalid data or a failed read clears all cached frame evidence. Read-only
reconciliation may retry with bounded backoff; no mutating call is retried.

## Immutable pixels

`Frame = (Context, width: u32, height: u32, pixels: UnixFD)`, signature
`((ssss(ss))uuh)`. Pixels are tightly packed **straight-alpha RGBA8888 in sRGB**,
with exactly `width * height * 4` bytes and no row padding, codec, path or URL.
Physical width is 1–1920, height 1–1080; file size is at most 8 MiB. Validate
extents before multiplication, conversion, copying, decoding or allocation.
These limits are independent of screen DPR, logical size and compositor output.
The descriptor must be a regular sealed memfd with exact size, close-on-exec
on local copies, and `F_SEAL_WRITE | F_SEAL_GROW | F_SEAL_SHRINK`. Pipes, sockets,
devices, ordinary files, truncated/extra bytes and writable mappings are rejected
before reading pixels. Rust metadata validation does not prove these runtime
checks; daemon and shell implementations must both perform them.

`Publish(Frame)` authenticates the current selected platform service's unique
owner through the existing platform boundary, never a provider claim. Validate
metadata, seals and size, then atomically recheck the still-live pending context:
Home visible, exact target/output, authoritative window generation/token still
present and associated, registered shell owner, unblocked session and unexpired
request. Only then retain the descriptor and publish ready. A stale/mismatching,
expired, superseded or already-completed request is `Conflict`; invalid data is
`InvalidArgument`; other senders get `PermissionDenied`. Errors contain no
context, paths or pixel data. Late descriptors are closed, never installed.

`ReadFrame(request_id)` is allowed only for the current `org.arinn.Shell1` owner.
It returns a duplicate descriptor for the current ready context and echoes that
context. Missing/unsupported frames are `Unavailable`; a superseded request is
`Conflict`. Frame reads neither capture again nor create Home intent. The shell
checks the echoed context against current metadata/Home/Return evidence before
and after asynchronous bounded copying; cancelled or stale results are disposed.

`Cancel(request_id)` uses the same selected-platform authentication. It releases
only that current attempt/frame; stale cancellation cannot clear a replacement.
Cancellation and publication are serialized with context transitions. No caller
can supply a screenshot filename, script, shell command or arbitrary image URL.
The platform worker uses the same bus connection owning the selected platform
service; this contract grants no authority to a new helper service. It preserves
the existing trusted-local-plugin posture rather than promising a hostile-plugin
sandbox. Compositor protections are not disabled or bypassed.

## Capture and invalidation lifecycle

Only an eligible **explicit Home presentation intent** arms capture: an
explicit Home request, or a quick-menu request, which commits the same Home
state ([ADR-0106](../adr/0106-quick-menu-presentation.md)). Ordinary
Status/ReportHome recovery or reconnect does not. Home without an associated
external Activity stays on the normal background. One attempt per visible Home
context: repeated Home on that context does not recapture, even after failure.
A replacement lifetime needs a new explicit Home intent. Awaiting association,
GPU readback and producer work share a non-renewable 1500 ms deadline measured
from intent; failure/protected content/unsupported capture gives unavailable and
normal Home immediately, without delaying keyboard focus or Return.

Return/Home hide, target or output change, window disappearance/remap, inventory
or producer generation loss, relevant provider loss, daemon/shell owner loss,
lock/sleep or unknown session-guard state invalidate pending and ready frames.
The shell independently clears and hides frames when its session guard blocks;
it must not wait for daemon IPC to protect the lock screen. Producer and daemon
also release their retained resources. Unrelated metadata cannot resurrect a
frame. At most one producer grab/worker and one installed frame are retained;
a cancelled worker may finish, but closes its result and cannot spawn an
unbounded queue of replacements. Frame descriptors and pixel references are
released on invalidation; no runtime screenshot files are written or persisted.

The first KDE producer remains WindowThumbnail of the exact verified window.
A small trusted in-process C++ worker can copy the grab result and export sealed
pixels asynchronously, converting Qt premultiplied ARGB grab results to the
contract's straight-alpha RGBA8888 in sRGB on the worker; Qt's grab URL is an
in-process image key, not a transport file. No PNG encoding runs in the compositor callback. An X11 producer uses the
same contract with platform-owned native capture; it is separate implementation
work and cannot substitute a desktop/root capture for the selected window.
Near/far rendering remains .95/.90 scale, blur 8/36 and .6/.32 brightness through
the shell's own rendering, preserving the accepted visual contract.

## Budgets and verification obligations

Hard admission bounds: one descriptor, 8 MiB, 1920×1080 physical pixels, at most
one active capture/worker and one ready frame, and 1500 ms capture deadline.
Metadata bodies retain the ordinary 256 KiB limit. The implementation must bound
DPR-sensitive intermediate/effect textures and keep transformation off the input
path. These bounds are not a measured GPU or RSS acceptance report.

Implementation tests must cover exact identity, wrong publisher/reader, missing
association, repeated Home, late publication, Return and generation invalidation,
lock/unknown guard/provider/owner loss, malformed/oversized/unsealed descriptors,
resource release, and responsive local focus during slow capture. Use fake
windows, synthetic pixels, private buses and clocks; no real-window pixels in
repository fixtures, diagnostics, Sentry events, breadcrumbs, traces or dumps.
Runtime diagnostics may record dimensions, durations and normalized results only.

Owner-run KDE/Wayland and XFCE/X11 reports must record physical/logical size and
DPR; capture/readback/worker/publication latency; Home/focus latency; incremental
CPU/GPU frame allocation and RSS before capture, near/far, invalidation and repeated
cycles; frame time while media plays; shader/kernel/colour differences; and
lock/Return/protected-content behavior. Keep G-5.2/G-5.4/G-5.5 pending until this
real-device evidence and visual acceptance exist. No operator test is authorized
by defining this contract.

Sources: [Qt grab result](https://doc.qt.io/qt-6/qquickitemgrabresult.html),
[Qt QML grab](https://doc.qt.io/qt-6/qml-qtquick-item.html#grabToImage-method),
and the existing [runtime experiment](background-experiment.md).

## Daemon implementation

`crates/arinnd/src/runtime/background.rs` exports both interfaces at the daemon
root ([#149](https://github.com/LimeBeck/arinn/issues/149)). It follows the
contract above; these are the implementation choices within it:

- **Arming.** The handoff is exported before `Daemon1`, so no Home request
  arrives before it exists. `HomeRequested` is emitted before arming, and arming
  runs in the background whenever a publication or reconciliation holds the
  state, so a Home press never waits for capture work. The deadline and the
  reported latency count from the committed request. Only a committed
  `Daemon1.ReportHomeRequest` or `Daemon1.ReportQuickMenuRequest` arms an
  attempt ([ADR-0106](../adr/0106-quick-menu-presentation.md): the quick menu
  commits the same Home state, so Home opened from it shows the frame).
  `QuickMenuRequested` is emitted before arming, as `HomeRequested` is. Either
  request arms only with a non-empty target and output that still match the
  committed Home report. The X11/EWMH adapter reports its one output,
  `x11-screen`, with both requests ([X11/EWMH](x11-ewmh.md)), so they arm there
  too. `ReportHome` and adapter reconnects never arm.
- **Home evidence.** The daemon reads Home visibility, target and output, the
  Return association, the adapter's unique owner and liveness, and whether the
  resolved window token is still in the committed inventory generation, in one
  step under the inventory lock (`platform/kde/windows.rs`, `HomeEvidence`).
  Every Home, Return, inventory or adapter-lifetime change wakes the reconciler.
- **Repeated Home.** A repeated `ReportHomeRequest` clears the Return
  association until the adapter reports it again (ADR-0073). That transient
  empty association does not invalidate the attempt; a removed token, a new
  inventory generation or a different reported window does. Every attempt is
  bound to the Home lifetime that armed it (a counter of Home becoming
  visible), so a quick Return and Home with the same target and output starts
  a fresh request even if nothing reconciled while Home was hidden.
- **Session guard.** `platform/session_lock.rs` mirrors the shell's guard on the
  daemon side: the first answering freedesktop/XFCE screensaver endpoint, pinned
  to its owner, and `PrepareForSleep` on the system bus, accepted only from
  the current `org.freedesktop.login1` owner and seeded from its
  `PreparingForSleep` property. Unknown state is blocked, including:
  - no endpoint, or an owner change;
  - a failed or timed-out owner lookup of a preferred endpoint, which never
    falls through to a weaker one;
  - a probe reply overtaken by a lock, owner or sleep event, so the guard stays
    blocked and probes again;
  - a lagged or closed subscription, so the guard blocks and subscribes again.

  A guard that stops reporting altogether counts as blocked. Without a system bus,
  sleep is seen only through the lock that KDE and XFCE take before suspend.
  The shell keeps its own guard and never waits for this one.
- **Provider and owner loss.** The relevant provider is the selected platform
  adapter, the producer: its owner loss, staleness or reconnect invalidates the
  attempt. An application provider's loss reaches the attempt through the
  inventory when its window goes away; the opaque target is never mapped to a
  provider. A change of the `org.arinn.Shell1` owner, including none, also
  invalidates; publication without a shell owner is `Conflict`. A ready frame
  is bound to the shell owner at publication, so a successor never reads it,
  even before the owner change is processed.
- **Publisher.** `Publish` and `Cancel` read the selected platform service's
  owner just before taking the state lock and require it, together with the
  inventory owner, at commit. No bus round trip separates the evidence check
  from the commit, and a former owner cannot install a frame after an owner
  change it raced with. A Home request queued behind the state arms only the
  Home lifetime it was made in.
- **Descriptors.** `Publish` checks `fstat` (regular file, exact size) and
  `F_GET_SEALS` (write, grow and shrink) before taking the state lock. The
  daemon never maps or reads pixels. `ReadFrame` returns a close-on-exec
  duplicate. Receivers must read positionally because duplicates share one
  file offset.
- **Diagnostics.** Normalized JSON lines with no identifiers:
  `background-armed`, `background-ready` (`duration_ms` from the Home request to
  accepted publication), `background-unavailable` (`outcome` is one of
  `deadline`, `session`, `window`, `adapter`, `shell` or `cancelled`) and
  `background-released`. They are the daemon's hooks for the owner's G-5.2
  latency report ([performance harness](performance-harness.md#activity-background-g-52)).

Private-bus tests in `crates/arinnd/src/runtime/background_tests.rs` cover exact
identity, wrong publisher and reader, missing association, repeated Home,
late and duplicate publication, Return, target change, window removal and
remap, adapter reconnect, lock and blocked publication, shell owner loss,
cancellation, malformed, oversized and unsealed descriptors, the unrenewed
deadline and the absence of runtime files. They use synthetic pixels and fake
windows; they are not producer, consumer or device evidence.
