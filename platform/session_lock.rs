// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Session lock and sleep state for daemon-side guards (specification §12.3,
//! §13). It follows the shell's `DbusSessionLifecycle`: the first freedesktop
//! or XFCE screensaver endpoint that answers `GetActive` decides the lock
//! state, `PrepareForSleep` on the system bus marks sleep, and anything
//! unknown — no endpoint, a failed or timed-out call, an owner change — is
//! blocked. Every platform backend uses these freedesktop services.
//!
//! The daemon uses it to refuse and invalidate Activity background frames
//! (ADR-0087). The shell keeps its own guard and never waits for this one.
use futures_util::StreamExt;
use std::time::Duration;
use tokio::sync::watch;
use zbus::{Connection, MatchRule, MessageStream, message::Message, proxy::Proxy};

/// Probe order, identical to the shell's guard.
pub const LOCK_ENDPOINTS: [(&str, &str, &str); 3] = [
    (
        "org.freedesktop.ScreenSaver",
        "/org/freedesktop/ScreenSaver",
        "org.freedesktop.ScreenSaver",
    ),
    (
        "org.freedesktop.ScreenSaver",
        "/ScreenSaver",
        "org.freedesktop.ScreenSaver",
    ),
    (
        "org.xfce.ScreenSaver",
        "/org/xfce/ScreenSaver",
        "org.xfce.ScreenSaver",
    ),
];
const CALL_TIMEOUT: Duration = Duration::from_millis(1500);
/// Delay before a failed or ended subscription is renewed; blocked meanwhile.
const RESUBSCRIBE: Duration = Duration::from_secs(1);
const LOGIND: &str = "org.freedesktop.login1";
/// Without a lock endpoint the state stays blocked and is probed again.
const REPROBE: Duration = Duration::from_secs(5);

/// The endpoint that answered, pinned to its unique owner.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Endpoint {
    index: usize,
    owner: String,
}

/// Starts the watcher. The receiver reads `true` (blocked) until the first
/// probe proves the session unlocked. Without a system bus, sleep is only
/// seen through the lock that KDE and XFCE take before suspending.
pub fn watch(session: Connection, system: Option<Connection>) -> watch::Receiver<bool> {
    let (sender, receiver) = watch::channel(true);
    tokio::spawn(run(session, system, sender));
    receiver
}

fn signal_rule(member: &'static str) -> Option<MatchRule<'static>> {
    Some(
        MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .member(member)
            .ok()?
            .build(),
    )
}

async fn run(session: Connection, system: Option<Connection>, sender: watch::Sender<bool>) {
    loop {
        // Fail closed while (re)subscribing: a lagged, failed or closed
        // subscription never leaves an old unlocked value in place.
        if sender.send(true).is_err()
            || watch_once(&session, system.as_ref(), &sender)
                .await
                .is_err()
        {
            return;
        }
        tokio::time::sleep(RESUBSCRIBE).await;
    }
}

/// One subscription lifetime. `Ok` when a stream ended or failed (the caller
/// blocks and subscribes again); `Err` when every receiver is gone.
async fn watch_once(
    session: &Connection,
    system: Option<&Connection>,
    sender: &watch::Sender<bool>,
) -> Result<(), ()> {
    let publish = |blocked: bool| sender.send(blocked).map_err(|_| ());
    let Some(rule) = signal_rule("ActiveChanged") else {
        return Ok(());
    };
    // Subscribe before probing so a transition during the probe is not lost.
    let Ok(mut lock_signals) = MessageStream::for_match_rule(rule, session, Some(16)).await else {
        return Ok(());
    };
    let Ok(bus) = zbus::fdo::DBusProxy::new(session).await else {
        return Ok(());
    };
    let Ok(freedesktop) = bus
        .receive_name_owner_changed_with_args(&[(0, LOCK_ENDPOINTS[0].0)])
        .await
    else {
        return Ok(());
    };
    let Ok(xfce) = bus
        .receive_name_owner_changed_with_args(&[(0, LOCK_ENDPOINTS[2].0)])
        .await
    else {
        return Ok(());
    };
    let mut owners = futures_util::stream::select(freedesktop.map(|_| ()), xfce.map(|_| ()));
    let mut sleep_signals = match system {
        Some(system) => {
            let Some(rule) = signal_rule("PrepareForSleep") else {
                return Ok(());
            };
            let Ok(stream) = MessageStream::for_match_rule(rule, system, Some(16)).await else {
                return Ok(());
            };
            Some(stream)
        }
        None => None,
    };
    // Subscribed first, so a transition after this read is seen as a signal.
    let mut sleeping = match system {
        Some(system) => preparing_for_sleep(system).await,
        None => false,
    };
    loop {
        // Any lifecycle event during the probe makes its reply stale: stay
        // blocked and probe again, as the shell guard's epoch does.
        let mut stale = false;
        let (endpoint, mut locked) = {
            let probing = probe(session);
            tokio::pin!(probing);
            loop {
                tokio::select! {
                    result = &mut probing => break result,
                    message = lock_signals.next() => {
                        let Some(Ok(_)) = message else { return Ok(()); };
                        stale = true;
                    }
                    change = owners.next() => {
                        if change.is_none() { return Ok(()); }
                        stale = true;
                    }
                    entering = next_sleep(&mut sleep_signals, system) => {
                        let Some(entering) = entering else { return Ok(()); };
                        sleeping = entering;
                        stale = true;
                    }
                }
            }
        };
        if stale {
            publish(true)?;
            continue;
        }
        publish(locked || sleeping)?;
        let reprobe = tokio::time::sleep(REPROBE);
        tokio::pin!(reprobe);
        loop {
            tokio::select! {
                message = lock_signals.next() => {
                    let Some(Ok(message)) = message else { return Ok(()); };
                    if let (Some(endpoint), Some(active)) = (&endpoint, active_changed(&message)) {
                        let (_, path, interface) = LOCK_ENDPOINTS[endpoint.index];
                        let header = message.header();
                        if header.sender().map(|s| s.as_str()) == Some(endpoint.owner.as_str())
                            && header.path().map(|p| p.as_str()) == Some(path)
                            && header.interface().map(|i| i.as_str()) == Some(interface)
                        {
                            locked = active;
                            publish(locked || sleeping)?;
                        }
                    }
                }
                change = owners.next() => {
                    if change.is_none() { return Ok(()); }
                    // A new or vanished screensaver owner is unknown until probed.
                    publish(true)?;
                    break;
                }
                entering = next_sleep(&mut sleep_signals, system) => {
                    let Some(entering) = entering else { return Ok(()); };
                    sleeping = entering;
                    publish(true)?;
                    if !entering {
                        break;
                    }
                }
                _ = &mut reprobe, if endpoint.is_none() => break,
            }
        }
    }
}

/// logind's `PreparingForSleep`. No logind on the bus means no sleep source;
/// any other failure is unknown, hence sleeping.
async fn preparing_for_sleep(system: &Connection) -> bool {
    let read = async {
        let proxy = zbus::fdo::PropertiesProxy::builder(system)
            .destination(LOGIND)?
            .path("/org/freedesktop/login1")?
            .build()
            .await?;
        proxy
            .get(
                zbus::names::InterfaceName::try_from("org.freedesktop.login1.Manager")?,
                "PreparingForSleep",
            )
            .await
            .map_err(zbus::Error::from)
    };
    match tokio::time::timeout(CALL_TIMEOUT, read).await {
        Ok(Ok(value)) => bool::try_from(value).unwrap_or(true),
        Ok(Err(zbus::Error::MethodError(name, _, _)))
            if [
                "org.freedesktop.DBus.Error.ServiceUnknown",
                "org.freedesktop.DBus.Error.NameHasNoOwner",
            ]
            .contains(&name.as_str()) =>
        {
            false
        }
        Ok(Err(zbus::Error::FDO(error)))
            if matches!(
                *error,
                zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_)
            ) =>
        {
            false
        }
        _ => true,
    }
}

/// The next `PrepareForSleep`, `None` when the stream ended or failed. A
/// sleep start only blocks, so it is accepted even when its sender cannot be
/// verified as logind (an unknown or a different owner). A resume unblocks,
/// so it is accepted only from the current `org.freedesktop.login1` owner.
async fn next_sleep(
    stream: &mut Option<MessageStream>,
    system: Option<&Connection>,
) -> Option<bool> {
    let (Some(stream), Some(system)) = (stream, system) else {
        return std::future::pending().await;
    };
    loop {
        let message = stream.next().await?.ok()?;
        let header = message.header();
        if header.interface().map(|i| i.as_str()) != Some("org.freedesktop.login1.Manager")
            || header.path().map(|p| p.as_str()) != Some("/org/freedesktop/login1")
        {
            continue;
        }
        let Ok(entering) = message.body().deserialize::<bool>() else {
            continue;
        };
        if entering {
            return Some(true);
        }
        let owner = async {
            zbus::fdo::DBusProxy::new(system)
                .await
                .ok()?
                .get_name_owner(zbus::names::BusName::try_from(LOGIND).ok()?)
                .await
                .ok()
        };
        let owner = tokio::time::timeout(CALL_TIMEOUT, owner)
            .await
            .ok()
            .flatten();
        // An unverified resume is ignored: it could unblock during sleep.
        if owner.is_some_and(|owner| header.sender().map(|s| s.as_str()) == Some(owner.as_str())) {
            return Some(false);
        }
    }
}

fn active_changed(message: &Message) -> Option<bool> {
    message.body().deserialize::<bool>().ok()
}

/// The first answering endpoint and its lock state; `(None, true)` when no
/// endpoint answers. A call that fails other than with a missing name,
/// object, interface or method cannot be bypassed with a weaker answer.
async fn probe(session: &Connection) -> (Option<Endpoint>, bool) {
    let Ok(bus) = zbus::fdo::DBusProxy::new(session).await else {
        return (None, true);
    };
    for (index, (service, path, interface)) in LOCK_ENDPOINTS.iter().enumerate() {
        let Ok(name) = zbus::names::BusName::try_from(*service) else {
            continue;
        };
        // Pin calls to an existing owner: never auto-start a screensaver.
        // Only a missing name falls through to a weaker endpoint; a timeout
        // or another bus error is an unknown state, hence locked.
        let owner = match tokio::time::timeout(CALL_TIMEOUT, bus.get_name_owner(name)).await {
            Ok(Ok(owner)) => owner,
            Ok(Err(zbus::fdo::Error::NameHasNoOwner(_) | zbus::fdo::Error::ServiceUnknown(_))) => {
                continue;
            }
            _ => return (None, true),
        };
        let Ok(proxy) = Proxy::new(session, owner.to_string(), *path, *interface).await else {
            return (None, true);
        };
        match tokio::time::timeout(CALL_TIMEOUT, proxy.call::<_, _, bool>("GetActive", &())).await {
            Ok(Ok(active)) => {
                return (
                    Some(Endpoint {
                        index,
                        owner: owner.to_string(),
                    }),
                    active,
                );
            }
            Ok(Err(zbus::Error::MethodError(name, _, _)))
                if [
                    "org.freedesktop.DBus.Error.UnknownObject",
                    "org.freedesktop.DBus.Error.UnknownMethod",
                    "org.freedesktop.DBus.Error.UnknownInterface",
                    "org.freedesktop.DBus.Error.ServiceUnknown",
                    "org.freedesktop.DBus.Error.NameHasNoOwner",
                ]
                .contains(&name.as_str()) => {}
            _ => return (None, true),
        }
    }
    (None, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };
    struct Bus(Child, String);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    impl Bus {
        fn new() -> Self {
            let mut child = Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut address = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut address)
                .unwrap();
            Self(child, address.trim().into())
        }
        async fn connect(&self) -> Connection {
            zbus::connection::Builder::address(self.1.as_str())
                .unwrap()
                .build()
                .await
                .unwrap()
        }
    }
    #[derive(Clone, Default)]
    struct Lock(Arc<AtomicBool>);
    #[zbus::interface(name = "org.freedesktop.ScreenSaver")]
    impl Lock {
        fn get_active(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
        #[zbus(signal)]
        async fn active_changed(
            emitter: &zbus::object_server::SignalEmitter<'_>,
            active: bool,
        ) -> zbus::Result<()>;
    }
    struct Sleep;
    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl Sleep {
        #[zbus(property)]
        fn preparing_for_sleep(&self) -> bool {
            false
        }
        #[zbus(signal)]
        async fn prepare_for_sleep(
            emitter: &zbus::object_server::SignalEmitter<'_>,
            start: bool,
        ) -> zbus::Result<()>;
    }
    /// A screensaver whose GetActive reply reflects the state when the call
    /// arrived, delivered only after a delay: a stale reply.
    #[derive(Clone, Default)]
    struct SlowLock(Arc<AtomicBool>);
    #[zbus::interface(name = "org.freedesktop.ScreenSaver")]
    impl SlowLock {
        async fn get_active(&self) -> bool {
            let active = self.0.load(Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(400)).await;
            active
        }
        #[zbus(signal)]
        async fn active_changed(
            emitter: &zbus::object_server::SignalEmitter<'_>,
            active: bool,
        ) -> zbus::Result<()>;
    }
    #[tokio::test]
    async fn a_probe_overtaken_by_a_lock_never_unblocks() {
        let bus = Bus::new();
        let session = bus.connect().await;
        let screensaver = bus.connect().await;
        let lock = SlowLock::default();
        screensaver
            .object_server()
            .at("/ScreenSaver", lock.clone())
            .await
            .unwrap();
        screensaver
            .request_name("org.freedesktop.ScreenSaver")
            .await
            .unwrap();
        let guard = watch(session.clone(), None);
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let record = {
            let seen = seen.clone();
            let mut guard = guard.clone();
            tokio::spawn(async move {
                while guard.changed().await.is_ok() {
                    seen.lock().unwrap().push(*guard.borrow());
                }
            })
        };
        // The session locks while the probe still waits for its stale reply.
        // The service announces it on its other endpoint path, which the
        // probe has not selected, so only the probe fence notices it.
        tokio::time::sleep(Duration::from_millis(150)).await;
        lock.0.store(true, Ordering::SeqCst);
        let emitter =
            zbus::object_server::SignalEmitter::new(&screensaver, "/org/freedesktop/ScreenSaver")
                .unwrap();
        SlowLock::active_changed(&emitter, true).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert!(
            !seen.lock().unwrap().contains(&false),
            "the stale probe reply must not unblock: {:?}",
            seen.lock().unwrap()
        );
        assert!(*guard.borrow());
        record.abort();
    }
    async fn wait(receiver: &mut watch::Receiver<bool>, blocked: bool) {
        tokio::time::timeout(
            Duration::from_secs(5),
            receiver.wait_for(|value| *value == blocked),
        )
        .await
        .expect("guard state")
        .unwrap();
    }

    #[tokio::test]
    async fn unknown_lock_and_sleep_states_stay_blocked() {
        let bus = Bus::new();
        let session = bus.connect().await;
        let screensaver = bus.connect().await;
        let login = bus.connect().await;
        // No screensaver: unknown is blocked.
        let mut guard = watch(session.clone(), Some(session.clone()));
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(*guard.borrow());
        let lock = Lock::default();
        screensaver
            .object_server()
            .at("/ScreenSaver", lock.clone())
            .await
            .unwrap();
        screensaver
            .request_name("org.freedesktop.ScreenSaver")
            .await
            .unwrap();
        // The owner change re-probes: the KDE endpoint answers unlocked.
        wait(&mut guard, false).await;
        let emitter =
            zbus::object_server::SignalEmitter::new(&screensaver, "/ScreenSaver").unwrap();
        lock.0.store(true, Ordering::SeqCst);
        Lock::active_changed(&emitter, true).await.unwrap();
        wait(&mut guard, true).await;
        lock.0.store(false, Ordering::SeqCst);
        Lock::active_changed(&emitter, false).await.unwrap();
        wait(&mut guard, false).await;
        // A signal from another connection is not the lock owner's.
        let stranger = bus.connect().await;
        stranger
            .object_server()
            .at("/ScreenSaver", Lock::default())
            .await
            .unwrap();
        let fake = zbus::object_server::SignalEmitter::new(&stranger, "/ScreenSaver").unwrap();
        Lock::active_changed(&fake, true).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!*guard.borrow());
        // Sleep blocks until resume, even when the lock reports unlocked.
        login
            .object_server()
            .at("/org/freedesktop/login1", Sleep)
            .await
            .unwrap();
        login.request_name(LOGIND).await.unwrap();
        let sleep =
            zbus::object_server::SignalEmitter::new(&login, "/org/freedesktop/login1").unwrap();
        Sleep::prepare_for_sleep(&sleep, true).await.unwrap();
        wait(&mut guard, true).await;
        Lock::active_changed(&emitter, false).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(*guard.borrow());
        // Only logind ends sleep: a forged resume from another peer is ignored.
        stranger
            .object_server()
            .at("/org/freedesktop/login1", Sleep)
            .await
            .unwrap();
        let forged =
            zbus::object_server::SignalEmitter::new(&stranger, "/org/freedesktop/login1").unwrap();
        Sleep::prepare_for_sleep(&forged, false).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(*guard.borrow(), "forged resume");
        Sleep::prepare_for_sleep(&sleep, false).await.unwrap();
        wait(&mut guard, false).await;
        // Losing the screensaver owner is unknown again.
        screensaver
            .release_name("org.freedesktop.ScreenSaver")
            .await
            .unwrap();
        wait(&mut guard, true).await;
    }

    #[tokio::test]
    async fn an_unverified_sleep_start_blocks_and_an_unverified_resume_does_not_unblock() {
        let bus = Bus::new();
        let session = bus.connect().await;
        let screensaver = bus.connect().await;
        let login = bus.connect().await;
        let stranger = bus.connect().await;
        screensaver
            .object_server()
            .at("/ScreenSaver", Lock::default())
            .await
            .unwrap();
        screensaver
            .request_name("org.freedesktop.ScreenSaver")
            .await
            .unwrap();
        login
            .object_server()
            .at("/org/freedesktop/login1", Sleep)
            .await
            .unwrap();
        login.request_name(LOGIND).await.unwrap();
        let mut guard = watch(session.clone(), Some(session.clone()));
        wait(&mut guard, false).await;
        // A sleep start whose sender is not the logind owner (logind handed
        // its name over, or anyone else) cannot be verified. It only blocks,
        // so it is accepted.
        stranger
            .object_server()
            .at("/org/freedesktop/login1", Sleep)
            .await
            .unwrap();
        let unverified =
            zbus::object_server::SignalEmitter::new(&stranger, "/org/freedesktop/login1").unwrap();
        Sleep::prepare_for_sleep(&unverified, true).await.unwrap();
        wait(&mut guard, true).await;
        // An unverified resume would unblock, so it is ignored.
        Sleep::prepare_for_sleep(&unverified, false).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(*guard.borrow(), "unverified resume");
        // Only logind's own resume unblocks.
        let logind =
            zbus::object_server::SignalEmitter::new(&login, "/org/freedesktop/login1").unwrap();
        Sleep::prepare_for_sleep(&logind, false).await.unwrap();
        wait(&mut guard, false).await;
    }
}
