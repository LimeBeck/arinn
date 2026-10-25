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
//!    events only, never a grab, so xfce4-power-manager's own grab always
//!    succeeds.
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
    /// Reads the power key (`enabled`): grabbed, or with `raw` from raw
    /// events only, never grabbed. Returns how it is read now.
    fn set(&mut self, enabled: bool, raw: bool) -> Result<Reading>;
    /// Whether another client grabs a power key now; called while Arinn
    /// grabs none.
    fn held_elsewhere(&mut self) -> Result<bool>;
}

/// The session-bus names of an XFCE desktop: its session manager
/// (ADR-0107) and xfce4-power-manager, which also runs outside XFCE.
pub const XFCE_NAMES: [&str; 2] = ["org.xfce.SessionManager", "org.xfce.PowerManager"];

/// xfce4-power-manager's bus name and its restart call.
const POWER_MANAGER: &str = "org.xfce.PowerManager";
const POWER_MANAGER_PATH: &str = "/org/xfce/PowerManager";
const POWER_MANAGER_INTERFACE: &str = "org.xfce.Power.Manager";

/// Which XFCE names own their name on the session bus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Xfce {
    /// `org.xfce.SessionManager`.
    pub session: bool,
    /// `org.xfce.PowerManager`: xfce4-power-manager runs.
    pub power_manager: bool,
}
impl Xfce {
    /// An XFCE desktop, or xfce4-power-manager on another one.
    pub fn any(self) -> bool {
        self.session || self.power_manager
    }
}

/// Which XFCE names own their name on this bus. An unanswered query counts
/// as owned: the key then stays the desktop's.
pub async fn xfce_on_bus(bus: &zbus::fdo::DBusProxy<'_>) -> Xfce {
    let owned = async |name: &str| match zbus::names::BusName::try_from(name) {
        Ok(name) => bus.name_has_owner(name).await.unwrap_or(true),
        Err(_) => true,
    };
    Xfce {
        session: owned(XFCE_NAMES[0]).await,
        power_manager: owned(XFCE_NAMES[1]).await,
    }
}

/// Restarts xfce4-power-manager through its own D-Bus `Restart`, so that
/// it grabs the power key again: its grab failed while Arinn held the key,
/// and it never grabs again by itself (review of #516). Waits up to `wait`
/// for the new instance to own its name. Returns whether it was asked.
pub async fn restart_power_manager(bus: &Connection, wait: Duration) -> bool {
    let Ok(dbus) = zbus::fdo::DBusProxy::new(bus).await else {
        return false;
    };
    let Ok(name) = zbus::names::BusName::try_from(POWER_MANAGER) else {
        return false;
    };
    let before = dbus.get_name_owner(name.clone()).await.ok();
    let asked = tokio::time::timeout(
        Duration::from_secs(5),
        bus.call_method(
            Some(POWER_MANAGER),
            POWER_MANAGER_PATH,
            Some(POWER_MANAGER_INTERFACE),
            "Restart",
            &(),
        ),
    )
    .await
    .is_ok_and(|reply| reply.is_ok());
    eprintln!(
        "arinn-platform-x11: xfce4-power-manager's power-key grab failed while Arinn held the key; {}",
        if asked {
            "restarted it"
        } else {
            "its restart failed"
        }
    );
    let deadline = Instant::now() + wait;
    while asked && Instant::now() < deadline {
        let now = dbus.get_name_owner(name.clone()).await.ok();
        if now.is_some() && now != before {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    asked
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
    /// The session bus, to restart xfce4-power-manager.
    bus: Option<Connection>,
    /// Arinn may have broken xfce4-power-manager's grab: it held the key by
    /// a grab (or an earlier adapter may have, at the start), and nobody
    /// has checked since it let go.
    heal: bool,
    /// How long a restarted xfce4-power-manager has to own its name again.
    restart_wait: Duration,
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
        bus: Option<Connection>,
        xfconf: Option<(Connection, xfce_power::Files)>,
        system: Option<Connection>,
    ) -> Self {
        Self {
            dedicated,
            bus,
            heal: true,
            restart_wait: Duration::from_secs(10),
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
            bus.clone(),
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
    /// One step: `shell` is alive, `xfce` names the XFCE names on the bus
    /// (`xfce_on_bus`), and `reading` is how the snapshot found the key read.
    pub async fn follow<R: Reader>(
        &mut self,
        reader: &mut R,
        shell: bool,
        xfce: Xfce,
        reading: Reading,
    ) -> Result<()> {
        let wanted = allowed(self.dedicated, xfce.any()) && shell;
        if self.owned != Reading::None {
            // The shell is gone, or the reader stopped or changed (a regrab
            // after a mapping change lost the key, or moved it from a grab
            // to raw events because a restarting xfce4-power-manager won
            // the grab): everything goes back, and the next pass takes it
            // again the right way, with xfconf for raw reading (Codex P2 on
            // #517).
            if !wanted || reading != self.owned {
                self.give_back(reader, xfce).await?;
            }
            return Ok(());
        }
        // An earlier adapter may have held the key by a grab (a crash).
        self.heal(reader, xfce).await;
        if !wanted || self.retry.is_some_and(|at| Instant::now() < at) {
            return Ok(());
        }
        // The dedicated session reads raw events only, and needs xfconf to
        // set xfce4-power-manager to "do nothing" first.
        if self.dedicated && self.xfconf.is_none() {
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
        // The dedicated session never grabs: xfce4-power-manager's grab must
        // succeed whenever it starts (review of #516). Other desktops grab.
        let raw = self.dedicated;
        match reader.set(true, raw)? {
            Reading::Grab if !raw => {
                self.owned = Reading::Grab;
                self.heal = true;
            }
            Reading::Raw if raw => {
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
            _ => {
                // Held by another client, or not readable (no XInput 2.1):
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
    /// Gives everything back: the reader first, then a restart of
    /// xfce4-power-manager if Arinn's grab broke its own, then the desktop's
    /// setting, then the inhibitor (held through the restart).
    pub async fn give_back<R: Reader>(&mut self, reader: &mut R, xfce: Xfce) -> Result<()> {
        let stopped = reader.set(false, false);
        self.heal(reader, xfce).await;
        self.end().await;
        stopped.map(drop)
    }
    /// After Arinn let go of a grab: if xfce4-power-manager runs and nobody
    /// grabs the power key, its grab failed while Arinn held it, and it
    /// never grabs again; restart it so the desktop handles the key (review
    /// of #516). Nothing is done when it does not run (it will grab when it
    /// starts) or when it holds its grab.
    async fn heal<R: Reader>(&mut self, reader: &mut R, xfce: Xfce) {
        if !std::mem::take(&mut self.heal) || !xfce.power_manager {
            return;
        }
        if let (Ok(false), Some(bus)) = (reader.held_elsewhere(), self.bus.as_ref()) {
            restart_power_manager(bus, self.restart_wait).await;
        }
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
    use std::sync::{Arc, Mutex};
    use zbus::zvariant::Value;

    /// A reader that answers `Grab`, `Raw` or `None` when enabled.
    pub(crate) struct FakeReader {
        pub(crate) answer: Reading,
        pub(crate) calls: Vec<(bool, bool)>,
        /// Another client grabs a power key.
        pub(crate) held: bool,
    }
    impl Reader for FakeReader {
        fn set(&mut self, enabled: bool, raw: bool) -> Result<Reading> {
            self.calls.push((enabled, raw));
            Ok(if enabled { self.answer } else { Reading::None })
        }
        fn held_elsewhere(&mut self) -> Result<bool> {
            Ok(self.held)
        }
    }
    pub(crate) fn reader(answer: Reading) -> FakeReader {
        FakeReader {
            answer,
            calls: Vec::new(),
            held: true,
        }
    }

    /// xfce4-power-manager's `Restart`; records how many logind inhibitors
    /// Arinn held at each call.
    struct Xfpm(logind::Shared, Arc<Mutex<Vec<usize>>>);
    #[zbus::interface(name = "org.xfce.Power.Manager")]
    impl Xfpm {
        fn restart(&self) {
            let open = self.0.lock().unwrap().open();
            self.1.lock().unwrap().push(open);
        }
    }

    pub(crate) struct Fixture {
        pub(crate) xfconf: xfconf::Fixture,
        login: logind::Shared,
        _logind: Connection,
        pub(crate) owner: PowerOwner,
        /// Inhibitors Arinn held at each xfce4-power-manager restart.
        restarts: Arc<Mutex<Vec<usize>>>,
        _xfpm: Option<Connection>,
    }
    pub(crate) async fn fixture(dedicated: bool) -> Fixture {
        let xfconf = xfconf::fixture(Some(Value::U32(3))).await;
        let login = logind::Shared::new(Mutex::new(logind::Fake::default()));
        let service = logind::connect(&xfconf.bus.1).await;
        logind::serve(&service, login.clone()).await;
        let mut owner = PowerOwner::new(
            dedicated,
            Some(xfconf.adapter.clone()),
            Some((xfconf.adapter.clone(), xfconf.files.clone())),
            Some(logind::connect(&xfconf.bus.1).await),
        );
        owner.retry_after = Duration::ZERO;
        owner.restart_wait = Duration::from_millis(50);
        Fixture {
            xfconf,
            login,
            _logind: service,
            owner,
            restarts: Arc::default(),
            _xfpm: None,
        }
    }
    impl Fixture {
        /// xfce4-power-manager starts: it owns its name and answers
        /// `Restart`.
        async fn start_xfpm(&mut self) {
            let xfpm = logind::connect(&self.xfconf.bus.1).await;
            xfpm.object_server()
                .at(
                    POWER_MANAGER_PATH,
                    Xfpm(self.login.clone(), self.restarts.clone()),
                )
                .await
                .unwrap();
            xfpm.request_name(POWER_MANAGER).await.unwrap();
            self._xfpm = Some(xfpm);
        }
        fn restarts(&self) -> Vec<usize> {
            self.restarts.lock().unwrap().clone()
        }
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

    /// An XFCE desktop (its session manager and xfce4-power-manager); `OTHER`:
    /// no XFCE name; `XFPM`: xfce4-power-manager on another desktop.
    const XFCE: Xfce = Xfce {
        session: true,
        power_manager: true,
    };
    const OTHER: Xfce = Xfce {
        session: false,
        power_manager: false,
    };
    const XFPM: Xfce = Xfce {
        session: false,
        power_manager: true,
    };

    #[test]
    fn the_ordinary_xfce_desktop_keeps_the_key_and_other_desktops_take_it() {
        // Reviews of #516 and #517: XFCE is told apart on the bus, not by
        // XDG_CURRENT_DESKTOP, which need not reach the unit.
        assert!(!allowed(false, XFCE.any()));
        assert!(!allowed(false, XFPM.any()));
        assert!(allowed(true, XFCE.any()));
        assert!(allowed(false, OTHER.any()));
        assert!(allowed(true, OTHER.any()));
    }

    /// `xfce_on_bus` on a private bus: no XFCE name, the session manager,
    /// xfce4-power-manager alone (it also runs outside XFCE).
    #[tokio::test]
    async fn xfce_is_recognised_by_its_bus_names() {
        let bus = logind::bus();
        let adapter = logind::connect(&bus.1).await;
        let dbus = zbus::fdo::DBusProxy::new(&adapter).await.unwrap();
        assert_eq!(xfce_on_bus(&dbus).await, OTHER);
        for (name, seen) in [
            (
                "org.xfce.SessionManager",
                Xfce {
                    session: true,
                    power_manager: false,
                },
            ),
            ("org.xfce.PowerManager", XFPM),
        ] {
            let owner = logind::connect(&bus.1).await;
            owner.request_name(name).await.unwrap();
            assert_eq!(xfce_on_bus(&dbus).await, seen, "{name}");
            owner.release_name(name).await.unwrap();
            assert_eq!(xfce_on_bus(&dbus).await, OTHER, "{name}");
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

    /// Review of #516: the dedicated session never grabs the power key; it
    /// asks for raw reading only, and a reader that grabbed instead is let
    /// go, so xfce4-power-manager's grab succeeds whenever it starts.
    #[tokio::test]
    async fn the_dedicated_session_never_grabs() {
        let mut f = fixture(true).await;
        let mut r = reader(Reading::Grab);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(r.calls, [(true, true), (false, false)]);
        assert_eq!(f.owner.owned(), Reading::None);
        assert_eq!(f.xfconf.writes(), 0);
        f.settle(0).await;
    }

    /// Review of #516: xfce4-power-manager starts on another desktop while
    /// Arinn grabs the key, so its own grab fails and it never grabs again.
    /// Arinn gives the key back and restarts it, holding logind's inhibitor
    /// through the restart, so the desktop handles the key again.
    #[tokio::test]
    async fn xfce4_power_manager_starting_after_arinns_grab_is_restarted() {
        let mut f = fixture(false).await;
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Grab);
        f.start_xfpm().await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::Grab)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        assert_eq!(f.restarts(), [1]);
        f.settle(0).await;
        // Once is enough: no restart while Arinn does not grab again.
        f.owner
            .follow(&mut r, true, XFPM, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.restarts(), [1]);
    }

    /// No restart when xfce4-power-manager holds its own grab, or when it
    /// does not run when Arinn lets go (it grabs when it starts).
    #[tokio::test]
    async fn xfce4_power_manager_is_restarted_only_when_its_grab_failed() {
        let mut f = fixture(false).await;
        f.start_xfpm().await;
        let mut r = reader(Reading::Grab);
        r.held = true;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        f.owner
            .follow(&mut r, true, XFPM, Reading::Grab)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        assert!(f.restarts().is_empty());
        let mut f = fixture(false).await;
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        f.owner
            .follow(&mut r, false, OTHER, Reading::Grab)
            .await
            .unwrap();
        f.start_xfpm().await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::None)
            .await
            .unwrap();
        assert!(f.restarts().is_empty());
    }

    /// A crashed adapter may have held the grab: the next adapter's first
    /// pass restarts an xfce4-power-manager whose grab is missing, and
    /// leaves one that holds it alone (the dedicated session, the ordinary
    /// XFCE desktop).
    #[tokio::test]
    async fn the_next_adapter_heals_a_power_manager_a_crash_left_without_its_grab() {
        let mut f = fixture(false).await;
        f.start_xfpm().await;
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.restarts(), [0]);
        let mut f = fixture(true).await;
        f.start_xfpm().await;
        let mut r = reader(Reading::Raw);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert!(f.restarts().is_empty());
        assert_eq!(f.owner.owned(), Reading::Raw);
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
