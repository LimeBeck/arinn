// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! ScreenSaver endpoint list and probing shared by the daemon guard, the X11
//! adapter, and the XFCE session helper.
#[path = "lock_endpoints.rs"]
mod lock_endpoints;
pub use lock_endpoints::LOCK_ENDPOINTS;
use std::time::Duration;
use zbus::{Connection, proxy::Proxy};

const CALL_TIMEOUT: Duration = Duration::from_millis(1500);

/// The endpoint that answered, pinned to its unique owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub index: usize,
    pub owner: String,
}

/// The first answering endpoint and its lock state; `(None, true)` when no
/// endpoint answers. A call that fails other than with a missing name,
/// object, interface or method cannot be bypassed with a weaker answer.
pub async fn probe(session: &Connection) -> (Option<Endpoint>, bool) {
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

/// True when one of [`LOCK_ENDPOINTS`] has an owner that answers `GetActive`.
pub async fn provider_present(session: &Connection) -> bool {
    probe(session).await.0.is_some()
}

/// How long the XFCE session helper waits for a ScreenSaver autostart entry
/// (`xfce4-screensaver`, `light-locker`) to claim a [`LOCK_ENDPOINTS`] name.
pub const PROVIDER_WAIT: Duration = Duration::from_secs(20);

/// Reads [`PROVIDER_WAIT`], overridable in tests via `ARINN_LOCK_PROVIDER_WAIT_SECS`.
pub fn provider_wait_from_env() -> Duration {
    match std::env::var("ARINN_LOCK_PROVIDER_WAIT_SECS") {
        Ok(value) if !value.is_empty() => value
            .parse::<u64>()
            .ok()
            .filter(|&seconds| seconds > 0)
            .map(Duration::from_secs)
            .unwrap_or(PROVIDER_WAIT),
        _ => PROVIDER_WAIT,
    }
}

/// Waits up to [`provider_wait_from_env()`], re-probing on ScreenSaver
/// `NameOwnerChanged`, until [`provider_present`] succeeds.
pub async fn wait_for_provider(session: &Connection) -> bool {
    wait_for_provider_for(session, provider_wait_from_env()).await
}

/// Same as [`wait_for_provider`] with an explicit bound (used in unit tests).
pub async fn wait_for_provider_for(session: &Connection, wait: Duration) -> bool {
    if provider_present(session).await {
        return true;
    }
    if wait.is_zero() {
        return false;
    }
    let Ok(bus) = zbus::fdo::DBusProxy::new(session).await else {
        return false;
    };
    let deadline = tokio::time::Instant::now() + wait;
    let Ok(freedesktop) = bus
        .receive_name_owner_changed_with_args(&[(0, LOCK_ENDPOINTS[0].0)])
        .await
    else {
        return provider_present(session).await;
    };
    let Ok(xfce) = bus
        .receive_name_owner_changed_with_args(&[(0, LOCK_ENDPOINTS[2].0)])
        .await
    else {
        return provider_present(session).await;
    };
    use futures_util::StreamExt;
    let mut owners = futures_util::stream::select(freedesktop.map(|_| ()), xfce.map(|_| ()));
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return provider_present(session).await;
        }
        tokio::select! {
            change = owners.next() => {
                if change.is_none() {
                    return provider_present(session).await;
                }
                if provider_present(session).await {
                    return true;
                }
            }
            _ = tokio::time::sleep(remaining) => {
                return provider_present(session).await;
            }
        }
    }
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
    #[tokio::test]
    async fn provider_present_requires_a_working_get_active() {
        let bus = Bus::new();
        let session = bus.connect().await;
        assert!(!provider_present(&session).await);
        let screensaver = bus.connect().await;
        #[derive(Clone, Default)]
        struct Xfce(Arc<AtomicBool>);
        #[zbus::interface(name = "org.xfce.ScreenSaver")]
        impl Xfce {
            fn get_active(&self) -> bool {
                self.0.load(Ordering::SeqCst)
            }
        }
        screensaver
            .object_server()
            .at(LOCK_ENDPOINTS[2].1, Xfce::default())
            .await
            .unwrap();
        screensaver.request_name(LOCK_ENDPOINTS[2].0).await.unwrap();
        assert!(provider_present(&session).await);
    }

    #[tokio::test]
    async fn wait_for_provider_accepts_a_late_screensaver() {
        let bus = Bus::new();
        let session = bus.connect().await;
        let waiting = tokio::spawn({
            let session = session.clone();
            async move { wait_for_provider_for(&session, Duration::from_secs(5)).await }
        });
        tokio::time::sleep(Duration::from_millis(250)).await;
        let screensaver = bus.connect().await;
        #[derive(Clone, Default)]
        struct Xfce(Arc<AtomicBool>);
        #[zbus::interface(name = "org.xfce.ScreenSaver")]
        impl Xfce {
            fn get_active(&self) -> bool {
                self.0.load(Ordering::SeqCst)
            }
        }
        screensaver
            .object_server()
            .at(LOCK_ENDPOINTS[2].1, Xfce::default())
            .await
            .unwrap();
        screensaver.request_name(LOCK_ENDPOINTS[2].0).await.unwrap();
        assert!(waiting.await.unwrap());
    }

    #[tokio::test]
    async fn wait_for_provider_times_out_without_a_provider() {
        let bus = Bus::new();
        let session = bus.connect().await;
        let started = tokio::time::Instant::now();
        assert!(!wait_for_provider_for(&session, Duration::from_millis(400)).await);
        assert!(started.elapsed() >= Duration::from_millis(400));
    }
}
