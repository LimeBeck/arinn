// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! XFCE session ScreenSaver preflight: bounded wait and operator diagnostics.
#[path = "lock_endpoints.rs"]
mod lock_endpoints;
use lock_endpoints::LOCK_ENDPOINTS;
use std::time::Duration;
use zbus::{Connection, proxy::Proxy};

const CALL_TIMEOUT: Duration = Duration::from_millis(1500);

/// Result of the XFCE session helper's ScreenSaver preflight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderWaitOutcome {
    Ready,
    Missing,
    Unresponsive { service: &'static str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderCheck {
    Present,
    Absent,
    Unresponsive { service: &'static str },
}

async fn check_provider(session: &Connection) -> ProviderCheck {
    let Ok(bus) = zbus::fdo::DBusProxy::new(session).await else {
        return ProviderCheck::Absent;
    };
    for (service, path, interface) in LOCK_ENDPOINTS.iter() {
        let Ok(name) = zbus::names::BusName::try_from(*service) else {
            continue;
        };
        let owner = match tokio::time::timeout(CALL_TIMEOUT, bus.get_name_owner(name)).await {
            Ok(Ok(owner)) => owner,
            Ok(Err(zbus::fdo::Error::NameHasNoOwner(_) | zbus::fdo::Error::ServiceUnknown(_))) => {
                continue;
            }
            _ => return ProviderCheck::Absent,
        };
        let Ok(proxy) = Proxy::new(session, owner.to_string(), *path, *interface).await else {
            return ProviderCheck::Unresponsive { service };
        };
        match tokio::time::timeout(CALL_TIMEOUT, proxy.call::<_, _, bool>("GetActive", &())).await {
            Ok(Ok(_active)) => return ProviderCheck::Present,
            Ok(Err(zbus::Error::MethodError(name, _, _)))
                if [
                    "org.freedesktop.DBus.Error.UnknownObject",
                    "org.freedesktop.DBus.Error.UnknownMethod",
                    "org.freedesktop.DBus.Error.UnknownInterface",
                    "org.freedesktop.DBus.Error.ServiceUnknown",
                    "org.freedesktop.DBus.Error.NameHasNoOwner",
                ]
                .contains(&name.as_str()) => {}
            _ => return ProviderCheck::Unresponsive { service },
        }
    }
    ProviderCheck::Absent
}

fn outcome_from_check(check: ProviderCheck) -> ProviderWaitOutcome {
    match check {
        ProviderCheck::Present => ProviderWaitOutcome::Ready,
        ProviderCheck::Absent => ProviderWaitOutcome::Missing,
        ProviderCheck::Unresponsive { service } => ProviderWaitOutcome::Unresponsive { service },
    }
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
/// `NameOwnerChanged`, until a provider answers `GetActive`.
pub async fn wait_for_provider(session: &Connection) -> ProviderWaitOutcome {
    wait_for_provider_for(session, provider_wait_from_env()).await
}

/// Same as [`wait_for_provider`] with an explicit bound (used in unit tests).
pub async fn wait_for_provider_for(session: &Connection, wait: Duration) -> ProviderWaitOutcome {
    let Ok(bus) = zbus::fdo::DBusProxy::new(session).await else {
        return ProviderWaitOutcome::Missing;
    };
    let Ok(freedesktop) = bus
        .receive_name_owner_changed_with_args(&[(0, LOCK_ENDPOINTS[0].0)])
        .await
    else {
        return outcome_from_check(check_provider(session).await);
    };
    let Ok(xfce) = bus
        .receive_name_owner_changed_with_args(&[(0, LOCK_ENDPOINTS[2].0)])
        .await
    else {
        return outcome_from_check(check_provider(session).await);
    };
    use futures_util::StreamExt;
    let mut owners = futures_util::stream::select(freedesktop.map(|_| ()), xfce.map(|_| ()));
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        match check_provider(session).await {
            ProviderCheck::Present => return ProviderWaitOutcome::Ready,
            ProviderCheck::Unresponsive { service } => {
                return ProviderWaitOutcome::Unresponsive { service };
            }
            ProviderCheck::Absent => {}
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return ProviderWaitOutcome::Missing;
        }
        tokio::select! {
            change = owners.next() => {
                if change.is_none() {
                    return outcome_from_check(check_provider(session).await);
                }
            }
            _ = tokio::time::sleep(remaining) => {
                return ProviderWaitOutcome::Missing;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lock_endpoints::LOCK_ENDPOINTS;
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
        assert_eq!(waiting.await.unwrap(), ProviderWaitOutcome::Ready);
    }

    #[tokio::test]
    async fn wait_for_provider_times_out_without_a_provider() {
        let bus = Bus::new();
        let session = bus.connect().await;
        let started = tokio::time::Instant::now();
        assert_eq!(
            wait_for_provider_for(&session, Duration::from_millis(400)).await,
            ProviderWaitOutcome::Missing
        );
        assert!(started.elapsed() >= Duration::from_millis(400));
    }

    #[derive(Clone, Default)]
    struct SlowLock(Arc<AtomicBool>);
    #[zbus::interface(name = "org.freedesktop.ScreenSaver")]
    impl SlowLock {
        async fn get_active(&self) -> bool {
            tokio::time::sleep(Duration::from_secs(5)).await;
            self.0.load(Ordering::SeqCst)
        }
    }

    #[tokio::test]
    async fn an_owned_name_that_never_answers_get_active_is_unresponsive() {
        let bus = Bus::new();
        let session = bus.connect().await;
        let screensaver = bus.connect().await;
        screensaver
            .object_server()
            .at("/ScreenSaver", SlowLock::default())
            .await
            .unwrap();
        screensaver
            .request_name("org.freedesktop.ScreenSaver")
            .await
            .unwrap();
        let started = tokio::time::Instant::now();
        assert_eq!(
            wait_for_provider_for(&session, Duration::from_secs(10)).await,
            ProviderWaitOutcome::Unresponsive {
                service: "org.freedesktop.ScreenSaver"
            }
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
