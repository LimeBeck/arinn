// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Activity background producer of the EWMH backend (ADR-0087, specification
//! §12.3). It follows the daemon's `Background1` and, for a pending attempt,
//! captures exactly the attempt's window: its top-level frame's composited
//! pixmap through XComposite `NameWindowPixmap`. It never captures the root
//! window or the desktop. Without a compositing manager, an unredirected or
//! unmapped window, an unsupported pixel format or a locked session there is
//! no frame and the attempt is cancelled; Home keeps its normal background.
//!
//! Pixels are converted to straight-alpha RGBA8888, box-filtered into the
//! 1920×1080 bounds and published as one sealed memfd from the adapter's own
//! bus connection, which owns `org.arinn.X11`. Nothing is written to a file
//! or logged except normalized outcomes.
use anyhow::{Context as _, Result, bail, ensure};
use arinn_protocol::background::{
    BackgroundCaptureProxy, BackgroundContext, BackgroundFrame, BackgroundProxy, MAX_HEIGHT,
    MAX_WIDTH,
};
use futures_util::StreamExt;
use std::{
    io::Write,
    os::fd::{FromRawFd, OwnedFd},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use x11rb::{
    connection::{Connection as _, RequestConnection as _},
    protocol::{composite::ConnectionExt as _, xproto::*},
};
use zbus::Connection;

/// The Home output the EWMH backend reports. It names the X screen, not a
/// RandR output: the backend does not place by output (x11-ewmh.md), and no
/// Qt screen carries this name, so the shell never moves for it.
pub const OUTPUT: &str = "x11-screen";
/// The local guard; the daemon's deadline is authoritative.
const DEADLINE: Duration = Duration::from_millis(1500);
/// Largest window read from the server: 16 Mi pixels, a 64 MiB reply, the
/// same input bound as the KDE producer (twice a 4K window).
const MAX_SOURCE_PIXELS: u64 = 16 * 1024 * 1024;

/// A current inventory row: the opaque token and its native window.
pub type Windows = Vec<(String, Window)>;
/// The capture worker's result: a bounded sealed frame, or none.
type Work = tokio::task::JoinHandle<Result<Option<(u32, u32, OwnedFd)>>>;

/// Publishes the current inventory rows to the producer.
pub fn offer(windows: &watch::Sender<Windows>, rows: &[crate::native::Row]) {
    windows.send_replace(rows.iter().map(|r| (r.token.clone(), r.xid)).collect());
}

/// Normalized diagnostics only: no window, token, target or request ID.
fn log(outcome: &'static str, started: Instant) {
    eprintln!(
        "{{\"level\":\"info\",\"component\":\"arinn-platform-x11\",\"provider_id\":null,\"request_id\":null,\"operation\":\"background-capture\",\"outcome\":\"{outcome}\",\"error\":null,\"duration_ms\":{}}}",
        started.elapsed().as_millis()
    );
}

/// Follows `Background1` on the daemon `owner` until the connection ends.
pub async fn run(
    c: Connection,
    owner: String,
    windows: watch::Receiver<Windows>,
    display: Option<String>,
) -> Result<()> {
    let reader = BackgroundProxy::builder(&c)
        .destination(owner.clone())?
        .build()
        .await?;
    let producer = BackgroundCaptureProxy::builder(&c)
        .destination(owner)?
        .build()
        .await?;
    // Subscribe before the first Get. A daemon without the interface ends
    // this optional task quietly.
    let Ok(mut changes) = reader.receive_changed().await else {
        return Ok(());
    };
    let mut done = String::new();
    // At most one capture worker: a timed-out one may finish, but no other
    // starts until it has (ADR-0087).
    let mut worker: Option<Work> = None;
    loop {
        match reader.get().await {
            Ok((snapshot,)) => {
                if snapshot.validate().is_ok()
                    && snapshot.state == "pending"
                    && let Some(context) = snapshot.context.first()
                    && context.request_id != done
                {
                    done = context.request_id.clone();
                    produce(
                        &c,
                        &producer,
                        context,
                        &windows,
                        display.clone(),
                        &mut worker,
                    )
                    .await;
                }
            }
            // A failed read (an older daemon, an exhausted epoch, a timeout)
            // waits for the next Changed; nothing is retried in a loop.
            Err(_) => log("unreadable", Instant::now()),
        }
        if changes.next().await.is_none() {
            return Ok(());
        }
    }
}

async fn produce(
    c: &Connection,
    producer: &BackgroundCaptureProxy<'_>,
    context: &BackgroundContext,
    windows: &watch::Receiver<Windows>,
    display: Option<String>,
    worker: &mut Option<Work>,
) {
    let started = Instant::now();
    // Exact identity: the Return target is the token of one current window.
    let xid = windows
        .borrow()
        .iter()
        .find(|(token, _)| *token == context.window.token && *token == context.target)
        .map(|(_, xid)| *xid);
    if worker.as_ref().is_some_and(|w| w.is_finished()) {
        *worker = None;
    }
    let outcome = match xid {
        None => Err("window"),
        // A previous capture is still running (a stalled or grabbed server).
        Some(_) if worker.is_some() => Err("busy"),
        Some(_) if !crate::unlocked(c).await => Err("session"),
        Some(xid) => {
            let work = worker.insert(tokio::task::spawn_blocking(
                move || -> Result<Option<(u32, u32, OwnedFd)>> {
                    let Some((width, height, rgba)) = capture(display.as_deref(), xid)? else {
                        return Ok(None);
                    };
                    Ok(Some((width, height, sealed(&rgba)?)))
                },
            ));
            let result = tokio::time::timeout(DEADLINE, work).await;
            if result.is_ok() {
                *worker = None;
            }
            match result {
                Ok(Ok(Ok(Some(frame)))) => Ok(frame),
                Ok(Ok(Ok(None))) => Err("unsupported"),
                Ok(_) => Err("capture"),
                // The worker keeps its slot until it finishes.
                Err(_) => Err("deadline"),
            }
        }
    };
    match outcome {
        Ok((width, height, pixels)) => {
            let frame = BackgroundFrame {
                context: context.clone(),
                width,
                height,
                pixels: pixels.into(),
            };
            let published = producer.publish(&frame).await.is_ok();
            log(if published { "published" } else { "rejected" }, started);
        }
        Err(reason) => {
            log(reason, started);
            let _ = producer.cancel(&context.request_id).await;
        }
    }
}

/// Reads the composited pixmap of `xid`'s top-level frame and returns
/// bounded straight-alpha RGBA, or `None` when no frame can be taken.
pub fn capture(display: Option<&str>, xid: Window) -> Result<Option<(u32, u32, Vec<u8>)>> {
    #[cfg(test)]
    tests::CAPTURES
        .lock()
        .unwrap()
        .push(display.unwrap_or_default().to_owned());
    let (c, screen) = x11rb::connect(display)?;
    let setup = c.setup();
    let root = setup.roots[screen].root;
    if setup.image_byte_order != ImageOrder::LSB_FIRST
        || c.extension_information(x11rb::protocol::composite::X11_EXTENSION_NAME)?
            .is_none()
    {
        return Ok(None);
    }
    let version = c.composite_query_version(0, 4)?.reply()?;
    if (version.major_version, version.minor_version) < (0, 2) {
        return Ok(None);
    }
    // Only a running compositing manager keeps redirected window contents.
    let selection = c
        .intern_atom(false, format!("_NET_WM_CM_S{screen}").as_bytes())?
        .reply()?
        .atom;
    if c.get_selection_owner(selection)?.reply()?.owner == x11rb::NONE {
        return Ok(None);
    }
    // The window manager's frame is the redirected top-level ancestor.
    let mut frame = xid;
    loop {
        let tree = c.query_tree(frame)?.reply()?;
        if tree.parent == root || tree.parent == x11rb::NONE {
            break;
        }
        frame = tree.parent;
    }
    let attributes = c.get_window_attributes(frame)?.reply()?;
    if attributes.map_state != MapState::VIEWABLE {
        return Ok(None);
    }
    let geometry = c.get_geometry(frame)?.reply()?;
    let (width, height) = (u32::from(geometry.width), u32::from(geometry.height));
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_SOURCE_PIXELS {
        return Ok(None);
    }
    let Some(format) = pixel_format(setup, &attributes, geometry.depth, screen) else {
        return Ok(None);
    };
    let pixmap = c.generate_id()?;
    if c.composite_name_window_pixmap(frame, pixmap)?
        .check()
        .is_err()
    {
        // Not redirected (an unredirected fullscreen window, for example).
        return Ok(None);
    }
    let image = c
        .get_image(
            ImageFormat::Z_PIXMAP,
            pixmap,
            0,
            0,
            geometry.width,
            geometry.height,
            !0,
        )?
        .reply();
    let _ = c.free_pixmap(pixmap);
    let _ = c.flush();
    let image = image.context("window pixmap")?;
    ensure!(
        image.data.len() as u64 == u64::from(width) * u64::from(height) * 4,
        "unexpected image size"
    );
    Ok(Some(bound(width, height, &image.data, format)))
}

/// How 32-bit server pixels map to colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Depth 24 in 32 bits: B, G, R, unused (LSB first).
    Opaque,
    /// Depth 32 ARGB, premultiplied as composited windows are: B, G, R, A.
    Premultiplied,
}

fn pixel_format(
    setup: &Setup,
    attributes: &GetWindowAttributesReply,
    depth: u8,
    screen: usize,
) -> Option<Format> {
    let format = setup.pixmap_formats.iter().find(|f| f.depth == depth)?;
    if format.bits_per_pixel != 32 {
        return None;
    }
    let visual = setup.roots[screen]
        .allowed_depths
        .iter()
        .flat_map(|d| d.visuals.iter())
        .find(|v| v.visual_id == attributes.visual)?;
    if visual.class != VisualClass::TRUE_COLOR
        || (visual.red_mask, visual.green_mask, visual.blue_mask) != (0xff0000, 0xff00, 0xff)
    {
        return None;
    }
    match depth {
        24 => Some(Format::Opaque),
        32 => Some(Format::Premultiplied),
        _ => None,
    }
}

/// Scales LSB-first 32-bit server pixels into the physical bounds with a box
/// filter, keeping the aspect ratio, and converts them to straight-alpha
/// RGBA8888 in the same pass. Averaging happens on premultiplied values, so
/// transparent pixels add no dark fringes; each output pixel is then
/// unpremultiplied. Smaller images keep their size. No full-size copy of the
/// source is made.
pub fn bound(width: u32, height: u32, data: &[u8], format: Format) -> (u32, u32, Vec<u8>) {
    let scale = f64::max(
        1.0,
        f64::max(
            f64::from(width) / f64::from(MAX_WIDTH),
            f64::from(height) / f64::from(MAX_HEIGHT),
        ),
    );
    let w = ((f64::from(width) / scale).round() as u32).clamp(1, MAX_WIDTH);
    let h = ((f64::from(height) / scale).round() as u32).clamp(1, MAX_HEIGHT);
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        let y0 = u64::from(y) * u64::from(height) / u64::from(h);
        let y1 = (u64::from(y + 1) * u64::from(height) / u64::from(h)).max(y0 + 1);
        for x in 0..w {
            let x0 = u64::from(x) * u64::from(width) / u64::from(w);
            let x1 = (u64::from(x + 1) * u64::from(width) / u64::from(w)).max(x0 + 1);
            // Premultiplied sums of R, G, B and alpha.
            let mut sum = [0u64; 4];
            for sy in y0..y1 {
                let row = (sy * u64::from(width) * 4) as usize;
                for sx in x0..x1 {
                    let i = row + (sx * 4) as usize;
                    let alpha = match format {
                        Format::Opaque => 255,
                        Format::Premultiplied => data[i + 3],
                    };
                    sum[0] += u64::from(data[i + 2]);
                    sum[1] += u64::from(data[i + 1]);
                    sum[2] += u64::from(data[i]);
                    sum[3] += u64::from(alpha);
                }
            }
            let n = (y1 - y0) * (x1 - x0);
            let alpha = (sum[3] + n / 2) / n;
            let o = ((y * w + x) * 4) as usize;
            for k in 0..3 {
                // Unpremultiply the averaged colour: sum / n * 255 / alpha.
                out[o + k] = if alpha == 0 {
                    0
                } else {
                    ((sum[k] * 255 + sum[3] / 2) / sum[3]).min(255) as u8
                };
            }
            out[o + 3] = alpha as u8;
        }
    }
    (w, h, out)
}

/// One sealed memfd holding exactly `rgba` (ADR-0087): write, grow and shrink
/// sealed, plus the seal seal, close-on-exec, no path.
pub fn sealed(rgba: &[u8]) -> Result<OwnedFd> {
    // SAFETY: memfd_create returns a new descriptor this function owns.
    let raw = unsafe {
        libc::memfd_create(
            c"arinn-background".as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };
    if raw < 0 {
        bail!("memfd_create failed");
    }
    // SAFETY: `raw` is a fresh, valid descriptor not owned elsewhere.
    let mut file = unsafe { std::fs::File::from_raw_fd(raw) };
    file.write_all(rgba)?;
    let seals = libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;
    // SAFETY: F_ADD_SEALS on the descriptor `file` owns.
    if unsafe { libc::fcntl(raw, libc::F_ADD_SEALS, seals) } != 0 {
        bail!("sealing failed");
    }
    Ok(file.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;
    /// Displays of the captures started, to prove that one worker runs at a
    /// time per producer.
    pub(super) static CAPTURES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    #[test]
    fn converts_server_pixels_to_straight_rgba() {
        assert_eq!(
            bound(1, 1, &[10, 20, 30, 99], Format::Opaque),
            (1, 1, vec![30, 20, 10, 255])
        );
        // Premultiplied half-transparent pixel and a fully transparent one.
        assert_eq!(
            bound(2, 1, &[64, 64, 128, 128, 5, 5, 5, 0], Format::Premultiplied).2,
            vec![255, 128, 128, 128, 0, 0, 0, 0]
        );
    }
    #[test]
    fn bounds_large_frames_with_aspect_ratio() {
        let (w, h, out) = bound(4, 2, &[7; 32], Format::Opaque);
        assert_eq!((w, h, out.len()), (4, 2, 32));
        let source = vec![200u8; 3840 * 2160 * 4];
        let (w, h, out) = bound(3840, 2160, &source, Format::Opaque);
        assert_eq!((w, h), (1920, 1080));
        assert_eq!(
            out.len() as u64,
            arinn_protocol::background::pixel_bytes(w, h).unwrap()
        );
        assert!(
            out.as_chunks::<4>()
                .0
                .iter()
                .all(|px| *px == [200, 200, 200, 255])
        );
        let (w, h, _) = bound(1000, 3000, &vec![0; 1000 * 3000 * 4], Format::Opaque);
        assert_eq!((w, h), (360, 1080));
        // Box filter averages a 2×2 block.
        let mut checker = vec![0u8; 3840 * 2 * 4];
        for (i, px) in checker.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            px.copy_from_slice(if i % 2 == 0 {
                &[0, 0, 0, 255]
            } else {
                &[255, 255, 255, 255]
            });
        }
        let (w, _, out) = bound(3840, 2, &checker, Format::Opaque);
        assert_eq!(w, 1920);
        assert_eq!(&out[..4], &[128, 128, 128, 255]);
    }
    #[test]
    fn transparent_neighbours_add_no_dark_fringe() {
        // Opaque red beside fully transparent pixels (a CSD shadow edge):
        // averaging in premultiplied space keeps the colour red.
        let mut source = vec![0u8; 3840 * 2 * 4];
        for (i, px) in source.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            if i % 2 == 0 {
                px.copy_from_slice(&[0, 0, 255, 255]);
            }
        }
        let (_, _, out) = bound(3840, 2, &source, Format::Premultiplied);
        assert_eq!(&out[..4], &[255, 0, 0, 128]);
    }
    #[test]
    fn sealed_memfd_is_immutable_and_exact() {
        let fd = sealed(&[1, 2, 3, 4]).unwrap();
        let raw = fd.as_raw_fd();
        // SAFETY: plain queries on a live descriptor.
        unsafe {
            let seals = libc::fcntl(raw, libc::F_GET_SEALS);
            let required = libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK;
            assert_eq!(seals & required, required);
            assert!(libc::fcntl(raw, libc::F_GETFD) & libc::FD_CLOEXEC != 0);
            assert_eq!(libc::pwrite(raw, [9u8].as_ptr().cast(), 1, 0), -1);
        }
        let mut file = std::fs::File::from(fd);
        assert_eq!(file.metadata().unwrap().len(), 4);
        assert!(file.write_all(&[9]).is_err());
    }
    /// A received frame: width, height and its bytes.
    type Received = (u32, u32, Vec<u8>);
    /// A fake daemon: one pending attempt, and what the producer sent.
    #[derive(Clone, Default)]
    struct Daemon {
        context: Arc<Mutex<Option<BackgroundContext>>>,
        published: Arc<Mutex<Vec<Received>>>,
        cancelled: Arc<Mutex<Vec<String>>>,
    }
    #[zbus::interface(name = "org.arinn.Background1")]
    impl Daemon {
        fn get(&self) -> (arinn_protocol::background::BackgroundSnapshot,) {
            let context = self.context.lock().unwrap().clone();
            (arinn_protocol::background::BackgroundSnapshot {
                epoch: context.as_ref().map_or(EPOCH.into(), |c| c.epoch.clone()),
                revision: 2,
                state: if context.is_some() { "pending" } else { "idle" }.into(),
                context: context.into_iter().collect(),
            },)
        }
    }
    struct Capture(Daemon);
    #[zbus::interface(name = "org.arinn.BackgroundCapture1")]
    impl Capture {
        fn publish(&self, frame: BackgroundFrame) {
            use std::os::unix::fs::FileExt;
            let fd: OwnedFd = frame.pixels.into();
            // SAFETY: a plain query on a live descriptor.
            let seals = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GET_SEALS) };
            assert!(seals & libc::F_SEAL_WRITE != 0);
            let mut bytes = vec![0; (frame.width * frame.height * 4) as usize];
            std::fs::File::from(fd)
                .read_exact_at(&mut bytes, 0)
                .unwrap();
            self.0
                .published
                .lock()
                .unwrap()
                .push((frame.width, frame.height, bytes));
        }
        fn cancel(&self, request_id: &str) {
            self.0.cancelled.lock().unwrap().push(request_id.into());
        }
    }
    #[derive(Clone, Default)]
    struct Lock(Arc<std::sync::atomic::AtomicBool>);
    #[zbus::interface(name = "org.freedesktop.ScreenSaver")]
    impl Lock {
        fn get_active(&self) -> bool {
            self.0.load(std::sync::atomic::Ordering::SeqCst)
        }
    }
    const EPOCH: &str = "11111111-1111-4111-8111-111111111111";
    use std::sync::{Arc, Mutex};
    #[tokio::test]
    #[ignore = "requires Xvfb; private bus and isolated composited window"]
    async fn publishes_the_exact_window_from_the_adapter_connection() -> Result<()> {
        use crate::native::fixture::Desktop;
        use x11rb::protocol::composite::Redirect;
        let bus = crate::lock_tests::Bus::new().await;
        let desktop = Desktop::new()?;
        let window = desktop.client("org.example.App.desktop", "")?;
        desktop.c.change_window_attributes(
            window,
            &ChangeWindowAttributesAux::new().background_pixel(0x002040ff),
        )?;
        desktop.c.map_window(window)?.check()?;
        desktop.c.composite_query_version(0, 4)?.reply()?;
        desktop
            .c
            .composite_redirect_subwindows(desktop.root, Redirect::MANUAL)?
            .check()?;
        desktop.compositing(true)?;
        desktop.c.clear_area(false, window, 0, 0, 0, 0)?.check()?;
        let daemon = Daemon::default();
        let lock = Lock::default();
        bus.server
            .object_server()
            .at("/org/arinn/Daemon1", daemon.clone())
            .await?;
        bus.server
            .object_server()
            .at("/org/arinn/Daemon1", Capture(daemon.clone()))
            .await?;
        bus.server
            .object_server()
            .at("/ScreenSaver", lock.clone())
            .await?;
        bus.server
            .request_name("org.freedesktop.ScreenSaver")
            .await?;
        let context = |request: &str, token: &str| BackgroundContext {
            epoch: EPOCH.into(),
            request_id: request.into(),
            target: token.into(),
            output: OUTPUT.into(),
            window: arinn_protocol::return_association::WindowRef {
                generation: "33333333-3333-4333-8333-333333333333".into(),
                token: token.into(),
            },
        };
        let first = "22222222-2222-4222-8222-222222222222";
        *daemon.context.lock().unwrap() = Some(context(first, "token-a"));
        let (windows, current) = watch::channel(vec![("token-a".to_string(), window)]);
        let owner = bus.server.unique_name().unwrap().to_string();
        let task = tokio::spawn(run(
            bus.client.clone(),
            owner,
            current,
            Some(desktop.display.clone()),
        ));
        let published = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(frame) = daemon.published.lock().unwrap().first().cloned() {
                    break frame;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await?;
        assert_eq!((published.0, published.1), (100, 100));
        assert_eq!(&published.2[..4], &[0x20, 0x40, 0xff, 0xff]);
        // Locked, or a target that is not a current window: cancelled, no frame.
        let emit = |request: &'static str| {
            let server = bus.server.clone();
            async move {
                server
                    .emit_signal(
                        None::<&str>,
                        "/org/arinn/Daemon1",
                        "org.arinn.Background1",
                        "Changed",
                        &(EPOCH, 3u64),
                    )
                    .await
                    .unwrap();
                request
            }
        };
        lock.0.store(true, std::sync::atomic::Ordering::SeqCst);
        let locked = "44444444-4444-4444-8444-444444444444";
        *daemon.context.lock().unwrap() = Some(context(locked, "token-a"));
        emit(locked).await;
        let unknown = "55555555-5555-4555-8555-555555555555";
        tokio::time::timeout(Duration::from_secs(5), async {
            while !daemon
                .cancelled
                .lock()
                .unwrap()
                .contains(&locked.to_string())
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await?;
        lock.0.store(false, std::sync::atomic::Ordering::SeqCst);
        windows.send_replace(vec![("token-b".to_string(), window)]);
        *daemon.context.lock().unwrap() = Some(context(unknown, "token-a"));
        emit(unknown).await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while !daemon
                .cancelled
                .lock()
                .unwrap()
                .contains(&unknown.to_string())
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await?;
        assert_eq!(daemon.published.lock().unwrap().len(), 1);
        task.abort();
        Ok(())
    }
    #[tokio::test]
    #[ignore = "requires Xvfb; private bus and a grabbed isolated server"]
    async fn a_stalled_capture_blocks_further_workers_until_it_finishes() -> Result<()> {
        use crate::native::fixture::Desktop;
        use x11rb::protocol::composite::Redirect;
        let bus = crate::lock_tests::Bus::new().await;
        let desktop = Desktop::new()?;
        let window = desktop.client("org.example.App.desktop", "")?;
        desktop.c.map_window(window)?.check()?;
        desktop.c.composite_query_version(0, 4)?.reply()?;
        desktop
            .c
            .composite_redirect_subwindows(desktop.root, Redirect::MANUAL)?
            .check()?;
        desktop.compositing(true)?;
        let daemon = Daemon::default();
        let lock = Lock::default();
        bus.server
            .object_server()
            .at("/org/arinn/Daemon1", daemon.clone())
            .await?;
        bus.server
            .object_server()
            .at("/org/arinn/Daemon1", Capture(daemon.clone()))
            .await?;
        bus.server.object_server().at("/ScreenSaver", lock).await?;
        bus.server
            .request_name("org.freedesktop.ScreenSaver")
            .await?;
        let context = |request: &str| BackgroundContext {
            epoch: EPOCH.into(),
            request_id: request.into(),
            target: "token-a".into(),
            output: OUTPUT.into(),
            window: arinn_protocol::return_association::WindowRef {
                generation: "33333333-3333-4333-8333-333333333333".into(),
                token: "token-a".into(),
            },
        };
        let changed = || async {
            bus.server
                .emit_signal(
                    None::<&str>,
                    "/org/arinn/Daemon1",
                    "org.arinn.Background1",
                    "Changed",
                    &(EPOCH, 3u64),
                )
                .await
                .unwrap();
        };
        let cancelled = |request: &'static str| {
            let daemon = daemon.clone();
            async move {
                tokio::time::timeout(Duration::from_secs(5), async {
                    while !daemon
                        .cancelled
                        .lock()
                        .unwrap()
                        .contains(&request.to_string())
                    {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                })
                .await
                .is_ok()
            }
        };
        let captures = || {
            CAPTURES
                .lock()
                .unwrap()
                .iter()
                .filter(|d| **d == desktop.display)
                .count()
        };
        // Another client holds the server: the first capture stalls.
        desktop.c.grab_server()?.check()?;
        let first = "22222222-2222-4222-8222-222222222222";
        *daemon.context.lock().unwrap() = Some(context(first));
        let (_windows, current) = watch::channel(vec![("token-a".to_string(), window)]);
        let task = tokio::spawn(run(
            bus.client.clone(),
            bus.server.unique_name().unwrap().to_string(),
            current,
            Some(desktop.display.clone()),
        ));
        assert!(
            cancelled(first).await,
            "deadline cancels the stalled attempt"
        );
        assert_eq!(captures(), 1);
        // The next request is refused at once while that worker still runs.
        let second = "44444444-4444-4444-8444-444444444444";
        *daemon.context.lock().unwrap() = Some(context(second));
        let asked = Instant::now();
        changed().await;
        assert!(cancelled(second).await);
        assert!(asked.elapsed() < DEADLINE, "busy is immediate");
        assert_eq!(captures(), 1, "no second worker");
        // Once the server is released the worker finishes and frees its slot.
        desktop.c.ungrab_server()?.check()?;
        desktop.c.flush()?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let third = "55555555-5555-4555-8555-555555555555";
        *daemon.context.lock().unwrap() = Some(context(third));
        changed().await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while daemon.published.lock().unwrap().is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        assert_eq!(captures(), 2);
        task.abort();
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; isolated composited window capture"]
    fn captures_only_the_composited_window() -> Result<()> {
        use crate::native::fixture::Desktop;
        use x11rb::protocol::composite::Redirect;
        let desktop = Desktop::new()?;
        let window = desktop.client("org.example.App.desktop", "")?;
        desktop.c.change_window_attributes(
            window,
            &ChangeWindowAttributesAux::new().background_pixel(0x00ff8000),
        )?;
        desktop.c.map_window(window)?.check()?;
        desktop.c.clear_area(false, window, 0, 0, 0, 0)?.check()?;
        // No compositing manager: no frame, never a root capture.
        assert!(capture(Some(&desktop.display), window)?.is_none());
        // The fixture acts as the compositing manager.
        desktop.c.composite_query_version(0, 4)?.reply()?;
        desktop
            .c
            .composite_redirect_subwindows(desktop.root, Redirect::MANUAL)?
            .check()?;
        desktop.compositing(true)?;
        desktop.c.clear_area(false, window, 0, 0, 0, 0)?.check()?;
        desktop.c.flush()?;
        let (w, h, rgba) = capture(Some(&desktop.display), window)?.expect("composited frame");
        assert_eq!((w, h), (100, 100));
        assert_eq!(&rgba[..4], &[0xff, 0x80, 0x00, 0xff]);
        // An unmapped window gives nothing.
        desktop.c.unmap_window(window)?.check()?;
        assert!(capture(Some(&desktop.display), window)?.is_none());
        Ok(())
    }
}
