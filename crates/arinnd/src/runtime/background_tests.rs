// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Private-bus lifecycle tests of the Activity background handoff
//! (ADR-0087). Synthetic pixels and fake windows only.
use super::*;
use crate::{
    runtime::Daemon,
    storage::Paths,
    test_support::{Bounded, served},
};
use arinn_protocol::{
    background::{BackgroundCaptureProxy, BackgroundProxy},
    kwin::KWinBridgeProxy,
    production::ProductionDaemonProxy,
    return_association::KWinReturnProxy,
};
use std::{
    io::{BufRead, BufReader, Write},
    os::fd::FromRawFd,
    path::PathBuf,
    process::{Child, Command, Stdio},
};
use uuid::Uuid;

struct Fixture {
    bus: Child,
    folder: PathBuf,
    core: Core,
    kwin: Connection,
    shell: Connection,
    stranger: Connection,
    blocked: watch::Sender<bool>,
    generation: String,
    inventory: i32,
    /// Committed tokens; tokens are never reused after removal.
    tokens: Vec<String>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.bus.kill();
        let _ = self.bus.wait();
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}
async fn connect(address: &str) -> Connection {
    zbus::connection::Builder::address(address)
        .unwrap()
        .build()
        .await
        .unwrap()
}
impl Fixture {
    async fn new() -> Self {
        let mut bus = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(bus.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let address = address.trim();
        let server = connect(address).await;
        let kwin = connect(address).await;
        let shell = connect(address).await;
        let stranger = connect(address).await;
        server.request_name(production::SERVICE).await.unwrap();
        kwin.request_name(crate::platform::adapter_service().unwrap())
            .await
            .unwrap();
        shell.request_name(SHELL_SERVICE).await.unwrap();
        let folder = std::env::temp_dir().join(format!("arinn-background-{}", Uuid::new_v4()));
        std::fs::create_dir(&folder).unwrap();
        let core = Core::new(
            &server,
            Paths {
                config: folder.join("config"),
                data: folder.join("data"),
                state: folder.join("state"),
            },
            folder.join("runtime"),
        );
        server
            .object_server()
            .at(production::PATH, Daemon { core: core.clone() })
            .await
            .unwrap();
        core.export_platform().await.unwrap();
        let (blocked, receiver) = watch::channel(false);
        core.export_background(receiver).await.unwrap();
        // The first call follows the first export with no round trip (#528).
        served(&kwin, &server).await;
        let generation = KWinBridgeProxy::new(&kwin)
            .await
            .unwrap()
            .connect(&Uuid::new_v4().to_string())
            .bounded()
            .await
            .unwrap();
        let mut f = Self {
            bus,
            folder,
            core,
            kwin,
            shell,
            stranger,
            blocked,
            generation,
            inventory: 0,
            tokens: Vec::new(),
        };
        f.inventory(&["token-0", "token-1"]).await;
        f
    }
    async fn inventory(&mut self, tokens: &[&str]) {
        self.inventory += 1;
        self.tokens = tokens.iter().map(ToString::to_string).collect();
        let bridge = KWinBridgeProxy::new(&self.kwin).await.unwrap();
        bridge
            .begin_inventory(&self.generation, self.inventory, tokens.len() as i32)
            .await
            .unwrap();
        for (i, token) in tokens.iter().enumerate() {
            bridge
                .put_window(
                    &self.generation,
                    self.inventory,
                    i as i32,
                    token,
                    "fixture.desktop",
                    "",
                    12,
                )
                .await
                .unwrap();
        }
        bridge
            .commit_inventory(&self.generation, self.inventory)
            .await
            .unwrap();
    }
    async fn home(&self, target: &str, output: &str) {
        ProductionDaemonProxy::new(&self.kwin)
            .await
            .unwrap()
            .report_home_request(target, output)
            .await
            .unwrap();
    }
    async fn hide(&self) {
        zbus::Proxy::new(
            &self.kwin,
            production::SERVICE,
            production::PATH,
            production::SERVICE,
        )
        .await
        .unwrap()
        .call::<_, _, ()>("ReportHome", &(false, "", ""))
        .await
        .unwrap();
    }
    async fn associate(&self, target: &str, token: &str) {
        KWinReturnProxy::new(&self.kwin)
            .await
            .unwrap()
            .report(&self.generation, self.inventory, target, token)
            .await
            .unwrap();
    }
    async fn reader(&self, connection: &Connection) -> BackgroundProxy<'static> {
        BackgroundProxy::new(connection).await.unwrap()
    }
    async fn get(&self) -> BackgroundSnapshot {
        let (snapshot,) = self.reader(&self.stranger).await.get().await.unwrap();
        snapshot.validate().unwrap();
        snapshot
    }
    async fn producer(&self, connection: &Connection) -> BackgroundCaptureProxy<'static> {
        BackgroundCaptureProxy::new(connection).await.unwrap()
    }
    /// Home over `activity` with its Return window, the first committed
    /// token, resolved.
    async fn pending(&self) -> BackgroundContext {
        self.home("activity", "screen").await;
        let pending = self.get().await;
        assert_eq!(pending.state, "pending");
        assert!(pending.context.is_empty(), "no context before association");
        self.associate("activity", &self.tokens[0]).await;
        let snapshot = self.wait_state("pending", true).await;
        snapshot.context[0].clone()
    }
    async fn wait_state(&self, state: &str, with_context: bool) -> BackgroundSnapshot {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let snapshot = self.get().await;
                if snapshot.state == state && snapshot.context.is_empty() != with_context {
                    break snapshot;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("state {state}"))
    }
}

fn memfd(bytes: &[u8], seals: libc::c_int) -> OwnedFd {
    // SAFETY: plain syscalls on a descriptor this function owns.
    unsafe {
        let fd = libc::memfd_create(
            c"arinn-background-test".as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        );
        assert!(fd >= 0);
        let mut file = std::fs::File::from_raw_fd(fd);
        file.write_all(bytes).unwrap();
        if seals != 0 {
            assert_eq!(libc::fcntl(fd, libc::F_ADD_SEALS, seals), 0);
        }
        file.into()
    }
}
const SEALED: libc::c_int =
    libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;
fn pixels() -> Vec<u8> {
    (0..2 * 2 * 4).map(|i| i as u8).collect()
}
fn frame(context: &BackgroundContext, width: u32, height: u32, fd: OwnedFd) -> BackgroundFrame {
    BackgroundFrame {
        context: context.clone(),
        width,
        height,
        pixels: fd.into(),
    }
}
fn error<T: std::fmt::Debug>(result: zbus::Result<T>, code: &str) {
    assert!(
        matches!(result, Err(zbus::Error::MethodError(ref name, _, _)) if name.as_str() == format!("org.arinn.Error1.{code}")),
        "expected {code}: {result:?}"
    );
}
/// Positional read: descriptors passed over D-Bus share one file offset.
fn read_all(fd: OwnedFd, len: usize) -> Vec<u8> {
    use std::os::unix::fs::FileExt;
    let mut bytes = vec![0; len];
    std::fs::File::from(fd)
        .read_exact_at(&mut bytes, 0)
        .unwrap();
    bytes
}

#[tokio::test]
async fn explicit_home_arms_one_attempt_and_only_the_shell_reads_it() {
    let f = Fixture::new().await;
    let idle = f.get().await;
    assert_eq!(
        (idle.epoch.as_str(), idle.revision),
        (f.core.epoch.as_str(), 1)
    );
    assert_eq!(idle.state, "idle");
    let mut changes = f.reader(&f.shell).await.receive_changed().await.unwrap();
    let context = f.pending().await;
    let signal = tokio::time::timeout(Duration::from_secs(2), changes.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.args().unwrap().epoch, f.core.epoch);
    assert_eq!(context.epoch, f.core.epoch);
    assert_eq!(
        (context.target.as_str(), context.output.as_str()),
        ("activity", "screen")
    );
    assert_eq!(context.window.generation, f.generation);
    assert_eq!(context.window.token, "token-0");

    // Only the selected platform owner publishes.
    for connection in [&f.stranger, &f.shell] {
        error(
            f.producer(connection)
                .await
                .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
                .await,
            "PermissionDenied",
        );
        error(
            f.producer(connection)
                .await
                .cancel(&context.request_id)
                .await,
            "PermissionDenied",
        );
    }
    let producer = f.producer(&f.kwin).await;
    // Malformed descriptors and metadata are rejected before installation.
    let (read, _write) = std::io::pipe().unwrap();
    for bad in [
        frame(&context, 2, 2, memfd(&pixels(), 0)),
        frame(
            &context,
            2,
            2,
            memfd(&pixels(), libc::F_SEAL_GROW | libc::F_SEAL_SHRINK),
        ),
        frame(&context, 2, 2, memfd(&pixels()[..12], SEALED)),
        frame(
            &context,
            2,
            2,
            memfd(&[pixels(), vec![0; 4]].concat(), SEALED),
        ),
        frame(&context, 2, 2, OwnedFd::from(read)),
        frame(&context, 0, 2, memfd(&pixels(), SEALED)),
        frame(&context, 1921, 1, memfd(&pixels(), SEALED)),
    ] {
        error(producer.publish(&bad).await, "InvalidArgument");
    }
    let file = f.folder.join("ordinary");
    std::fs::write(&file, pixels()).unwrap();
    error(
        producer
            .publish(&frame(
                &context,
                2,
                2,
                std::fs::File::open(&file).unwrap().into(),
            ))
            .await,
        "InvalidArgument",
    );
    std::fs::remove_file(&file).unwrap();
    // Wrong identities are conflicts and never install pixels.
    for mismatch in [
        "request",
        "target",
        "output",
        "token",
        "generation",
        "epoch",
    ] {
        let mut wrong = context.clone();
        match mismatch {
            "request" => wrong.request_id = Uuid::new_v4().to_string(),
            "target" => wrong.target = "other".into(),
            "output" => wrong.output = "other".into(),
            "token" => wrong.window.token = "token-1".into(),
            "generation" => wrong.window.generation = Uuid::new_v4().to_string(),
            _ => wrong.epoch = Uuid::new_v4().to_string(),
        }
        error(
            producer
                .publish(&frame(&wrong, 2, 2, memfd(&pixels(), SEALED)))
                .await,
            "Conflict",
        );
    }
    assert_eq!(f.get().await.state, "pending");
    error(
        f.reader(&f.shell)
            .await
            .read_frame(&context.request_id)
            .await,
        "Unavailable",
    );

    producer
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    let ready = f.get().await;
    assert_eq!(ready.state, "ready");
    assert_eq!(ready.context, vec![context.clone()]);
    // A late or duplicate publication is closed, never installed.
    error(
        producer
            .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
            .await,
        "Conflict",
    );
    for connection in [&f.stranger, &f.kwin] {
        error(
            f.reader(connection)
                .await
                .read_frame(&context.request_id)
                .await,
            "PermissionDenied",
        );
    }
    let reader = f.reader(&f.shell).await;
    error(reader.read_frame("not-a-uuid").await, "InvalidArgument");
    error(
        reader.read_frame(&Uuid::new_v4().to_string()).await,
        "Conflict",
    );
    for _ in 0..2 {
        let (read,) = reader.read_frame(&context.request_id).await.unwrap();
        assert_eq!(read.context, context);
        assert_eq!((read.width, read.height), (2, 2));
        let fd: OwnedFd = read.pixels.into();
        sealed(&fd, 16).unwrap();
        assert_eq!(read_all(fd, 16), pixels());
    }

    // Repeated Home on the same context never recaptures.
    f.home("activity", "screen").await;
    let repeated = f.get().await;
    assert_eq!(repeated.state, "ready");
    assert_eq!(repeated.context[0].request_id, context.request_id);

    // Return ends the lifetime; the next explicit Home arms a fresh request.
    f.hide().await;
    assert!(f.wait_state("idle", false).await.revision > ready.revision);
    error(reader.read_frame(&context.request_id).await, "Unavailable");
    let next = f.pending().await;
    assert_ne!(next.request_id, context.request_id);
    error(
        producer
            .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
            .await,
        "Conflict",
    );
    // Cancellation releases only the current attempt.
    error(producer.cancel(&context.request_id).await, "Conflict");
    error(producer.cancel("bad").await, "InvalidArgument");
    producer.cancel(&next.request_id).await.unwrap();
    assert_eq!(f.get().await.state, "unavailable");
    error(
        producer
            .publish(&frame(&next, 2, 2, memfd(&pixels(), SEALED)))
            .await,
        "Conflict",
    );
    // Even after the failure, repeated Home does not retry.
    f.home("activity", "screen").await;
    let failed = f.get().await;
    assert_eq!(failed.state, "unavailable");
    assert_eq!(failed.context[0].request_id, next.request_id);
}

/// ADR-0106: the quick-menu request commits the Home state, so it arms the
/// same capture as an explicit Home request. Home opened from the quick menu
/// then shows the Activity's last frame (§12.3), and a later Home press on
/// that context does not recapture. Rejected calls arm nothing.
#[tokio::test]
async fn quick_menu_request_arms_the_activity_background() {
    let f = Fixture::new().await;
    error(
        ProductionDaemonProxy::new(&f.stranger)
            .await
            .unwrap()
            .report_quick_menu_request("activity", "screen")
            .await,
        "PermissionDenied",
    );
    assert_eq!(f.get().await.state, "idle");
    let adapter = ProductionDaemonProxy::new(&f.kwin).await.unwrap();
    adapter
        .report_quick_menu_request("activity", "screen")
        .await
        .unwrap();
    let pending = f.get().await;
    assert_eq!(pending.state, "pending");
    f.associate("activity", &f.tokens[0]).await;
    let snapshot = f.wait_state("pending", true).await;
    let context = &snapshot.context[0];
    assert_eq!(context.epoch, f.core.epoch);
    assert_eq!(
        (context.target.as_str(), context.output.as_str()),
        ("activity", "screen")
    );
    assert_eq!(context.window.token, "token-0");
    // One attempt per visible Home context: Home from the quick menu keeps it.
    f.home("activity", "screen").await;
    let after = f.get().await;
    assert_eq!(after.context[0].request_id, context.request_id);
}

/// ADR-0111: the power-menu request commits the Home state, so it arms the
/// same capture as an explicit Home request: the power menu opens over Home
/// with the Activity's last frame behind it (§5.7, §12.3). Rejected calls
/// arm nothing, and a later Home press on that context does not recapture.
#[tokio::test]
async fn power_menu_request_arms_the_activity_background() {
    let f = Fixture::new().await;
    error(
        ProductionDaemonProxy::new(&f.stranger)
            .await
            .unwrap()
            .report_power_menu_request("activity", "screen")
            .await,
        "PermissionDenied",
    );
    assert_eq!(f.get().await.state, "idle");
    let adapter = ProductionDaemonProxy::new(&f.kwin).await.unwrap();
    adapter
        .report_power_menu_request("activity", "screen")
        .await
        .unwrap();
    let pending = f.get().await;
    assert_eq!(pending.state, "pending");
    f.associate("activity", &f.tokens[0]).await;
    let snapshot = f.wait_state("pending", true).await;
    let context = &snapshot.context[0];
    assert_eq!(context.epoch, f.core.epoch);
    assert_eq!(
        (context.target.as_str(), context.output.as_str()),
        ("activity", "screen")
    );
    assert_eq!(context.window.token, "token-0");
    f.home("activity", "screen").await;
    let after = f.get().await;
    assert_eq!(after.context[0].request_id, context.request_id);
}

#[tokio::test]
async fn home_without_an_activity_or_output_keeps_the_normal_background() {
    let f = Fixture::new().await;
    f.home("", "screen").await;
    assert_eq!(f.get().await.state, "idle");
    // The EWMH adapter reports no output yet: nothing is armed.
    f.home("activity", "").await;
    assert_eq!(f.get().await.state, "idle");
    // A state-only Home report or reconnect never arms capture.
    zbus::Proxy::new(
        &f.kwin,
        production::SERVICE,
        production::PATH,
        production::SERVICE,
    )
    .await
    .unwrap()
    .call::<_, _, ()>("ReportHome", &(true, "activity", "screen"))
    .await
    .unwrap();
    assert_eq!(f.get().await.state, "idle");
}

#[tokio::test]
async fn invalidation_releases_pending_and_ready_frames_without_resurrection() {
    let mut f = Fixture::new().await;
    let producer = f.producer(&f.kwin).await;
    let reader = f.reader(&f.shell).await;

    // Lock: the ready frame is dropped; unlock does not bring it back.
    let context = f.pending().await;
    producer
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    f.blocked.send(true).unwrap();
    f.wait_state("unavailable", true).await;
    error(reader.read_frame(&context.request_id).await, "Unavailable");
    f.blocked.send(false).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(f.get().await.state, "unavailable");
    f.hide().await;
    f.wait_state("idle", false).await;

    // A locked session refuses publication.
    let context = f.pending().await;
    f.blocked.send(true).unwrap();
    f.wait_state("unavailable", true).await;
    error(
        producer
            .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
            .await,
        "Conflict",
    );
    f.blocked.send(false).unwrap();
    f.hide().await;
    f.wait_state("idle", false).await;

    // The Return window disappears from the inventory.
    let context = f.pending().await;
    producer
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    f.inventory(&["token-1"]).await;
    f.wait_state("unavailable", true).await;
    f.hide().await;
    f.wait_state("idle", false).await;

    // The shell owner changes: its successor never reads the old frame.
    f.inventory(&["token-2", "token-1"]).await;
    let context = f.pending().await;
    producer
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    f.shell.release_name(SHELL_SERVICE).await.unwrap();
    f.wait_state("unavailable", true).await;
    f.shell.request_name(SHELL_SERVICE).await.unwrap();
    error(reader.read_frame(&context.request_id).await, "Unavailable");
    f.hide().await;
    f.wait_state("idle", false).await;

    // Without a shell owner nobody may read, so publication is refused.
    let context = f.pending().await;
    f.shell.release_name(SHELL_SERVICE).await.unwrap();
    f.wait_state("unavailable", true).await;
    error(
        producer
            .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
            .await,
        "Conflict",
    );
    f.shell.request_name(SHELL_SERVICE).await.unwrap();
    f.hide().await;
    f.wait_state("idle", false).await;

    // The adapter reports another window for the same target (remap).
    let context = f.pending().await;
    producer
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    f.home("activity", "screen").await;
    assert_eq!(f.get().await.state, "ready", "re-reporting keeps the frame");
    f.associate("activity", &f.tokens[1]).await;
    f.wait_state("unavailable", true).await;
    f.associate("activity", &f.tokens[0]).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(f.get().await.state, "unavailable", "never resurrected");
    f.hide().await;
    f.wait_state("idle", false).await;

    // A target change (Home over another Activity) replaces the lifetime.
    let context = f.pending().await;
    f.home("other", "screen").await;
    let replaced = f.get().await;
    assert_eq!(replaced.state, "pending");
    assert!(replaced.context.is_empty());
    error(
        producer
            .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
            .await,
        "Conflict",
    );
    f.hide().await;
    f.wait_state("idle", false).await;

    // The adapter reconnects with a new generation (platform loss).
    let context = f.pending().await;
    producer
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    let generation = KWinBridgeProxy::new(&f.kwin)
        .await
        .unwrap()
        .connect(&Uuid::new_v4().to_string())
        .await
        .unwrap();
    assert_ne!(generation, f.generation);
    f.wait_state("unavailable", true).await;
}

#[tokio::test]
async fn a_quick_return_and_home_is_a_new_lifetime() {
    let f = Fixture::new().await;
    let context = f.pending().await;
    f.producer(&f.kwin)
        .await
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    // Hold the attempt state so nothing reconciles while Home is hidden and
    // shown again with the same target and output.
    let background = f.core.background.get().unwrap().clone();
    let held = background.inner.lock().await;
    zbus::Proxy::new(
        &f.kwin,
        production::SERVICE,
        production::PATH,
        production::SERVICE,
    )
    .await
    .unwrap()
    .call::<_, _, ()>("ReportHome", &(false, "activity", "screen"))
    .await
    .unwrap();
    let kwin = f.kwin.clone();
    let request = tokio::spawn(async move {
        ProductionDaemonProxy::new(&kwin)
            .await
            .unwrap()
            .report_home_request("activity", "screen")
            .await
            .unwrap();
    });
    // Both reports are committed before the held state is released.
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let evidence = f.core.windows.home_evidence(&f.core.platform, None).await;
            // The hide was committed above, so visible means the request is.
            if evidence.visible {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    drop(held);
    request.await.unwrap();
    let snapshot = f.get().await;
    assert_eq!(
        snapshot.state, "pending",
        "a new lifetime, not the old frame"
    );
    assert!(snapshot.context.is_empty());
    error(
        f.reader(&f.shell)
            .await
            .read_frame(&context.request_id)
            .await,
        "Conflict",
    );
}

#[tokio::test]
async fn a_new_shell_owner_never_reads_the_previous_frame() {
    let f = Fixture::new().await;
    let context = f.pending().await;
    f.producer(&f.kwin)
        .await
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    // Keep the reconciler busy (an evidence change that invalidates nothing
    // waits behind the held state), so it cannot process the owner change
    // before the successor reads.
    let background = f.core.background.get().unwrap().clone();
    let held = background.inner.lock().await;
    f.associate("activity", &f.tokens[0]).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    f.shell.release_name(SHELL_SERVICE).await.unwrap();
    f.stranger.request_name(SHELL_SERVICE).await.unwrap();
    let stranger = f.stranger.clone();
    let request = context.request_id.clone();
    let read = tokio::spawn(async move {
        BackgroundProxy::new(&stranger)
            .await
            .unwrap()
            .read_frame(&request)
            .await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(held);
    // The frame is bound to the shell that owned the name at publication.
    error(read.await.unwrap(), "PermissionDenied");
}

#[tokio::test]
async fn a_shell_that_loses_its_name_while_reading_gets_no_frame() {
    let f = Fixture::new().await;
    let context = f.pending().await;
    f.producer(&f.kwin)
        .await
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    // Keep the reconciler busy behind the held state, so the owner change is
    // not processed before the read takes the state.
    let background = f.core.background.get().unwrap().clone();
    let held = background.inner.lock().await;
    f.associate("activity", &f.tokens[0]).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    // The frame's own reader starts reading while it still owns the name.
    let shell = f.shell.clone();
    let request = context.request_id.clone();
    let read = tokio::spawn(async move {
        BackgroundProxy::new(&shell)
            .await
            .unwrap()
            .read_frame(&request)
            .await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    // It loses the name while its read waits for the state.
    f.shell.release_name(SHELL_SERVICE).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(held);
    error(read.await.unwrap(), "PermissionDenied");
}

#[tokio::test]
async fn a_release_and_reacquire_ends_the_previous_owner_lifetime() {
    let f = Fixture::new().await;
    let context = f.pending().await;
    f.producer(&f.kwin)
        .await
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    // Keep the reconciler busy behind the held state, so the queued
    // owner-change invalidation runs only after the read below.
    let background = f.core.background.get().unwrap().clone();
    let held = background.inner.lock().await;
    f.associate("activity", &f.tokens[0]).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    // The reader's connection releases the name and takes it again: the same
    // unique name owns Shell1, but in a new owner lifetime. Every task has
    // had its turn before the read starts, so a counter would already
    // include both changes when the read samples it.
    f.shell.release_name(SHELL_SERVICE).await.unwrap();
    f.shell.request_name(SHELL_SERVICE).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let shell = f.shell.clone();
    let request = context.request_id.clone();
    let read = tokio::spawn(async move {
        BackgroundProxy::new(&shell)
            .await
            .unwrap()
            .read_frame(&request)
            .await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(held);
    error(read.await.unwrap(), "PermissionDenied");
}

#[tokio::test]
async fn a_frame_without_owner_events_is_never_read() {
    let f = Fixture::new().await;
    let context = f.pending().await;
    // The Shell1 owner subscription cannot be opened for this publication.
    let background = f.core.background.get().unwrap().clone();
    background
        .owner_events_unavailable
        .store(true, std::sync::atomic::Ordering::SeqCst);
    f.producer(&f.kwin)
        .await
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    assert_eq!(f.wait_state("ready", true).await.state, "ready");
    // Owner changes cannot be verified: the frame is withheld, even from its
    // own reader that never lost the name.
    let reader = f.reader(&f.shell).await;
    error(
        reader.read_frame(&context.request_id).await,
        "PermissionDenied",
    );
}

#[tokio::test]
async fn a_frame_refused_after_an_owner_change_stays_refused() {
    let f = Fixture::new().await;
    let context = f.pending().await;
    f.producer(&f.kwin)
        .await
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    // Hold the state so the queued owner-change invalidation runs only after
    // both reads.
    let background = f.core.background.get().unwrap().clone();
    let held = background.inner.lock().await;
    f.associate("activity", &f.tokens[0]).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    f.shell.release_name(SHELL_SERVICE).await.unwrap();
    f.shell.request_name(SHELL_SERVICE).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let read = || {
        let shell = f.shell.clone();
        let request = context.request_id.clone();
        tokio::spawn(async move {
            BackgroundProxy::new(&shell)
                .await
                .unwrap()
                .read_frame(&request)
                .await
        })
    };
    // Three reads queue behind the held state, in order. The receiver holds
    // two events (release, reacquire); each refused read takes at most one.
    let mut reads = Vec::new();
    for _ in 0..3 {
        reads.push(read());
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    drop(held);
    // Every read is refused: once an owner change is seen the frame stays
    // refused, also after the receiver has no events left.
    for read in reads {
        error(read.await.unwrap(), "PermissionDenied");
    }
}

#[tokio::test]
async fn a_busy_background_never_delays_the_home_request() {
    let f = Fixture::new().await;
    let mut requests = ProductionDaemonProxy::new(&f.stranger)
        .await
        .unwrap()
        .receive_home_requested()
        .await
        .unwrap();
    // A publication or reconciliation holds the background state.
    let background = f.core.background.get().unwrap().clone();
    let held = background.inner.lock().await;
    let started = Instant::now();
    tokio::time::timeout(Duration::from_millis(500), f.home("activity", "screen"))
        .await
        .expect("the Home request is answered while the state is held");
    tokio::time::timeout(Duration::from_millis(500), requests.next())
        .await
        .expect("HomeRequested is emitted while the state is held")
        .unwrap();
    assert!(started.elapsed() < Duration::from_millis(500));
    // Arming still happens once the state is free, with the deadline
    // counted from the request.
    drop(held);
    f.wait_state("pending", false).await;
}

#[tokio::test]
async fn a_queued_request_never_arms_a_later_home_lifetime() {
    let f = Fixture::new().await;
    // Requests queue behind a held state: the first one, then (after its
    // deadline would have passed) a Return and a second Home.
    let background = f.core.background.get().unwrap().clone();
    let held = background.inner.lock().await;
    f.home("activity", "screen").await;
    tokio::time::sleep(DEADLINE + Duration::from_millis(100)).await;
    zbus::Proxy::new(
        &f.kwin,
        production::SERVICE,
        production::PATH,
        production::SERVICE,
    )
    .await
    .unwrap()
    .call::<_, _, ()>("ReportHome", &(false, "activity", "screen"))
    .await
    .unwrap();
    f.home("activity", "screen").await;
    drop(held);
    // The second lifetime is armed by its own request, with a live deadline,
    // not by the first request's expired one.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let snapshot = f.get().await;
    assert_eq!(snapshot.state, "pending", "{snapshot:?}");
}

#[tokio::test]
async fn a_closed_session_guard_fails_closed() {
    let mut f = Fixture::new().await;
    let context = f.pending().await;
    f.producer(&f.kwin)
        .await
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    // A ready frame has no deadline. The guard's sender goes away with its
    // last value "unblocked": that is unknown, so the frame is dropped.
    let (replacement, _) = watch::channel(false);
    drop(std::mem::replace(&mut f.blocked, replacement));
    f.wait_state("unavailable", true).await;
    error(
        f.reader(&f.shell)
            .await
            .read_frame(&context.request_id)
            .await,
        "Unavailable",
    );
}

#[tokio::test]
async fn the_unrenewed_deadline_ends_a_pending_attempt() {
    let f = Fixture::new().await;
    let started = Instant::now();
    f.home("activity", "screen").await;
    // Waiting for association counts against the same deadline.
    tokio::time::sleep(Duration::from_millis(800)).await;
    f.associate("activity", "token-0").await;
    let context = f.wait_state("pending", true).await.context[0].clone();
    // Repeated Home does not renew it.
    f.home("activity", "screen").await;
    let snapshot = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot = f.get().await;
            if snapshot.state == "unavailable" {
                break snapshot;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let elapsed = started.elapsed();
    assert!(elapsed >= DEADLINE, "{elapsed:?}");
    assert!(
        elapsed < DEADLINE + Duration::from_millis(900),
        "{elapsed:?}"
    );
    assert_eq!(snapshot.context, vec![context.clone()]);
    error(
        f.producer(&f.kwin)
            .await
            .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
            .await,
        "Conflict",
    );
}

#[tokio::test]
async fn no_runtime_or_state_files_are_written() {
    let f = Fixture::new().await;
    let before: Vec<_> = walk(&f.folder);
    let context = f.pending().await;
    f.producer(&f.kwin)
        .await
        .publish(&frame(&context, 2, 2, memfd(&pixels(), SEALED)))
        .await
        .unwrap();
    let _ = f
        .reader(&f.shell)
        .await
        .read_frame(&context.request_id)
        .await
        .unwrap();
    f.hide().await;
    f.wait_state("idle", false).await;
    assert_eq!(walk(&f.folder), before);
}
fn walk(root: &std::path::Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path.clone());
            }
            found.push(path);
        }
    }
    found.sort();
    found
}
