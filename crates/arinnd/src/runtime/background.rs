// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.Background1` and `org.arinn.BackgroundCapture1` (ADR-0087,
//! specification §12.3): the bounded Activity background frame handoff.
//!
//! An explicit Home presentation request arms one capture attempt for its
//! Home context. The attempt waits for the exact Return window, the selected
//! platform adapter publishes one sealed memfd, and only the current shell
//! owner reads it. Every change of Home, Return, inventory, adapter, shell
//! owner or session lock invalidates the attempt; nothing resurrects it.
//! Pixels stay in the descriptor: the daemon never maps, copies, logs or
//! writes them.
use super::Core;
use crate::windows::HomeEvidence;
use arinn_protocol::{
    background::{self, BackgroundContext, BackgroundFrame, BackgroundSnapshot},
    production,
    return_association::WindowRef,
    validation::ErrorCode,
};
use futures_util::StreamExt;
use std::{
    os::fd::{AsFd, AsRawFd, OwnedFd},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, watch};
use zbus::{Connection, MatchRule, MessageStream, message::Header};

pub const INTERFACE: &str = "org.arinn.Background1";
/// The only reader of pixels.
pub const SHELL_SERVICE: &str = "org.arinn.Shell1";
const DEADLINE: Duration = Duration::from_millis(background::CAPTURE_DEADLINE_MS);
const OWNER_TIMEOUT: Duration = Duration::from_secs(1);
/// Queue bound of the `Shell1` owner-change subscription.
const OWNER_EVENTS: usize = 16;

enum Phase {
    Pending,
    Ready {
        pixels: OwnedFd,
        width: u32,
        height: u32,
        /// The Shell1 owner when the frame was installed: the only reader.
        reader: String,
        /// `Shell1` owner changes the daemon has received since publication
        /// began (before the reader was looked up). Any event ends reading;
        /// `None` (no subscription, or an event already seen) reads nothing.
        owner_events: Option<Box<MessageStream>>,
    },
    Unavailable,
}
struct Attempt {
    request_id: String,
    /// The Home lifetime that armed it.
    lifetime: u64,
    target: String,
    output: String,
    window: Option<WindowRef>,
    armed: Instant,
    deadline: Instant,
    phase: Phase,
}
impl Attempt {
    fn context(&self, epoch: &str) -> Option<BackgroundContext> {
        Some(BackgroundContext {
            epoch: epoch.into(),
            request_id: self.request_id.clone(),
            target: self.target.clone(),
            output: self.output.clone(),
            window: self.window.clone()?,
        })
    }
}
struct Inner {
    revision: u64,
    /// The revision cannot grow: the API fails closed until a new epoch.
    exhausted: bool,
    attempt: Option<Attempt>,
}

/// Why an attempt became unavailable, for the normalized diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reason {
    Timeout,
    Session,
    Window,
    Adapter,
    Shell,
    Cancelled,
}
impl Reason {
    fn error(self) -> &'static str {
        match self {
            Reason::Timeout => "Timeout",
            Reason::Cancelled => "Cancelled",
            Reason::Session | Reason::Window | Reason::Adapter | Reason::Shell => "Unavailable",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Reason::Timeout => "deadline",
            Reason::Session => "session",
            Reason::Window => "window",
            Reason::Adapter => "adapter",
            Reason::Shell => "shell",
            Reason::Cancelled => "cancelled",
        }
    }
}
/// One committed change, emitted after the lock is released.
#[derive(Debug, Default)]
struct Outcome {
    revision: Option<u64>,
    log: Option<(
        &'static str,
        Option<&'static str>,
        Option<&'static str>,
        u64,
    )>,
}

#[derive(Clone)]
pub(super) struct Background {
    epoch: String,
    inner: Arc<Mutex<Inner>>,
    connection: Connection,
    windows: crate::windows::Windows,
    platform: crate::platform::Platform,
    /// `true` while the session is locked, asleep or unknown.
    blocked: watch::Receiver<bool>,
}

fn log(operation: &str, outcome: Option<&str>, error: Option<&str>, duration_ms: u64) {
    // Normalized results only: no target, output, token, request ID or pixels.
    eprintln!(
        "{}",
        serde_json::json!({"level":"info","component":"arinnd","provider_id":null,"request_id":null,"operation":operation,"outcome":outcome,"error":error,"duration_ms":duration_ms})
    );
}

impl Background {
    pub(super) fn new(core: &Core, blocked: watch::Receiver<bool>) -> Self {
        Self {
            epoch: core.epoch.clone(),
            inner: Arc::new(Mutex::new(Inner {
                revision: 1,
                exhausted: false,
                attempt: None,
            })),
            connection: core.connection.clone(),
            windows: core.windows.clone(),
            platform: core.platform.clone(),
            blocked,
        }
    }

    /// `NameOwnerChanged` for `org.arinn.Shell1`.
    fn shell_owner_rule() -> Option<MatchRule<'static>> {
        Some(
            MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender("org.freedesktop.DBus")
                .ok()?
                .path("/org/freedesktop/DBus")
                .ok()?
                .interface("org.freedesktop.DBus")
                .ok()?
                .member("NameOwnerChanged")
                .ok()?
                .add_arg(SHELL_SERVICE)
                .ok()?
                .build(),
        )
    }

    /// A new receiver of `Shell1` owner changes. zbus's socket reader puts
    /// every matching signal into it before it handles any later message, so
    /// whatever the daemon has received after this call is in it, whether or
    /// not any task has run since. While `follow_shell_owner` keeps the rule
    /// registered, this costs no bus round trip.
    async fn shell_owner_events(&self) -> Option<MessageStream> {
        MessageStream::for_match_rule(
            Self::shell_owner_rule()?,
            &self.connection,
            Some(OWNER_EVENTS),
        )
        .await
        .ok()
    }

    /// Keeps the `Shell1` owner-change rule registered for the daemon's
    /// lifetime and drains it. Without it, frames are still published, but
    /// reads add the match first; a failed subscription is logged and such a
    /// frame is never read.
    async fn follow_shell_owner(&self) {
        let Some(mut changes) = self.shell_owner_events().await else {
            log("background-follow", Some("unavailable"), None, 0);
            return;
        };
        tokio::spawn(async move {
            while changes.next().await.is_some() {}
            log("background-follow", Some("ended"), None, 0);
        });
    }

    /// The session guard; a guard that stopped reporting is unknown, hence
    /// blocked (fail closed).
    fn blocked(&self) -> bool {
        *self.blocked.borrow() || self.blocked.has_changed().is_err()
    }

    /// Home evidence, probing the attempt's resolved Return window.
    async fn evidence(&self, inner: &Inner) -> HomeEvidence {
        let probe = inner.attempt.as_ref().and_then(|a| a.window.as_ref());
        self.windows.home_evidence(&self.platform, probe).await
    }

    fn bump(inner: &mut Inner) -> Option<u64> {
        match inner.revision.checked_add(1) {
            Some(revision) => {
                inner.revision = revision;
                Some(revision)
            }
            None => {
                inner.exhausted = true;
                inner.attempt = None;
                None
            }
        }
    }

    /// Applies the current evidence to the attempt. Returns why it became
    /// unavailable, `Some(None)` for another change, `None` for no change.
    fn reconcile(
        inner: &mut Inner,
        evidence: &HomeEvidence,
        blocked: bool,
        now: Instant,
    ) -> Option<Option<Reason>> {
        let attempt = inner.attempt.as_mut()?;
        // Return, Home hide (even one no reader saw), target or output change
        // end the Home lifetime.
        if !evidence.visible
            || evidence.lifetime != attempt.lifetime
            || evidence.target != attempt.target
            || evidence.output != attempt.output
        {
            inner.attempt = None;
            return Some(None);
        }
        if matches!(attempt.phase, Phase::Unavailable) {
            return None;
        }
        let reason = if blocked {
            Some(Reason::Session)
        } else if !evidence.live {
            Some(Reason::Adapter)
        } else if attempt.window.as_ref().is_some_and(|window| {
            // Removed, remapped into a new generation, or another window
            // reported for the target. An empty association alone is the
            // adapter re-reporting after a repeated Home.
            !evidence.present || evidence.window.as_ref().is_some_and(|w| w != window)
        }) {
            Some(Reason::Window)
        } else if matches!(attempt.phase, Phase::Pending) && now >= attempt.deadline {
            Some(Reason::Timeout)
        } else {
            None
        };
        if let Some(reason) = reason {
            // Dropping the phase closes a retained descriptor.
            attempt.phase = Phase::Unavailable;
            return Some(Some(reason));
        }
        if attempt.window.is_none()
            && let Some(window) = &evidence.window
        {
            // KWinReturn1/the EWMH adapter validated the reference already.
            attempt.window = Some(window.clone());
            return Some(None);
        }
        None
    }

    fn commit(inner: &mut Inner, change: Option<Option<Reason>>, now: Instant) -> Outcome {
        let Some(reason) = change else {
            return Outcome::default();
        };
        let elapsed = inner
            .attempt
            .as_ref()
            .map(|a| now.saturating_duration_since(a.armed).as_millis() as u64)
            .unwrap_or(0);
        let log = match reason {
            Some(reason) => Some((
                "background-unavailable",
                Some(reason.name()),
                Some(reason.error()),
                elapsed,
            )),
            None if inner.attempt.is_none() => Some(("background-released", None, None, 0)),
            None => None,
        };
        Outcome {
            revision: Self::bump(inner),
            log,
        }
    }

    async fn emit(&self, outcome: Outcome) {
        if let Some((operation, name, error, duration)) = outcome.log {
            log(operation, name, error, duration);
        }
        if let Some(revision) = outcome.revision {
            let _ = self
                .connection
                .emit_signal(
                    None::<&str>,
                    production::PATH,
                    INTERFACE,
                    "Changed",
                    &(&self.epoch, revision),
                )
                .await;
        }
    }

    /// Reconciles with the current evidence and lock state.
    pub(super) async fn refresh(&self) {
        let mut inner = self.inner.lock().await;
        let evidence = self.evidence(&inner).await;
        let now = Instant::now();
        let change = Self::reconcile(&mut inner, &evidence, self.blocked(), now);
        let outcome = Self::commit(&mut inner, change, now);
        drop(inner);
        self.emit(outcome).await;
    }

    /// An explicit Home presentation request for `target` on `output` was
    /// committed. Arms one attempt for this Home context; repeated Home on
    /// the same context never recaptures, even after a failure.
    pub(super) async fn arm(&self, target: &str, output: &str, requested: Instant, lifetime: u64) {
        let inner = self.inner.lock().await;
        self.arm_locked(inner, target, output, requested, lifetime)
            .await;
    }

    /// Arms with the state already locked. `requested` is when the Home
    /// request was committed: the deadline and the reported latency count
    /// from it, not from when the state became free.
    async fn arm_locked(
        &self,
        mut inner: tokio::sync::MutexGuard<'_, Inner>,
        target: &str,
        output: &str,
        requested: Instant,
        lifetime: u64,
    ) {
        if inner.exhausted {
            return;
        }
        let evidence = self.evidence(&inner).await;
        let now = Instant::now();
        let blocked = self.blocked();
        // A stale continuation or a lifetime that already ended is reconciled
        // first, so the check below sees only a live context.
        let change = Self::reconcile(&mut inner, &evidence, blocked, now);
        let mut outcome = Self::commit(&mut inner, change, now);
        // Home without an external Activity keeps the normal background. An
        // adapter without an output, or a request another Home report already
        // superseded, arms nothing. ReportHomeRequest validated both texts.
        // A queued request belongs to the Home lifetime it was made in; a
        // later lifetime is armed by its own request, with its own deadline.
        let eligible = !target.is_empty()
            && !output.is_empty()
            && evidence.lifetime == lifetime
            && evidence.visible
            && evidence.target == target
            && evidence.output == output;
        let repeated = inner.attempt.as_ref().is_some_and(|a| {
            a.lifetime == evidence.lifetime && a.target == target && a.output == output
        });
        if eligible && !repeated {
            inner.attempt = Some(Attempt {
                request_id: uuid::Uuid::new_v4().to_string(),
                lifetime: evidence.lifetime,
                target: target.into(),
                output: output.into(),
                window: None,
                armed: requested,
                deadline: requested + DEADLINE,
                phase: Phase::Pending,
            });
            let change = Self::reconcile(&mut inner, &evidence, blocked, now);
            let revision = Self::bump(&mut inner);
            let unavailable = Self::commit(&mut inner, change, now);
            outcome = Outcome {
                revision: unavailable.revision.or(revision),
                log: unavailable
                    .log
                    .or(Some(("background-armed", None, None, 0))),
            };
        }
        drop(inner);
        self.emit(outcome).await;
    }

    async fn snapshot(&self) -> Result<BackgroundSnapshot, ErrorCode> {
        let mut inner = self.inner.lock().await;
        let evidence = self.evidence(&inner).await;
        let now = Instant::now();
        let change = Self::reconcile(&mut inner, &evidence, self.blocked(), now);
        let outcome = Self::commit(&mut inner, change, now);
        let snapshot = if inner.exhausted {
            Err(ErrorCode::Unavailable)
        } else {
            let (state, context) = match &inner.attempt {
                None => ("idle", None),
                Some(a) => (
                    match a.phase {
                        Phase::Pending => "pending",
                        Phase::Ready { .. } => "ready",
                        Phase::Unavailable => "unavailable",
                    },
                    a.context(&self.epoch),
                ),
            };
            let snapshot = BackgroundSnapshot {
                epoch: self.epoch.clone(),
                revision: inner.revision,
                context: context.into_iter().collect(),
                state: state.into(),
            };
            snapshot
                .validate()
                .map(|_| snapshot)
                .map_err(|_| ErrorCode::Internal)
        };
        drop(inner);
        self.emit(outcome).await;
        snapshot
    }

    /// The current unique owner of `service`, or `None`.
    async fn owner_of(&self, service: &str) -> Result<Option<String>, ErrorCode> {
        let bus = zbus::fdo::DBusProxy::new(&self.connection)
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let name = zbus::names::BusName::try_from(service).map_err(|_| ErrorCode::Internal)?;
        match tokio::time::timeout(OWNER_TIMEOUT, bus.get_name_owner(name)).await {
            Ok(Ok(owner)) => Ok(Some(owner.to_string())),
            Ok(Err(zbus::fdo::Error::NameHasNoOwner(_))) => Ok(None),
            _ => Err(ErrorCode::Unavailable),
        }
    }

    /// The current unique owner of the selected platform service, if any.
    async fn platform_owner(&self) -> Option<String> {
        let service = crate::platform::adapter_service()?;
        self.owner_of(service).await.ok().flatten()
    }

    /// The caller must be the current owner of the selected platform service
    /// and the adapter connection that owns the inventory.
    async fn authorize_platform(&self, header: &Header<'_>) -> Result<String, ErrorCode> {
        let sender = header
            .sender()
            .map(ToString::to_string)
            .ok_or(ErrorCode::PermissionDenied)?;
        crate::platform::Platform::authorize(&self.connection, header)
            .await
            .map_err(|_| ErrorCode::PermissionDenied)?;
        Ok(sender)
    }

    async fn publish(&self, frame: BackgroundFrame, header: &Header<'_>) -> Result<(), ErrorCode> {
        let sender = self.authorize_platform(header).await?;
        frame.validate_metadata()?;
        let bytes = background::pixel_bytes(frame.width, frame.height)?;
        let pixels: OwnedFd = frame.pixels.into();
        sealed(&pixels, bytes)?;
        // Subscribed before the reader is looked up: every later owner change
        // of Shell1 ends reading of this frame.
        let owner_events = self.shell_owner_events().await;
        if owner_events.is_none() {
            log("background-follow", Some("unavailable"), None, 0);
        }
        // A shell must own the reader name; otherwise nobody may read.
        let shell = self.owner_of(SHELL_SERVICE).await?;
        // The selected platform owner is read here, before the state lock, so
        // no bus round trip separates the evidence check from the commit. An
        // owner change after this read resets the inventory owner, which the
        // commit checks atomically.
        let platform_owner = self.platform_owner().await;
        let mut inner = self.inner.lock().await;
        let evidence = self.evidence(&inner).await;
        let now = Instant::now();
        let change = Self::reconcile(&mut inner, &evidence, self.blocked(), now);
        let mut outcome = Self::commit(&mut inner, change, now);
        let result = if evidence.owner != sender
            || platform_owner.as_deref() != Some(sender.as_str())
        {
            Err(ErrorCode::PermissionDenied)
        } else if frame.context.epoch != self.epoch || inner.exhausted {
            Err(ErrorCode::Conflict)
        } else {
            match inner.attempt.as_mut() {
                Some(attempt)
                    if matches!(attempt.phase, Phase::Pending)
                        && attempt.context(&self.epoch).as_ref() == Some(&frame.context) =>
                {
                    if shell.is_none() {
                        attempt.phase = Phase::Unavailable;
                        let unavailable = Self::commit(&mut inner, Some(Some(Reason::Shell)), now);
                        outcome.revision = unavailable.revision.or(outcome.revision);
                        outcome.log = unavailable.log;
                        Err(ErrorCode::Conflict)
                    } else {
                        let elapsed = now.saturating_duration_since(attempt.armed).as_millis();
                        attempt.phase = Phase::Ready {
                            pixels,
                            width: frame.width,
                            height: frame.height,
                            reader: shell.clone().unwrap_or_default(),
                            owner_events: owner_events.map(Box::new),
                        };
                        outcome.revision = Self::bump(&mut inner).or(outcome.revision);
                        outcome.log = Some(("background-ready", None, None, elapsed as u64));
                        Ok(())
                    }
                }
                // Late, superseded, expired, already completed or mismatching:
                // the descriptor is closed when `pixels` drops.
                _ => Err(ErrorCode::Conflict),
            }
        };
        drop(inner);
        self.emit(outcome).await;
        result
    }

    async fn cancel(&self, request_id: &str, header: &Header<'_>) -> Result<(), ErrorCode> {
        let sender = self.authorize_platform(header).await?;
        arinn_protocol::platform::generation(request_id)?;
        // As for Publish: the selected platform owner is read before the
        // state lock and checked with the inventory owner at commit.
        let platform_owner = self.platform_owner().await;
        let mut inner = self.inner.lock().await;
        let evidence = self.evidence(&inner).await;
        if evidence.owner != sender || platform_owner.as_deref() != Some(sender.as_str()) {
            return Err(ErrorCode::PermissionDenied);
        }
        let now = Instant::now();
        let change = Self::reconcile(&mut inner, &evidence, self.blocked(), now);
        let mut outcome = Self::commit(&mut inner, change, now);
        let result = match inner.attempt.as_mut() {
            Some(attempt) if attempt.request_id == request_id => {
                if !matches!(attempt.phase, Phase::Unavailable) {
                    attempt.phase = Phase::Unavailable;
                    let cancelled = Self::commit(&mut inner, Some(Some(Reason::Cancelled)), now);
                    outcome.revision = cancelled.revision.or(outcome.revision);
                    outcome.log = cancelled.log;
                }
                Ok(())
            }
            // A stale cancellation cannot clear a replacement.
            _ => Err(ErrorCode::Conflict),
        };
        drop(inner);
        self.emit(outcome).await;
        result
    }

    async fn read_frame(
        &self,
        request_id: &str,
        header: &Header<'_>,
    ) -> Result<BackgroundFrame, ErrorCode> {
        let sender = header.sender().map(|s| s.to_string());
        let shell = self
            .owner_of(SHELL_SERVICE)
            .await
            .map_err(|_| ErrorCode::PermissionDenied)?;
        if shell.is_none() || sender != shell {
            return Err(ErrorCode::PermissionDenied);
        }
        arinn_protocol::platform::generation(request_id)?;
        let mut inner = self.inner.lock().await;
        let evidence = self.evidence(&inner).await;
        let now = Instant::now();
        let change = Self::reconcile(&mut inner, &evidence, self.blocked(), now);
        let outcome = Self::commit(&mut inner, change, now);
        let result = match inner.attempt.as_mut() {
            Some(attempt) if attempt.request_id == request_id => {
                let context = attempt.context(&self.epoch);
                match &mut attempt.phase {
                    Phase::Ready {
                        pixels,
                        width,
                        height,
                        reader,
                        owner_events,
                    } => {
                        if Some(&*reader) != sender.as_ref() {
                            // Bound to the shell that owned the reader name at
                            // publication: a successor never reads it, even
                            // before the owner change is processed.
                            Err(ErrorCode::PermissionDenied)
                        } else if owner_changed(owner_events) {
                            // Checked immediately before the descriptor is
                            // duplicated: after any owner change of Shell1 the
                            // daemon has received since publication, even one
                            // whose invalidation is still queued, or a release
                            // and reacquire by the same connection, the
                            // former owner lifetime gets no frame.
                            Err(ErrorCode::PermissionDenied)
                        } else {
                            match (context, pixels.try_clone()) {
                                // try_clone duplicates with close-on-exec.
                                (Some(context), Ok(copy)) => Ok(BackgroundFrame {
                                    context,
                                    width: *width,
                                    height: *height,
                                    pixels: copy.into(),
                                }),
                                _ => Err(ErrorCode::Unavailable),
                            }
                        }
                    }
                    _ => Err(ErrorCode::Unavailable),
                }
            }
            Some(_) => Err(ErrorCode::Conflict),
            None => Err(ErrorCode::Unavailable),
        };
        drop(inner);
        self.emit(outcome).await;
        result
    }

    /// Invalidates a pending or ready attempt for a reason outside the Home
    /// evidence (a shell owner change).
    async fn invalidate(&self, reason: Reason) {
        let mut inner = self.inner.lock().await;
        let now = Instant::now();
        let change = match inner.attempt.as_mut() {
            Some(attempt) if !matches!(attempt.phase, Phase::Unavailable) => {
                attempt.phase = Phase::Unavailable;
                Some(Some(reason))
            }
            _ => None,
        };
        let outcome = Self::commit(&mut inner, change, now);
        drop(inner);
        self.emit(outcome).await;
    }

    /// The next pending deadline, if any.
    async fn deadline(&self) -> Option<Instant> {
        let inner = self.inner.lock().await;
        inner
            .attempt
            .as_ref()
            .filter(|a| matches!(a.phase, Phase::Pending))
            .map(|a| a.deadline)
    }

    /// Follows Home evidence, the session guard, the shell owner and the
    /// deadline until the connection closes.
    async fn run(self) {
        let mut evidence = self.windows.subscribe_evidence();
        let mut blocked = self.blocked.clone();
        let mut guard_open = true;
        let shells = match zbus::fdo::DBusProxy::new(&self.connection).await {
            Ok(bus) => bus
                .receive_name_owner_changed_with_args(&[(0, SHELL_SERVICE)])
                .await
                .ok(),
            Err(_) => None,
        };
        let Some(mut shells) = shells else {
            return;
        };
        loop {
            let deadline = self.deadline().await;
            let wait = async {
                match deadline {
                    Some(at) => tokio::time::sleep_until(at.into()).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                changed = evidence.changed() => {
                    if changed.is_err() { return; }
                    self.refresh().await;
                }
                changed = blocked.changed(), if guard_open => {
                    // A closed guard is blocked from now on; keep following
                    // everything else instead of stopping silently.
                    guard_open = changed.is_ok();
                    self.refresh().await;
                }
                change = shells.next() => {
                    if change.is_none() { return; }
                    self.invalidate(Reason::Shell).await;
                }
                _ = wait => self.refresh().await,
            }
        }
    }
}

/// Whether `Shell1` changed owner since the stream was opened. Polls without
/// waiting. A seen event, an ended stream or no stream at all counts as a
/// change, and stays one.
fn owner_changed(events: &mut Option<Box<MessageStream>>) -> bool {
    use futures_util::FutureExt as _;
    let changed = match events.as_mut() {
        None => true,
        Some(stream) => stream.next().now_or_never().is_some(),
    };
    if changed {
        *events = None;
    }
    changed
}

/// The descriptor is a regular sealed memfd of exactly `bytes` bytes whose
/// write, grow and shrink seals forbid later mutation. `F_SEAL_WRITE` cannot
/// be set while a writable shared mapping exists. Pipes, sockets, devices and
/// ordinary files have no seals and are rejected; the daemon never reads the
/// pixels.
fn sealed(fd: &OwnedFd, bytes: u64) -> Result<(), ErrorCode> {
    let raw = fd.as_fd().as_raw_fd();
    let mut info = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: `raw` is a live descriptor owned by `fd`; fstat fills `info`.
    if unsafe { libc::fstat(raw, info.as_mut_ptr()) } != 0 {
        return Err(ErrorCode::InvalidArgument);
    }
    // SAFETY: fstat succeeded and initialized the structure.
    let info = unsafe { info.assume_init() };
    if info.st_mode & libc::S_IFMT != libc::S_IFREG || u64::try_from(info.st_size) != Ok(bytes) {
        return Err(ErrorCode::InvalidArgument);
    }
    // SAFETY: F_GET_SEALS only reads descriptor state.
    let seals = unsafe { libc::fcntl(raw, libc::F_GET_SEALS) };
    let required = libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK;
    if seals < 0 || seals & required != required {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(())
}

pub(super) struct BackgroundExport(pub(super) Background);
#[zbus::interface(name = "org.arinn.Background1")]
impl BackgroundExport {
    async fn get(&self) -> production::Result<(BackgroundSnapshot,)> {
        Ok((self.0.snapshot().await?,))
    }
    /// Only the current `org.arinn.Shell1` owner receives pixels.
    async fn read_frame(
        &self,
        request_id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(BackgroundFrame,)> {
        Ok((self.0.read_frame(request_id, &header).await?,))
    }
    #[zbus(signal)]
    async fn changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        epoch: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}

pub(super) struct CaptureExport(pub(super) Background);
#[zbus::interface(name = "org.arinn.BackgroundCapture1")]
impl CaptureExport {
    async fn publish(
        &self,
        frame: BackgroundFrame,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        Ok(self.0.publish(frame, &header).await?)
    }
    async fn cancel(
        &self,
        request_id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        Ok(self.0.cancel(request_id, &header).await?)
    }
}

impl Core {
    /// Export `Background1` and `BackgroundCapture1` at the daemon root.
    /// `blocked` is the daemon-side session guard (true while locked, asleep
    /// or unknown).
    pub async fn export_background(&self, blocked: watch::Receiver<bool>) -> Result<(), ErrorCode> {
        let background = Background::new(self, blocked);
        // The owner-change rule is registered before the export, so reading
        // a frame's owner events never needs a bus round trip.
        background.follow_shell_owner().await;
        let server = self.connection.object_server();
        server
            .at(production::PATH, BackgroundExport(background.clone()))
            .await
            .map_err(|_| ErrorCode::Internal)?;
        server
            .at(production::PATH, CaptureExport(background.clone()))
            .await
            .map_err(|_| ErrorCode::Internal)?;
        let _ = self.background.set(background.clone());
        tokio::spawn(background.run());
        Ok(())
    }

    /// Called after a committed explicit Home presentation request. Arms
    /// inline when the state is free; otherwise (a publication or
    /// reconciliation holds it) in the background, so the Home request is
    /// never delayed by capture work (§6.5).
    pub(super) async fn background_home_requested(&self, target: &str, output: &str) {
        let Some(background) = self.background.get() else {
            return;
        };
        let requested = Instant::now();
        // The Home lifetime this request committed in.
        let lifetime = self
            .windows
            .home_evidence(&self.platform, None)
            .await
            .lifetime;
        match background.inner.try_lock() {
            Ok(inner) => {
                background
                    .arm_locked(inner, target, output, requested, lifetime)
                    .await
            }
            Err(_) => {
                let background = background.clone();
                let (target, output) = (target.to_owned(), output.to_owned());
                tokio::spawn(
                    async move { background.arm(&target, &output, requested, lifetime).await },
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
