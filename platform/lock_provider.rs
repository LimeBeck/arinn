// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! ScreenSaver endpoint list and probing shared by the daemon guard and the X11
//! adapter (`provider_present` / `probe` only; XFCE preflight is `lock_preflight.rs`).
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
}
