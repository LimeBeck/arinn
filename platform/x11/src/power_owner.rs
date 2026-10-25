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
//! everything goes back at once in the reverse order.
//!
//! The ordinary XFCE desktop keeps its dialog (owner decision of
//! 2026-10-24). It is recognised on the session bus, not from the
//! environment, which does not reach `arinn-ewmh.service` on every
//! distribution (reviews of #516 and #517): while `org.xfce.SessionManager`
//! or xfce4-power-manager's `org.xfce.PowerManager` has an owner, only the
//! dedicated Arinn XFCE session (`ARINN_SESSION_ENTRY=xfce`, which its
//! session wrapper exports to the user manager) takes the key. Any other
//! X11/EWMH desktop takes it. A name that appears later gives it back.
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

/// The session-bus names of an XFCE desktop: its session manager
/// (ADR-0107) and xfce4-power-manager, which also runs outside XFCE.
pub const XFCE_NAMES: [&str; 2] = ["org.xfce.SessionManager", "org.xfce.PowerManager"];

/// Whether an XFCE session manager or xfce4-power-manager owns its name on
/// this bus. An unanswered query counts as XFCE: the key then stays the
/// desktop's.
pub async fn xfce_on_bus(bus: &zbus::fdo::DBusProxy<'_>) -> bool {
    for name in XFCE_NAMES {
        let Ok(name) = zbus::names::BusName::try_from(name) else {
            return true;
        };
        if bus.name_has_owner(name).await.unwrap_or(true) {
            return true;
        }
    }
    false
}

/// Whether this session may take the power key: the dedicated Arinn XFCE
/// session, and any X11/EWMH desktop where no XFCE name is on the bus. The
/// ordinary XFCE desktop, and any desktop where xfce4-power-manager runs,
/// keeps it.
pub fn allowed(dedicated: bool, xfce: bool) -> bool {
    dedicated || !xfce
}

pub struct PowerOwner {
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
        dedicated: bool,
        xfconf: Option<(Connection, xfce_power::Files)>,
        system: Option<Connection>,
    ) -> Self {
        Self {
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
            xfce_power::dedicated(entry.as_deref()),
            bus.zip(files),
            None,
        )
    }
    /// Whether Arinn holds the key, and how it reads it.
    #[cfg(test)]
    pub fn owned(&self) -> Reading {
        self.owned
    }
    /// Whether logind's inhibitor is held.
    #[cfg(test)]
    pub fn inhibiting(&self) -> bool {
        self.inhibitor.is_some()
    }
    /// One step: `shell` is alive, `xfce` an XFCE name is on the bus
    /// (`xfce_on_bus`), and `reading` is how the snapshot found the key read.
    pub async fn follow<R: Reader>(
        &mut self,
        reader: &mut R,
        shell: bool,
        xfce: bool,
        reading: Reading,
    ) -> Result<()> {
        let wanted = allowed(self.dedicated, xfce) && shell;
        if self.owned != Reading::None {
            // The shell is gone, or the reader stopped or changed (a regrab
            // after a mapping change lost the key, or moved it from a grab
            // to raw events because a restarting xfce4-power-manager won
            // the grab): everything goes back, and the next pass takes it
            // again the right way, with xfconf for raw reading (Codex P2 on
            // #517).
            if !wanted || reading != self.owned {
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
pub(crate) mod tests {
    use super::*;
    use crate::{power_inhibitor::tests as logind, xfce_power::tests as xfconf};
    use std::sync::Mutex;
    use zbus::zvariant::Value;

    /// A reader that answers `Grab`, `Raw` or `None` when enabled.
    pub(crate) struct FakeReader {
        pub(crate) answer: Reading,
        pub(crate) calls: Vec<(bool, bool)>,
    }
    impl Reader for FakeReader {
        fn set(&mut self, enabled: bool, raw: bool) -> Result<Reading> {
            self.calls.push((enabled, raw));
            Ok(if enabled { self.answer } else { Reading::None })
        }
    }
    pub(crate) fn reader(answer: Reading) -> FakeReader {
        FakeReader {
            answer,
            calls: Vec::new(),
        }
    }

    pub(crate) struct Fixture {
        pub(crate) xfconf: xfconf::Fixture,
        login: logind::Shared,
        _logind: Connection,
        pub(crate) owner: PowerOwner,
    }
    pub(crate) async fn fixture(dedicated: bool) -> Fixture {
        let xfconf = xfconf::fixture(Some(Value::U32(3))).await;
        let login = logind::Shared::new(Mutex::new(logind::Fake::default()));
        let service = logind::connect(&xfconf.bus.1).await;
        logind::serve(&service, login.clone()).await;
        let mut owner = PowerOwner::new(
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
        pub(crate) fn open(&self) -> usize {
            self.login.lock().unwrap().open()
        }
        pub(crate) async fn settle(&self, open: usize) {
            for _ in 0..100 {
                if self.open() == open {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            panic!("inhibitors open: {} instead of {open}", self.open());
        }
    }

    /// An XFCE name on the bus (the session manager, or
    /// xfce4-power-manager); `OTHER`: none.
    const XFCE: bool = true;
    const OTHER: bool = false;

    #[test]
    fn the_ordinary_xfce_desktop_keeps_the_key_and_other_desktops_take_it() {
        // Reviews of #516 and #517: XFCE is told apart on the bus, not by
        // XDG_CURRENT_DESKTOP, which need not reach the unit.
        assert!(!allowed(false, XFCE));
        assert!(allowed(true, XFCE));
        assert!(allowed(false, OTHER));
        assert!(allowed(true, OTHER));
    }

    /// `xfce_on_bus` on a private bus: no XFCE name, the session manager,
    /// xfce4-power-manager alone (it also runs outside XFCE).
    #[tokio::test]
    async fn xfce_is_recognised_by_its_bus_names() {
        let bus = logind::bus();
        let adapter = logind::connect(&bus.1).await;
        let dbus = zbus::fdo::DBusProxy::new(&adapter).await.unwrap();
        assert!(!xfce_on_bus(&dbus).await);
        for name in ["org.xfce.SessionManager", "org.xfce.PowerManager"] {
            let owner = logind::connect(&bus.1).await;
            owner.request_name(name).await.unwrap();
            assert!(xfce_on_bus(&dbus).await, "{name}");
            owner.release_name(name).await.unwrap();
            assert!(!xfce_on_bus(&dbus).await, "{name}");
        }
    }

    /// The ordinary XFCE desktop (an XFCE name, no dedicated marker, also
    /// with no variable set): nothing is read, inhibited or written.
    #[tokio::test]
    async fn the_ordinary_xfce_desktop_is_left_alone() {
        let mut f = fixture(xfce_power::dedicated(None)).await;
        let mut r = reader(Reading::Grab);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert!(r.calls.is_empty());
        assert!(f.login.lock().unwrap().calls.is_empty());
        assert_eq!(f.xfconf.writes(), 0);
    }

    /// Any other X11/EWMH desktop (no XFCE name): the key is grabbed with
    /// the inhibitor, as without XFCE before. When xfce4-power-manager
    /// starts later, everything goes back at once; it never comes back
    /// while that name is owned, and returns when it goes.
    #[tokio::test]
    async fn another_desktop_takes_the_key_until_xfce4_power_manager_appears() {
        let mut f = fixture(false).await;
        let mut r = reader(Reading::Grab);
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Grab);
        assert_eq!(r.calls, [(true, false)]);
        assert_eq!(f.open(), 1);
        f.owner
            .follow(&mut r, true, XFCE, Reading::Grab)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        assert_eq!(r.calls.last(), Some(&(false, false)));
        f.settle(0).await;
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(r.calls.len(), 2);
        assert_eq!(f.xfconf.writes(), 0);
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Grab);
    }

    /// Dedicated XFCE session: the inhibitor, raw reading, then "do
    /// nothing"; when the shell exits (Close Arinn, no restart) everything
    /// goes back.
    #[tokio::test]
    async fn a_shell_exit_gives_everything_back() {
        let mut f = fixture(true).await;
        let mut r = reader(Reading::Raw);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Raw);
        assert_eq!(r.calls, [(true, true)]);
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(0)));
        assert_eq!(f.open(), 1);
        // A steady state changes nothing.
        f.owner
            .follow(&mut r, true, XFCE, Reading::Raw)
            .await
            .unwrap();
        assert_eq!(r.calls.len(), 1);
        f.owner
            .follow(&mut r, false, XFCE, Reading::Raw)
            .await
            .unwrap();
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
        let mut f = fixture(true).await;
        let mut r = reader(Reading::Raw);
        for _ in 0..5 {
            f.owner
                .follow(&mut r, true, XFCE, Reading::None)
                .await
                .unwrap();
            assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(0)));
            f.owner
                .follow(&mut r, false, XFCE, Reading::Raw)
                .await
                .unwrap();
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
        let mut f = fixture(true).await;
        let mut r = reader(Reading::None);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        assert_eq!(r.calls, [(true, true), (false, false)]);
        assert_eq!(f.xfconf.writes(), 0);
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
        f.settle(0).await;
    }

    /// Codex P2 on #517: in the dedicated session Arinn grabbed the key
    /// (xfce4-power-manager was not running); a regrab after a mapping
    /// change lost it to a restarting xfce4-power-manager and reads it from
    /// raw events instead. The change gives everything back, and the next
    /// pass takes it again with xfconf at "do nothing", so the two never
    /// both act on one press.
    #[tokio::test]
    async fn a_reading_that_moves_from_grab_to_raw_takes_xfconf() {
        let mut f = fixture(true).await;
        let mut r = reader(Reading::Grab);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Grab);
        assert_eq!(f.xfconf.writes(), 0);
        r.answer = Reading::Raw;
        f.owner
            .follow(&mut r, true, XFCE, Reading::Raw)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Raw);
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(0)));
    }

    /// A reader that stops later (a regrab lost the key) gives everything
    /// back, and the key is taken again when it reads once more.
    #[tokio::test]
    async fn a_reader_that_stops_gives_everything_back() {
        let mut f = fixture(true).await;
        let mut r = reader(Reading::Raw);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
        assert_eq!(f.owner.owned(), Reading::None);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Raw);
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(0)));
    }

    /// Codex P1: without logind's inhibitor the key is not read at all.
    #[tokio::test]
    async fn without_the_inhibitor_nothing_is_read() {
        let mut f = fixture(true).await;
        f.login.lock().unwrap().refuse = true;
        let mut r = reader(Reading::Raw);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert!(r.calls.is_empty());
        assert_eq!(f.xfconf.writes(), 0);
        // logind answers again: the next attempt takes the key.
        f.login.lock().unwrap().refuse = false;
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Raw);
    }

    /// A grab (no power manager holds the key): the inhibitor with it, no
    /// xfconf change; the session's end drops the inhibitor.
    #[tokio::test]
    async fn a_grab_holds_the_inhibitor_until_the_session_ends() {
        let mut f = fixture(false).await;
        let mut r = reader(Reading::Grab);
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert_eq!(r.calls, [(true, false)]);
        assert_eq!(f.owner.owned(), Reading::Grab);
        assert_eq!(f.open(), 1);
        assert_eq!(f.xfconf.writes(), 0);
        f.owner.end().await;
        assert_eq!(f.owner.owned(), Reading::None);
        f.settle(0).await;
    }
}
