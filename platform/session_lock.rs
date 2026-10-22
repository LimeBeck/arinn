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
    let Some(rule) = signal_rule("ActiveChanged") else {
        return;
    };
    // Subscribe before probing so a transition during the probe is not lost.
    let Ok(mut lock_signals) = MessageStream::for_match_rule(rule, &session, Some(16)).await else {
        return;
    };
    let Ok(bus) = zbus::fdo::DBusProxy::new(&session).await else {
        return;
    };
    let Ok(freedesktop) = bus
        .receive_name_owner_changed_with_args(&[(0, LOCK_ENDPOINTS[0].0)])
        .await
    else {
        return;
    };
    let Ok(xfce) = bus
        .receive_name_owner_changed_with_args(&[(0, LOCK_ENDPOINTS[2].0)])
        .await
    else {
        return;
    };
    let mut owners = futures_util::stream::select(freedesktop.map(|_| ()), xfce.map(|_| ()));
    let mut sleep_signals = match (&system, signal_rule("PrepareForSleep")) {
        (Some(system), Some(rule)) => MessageStream::for_match_rule(rule, system, Some(16))
            .await
            .ok(),
        _ => None,
    };
    let mut sleeping = false;
    loop {
        let (endpoint, locked) = probe(&session).await;
        let mut locked = locked;
        if sender.send(locked || sleeping).is_err() {
            return;
        }
        let reprobe = tokio::time::sleep(REPROBE);
        tokio::pin!(reprobe);
        loop {
            tokio::select! {
                message = lock_signals.next() => {
                    let Some(Ok(message)) = message else { return; };
                    if let (Some(endpoint), Some(active)) = (&endpoint, active_changed(&message)) {
                        let (_, path, interface) = LOCK_ENDPOINTS[endpoint.index];
                        let header = message.header();
                        if header.sender().map(|s| s.as_str()) == Some(endpoint.owner.as_str())
                            && header.path().map(|p| p.as_str()) == Some(path)
                            && header.interface().map(|i| i.as_str()) == Some(interface)
                        {
                            locked = active;
                            let _ = sender.send(locked || sleeping);
                        }
                    }
                }
                change = owners.next() => {
                    if change.is_none() { return; }
                    // A new or vanished screensaver owner is unknown until probed.
                    let _ = sender.send(true);
                    break;
                }
                message = next_sleep(&mut sleep_signals) => {
                    let Some(entering) = message else {
                        sleep_signals = None;
                        continue;
                    };
                    sleeping = entering;
                    let _ = sender.send(true);
                    if !entering {
                        break;
                    }
                }
                _ = &mut reprobe, if endpoint.is_none() => break,
            }
        }
    }
}

async fn next_sleep(stream: &mut Option<MessageStream>) -> Option<bool> {
    let Some(stream) = stream else {
        return std::future::pending().await;
    };
    loop {
        let message = stream.next().await?.ok()?;
        let header = message.header();
        if header.interface().map(|i| i.as_str()) == Some("org.freedesktop.login1.Manager")
            && header.path().map(|p| p.as_str()) == Some("/org/freedesktop/login1")
            && let Ok(entering) = message.body().deserialize::<bool>()
        {
            return Some(entering);
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
        let Ok(Ok(owner)) = tokio::time::timeout(CALL_TIMEOUT, bus.get_name_owner(name)).await
        else {
            continue;
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
        #[zbus(signal)]
        async fn prepare_for_sleep(
            emitter: &zbus::object_server::SignalEmitter<'_>,
            start: bool,
        ) -> zbus::Result<()>;
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
        let sleep =
            zbus::object_server::SignalEmitter::new(&login, "/org/freedesktop/login1").unwrap();
        Sleep::prepare_for_sleep(&sleep, true).await.unwrap();
        wait(&mut guard, true).await;
        Lock::active_changed(&emitter, false).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(*guard.borrow());
        Sleep::prepare_for_sleep(&sleep, false).await.unwrap();
        wait(&mut guard, false).await;
        // Losing the screensaver owner is unknown again.
        screensaver
            .release_name("org.freedesktop.ScreenSaver")
            .await
            .unwrap();
        wait(&mut guard, true).await;
    }
}
