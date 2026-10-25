// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! logind's `handle-power-key` inhibitor while Arinn owns the power key
//! (spec §5.7 *Power button*, ADR-0111).
//!
//! logind reads the power key from the input device itself and acts on it
//! (`HandlePowerKey=`, power-off by default) unless a process holds a
//! `handle-power-key` block inhibitor. The platform adapter that owns the key
//! takes one, so a press only opens Arinn's power menu. logind keeps the
//! inhibitor exactly as long as its file descriptor is open: dropping
//! [`Inhibitor`] releases it, and so does the end of the process, a crash
//! included.
use std::time::Duration;
use zbus::Connection;

const SERVICE: &str = "org.freedesktop.login1";
const PATH: &str = "/org/freedesktop/login1";
const INTERFACE: &str = "org.freedesktop.login1.Manager";
/// The lock this module takes; logind lists it with `ListInhibitors`.
pub const WHAT: &str = "handle-power-key";
pub const WHO: &str = "Arinn";
pub const WHY: &str = "The power key opens Arinn's power menu";
const MODE: &str = "block";
/// A missing or stalled logind never holds up the adapter.
const TIMEOUT: Duration = Duration::from_secs(2);

/// A held inhibitor. Dropping it releases the lock.
#[derive(Debug)]
pub struct Inhibitor(#[allow(dead_code)] zbus::zvariant::OwnedFd);

/// Takes the inhibitor on `system`, the system bus. `None` when logind is
/// missing, refuses (polkit) or does not answer in time.
pub async fn inhibit(system: &Connection) -> Option<Inhibitor> {
    let reply = tokio::time::timeout(
        TIMEOUT,
        system.call_method(
            Some(SERVICE),
            PATH,
            Some(INTERFACE),
            "Inhibit",
            &(WHAT, WHO, WHY, MODE),
        ),
    )
    .await
    .ok()?
    .ok()?;
    reply.body().deserialize().ok().map(Inhibitor)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Read},
        os::unix::net::UnixStream,
        process::{Child, Command, Stdio},
        sync::{Arc, Mutex},
    };

    pub(crate) struct Bus(Child, pub(crate) String);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    /// A private bus that stands in for the system bus.
    pub(crate) fn bus() -> Bus {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Bus(child, address.trim().to_owned())
    }
    pub(crate) async fn connect(address: &str) -> Connection {
        zbus::connection::Builder::address(address)
            .unwrap()
            .build()
            .await
            .unwrap()
    }

    /// The far end of each inhibitor fd handed out, and the arguments of
    /// each `Inhibit` call.
    #[derive(Default)]
    pub(crate) struct Fake {
        pub(crate) calls: Vec<(String, String, String, String)>,
        pub(crate) held: Vec<UnixStream>,
        pub(crate) refuse: bool,
        /// `ListSessions`: id, uid, user, seat, object path.
        pub(crate) sessions: Vec<Listed>,
        /// `ListSessions` fails.
        pub(crate) no_sessions: bool,
    }
    impl Fake {
        /// How many handed-out inhibitors are still open.
        pub(crate) fn open(&mut self) -> usize {
            self.held
                .iter_mut()
                .map(|end| {
                    end.set_nonblocking(true).unwrap();
                    // EOF: every copy of the client's end is closed.
                    !matches!(end.read(&mut [0; 1]), Ok(0))
                })
                .filter(|open| *open)
                .count()
        }
    }
    pub(crate) type Shared = Arc<Mutex<Fake>>;
    /// One `ListSessions` entry.
    pub(crate) type Listed = (String, u32, String, String, zbus::zvariant::OwnedObjectPath);
    struct Manager(Shared);
    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl Manager {
        fn list_sessions(&self) -> zbus::fdo::Result<Vec<Listed>> {
            let fake = self.0.lock().unwrap();
            if fake.no_sessions {
                return Err(zbus::fdo::Error::Failed("fixture".into()));
            }
            Ok(fake.sessions.clone())
        }
        fn inhibit(
            &self,
            what: String,
            who: String,
            why: String,
            mode: String,
        ) -> zbus::fdo::Result<zbus::zvariant::OwnedFd> {
            let mut fake = self.0.lock().unwrap();
            fake.calls.push((what, who, why, mode));
            if fake.refuse {
                return Err(zbus::fdo::Error::AccessDenied("fixture".into()));
            }
            let (ours, theirs) = UnixStream::pair().unwrap();
            fake.held.push(ours);
            Ok(std::os::fd::OwnedFd::from(theirs).into())
        }
    }
    /// A fake logind on `bus`; the returned connection owns its name.
    pub(crate) async fn logind(bus: &Bus, fake: Shared) -> Connection {
        let service = connect(&bus.1).await;
        serve(&service, fake).await;
        service
    }
    /// Serves a fake logind on `service`, which takes logind's name.
    pub(crate) async fn serve(service: &Connection, fake: Shared) {
        service
            .object_server()
            .at(PATH, Manager(fake))
            .await
            .unwrap();
        service.request_name(SERVICE).await.unwrap();
    }

    #[tokio::test]
    async fn holds_the_power_key_lock_until_dropped() {
        let bus = bus();
        let fake = Shared::default();
        let _logind = logind(&bus, fake.clone()).await;
        let system = connect(&bus.1).await;
        let inhibitor = inhibit(&system).await.expect("inhibitor");
        assert_eq!(
            fake.lock().unwrap().calls,
            [(WHAT.into(), WHO.into(), WHY.into(), "block".into())]
        );
        assert_eq!(fake.lock().unwrap().open(), 1);
        drop(inhibitor);
        // logind sees the fd close; the lock is gone.
        for _ in 0..50 {
            if fake.lock().unwrap().open() == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the inhibitor stayed open");
    }

    #[tokio::test]
    async fn a_refusal_or_missing_logind_is_no_inhibitor() {
        let bus = bus();
        let system = connect(&bus.1).await;
        assert!(inhibit(&system).await.is_none());
        let fake = Shared::default();
        fake.lock().unwrap().refuse = true;
        let _logind = logind(&bus, fake.clone()).await;
        assert!(inhibit(&system).await.is_none());
        assert_eq!(fake.lock().unwrap().calls.len(), 1);
    }
}
