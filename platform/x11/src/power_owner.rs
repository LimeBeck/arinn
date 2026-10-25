// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Who owns the power key on X11/EWMH (spec §5.7 *Power button*, ADR-0111).
//!
//! Arinn holds the key only while something can show the menu and the key is
//! really read: the shell owns `org.arinn.Shell1`, and the grab or the raw
//! reading works. The order keeps the desktop and Arinn from both acting:
//!
//! 1. logind's `handle-power-key` inhibitor first; without it nothing is
//!    taken.
//! 2. Then the reader: a grab, or in the dedicated Arinn XFCE session raw
//!    events for a key xfce4-power-manager grabs.
//! 3. Only once raw reading works, xfce4-power-manager's power button is set
//!    to "do nothing" (`xfce_power`).
//!
//! When the shell goes, the reader fails or the adapter's session ends,
//! everything goes back at once in the reverse order. On an ordinary XFCE
//! desktop the key is never taken: the owner decision of 2026-10-24 leaves
//! it the desktop's dialog there.
use crate::{native::Reading, power_inhibitor, xfce_power};
use anyhow::Result;
use std::time::{Duration, Instant};
use zbus::Connection;

/// Reads the power key; the adapter's native layer, or a test double.
pub trait Reader {
    /// Reads the power key (`enabled`), with raw events for one another
    /// client grabs (`raw`). Returns how it is read now.
    fn set(&mut self, enabled: bool, raw: bool) -> Result<Reading>;
}

/// Whether this session may take the power key: everywhere but on an
/// ordinary XFCE desktop, where xfce4-power-manager keeps it; there only the
/// dedicated Arinn XFCE session (`ARINN_SESSION_ENTRY=xfce`) takes it.
pub fn allowed(current_desktop: Option<&str>, entry: Option<&str>) -> bool {
    let xfce = current_desktop.is_some_and(|desktops| {
        desktops
            .split(':')
            .any(|desktop| desktop.eq_ignore_ascii_case("XFCE"))
    });
    !xfce || xfce_power::dedicated(entry)
}

pub struct PowerOwner {
    /// This session may take the key at all.
    allowed: bool,
    /// The dedicated Arinn XFCE session: raw reading and xfconf.
    dedicated: bool,
    /// The session bus and files for xfconf; none without them.
    xfconf: Option<(Connection, xfce_power::Files)>,
    /// The system bus for logind; opened on first use.
    system: Option<Connection>,
    inhibitor: Option<power_inhibitor::Inhibitor>,
    /// xfce4-power-manager's power button is Arinn's "do nothing".
    taken_xfconf: bool,
    /// How Arinn reads the key; `None` while it does not hold it.
    owned: Reading,
    /// A failed attempt waits until then.
    retry: Option<Instant>,
    retry_after: Duration,
}
impl PowerOwner {
    pub fn new(
        allowed: bool,
        dedicated: bool,
        xfconf: Option<(Connection, xfce_power::Files)>,
        system: Option<Connection>,
    ) -> Self {
        Self {
            allowed,
            dedicated,
            xfconf,
            system,
            inhibitor: None,
            taken_xfconf: false,
            owned: Reading::None,
            retry: None,
            retry_after: Duration::from_secs(30),
        }
    }
    /// From the environment of the adapter.
    pub async fn from_env() -> Self {
        let entry = std::env::var("ARINN_SESSION_ENTRY").ok();
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
        let files = xfce_power::Files::from_env(
            std::env::var_os("HOME").as_deref(),
            std::env::var_os("XDG_STATE_HOME").as_deref(),
            std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        );
        let bus = Connection::session().await.ok();
        // A copy an earlier dedicated run left goes back first, in any
        // X11/EWMH session.
        if let (Some(bus), Some(files)) = (bus.as_ref(), files.as_ref()) {
            xfce_power::release(bus, files).await;
        }
        Self::new(
            allowed(desktop.as_deref(), entry.as_deref()),
            xfce_power::dedicated(entry.as_deref()),
            bus.zip(files),
            None,
        )
    }
    /// Whether Arinn holds the key, and how it reads it.
    pub fn owned(&self) -> Reading {
        self.owned
    }
    /// Whether logind's inhibitor is held.
    #[cfg(test)]
    pub fn inhibiting(&self) -> bool {
        self.inhibitor.is_some()
    }
    /// One step: `shell` is alive, and `reading` is how the snapshot found
    /// the key read.
    pub async fn follow<R: Reader>(
        &mut self,
        reader: &mut R,
        shell: bool,
        reading: Reading,
    ) -> Result<()> {
        let wanted = self.allowed && shell;
        if self.owned != Reading::None {
            // The shell is gone, or the reader stopped (a regrab after a
            // mapping change lost the key): everything goes back.
            if !wanted || reading == Reading::None {
                self.give_back(reader).await?;
            }
            return Ok(());
        }
        if !wanted || self.retry.is_some_and(|at| Instant::now() < at) {
            return Ok(());
        }
        self.retry = Some(Instant::now() + self.retry_after);
        if self.inhibitor.is_none() {
            if self.system.is_none() {
                self.system = Connection::system().await.ok();
            }
            if let Some(system) = self.system.as_ref() {
                self.inhibitor = power_inhibitor::inhibit(system).await;
            }
        }
        if self.inhibitor.is_none() {
            eprintln!(
                "arinn-platform-x11: logind power-key inhibitor unavailable; the key stays the desktop's"
            );
            return Ok(());
        }
        let raw = self.dedicated && self.xfconf.is_some();
        match reader.set(true, raw)? {
            Reading::Grab => self.owned = Reading::Grab,
            Reading::Raw => {
                let (bus, files) = self.xfconf.as_ref().expect("raw needs xfconf");
                if xfce_power::take(bus, files).await {
                    self.taken_xfconf = true;
                    self.owned = Reading::Raw;
                } else {
                    // xfce4-power-manager still acts on the key: leave it.
                    reader.set(false, false)?;
                    self.inhibitor = None;
                }
            }
            Reading::None => {
                // Held by another client and not readable (no XInput 2.1):
                // the key stays the desktop's.
                reader.set(false, false)?;
                self.inhibitor = None;
            }
        }
        if self.owned != Reading::None {
            self.retry = None;
        }
        Ok(())
    }
    /// Gives everything back: the reader first, then the desktop's setting,
    /// then the inhibitor.
    pub async fn give_back<R: Reader>(&mut self, reader: &mut R) -> Result<()> {
        let stopped = reader.set(false, false);
        self.end().await;
        stopped.map(drop)
    }
    /// The reader is gone (the adapter's session ended): the desktop's
    /// setting and the inhibitor go back.
    pub async fn end(&mut self) {
        if std::mem::take(&mut self.taken_xfconf)
            && let Some((bus, files)) = self.xfconf.as_ref()
        {
            xfce_power::release(bus, files).await;
        }
        self.inhibitor = None;
        self.owned = Reading::None;
        self.retry = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{power_inhibitor::tests as logind, xfce_power::tests as xfconf};
    use std::sync::Mutex;
    use zbus::zvariant::Value;

    /// A reader that answers `Grab`, `Raw` or `None` when enabled.
    struct FakeReader {
        answer: Reading,
        calls: Vec<(bool, bool)>,
    }
    impl Reader for FakeReader {
        fn set(&mut self, enabled: bool, raw: bool) -> Result<Reading> {
            self.calls.push((enabled, raw));
            Ok(if enabled { self.answer } else { Reading::None })
        }
    }
    fn reader(answer: Reading) -> FakeReader {
        FakeReader {
            answer,
            calls: Vec::new(),
        }
    }

    struct Fixture {
        xfconf: xfconf::Fixture,
        login: logind::Shared,
        _logind: Connection,
        owner: PowerOwner,
    }
    async fn fixture(allowed: bool, dedicated: bool) -> Fixture {
        let xfconf = xfconf::fixture(Some(Value::U32(3))).await;
        let login = logind::Shared::new(Mutex::new(logind::Fake::default()));
        let service = logind::connect(&xfconf.bus.1).await;
        logind::serve(&service, login.clone()).await;
        let mut owner = PowerOwner::new(
            allowed,
            dedicated,
            Some((xfconf.adapter.clone(), xfconf.files.clone())),
            Some(logind::connect(&xfconf.bus.1).await),
        );
        owner.retry_after = Duration::ZERO;
        Fixture {
            xfconf,
            login,
            _logind: service,
            owner,
        }
    }
    impl Fixture {
        fn open(&self) -> usize {
            self.login.lock().unwrap().open()
        }
        async fn settle(&self, open: usize) {
            for _ in 0..100 {
                if self.open() == open {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            panic!("inhibitors open: {} instead of {open}", self.open());
        }
    }

    #[test]
    fn the_ordinary_xfce_desktop_keeps_its_power_key() {
        // Review of #517: on XFCE only the dedicated session takes it.
        assert!(!allowed(Some("XFCE"), None));
        assert!(!allowed(Some("ubuntu:xfce"), Some("plasma")));
        assert!(allowed(Some("XFCE"), Some("xfce")));
        assert!(allowed(Some("LXQt"), None));
        assert!(allowed(None, None));
    }

    /// Dedicated XFCE session: the inhibitor, raw reading, then "do
    /// nothing"; when the shell exits (Close Arinn, no restart) everything
    /// goes back.
    #[tokio::test]
    async fn a_shell_exit_gives_everything_back() {
        let mut f = fixture(true, true).await;
        let mut r = reader(Reading::Raw);
        f.owner.follow(&mut r, true, Reading::None).await.unwrap();
        assert_eq!(f.owner.owned(), Reading::Raw);
        assert_eq!(r.calls, [(true, true)]);
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(0)));
        assert_eq!(f.open(), 1);
        // A steady state changes nothing.
        f.owner.follow(&mut r, true, Reading::Raw).await.unwrap();
        assert_eq!(r.calls.len(), 1);
        f.owner.follow(&mut r, false, Reading::Raw).await.unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        assert_eq!(r.calls.last(), Some(&(false, false)));
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
        assert!(!f.xfconf.files.record.exists());
        f.settle(0).await;
    }

    /// A shell crash loop: each loss gives everything back, each return
    /// takes it again, and the end state is the user's.
    #[tokio::test]
    async fn a_crash_loop_ends_with_the_desktops_key() {
        let mut f = fixture(true, true).await;
        let mut r = reader(Reading::Raw);
        for _ in 0..5 {
            f.owner.follow(&mut r, true, Reading::None).await.unwrap();
            assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(0)));
            f.owner.follow(&mut r, false, Reading::Raw).await.unwrap();
            assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
        }
        assert!(!f.owner.inhibiting());
        assert!(!f.xfconf.files.record.exists());
        f.settle(0).await;
    }

    /// The reader never becomes ready (the key held and not readable, for
    /// example without XInput 2.1): xfconf is never touched and the
    /// inhibitor is dropped.
    #[tokio::test]
    async fn a_reader_that_never_works_changes_nothing() {
        let mut f = fixture(true, true).await;
        let mut r = reader(Reading::None);
        f.owner.follow(&mut r, true, Reading::None).await.unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        assert_eq!(r.calls, [(true, true), (false, false)]);
        assert_eq!(f.xfconf.writes(), 0);
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
        f.settle(0).await;
    }

    /// A reader that stops later (a regrab lost the key) gives everything
    /// back, and the key is taken again when it reads once more.
    #[tokio::test]
    async fn a_reader_that_stops_gives_everything_back() {
        let mut f = fixture(true, true).await;
        let mut r = reader(Reading::Raw);
        f.owner.follow(&mut r, true, Reading::None).await.unwrap();
        f.owner.follow(&mut r, true, Reading::None).await.unwrap();
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
        assert_eq!(f.owner.owned(), Reading::None);
        f.owner.follow(&mut r, true, Reading::None).await.unwrap();
        assert_eq!(f.owner.owned(), Reading::Raw);
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(0)));
    }

    /// Codex P1: without logind's inhibitor the key is not read at all.
    #[tokio::test]
    async fn without_the_inhibitor_nothing_is_read() {
        let mut f = fixture(true, true).await;
        f.login.lock().unwrap().refuse = true;
        let mut r = reader(Reading::Raw);
        f.owner.follow(&mut r, true, Reading::None).await.unwrap();
        assert!(r.calls.is_empty());
        assert_eq!(f.xfconf.writes(), 0);
        // logind answers again: the next attempt takes the key.
        f.login.lock().unwrap().refuse = false;
        f.owner.follow(&mut r, true, Reading::None).await.unwrap();
        assert_eq!(f.owner.owned(), Reading::Raw);
    }

    /// A grab (no power manager holds the key): the inhibitor with it, no
    /// xfconf change; the session's end drops the inhibitor.
    #[tokio::test]
    async fn a_grab_holds_the_inhibitor_until_the_session_ends() {
        let mut f = fixture(true, false).await;
        let mut r = reader(Reading::Grab);
        f.owner.follow(&mut r, true, Reading::None).await.unwrap();
        assert_eq!(r.calls, [(true, false)]);
        assert_eq!(f.owner.owned(), Reading::Grab);
        assert_eq!(f.open(), 1);
        assert_eq!(f.xfconf.writes(), 0);
        f.owner.end().await;
        assert_eq!(f.owner.owned(), Reading::None);
        f.settle(0).await;
    }

    /// The ordinary XFCE desktop: nothing is read, inhibited or written.
    #[tokio::test]
    async fn an_ordinary_xfce_desktop_is_left_alone() {
        let mut f = fixture(false, false).await;
        let mut r = reader(Reading::Grab);
        f.owner.follow(&mut r, true, Reading::None).await.unwrap();
        assert!(r.calls.is_empty());
        assert!(f.login.lock().unwrap().calls.is_empty());
        assert_eq!(f.xfconf.writes(), 0);
    }
}
