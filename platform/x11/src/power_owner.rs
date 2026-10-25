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
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use zbus::Connection;

/// Reads the power key; the adapter's native layer, or a test double.
pub trait Reader {
    /// Reads the power key (`enabled`): grabbed, or with `raw` from raw
    /// events only, never grabbed. Returns how it is read now.
    fn set(&mut self, enabled: bool, raw: bool) -> Result<Reading>;
    /// Whether another client grabs a power key now; called while Arinn
    /// grabs none.
    fn held_elsewhere(&mut self) -> Result<bool>;
    /// Raw presses count from now on; one read before is dropped.
    fn arm(&mut self) -> Result<()>;
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

/// How a restart of xfce4-power-manager ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Restarted {
    /// A new instance owns its name: it grabbed the key when it started.
    Confirmed,
    /// The old instance went away, and no new one owned the name in time.
    Missing,
    /// Nothing changed: `Restart` failed or timed out, or the old instance
    /// still owns the name.
    Unchanged,
    /// Its name had no owner any more: nothing was restarted (#519).
    Gone,
}

/// How long a restart of xfce4-power-manager may take, each counted from
/// its start.
#[derive(Clone, Copy, Debug)]
pub struct RestartTimes {
    /// The `Restart` call's answer.
    pub call: Duration,
    /// A new instance owning the name, while the old one still owns it.
    pub wait: Duration,
    /// A new instance owning the name, while the name has no owner: the
    /// whole restart, and so the caller's inhibitor (ADR-0111: at most
    /// 15 s).
    pub absent: Duration,
}
impl Default for RestartTimes {
    fn default() -> Self {
        Self {
            call: Duration::from_secs(5),
            wait: Duration::from_secs(10),
            absent: Duration::from_secs(15),
        }
    }
}

/// Restarts xfce4-power-manager through its own D-Bus `Restart`, so that
/// it grabs the power key again: its grab failed while Arinn held the key,
/// and it never grabs again by itself (review of #516). Waits up to
/// `times.wait` for a new instance to own its name; while the name has no
/// owner at all (the old instance quit, the new one starts slowly), up to
/// `times.absent`. Both count from the start, `Restart` call included, so
/// the caller's inhibitor is held at most `times.absent` (ADR-0111: at most
/// 15 s; review of #521). A call that is not answered in time may still
/// take effect, so the name is watched the same way after it; only an
/// error reply ends the restart at once (review of #521). Never calls
/// `Restart` when the name has no owner: an xfce4-power-manager that is
/// gone (stopped by the user) is not started.
pub async fn restart_power_manager(bus: &Connection, times: RestartTimes) -> Restarted {
    let start = Instant::now();
    let Ok(dbus) = zbus::fdo::DBusProxy::new(bus).await else {
        return Restarted::Unchanged;
    };
    let Ok(name) = zbus::names::BusName::try_from(POWER_MANAGER) else {
        return Restarted::Unchanged;
    };
    let Ok(before) = dbus.get_name_owner(name.clone()).await else {
        return Restarted::Gone;
    };
    let asked = tokio::time::timeout(
        times.call,
        bus.call_method(
            Some(POWER_MANAGER),
            POWER_MANAGER_PATH,
            Some(POWER_MANAGER_INTERFACE),
            "Restart",
            &(),
        ),
    )
    .await;
    eprintln!(
        "arinn-platform-x11: xfce4-power-manager's power-key grab failed while Arinn held the key; {}",
        match asked {
            Ok(Ok(_)) => "restarted it",
            Ok(Err(_)) => "its restart failed",
            Err(_) => "its restart was not answered in time; watching it",
        }
    );
    if matches!(asked, Ok(Err(_))) {
        return Restarted::Unchanged;
    }
    let (deadline, absent) = (start + times.wait, start + times.absent.max(times.wait));
    loop {
        let now = dbus.get_name_owner(name.clone()).await.ok();
        match &now {
            Some(now) if *now != before => return Restarted::Confirmed,
            Some(_) if Instant::now() >= deadline => return Restarted::Unchanged,
            None if Instant::now() >= absent => return Restarted::Missing,
            _ => {}
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// `$XDG_RUNTIME_DIR/arinn/power-grab-<display>`, naming this X display: its adapter
/// grabs, or grabbed, the power key and has not checked
/// xfce4-power-manager since it let go. Only then may an adapter start or
/// `--release-power-key` restart xfce4-power-manager (review of #516); the
/// dedicated session and the ordinary XFCE desktop never grab, so they
/// never restart it.
#[derive(Clone, Debug, Default)]
pub struct GrabMarker {
    path: Option<PathBuf>,
    display: String,
}
impl GrabMarker {
    /// `$XDG_RUNTIME_DIR/arinn/power-grab-<display>`, one per X display of
    /// the user, so two displays never overwrite each other's marker.
    pub fn from_env() -> Self {
        let display = std::env::var("DISPLAY").unwrap_or_default();
        Self::at(
            std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join("arinn").join(Self::file_name(&display))),
            display,
        )
    }
    /// The marker's file name for `display`, with anything but letters,
    /// digits, `.`, `-` and `_` replaced.
    fn file_name(display: &str) -> String {
        let display: String = display
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("power-grab-{display}")
    }
    pub fn at(path: Option<PathBuf>, display: String) -> Self {
        Self { path, display }
    }
    fn set(&self) {
        if let Some(path) = &self.path {
            let _ = path
                .parent()
                .map(std::fs::create_dir_all)
                .transpose()
                .and_then(|_| std::fs::write(path, &self.display));
        }
    }
    /// This display's adapter left the marker.
    pub fn present(&self) -> bool {
        self.path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .is_some_and(|display| display == self.display)
    }
    pub fn clear(&self) {
        if self.present()
            && let Some(path) = &self.path
        {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// `arinn-platform-x11 --release-power-key` (`ExecStopPost=`), after the
/// adapter ended: gives the XFCE setting back, and restarts
/// xfce4-power-manager if this display's adapter grabbed (`marker`) and
/// `held` finds no mapped power key grabbed while it runs. The adapter's
/// inhibitor ended with it, so a temporary one on `system` covers all of
/// it (Codex on #517).
pub async fn release_after_adapter<H: FnOnce() -> Option<bool>>(
    bus: &Connection,
    files: Option<&xfce_power::Files>,
    marker: &GrabMarker,
    held: H,
    system: Option<&Connection>,
    wait: Duration,
) {
    let _inhibitor = match system {
        Some(system) => power_inhibitor::inhibit(system).await,
        None => None,
    };
    if let Some(files) = files {
        xfce_power::release(bus, files).await;
    }
    if marker.present() {
        // The marker goes only once nothing is left to repair: no
        // xfce4-power-manager, its grab held, or a confirmed restart.
        // Otherwise a later adapter start tries again (Codex on #517).
        let repaired = match zbus::fdo::DBusProxy::new(bus).await {
            Ok(dbus) if !xfce_on_bus(&dbus).await.power_manager => true,
            Ok(_) => match held() {
                Some(true) => true,
                // A name without an owner: nothing runs to repair.
                Some(false) => {
                    matches!(
                        restart_power_manager(
                            bus,
                            RestartTimes {
                                wait,
                                absent: wait,
                                ..RestartTimes::default()
                            }
                        )
                        .await,
                        Restarted::Confirmed | Restarted::Gone
                    )
                }
                None => false,
            },
            Err(_) => false,
        };
        if repaired {
            marker.clear();
        }
    }
}

/// Opens the system bus: the default one, or the bus at `address` (tests).
pub async fn open_system(address: Option<&str>) -> Option<Connection> {
    match address {
        None => Connection::system().await.ok(),
        Some(address) => zbus::connection::Builder::address(address)
            .ok()?
            .build()
            .await
            .ok(),
    }
}

/// `arinn-platform-x11 --restore-power-button`, the system XDG autostart
/// entry at every XFCE login: gives back a copy an unclean end of the
/// dedicated session left (`xfce_power::restore_at_login`). With a copy to
/// give back it holds a temporary logind inhibitor meanwhile, on the system
/// bus (`system`: its address, `None` the default), as the other restores
/// do (Codex on #517, #519): no adapter holds one for it. Without a copy
/// logind is not asked.
pub async fn restore_at_login(
    bus: &Connection,
    files: &xfce_power::Files,
    system: Option<&str>,
) -> bool {
    let _inhibitor = if files.record.exists() {
        match open_system(system).await {
            Some(system) => power_inhibitor::inhibit(&system).await,
            None => None,
        }
    } else {
        None
    };
    xfce_power::restore_at_login(bus, files).await
}

/// Gives back a copy an earlier dedicated run left, at an adapter start,
/// holding a temporary logind inhibitor meanwhile: no adapter of this start
/// holds one yet (Codex on #517).
pub async fn release_stale(
    bus: &Connection,
    files: &xfce_power::Files,
    system: Option<&Connection>,
) -> bool {
    let _inhibitor = match system {
        Some(system) => power_inhibitor::inhibit(system).await,
        None => None,
    };
    xfce_power::release(bus, files).await
}

/// xfce4-power-manager restarts in a row that it did not come back from,
/// after which the running adapter gives up (review of #521).
const RESTARTS: u32 = 3;

/// How long each logind query of the session check may take: it runs on
/// the adapter's loop, so a hung logind delays Home and the quick menu by
/// little; no answer counts as another session (review of #517).
const LOGIND: Duration = Duration::from_millis(250);

/// Whether user `uid` has a graphical session (`x11` or `wayland`, not
/// closing) other than `own` in logind. xfconfd is per user, so in such a
/// session xfce4-power-manager would do nothing as well while Arinn is on
/// another display (review of #516, Codex P1). A query logind does not
/// answer counts as another session.
pub async fn other_graphical_session(system: &Connection, own: &str, uid: u32) -> bool {
    type Sessions = Vec<(String, u32, String, String, zbus::zvariant::OwnedObjectPath)>;
    let Ok(Ok(reply)) = tokio::time::timeout(
        LOGIND,
        system.call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "ListSessions",
            &(),
        ),
    )
    .await
    else {
        return true;
    };
    let Ok(sessions) = reply.body().deserialize::<Sessions>() else {
        return true;
    };
    for (id, owner, _, _, path) in sessions {
        if owner != uid || id == own {
            continue;
        }
        let property = async |name: &str| -> Option<String> {
            let reply = tokio::time::timeout(
                LOGIND,
                system.call_method(
                    Some("org.freedesktop.login1"),
                    path.as_str(),
                    Some("org.freedesktop.DBus.Properties"),
                    "Get",
                    &("org.freedesktop.login1.Session", name),
                ),
            )
            .await
            .ok()?
            .ok()?;
            let value: zbus::zvariant::OwnedValue = reply.body().deserialize().ok()?;
            String::try_from(value).ok()
        };
        let (Some(kind), Some(state)) = (property("Type").await, property("State").await) else {
            return true;
        };
        if matches!(kind.as_str(), "x11" | "wayland") && state != "closing" {
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
    /// The system bus for logind; opened on first use, and again after a
    /// failed attempt (#519).
    system: Option<Connection>,
    /// The system bus's address; `None`: the default (tests: a private bus).
    system_address: Option<String>,
    inhibitor: Option<power_inhibitor::Inhibitor>,
    /// The session bus, to restart xfce4-power-manager.
    bus: Option<Connection>,
    /// The login's logind session (`ARINN_LOGIN_SESSION_ID`, passed by
    /// `arinn-xfce-session`) and user: the dedicated session sets xfconf
    /// only while the user has no other graphical session.
    own_session: Option<String>,
    uid: u32,
    /// Arinn may have broken xfce4-power-manager's grab: it held the key by
    /// a grab (or an earlier adapter did, by `marker`), and nobody has
    /// checked since it let go. An unconfirmed repair sets it again (#519).
    heal: bool,
    /// The next repair may not run before then: repairs are at least
    /// `heal_every` apart, so xfce4-power-manager is never restarted in a
    /// loop (#519).
    heal_at: Option<Instant>,
    heal_every: Duration,
    marker: GrabMarker,
    /// How long a restart of xfce4-power-manager may take.
    restart: RestartTimes,
    /// A restart running off the loop, and how it ended.
    restarting: Option<tokio::task::JoinHandle<Restarted>>,
    /// Restarts in a row after which xfce4-power-manager did not come back
    /// (`Restarted::Missing`); at `RESTARTS` the adapter gives up
    /// (`gave_up`): it restarts nothing more and grabs no key until it
    /// restarts, so the desktop keeps the key (review of #521).
    unconfirmed: u32,
    gave_up: bool,
    /// "No session bus" was logged once.
    no_bus_logged: bool,
    /// A stale XFCE copy the start, or a release, could not give back, and
    /// its files.
    stale: Option<(Connection, xfce_power::Files)>,
    /// Arinn's inhibitor, kept while its own "do nothing" could not be
    /// given back (Codex on #517).
    stale_inhibitor: Option<power_inhibitor::Inhibitor>,
    /// A retry of it running off the loop (whether the copy is back, and a
    /// system bus it opened), and when the next may start.
    stale_retry: Option<tokio::task::JoinHandle<(bool, Option<Connection>)>>,
    stale_at: Option<Instant>,
    stale_every: Duration,
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
            own_session: None,
            // SAFETY: getuid has no preconditions and cannot fail.
            uid: unsafe { libc::getuid() },
            heal: false,
            heal_at: None,
            heal_every: Duration::from_secs(30),
            marker: GrabMarker::default(),
            restart: RestartTimes::default(),
            restarting: None,
            unconfirmed: 0,
            gave_up: false,
            no_bus_logged: false,
            stale: None,
            stale_inhibitor: None,
            stale_retry: None,
            stale_at: None,
            stale_every: Duration::from_secs(30),
            xfconf,
            system,
            system_address: None,
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
        let dedicated = xfce_power::dedicated(entry.as_deref());
        let var = |name: &str| std::env::var_os(name);
        let bus = Connection::session().await.ok();
        // The dedicated session changes xfconf only with the login's own
        // directories; without them it takes nothing.
        let files = xfce_power::Files::from_vars(var, dedicated);
        let mut owner = Self::new(dedicated, bus.clone(), bus.clone().zip(files), None);
        // A copy an earlier dedicated run left goes back first, in any
        // X11/EWMH session, under a temporary inhibitor (Codex on #517).
        if let (Some(bus), Some(files)) = (bus.as_ref(), xfce_power::Files::from_vars(var, false)) {
            owner
                .restore_stale(bus, files, Connection::system().await.ok())
                .await;
        }
        owner.own_session = std::env::var("ARINN_LOGIN_SESSION_ID")
            .ok()
            .filter(|id| !id.is_empty());
        owner.with_marker(GrabMarker::from_env())
    }
    /// Gives back a copy an earlier dedicated run left, at an adapter start,
    /// under a temporary inhibitor on `system`. The owner keeps `system`, so
    /// a copy that could not be given back is retried on later passes under
    /// an inhibitor as well (Codex on #517): the login restore leaves it to
    /// a running adapter.
    pub async fn restore_stale(
        &mut self,
        bus: &Connection,
        files: xfce_power::Files,
        system: Option<Connection>,
    ) {
        if self.system.is_none() {
            self.system = system;
        }
        if !release_stale(bus, &files, self.system.as_ref()).await {
            self.stale = Some((bus.clone(), files));
        }
    }
    /// Uses `marker`; one an earlier adapter of this display left is
    /// checked at the first pass.
    pub fn with_marker(mut self, marker: GrabMarker) -> Self {
        self.heal = marker.present();
        self.marker = marker;
        self
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
        // The dedicated session's marker counts only on an XFCE desktop: a
        // stale one (a killed wrapper, a lingering user manager) on another
        // desktop falls back to the grab, never to raw reading next to that
        // desktop's own handler (review of #516).
        let dedicated = self.dedicated && xfce.session;
        // The dedicated session's "do nothing" is per user (xfconfd): it
        // holds only while the user has no other graphical session, where
        // xfce4-power-manager would do nothing as well (review of #516).
        let alone = !dedicated || !shell || self.alone().await;
        let wanted = allowed(dedicated, xfce.any()) && shell && alone;
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
        // An earlier adapter may have held the key by a grab (a crash), or
        // a repair was not confirmed.
        self.reap_restart();
        self.heal(reader, xfce).await;
        self.retry_stale();
        // Never while an xfce4-power-manager restart is in flight: the old
        // instance's name is gone for a moment, and a grab now would make
        // the new instance's grab fail again (review of #517, Codex P1).
        if !wanted
            || self.retry.is_some_and(|at| Instant::now() < at)
            || self.restarting.as_ref().is_some_and(|t| !t.is_finished())
            || self.stale_retry.is_some()
        {
            return Ok(());
        }
        // The dedicated session reads raw events only, and needs xfconf to
        // set xfce4-power-manager to "do nothing" first.
        if dedicated && self.xfconf.is_none() {
            return Ok(());
        }
        // After giving up on xfce4-power-manager, a grab could break its
        // grab once more: the key stays the desktop's (review of #521).
        // Raw reading never grabs.
        if self.gave_up && !dedicated {
            return Ok(());
        }
        self.retry = Some(Instant::now() + self.retry_after);
        if self.inhibitor.is_none() {
            if self.system.is_none() {
                self.system = open_system(self.system_address.as_deref()).await;
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
        let raw = dedicated;
        match reader.set(true, raw)? {
            Reading::Grab if !raw => {
                self.owned = Reading::Grab;
                self.heal = true;
                self.marker.set();
            }
            Reading::Raw if raw => {
                let (bus, files) = self.xfconf.as_ref().expect("raw needs xfconf");
                if xfce_power::take(bus, files).await {
                    self.taken_xfconf = true;
                    self.owned = Reading::Raw;
                    // A press read while xfce4-power-manager still acted on
                    // the key was its own (Codex on #517).
                    reader.arm()?;
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
    /// Whether the user has no graphical session besides this login's.
    /// Without this login's session id, or without an answer, it counts as
    /// not alone.
    async fn alone(&mut self) -> bool {
        let Some(own) = self.own_session.clone() else {
            return false;
        };
        if self.system.is_none() {
            self.system = open_system(self.system_address.as_deref()).await;
        }
        match self.system.as_ref() {
            Some(system) => !other_graphical_session(system, &own, self.uid).await,
            None => false,
        }
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
    /// starts, and one the user stopped stays stopped) or when it holds its
    /// grab.
    ///
    /// A repair that is not confirmed (the grab check fails, `Restart`
    /// fails, or no new instance owns the name in time) is tried again by
    /// the running adapter, at most every `heal_every`, until it is
    /// confirmed or xfce4-power-manager is gone (Codex on #517, #519). After
    /// `RESTARTS` restarts in a row it did not come back from, it gives up
    /// (`gave_up`).
    async fn heal<R: Reader>(&mut self, reader: &mut R, xfce: Xfce) {
        if !self.heal {
            return;
        }
        if self.gave_up {
            // The marker stays for the next start (review of #521).
            self.heal = false;
            return;
        }
        // The marker goes only once nothing is left to repair, so a later
        // start retries an unconfirmed one (Codex on #517).
        // An absence right after an unconfirmed restart may be a slow new
        // instance: the count of missing instances stays (review of
        // #521).
        if !xfce.power_manager {
            self.heal = false;
            self.heal_at = None;
            self.marker.clear();
            return;
        }
        if self.heal_at.is_some_and(|at| Instant::now() < at) {
            return;
        }
        // Whatever follows, the next attempt waits.
        self.heal_at = Some(Instant::now() + self.heal_every);
        match reader.held_elsewhere() {
            Ok(true) => {
                self.repaired();
                return;
            }
            Ok(false) => {}
            Err(_) => {
                eprintln!(
                    "arinn-platform-x11: xfce4-power-manager's power-key grab could not be checked; retrying"
                );
                return;
            }
        }
        let Some(bus) = self.bus.clone() else {
            if !std::mem::replace(&mut self.no_bus_logged, true) {
                eprintln!(
                    "arinn-platform-x11: no session bus to restart xfce4-power-manager; retrying"
                );
            }
            return;
        };
        self.heal = false;
        // Off the adapter's loop (it can take seconds), with logind's
        // inhibitor held until the new instance owns its name: Arinn's own,
        // which `end` then no longer drops, or a temporary one at a start
        // or a retry. No key is taken meanwhile; while the name has no
        // owner (the new instance starts slowly) that lasts up to
        // `heal_every`, so a late instance grabs the key itself (review of
        // #521). `reap_restart` reads the outcome.
        let ours = self.inhibitor.take();
        let (system, address) = (self.system.clone(), self.system_address.clone());
        let (times, marker) = (self.restart, self.marker.clone());
        let restart = tokio::spawn(async move {
            let inhibitor = match ours {
                Some(inhibitor) => Some(inhibitor),
                None => match system {
                    Some(system) => power_inhibitor::inhibit(&system).await,
                    None => match open_system(address.as_deref()).await {
                        Some(system) => power_inhibitor::inhibit(&system).await,
                        None => None,
                    },
                },
            };
            // The inhibitor first, as everywhere (ADR-0111): without one
            // nothing is restarted, and the backoff tries again (Codex on
            // #521).
            let Some(_inhibitor) = inhibitor else {
                eprintln!(
                    "arinn-platform-x11: logind power-key inhibitor unavailable; xfce4-power-manager's restart waits"
                );
                return Restarted::Unchanged;
            };
            let restarted = restart_power_manager(&bus, times).await;
            if restarted == Restarted::Confirmed {
                marker.clear();
            }
            restarted
        });
        self.restarting = Some(restart);
    }
    /// The repair is confirmed: the marker goes, and so do the backoff and
    /// the count of missing instances.
    fn repaired(&mut self) {
        self.heal = false;
        self.heal_at = None;
        self.unconfirmed = 0;
        self.marker.clear();
    }
    /// A finished restart whose new instance was not seen arms the repair
    /// again (Codex on #517, #519). Only a restart after which the old
    /// instance went away counts: after `RESTARTS` of them in a row the
    /// adapter gives up and leaves the key to the desktop. One that changed
    /// nothing (a failed `Restart`, the old instance still there) is retried
    /// on the backoff without counting, so an instance whose grab is broken
    /// is not left alone (review of #521).
    fn reap_restart(&mut self) {
        if !self.restarting.as_ref().is_some_and(|t| t.is_finished()) {
            return;
        }
        let task = self.restarting.take().expect("checked");
        let restarted = futures_util::FutureExt::now_or_never(task)
            .and_then(Result::ok)
            .unwrap_or(Restarted::Unchanged);
        match restarted {
            Restarted::Confirmed => self.repaired(),
            // Nothing ran to restart: the next pass sees what runs now.
            Restarted::Gone => self.heal = true,
            Restarted::Unchanged => {
                eprintln!(
                    "arinn-platform-x11: xfce4-power-manager's restart changed nothing; retrying"
                );
                self.heal = true;
            }
            Restarted::Missing => {
                self.unconfirmed += 1;
                if self.unconfirmed >= RESTARTS {
                    eprintln!(
                        "arinn-platform-x11: xfce4-power-manager did not come back after {RESTARTS} restarts; the power key stays the desktop's until the adapter restarts"
                    );
                    self.gave_up = true;
                } else {
                    eprintln!(
                        "arinn-platform-x11: xfce4-power-manager did not come back after its restart; retrying"
                    );
                    self.heal = true;
                }
            }
        }
    }
    /// Gives a stale copy back, off the loop, until it succeeds: every
    /// `stale_every`, while Arinn does not hold the key (no take runs
    /// meanwhile). An ordinary XFCE login's restore entry leaves the copy to
    /// the running adapter, so it must not stay behind (Codex on #517).
    fn retry_stale(&mut self) {
        if let Some(task) = &self.stale_retry {
            if !task.is_finished() {
                return;
            }
            let task = self.stale_retry.take().expect("checked");
            let (done, system) = futures_util::FutureExt::now_or_never(task)
                .and_then(Result::ok)
                .unwrap_or((false, None));
            if self.system.is_none() {
                self.system = system;
            }
            if done {
                self.stale = None;
                self.stale_inhibitor = None;
            }
        }
        if self.owned != Reading::None || self.stale_at.is_some_and(|at| Instant::now() < at) {
            return;
        }
        let Some((bus, files)) = self.stale.clone() else {
            return;
        };
        self.stale_at = Some(Instant::now() + self.stale_every);
        // The system bus is opened again in each retry while there is none,
        // so a failed connection at the start does not leave every retry
        // without its inhibitor (#519). The owner keeps one that opens.
        let (system, address) = (self.system.clone(), self.system_address.clone());
        self.stale_retry = Some(tokio::spawn(async move {
            let system = match system {
                Some(system) => Some(system),
                None => open_system(address.as_deref()).await,
            };
            let done = release_stale(&bus, &files, system.as_ref()).await;
            (done, system)
        }));
    }
    /// Waits for a stale retry (tests).
    #[cfg(test)]
    pub async fn stale_retried(&mut self) {
        if let Some(task) = &mut self.stale_retry {
            while !task.is_finished() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    }
    /// Waits for a restart `heal` started, and reads its outcome (tests).
    #[cfg(test)]
    pub async fn restarted(&mut self) {
        if let Some(restart) = &self.restarting {
            while !restart.is_finished() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
        self.reap_restart();
    }
    /// The reader is gone (the adapter's session ended): the desktop's
    /// setting and the inhibitor go back. A setting xfconf did not take
    /// back stays pending with the inhibitor and is retried like a stale
    /// copy until it is back (Codex on #517).
    pub async fn end(&mut self) {
        if std::mem::take(&mut self.taken_xfconf)
            && let Some((bus, files)) = self.xfconf.as_ref()
            && !xfce_power::release(bus, files).await
        {
            eprintln!(
                "arinn-platform-x11: the XFCE power button setting could not be restored; retrying"
            );
            self.stale = Some((bus.clone(), files.clone()));
            self.stale_at = None;
            self.stale_inhibitor = self.inhibitor.take();
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
        /// The grab check fails (no X answer).
        pub(crate) held_fails: bool,
        /// `arm` was called, with xfconf's value at that moment.
        pub(crate) armed: Option<Option<xfce_power::Saved>>,
        /// xfconf, to see its value when `arm` is called.
        pub(crate) xfconf: Option<Arc<dyn Fn() -> Option<xfce_power::Saved> + Send + Sync>>,
    }
    impl Reader for FakeReader {
        fn set(&mut self, enabled: bool, raw: bool) -> Result<Reading> {
            self.calls.push((enabled, raw));
            Ok(if enabled { self.answer } else { Reading::None })
        }
        fn held_elsewhere(&mut self) -> Result<bool> {
            anyhow::ensure!(!self.held_fails, "fixture");
            Ok(self.held)
        }
        fn arm(&mut self) -> Result<()> {
            self.armed = Some(self.xfconf.as_ref().and_then(|value| value()));
            Ok(())
        }
    }
    pub(crate) fn reader(answer: Reading) -> FakeReader {
        FakeReader {
            answer,
            calls: Vec::new(),
            held: true,
            held_fails: false,
            armed: None,
            xfconf: None,
        }
    }

    /// This login's logind session id in the fixture.
    const OWN: &str = "c1";
    /// A logind session's `Type` and `State`.
    struct Session(String, String);
    #[zbus::interface(name = "org.freedesktop.login1.Session")]
    impl Session {
        #[zbus(property, name = "Type")]
        fn kind(&self) -> String {
            self.0.clone()
        }
        #[zbus(property, name = "State")]
        fn state(&self) -> String {
            self.1.clone()
        }
    }

    /// xfce4-power-manager's `Restart`; records how many logind inhibitors
    /// Arinn held at each call.
    struct Xfpm {
        login: logind::Shared,
        restarts: Arc<Mutex<Vec<usize>>>,
        /// What a restart does.
        after: Arc<Mutex<After>>,
        /// How long `Restart` takes to answer; its effect comes with it.
        reply: Arc<Mutex<Duration>>,
        /// The bus, to start the new instance on.
        address: String,
        next: Arc<Mutex<Vec<Connection>>>,
    }
    /// What a restart of the fake xfce4-power-manager does.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum After {
        /// The old instance stays: the restart never comes back.
        Stays,
        /// The old instance quits and a new one owns the name at once.
        ComesBack,
        /// The old instance quits; a new one owns the name after this long.
        Late(Duration),
        /// The old instance quits and no new one comes.
        Quits,
    }
    #[zbus::interface(name = "org.xfce.Power.Manager")]
    impl Xfpm {
        /// As xfpm_manager_dbus_restart: the old instance quits and a new
        /// one owns the name.
        async fn restart(&self, #[zbus(connection)] connection: &Connection) {
            let open = self.login.lock().unwrap().open();
            self.restarts.lock().unwrap().push(open);
            let after = *self.after.lock().unwrap();
            let reply = *self.reply.lock().unwrap();
            tokio::time::sleep(reply).await;
            if after != After::Stays {
                let _ = connection.release_name(POWER_MANAGER).await;
            }
            let delay = match after {
                After::Stays | After::Quits => return,
                After::ComesBack => Duration::ZERO,
                After::Late(delay) => delay,
            };
            let (address, next) = (self.address.clone(), self.next.clone());
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                let instance = logind::connect(&address).await;
                instance.request_name(POWER_MANAGER).await.unwrap();
                next.lock().unwrap().push(instance);
            });
        }
    }

    pub(crate) struct Fixture {
        pub(crate) xfconf: xfconf::Fixture,
        pub(crate) login: logind::Shared,
        _logind: Connection,
        pub(crate) owner: PowerOwner,
        /// Inhibitors Arinn held at each xfce4-power-manager restart.
        restarts: Arc<Mutex<Vec<usize>>>,
        /// What a restart of xfce4-power-manager does.
        after: Arc<Mutex<After>>,
        /// How long its `Restart` takes to answer.
        reply: Arc<Mutex<Duration>>,
        /// The instances restarts started.
        next: Arc<Mutex<Vec<Connection>>>,
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
        owner.restart.wait = Duration::from_millis(50);
        owner.restart.absent = Duration::from_millis(100);
        // This login's logind session; the fake lists no other.
        owner.own_session = Some(OWN.into());
        let run = xfconf
            .files
            .record
            .parent()
            .unwrap()
            .join("run/arinn/power-grab");
        let owner = owner.with_marker(GrabMarker::at(Some(run), ":9".into()));
        Fixture {
            xfconf,
            login,
            _logind: service,
            owner,
            restarts: Arc::default(),
            after: Arc::new(Mutex::new(After::ComesBack)),
            reply: Arc::default(),
            next: Arc::default(),
            _xfpm: None,
        }
    }
    impl Fixture {
        /// logind lists session `id` of user `uid`, of `kind` in `state`.
        async fn add_session(&mut self, id: &str, uid: u32, kind: &str, state: &str) {
            let path = format!("/org/freedesktop/login1/session/_{id}");
            self._logind
                .object_server()
                .at(path.as_str(), Session(kind.into(), state.into()))
                .await
                .unwrap();
            self.login.lock().unwrap().sessions.push((
                id.into(),
                uid,
                "user".into(),
                "seat0".into(),
                zbus::zvariant::OwnedObjectPath::try_from(path).unwrap(),
            ));
        }
        fn remove_session(&mut self, id: &str) {
            self.login.lock().unwrap().sessions.retain(|s| s.0 != id);
        }
        /// xfce4-power-manager starts: it owns its name and answers
        /// `Restart`.
        async fn start_xfpm(&mut self) {
            self.start_xfpm_with(true).await;
        }
        /// `comes_back`: a restart brings a new instance up.
        async fn start_xfpm_with(&mut self, comes_back: bool) {
            self.set_comes_back(comes_back);
            let xfpm = logind::connect(&self.xfconf.bus.1).await;
            xfpm.object_server()
                .at(
                    POWER_MANAGER_PATH,
                    Xfpm {
                        login: self.login.clone(),
                        restarts: self.restarts.clone(),
                        after: self.after.clone(),
                        reply: self.reply.clone(),
                        address: self.xfconf.bus.1.clone(),
                        next: self.next.clone(),
                    },
                )
                .await
                .unwrap();
            xfpm.request_name(POWER_MANAGER).await.unwrap();
            self._xfpm = Some(xfpm);
        }
        /// Whether a restart brings a new instance up from now on.
        fn set_comes_back(&self, comes_back: bool) {
            self.set_after(if comes_back {
                After::ComesBack
            } else {
                After::Stays
            });
        }
        fn set_after(&self, after: After) {
            *self.after.lock().unwrap() = after;
        }
        /// Every xfce4-power-manager instance quits (the user stops it).
        fn stop_xfpm(&mut self) {
            self._xfpm = None;
            self.next.lock().unwrap().clear();
        }
        /// The restarts so far, once a running one has finished.
        async fn restarts(&mut self) -> Vec<usize> {
            self.owner.restarted().await;
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

    /// Mutation B3 of the #517 review: a bus that does not answer counts
    /// as XFCE, so the key stays the desktop's.
    #[tokio::test]
    async fn an_unanswered_bus_counts_as_xfce() {
        let bus = logind::bus();
        let adapter = logind::connect(&bus.1).await;
        let dbus = zbus::fdo::DBusProxy::new(&adapter).await.unwrap();
        drop(bus);
        assert_eq!(xfce_on_bus(&dbus).await, XFCE);
        assert!(!allowed(false, xfce_on_bus(&dbus).await.any()));
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

    /// Codex on #517: raw presses count only once xfconf holds "do
    /// nothing"; a failed take arms nothing.
    #[tokio::test]
    async fn raw_presses_count_only_after_the_take() {
        let mut f = fixture(true).await;
        let mut r = reader(Reading::Raw);
        let value = f.xfconf.value_fn();
        r.xfconf = Some(value);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(r.armed, Some(Some(xfce_power::Saved::Uint(0))));
        let mut f = fixture(true).await;
        f.xfconf.lock();
        let mut r = reader(Reading::Raw);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(r.armed, None);
        assert_eq!(f.owner.owned(), Reading::None);
    }

    /// Review of #516: a stale dedicated marker on a desktop without XFCE
    /// (the manual recipe on LXQt, MATE or Openbox after a killed wrapper)
    /// does not count: the adapter grabs as on any other desktop, so that
    /// desktop's handler never acts on the same press, and xfconf is never
    /// touched there.
    #[tokio::test]
    async fn a_stale_dedicated_marker_without_xfce_grabs() {
        let mut f = fixture(true).await;
        let mut r = reader(Reading::Grab);
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert_eq!(r.calls, [(true, false)]);
        assert_eq!(f.owner.owned(), Reading::Grab);
        assert_eq!(f.xfconf.writes(), 0);
        assert!(!f.xfconf.files.entry.exists());
    }

    #[test]
    fn the_grab_marker_is_named_per_display() {
        assert_eq!(GrabMarker::file_name(":0"), "power-grab-_0");
        assert_eq!(GrabMarker::file_name("host:10.0"), "power-grab-host_10.0");
        assert_eq!(GrabMarker::file_name("../x"), "power-grab-.._x");
        assert_ne!(GrabMarker::file_name(":0"), GrabMarker::file_name(":1"));
    }

    /// Review of #516 (Codex P1): xfconfd is per user, so a second,
    /// ordinary XFCE session of the same user (another VT) would get "do
    /// nothing" as well while Arinn is on another display. The dedicated
    /// session takes the key only while logind lists no other graphical
    /// session of the user; one that appears gives everything back, and the
    /// key is taken again when it is gone.
    #[tokio::test]
    async fn another_graphical_session_of_the_user_keeps_the_desktop_key() {
        let mut f = fixture(true).await;
        // SAFETY: getuid has no preconditions.
        let uid = unsafe { libc::getuid() };
        f.add_session(OWN, uid, "x11", "active").await;
        // Not counted: another user's session, a closing one, a tty.
        f.add_session("c7", uid + 1, "x11", "active").await;
        f.add_session("c8", uid, "x11", "closing").await;
        f.add_session("c9", uid, "tty", "online").await;
        let mut r = reader(Reading::Raw);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Raw);
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(0)));
        // An ordinary XFCE login of the same user on another VT.
        f.add_session("c2", uid, "x11", "online").await;
        f.owner
            .follow(&mut r, true, XFCE, Reading::Raw)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        // A Wayland session counts too.
        f.remove_session("c2");
        f.add_session("c3", uid, "wayland", "active").await;
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        // Gone: taken again.
        f.remove_session("c3");
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Raw);
    }

    /// An unanswered logind query, or no session id for this login, counts
    /// as another session: nothing is taken. Other desktops do not ask.
    #[tokio::test]
    async fn the_dedicated_session_needs_logind_to_answer() {
        let mut f = fixture(true).await;
        f.login.lock().unwrap().no_sessions = true;
        let mut r = reader(Reading::Raw);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert!(r.calls.is_empty());
        assert_eq!(f.xfconf.writes(), 0);
        let mut f = fixture(true).await;
        f.owner.own_session = None;
        let mut r = reader(Reading::Raw);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert!(r.calls.is_empty());
        let mut f = fixture(false).await;
        f.login.lock().unwrap().no_sessions = true;
        let mut r = reader(Reading::Grab);
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Grab);
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
        assert!(f.owner.marker.present());
        f.start_xfpm().await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::Grab)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        assert_eq!(f.restarts().await, [1]);
        // Arinn's own inhibitor covered the restart: no gap, no second one.
        assert_eq!(f.login.lock().unwrap().calls.len(), 1);
        f.settle(0).await;
        // Once is enough: no restart while Arinn does not grab again.
        f.owner
            .follow(&mut r, true, XFPM, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.restarts().await, [1]);
    }

    /// Review of #517 (Codex P1, the reviewer's probe): while a restart is
    /// in flight, the old instance's name is gone for a moment. Arinn must
    /// not grab again then, or the new instance's grab fails as well.
    #[tokio::test]
    async fn no_grab_while_a_restart_is_in_flight() {
        let mut f = fixture(false).await;
        f.owner.restart.wait = Duration::from_millis(300);
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        f.start_xfpm().await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::Grab)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        // The old instance quit: no XFCE name on the next pass.
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert_eq!(
            f.owner.owned(),
            Reading::None,
            "re-grabbed during the restart"
        );
        assert_eq!(f.restarts().await, [1]);
        // Once it is done, a desktop without xfce4-power-manager grabs.
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Grab);
    }

    /// Codex on #517: the start-up restore of a stale copy runs under a
    /// temporary inhibitor.
    #[tokio::test]
    async fn the_start_up_restore_holds_an_inhibitor() {
        let f = fixture(true).await;
        assert!(xfce_power::take(&f.xfconf.adapter, &f.xfconf.files).await);
        let system = logind::connect(&f.xfconf.bus.1).await;
        release_stale(&f.xfconf.adapter, &f.xfconf.files, Some(&system)).await;
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
        assert_eq!(f.login.lock().unwrap().calls.len(), 1);
        f.settle(0).await;
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
        assert!(f.restarts().await.is_empty());
        // Mutation H1 of the #517 review: a user who stopped
        // xfce4-power-manager (the adapter sees no name when it lets go) is
        // never restarted, even though something still answers here.
        let mut f = fixture(false).await;
        f.start_xfpm().await;
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
        assert!(f.restarts().await.is_empty());
        f.owner
            .follow(&mut r, true, XFPM, Reading::None)
            .await
            .unwrap();
        assert!(f.restarts().await.is_empty());
    }

    /// Codex on #517: `--release-power-key` runs after the adapter's
    /// inhibitor is gone; it holds a temporary one while it gives the XFCE
    /// setting back and restarts xfce4-power-manager, and only for this
    /// display's grab.
    #[tokio::test]
    async fn the_release_after_the_adapter_holds_an_inhibitor() {
        let mut f = fixture(true).await;
        let mut r = reader(Reading::Raw);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(0)));
        // The adapter crashed after grabbing on another desktop earlier.
        f.owner.marker.set();
        f.owner.inhibitor = None;
        f.settle(0).await;
        f.start_xfpm().await;
        let system = logind::connect(&f.xfconf.bus.1).await;
        release_after_adapter(
            &f.xfconf.adapter,
            Some(&f.xfconf.files),
            &f.owner.marker,
            || Some(false),
            Some(&system),
            Duration::from_millis(50),
        )
        .await;
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
        assert_eq!(f.restarts.lock().unwrap().clone(), [1]);
        assert!(!f.owner.marker.present());
        f.settle(0).await;
        // Without this display's marker nothing is restarted.
        release_after_adapter(
            &f.xfconf.adapter,
            None,
            &f.owner.marker,
            || Some(false),
            Some(&system),
            Duration::from_millis(50),
        )
        .await;
        assert_eq!(f.restarts.lock().unwrap().len(), 1);
    }

    /// The same in the adapter: a restart whose new instance never comes
    /// keeps the marker for a later start; a confirmed one clears it.
    #[tokio::test]
    async fn the_adapter_keeps_the_marker_until_the_restart_is_confirmed() {
        for comes_back in [false, true] {
            let mut f = fixture(false).await;
            let mut r = reader(Reading::Grab);
            r.held = false;
            f.owner
                .follow(&mut r, true, OTHER, Reading::None)
                .await
                .unwrap();
            f.start_xfpm_with(comes_back).await;
            f.owner
                .follow(&mut r, true, XFPM, Reading::Grab)
                .await
                .unwrap();
            assert_eq!(f.restarts().await.len(), 1);
            assert_eq!(f.owner.marker.present(), !comes_back, "{comes_back}");
        }
    }

    /// Codex on #517: the stop helper keeps the grab marker unless the
    /// repair is confirmed (a restart whose new instance owns the name), so
    /// a later start tries again.
    #[tokio::test]
    async fn an_unconfirmed_repair_keeps_the_marker() {
        let mut f = fixture(false).await;
        f.owner.marker.set();
        f.start_xfpm_with(false).await;
        let system = logind::connect(&f.xfconf.bus.1).await;
        release_after_adapter(
            &f.xfconf.adapter,
            None,
            &f.owner.marker,
            || Some(false),
            Some(&system),
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(f.restarts.lock().unwrap().len(), 1);
        assert!(f.owner.marker.present());
        // The old instance quits and no new one comes: kept as well.
        f.set_after(After::Quits);
        release_after_adapter(
            &f.xfconf.adapter,
            None,
            &f.owner.marker,
            || Some(false),
            Some(&system),
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(f.restarts.lock().unwrap().len(), 2);
        assert!(f.owner.marker.present());
        f.start_xfpm_with(false).await;
        // Unknown whether a key is grabbed (no display): kept as well.
        release_after_adapter(
            &f.xfconf.adapter,
            None,
            &f.owner.marker,
            || None,
            Some(&system),
            Duration::from_millis(100),
        )
        .await;
        assert!(f.owner.marker.present());
        // Its grab is held: nothing to repair, the marker goes.
        release_after_adapter(
            &f.xfconf.adapter,
            None,
            &f.owner.marker,
            || Some(true),
            Some(&system),
            Duration::from_millis(100),
        )
        .await;
        assert!(!f.owner.marker.present());
    }

    /// Codex on #517, #519: a restart whose new instance never owns the
    /// name is tried again by the running adapter, never sooner than
    /// `heal_every`, each time under an inhibitor, until one is confirmed.
    #[tokio::test]
    async fn an_unconfirmed_restart_is_retried_until_confirmed() {
        let mut f = fixture(false).await;
        f.owner.heal_every = Duration::from_millis(300);
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        f.start_xfpm_with(false).await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::Grab)
            .await
            .unwrap();
        assert_eq!(f.restarts().await, [1]);
        assert!(f.owner.marker.present());
        f.settle(0).await;
        // Not again before the backoff.
        f.owner
            .follow(&mut r, true, XFPM, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.restarts().await, [1]);
        // After it, again, under a temporary inhibitor; this one comes back.
        f.set_comes_back(true);
        tokio::time::sleep(Duration::from_millis(350)).await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.restarts().await, [1, 1]);
        assert!(!f.owner.marker.present());
        f.settle(0).await;
        // Confirmed: nothing more.
        tokio::time::sleep(Duration::from_millis(350)).await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.restarts().await.len(), 2);
    }

    /// #519: a grab check that fails when Arinn lets go is tried again
    /// after the backoff, and the restart it finds needed then runs.
    #[tokio::test]
    async fn a_failed_grab_check_is_retried() {
        let mut f = fixture(false).await;
        f.owner.heal_every = Duration::from_millis(300);
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        f.start_xfpm().await;
        r.held_fails = true;
        f.owner
            .follow(&mut r, true, XFPM, Reading::Grab)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        assert!(f.restarts().await.is_empty());
        assert!(f.owner.marker.present());
        r.held_fails = false;
        f.owner
            .follow(&mut r, true, XFPM, Reading::None)
            .await
            .unwrap();
        assert!(f.restarts().await.is_empty(), "retried before the backoff");
        tokio::time::sleep(Duration::from_millis(350)).await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.restarts().await, [1]);
        assert!(!f.owner.marker.present());
    }

    /// #519: an unconfirmed repair is not retried once xfce4-power-manager
    /// is gone (the user stopped it), nor when it starts again later: a
    /// new instance grabs the key itself.
    #[tokio::test]
    async fn a_stopped_power_manager_is_never_restarted() {
        let mut f = fixture(false).await;
        f.owner.heal_every = Duration::from_millis(100);
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        f.start_xfpm_with(false).await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::Grab)
            .await
            .unwrap();
        assert_eq!(f.restarts().await, [1]);
        f._xfpm = None;
        tokio::time::sleep(Duration::from_millis(150)).await;
        f.owner
            .follow(&mut r, false, OTHER, Reading::None)
            .await
            .unwrap();
        assert!(!f.owner.marker.present());
        f.start_xfpm_with(false).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        f.owner
            .follow(&mut r, false, XFPM, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.restarts().await, [1]);
    }

    /// Review of #521 (its reproduction): `Restart` works, but the new
    /// instance owns its name only after `restart.wait`. Arinn takes no key
    /// meanwhile and keeps logind's inhibitor, so the late instance grabs
    /// the key itself and the repair is confirmed: no restart loop.
    #[tokio::test]
    async fn a_slow_new_instance_is_waited_for() {
        let mut f = fixture(false).await;
        f.owner.heal_every = Duration::from_millis(1000);
        f.owner.restart.absent = Duration::from_millis(1000);
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        f.start_xfpm().await;
        f.set_after(After::Late(Duration::from_millis(400)));
        f.owner
            .follow(&mut r, true, XFPM, Reading::Grab)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        // Past `restart.wait`: the old instance is gone, the new one is not
        // there yet.
        tokio::time::sleep(Duration::from_millis(120)).await;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None, "grabbed in the gap");
        assert_eq!(f.open(), 1, "no inhibitor in the gap");
        assert_eq!(f.restarts().await, [1]);
        assert!(!f.owner.marker.present());
        assert_eq!(f.owner.unconfirmed, 0);
        f.settle(0).await;
        // The late instance holds its grab: nothing more.
        tokio::time::sleep(Duration::from_millis(1050)).await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.restarts().await.len(), 1);
    }

    /// Review of #521: a new instance that comes even later than
    /// `restart.absent` loses its grab to Arinn's again; after `RESTARTS`
    /// such restarts in a row the adapter restarts nothing more and
    /// grabs no key, so the desktop keeps it, and the marker stays for the
    /// next start.
    #[tokio::test]
    async fn unconfirmed_restarts_stop_after_the_cap() {
        let mut f = fixture(false).await;
        f.owner.heal_every = Duration::from_millis(200);
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Grab);
        f.start_xfpm().await;
        f.set_after(After::Quits);
        for round in 1..=RESTARTS as usize {
            f.owner
                .follow(&mut r, true, XFPM, Reading::Grab)
                .await
                .unwrap();
            assert_eq!(f.owner.owned(), Reading::None);
            if f.restarts().await.len() < round {
                tokio::time::sleep(Duration::from_millis(250)).await;
                f.owner
                    .follow(&mut r, true, XFPM, Reading::None)
                    .await
                    .unwrap();
            }
            assert_eq!(f.restarts().await.len(), round);
            // The old instance is gone, the new one is not there yet.
            f.stop_xfpm();
            f.owner
                .follow(&mut r, true, OTHER, Reading::None)
                .await
                .unwrap();
            let grab = round < RESTARTS as usize;
            let expected = if grab { Reading::Grab } else { Reading::None };
            assert_eq!(f.owner.owned(), expected, "round {round}");
            // The late instance; its grab fails if Arinn grabbed.
            f.start_xfpm().await;
            f.set_after(After::Quits);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
        for xfce in [XFPM, OTHER, XFPM] {
            f.owner
                .follow(&mut r, true, xfce, Reading::None)
                .await
                .unwrap();
            assert_eq!(f.owner.owned(), Reading::None);
        }
        assert_eq!(f.restarts().await.len(), RESTARTS as usize);
        assert!(f.owner.marker.present());
    }

    /// Review of #521 (ADR-0111: at most 15 s): a restart whose new
    /// instance never comes holds logind's inhibitor no longer than
    /// `restart.absent`, the call included, not until the backoff ends.
    #[tokio::test]
    async fn a_missing_instance_holds_the_inhibitor_at_most_the_absent_bound() {
        let mut f = fixture(false).await;
        f.owner.heal_every = Duration::from_secs(10);
        f.owner.restart.absent = Duration::from_millis(200);
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        f.start_xfpm().await;
        f.set_after(After::Quits);
        f.owner
            .follow(&mut r, true, XFPM, Reading::Grab)
            .await
            .unwrap();
        assert_eq!(f.open(), 1);
        tokio::time::sleep(Duration::from_millis(450)).await;
        assert!(
            f.owner.restarting.as_ref().is_some_and(|t| t.is_finished()),
            "still waiting"
        );
        f.settle(0).await;
        assert_eq!(f.restarts().await, [1]);
        assert_eq!(f.owner.unconfirmed, 1);
    }

    /// Review of #521: a restart that changes nothing (the old instance
    /// stays) does not count toward `RESTARTS`: it is retried on the
    /// backoff for as long as it takes, so an instance whose grab is broken
    /// is never left alone.
    #[tokio::test]
    async fn a_restart_that_changes_nothing_is_retried_without_counting() {
        let mut f = fixture(false).await;
        f.owner.heal_every = Duration::from_millis(100);
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        f.start_xfpm_with(false).await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::Grab)
            .await
            .unwrap();
        assert_eq!(f.restarts().await.len(), 1);
        for round in 2..=RESTARTS as usize + 2 {
            tokio::time::sleep(Duration::from_millis(130)).await;
            f.owner
                .follow(&mut r, true, XFPM, Reading::None)
                .await
                .unwrap();
            assert_eq!(f.restarts().await.len(), round);
        }
        assert!(!f.owner.gave_up);
        assert_eq!(f.owner.unconfirmed, 0);
        assert!(f.owner.marker.present());
        // It answers at last: confirmed.
        f.set_comes_back(true);
        tokio::time::sleep(Duration::from_millis(130)).await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.restarts().await.len(), RESTARTS as usize + 3);
        assert!(!f.owner.marker.present());
    }

    /// Review of #521: a confirmed repair starts the count again, so only
    /// `RESTARTS` missing instances in a row give up.
    #[tokio::test]
    async fn a_confirmed_repair_resets_the_count() {
        let mut f = fixture(false).await;
        f.owner.heal_every = Duration::from_millis(100);
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        f.start_xfpm().await;
        f.set_after(After::Quits);
        // One restart whose instance never comes; then Arinn grabs in the
        // gap and the late instance loses its grab.
        async fn missing(f: &mut Fixture, r: &mut FakeReader) {
            let before = f.restarts().await.len();
            f.owner.follow(r, true, XFPM, Reading::Grab).await.unwrap();
            if f.restarts().await.len() == before {
                tokio::time::sleep(Duration::from_millis(130)).await;
                f.owner.follow(r, true, XFPM, Reading::None).await.unwrap();
            }
            assert_eq!(f.restarts().await.len(), before + 1);
            f.stop_xfpm();
            f.owner.follow(r, true, OTHER, Reading::None).await.unwrap();
            assert_eq!(f.owner.owned(), Reading::Grab);
            f.start_xfpm().await;
            f.set_after(After::Quits);
        }
        for _ in 1..RESTARTS {
            missing(&mut f, &mut r).await;
        }
        assert_eq!(f.owner.unconfirmed, RESTARTS - 1);
        // A confirmed repair.
        f.set_after(After::ComesBack);
        tokio::time::sleep(Duration::from_millis(130)).await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::Grab)
            .await
            .unwrap();
        f.restarts().await;
        assert_eq!(f.owner.unconfirmed, 0);
        // Arinn takes the key again once it is gone.
        f.stop_xfpm();
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Grab);
        f.start_xfpm().await;
        f.set_after(After::Quits);
        for _ in 1..RESTARTS {
            missing(&mut f, &mut r).await;
        }
        assert!(!f.owner.gave_up);
        assert_eq!(f.owner.unconfirmed, RESTARTS - 1);
    }

    /// Review of #521 (its reproduction, scaled down): `Restart` answers
    /// only after the call's timeout, and takes effect then; the new
    /// instance comes shortly after. The name is watched as after an
    /// answered call, with no key taken: confirmed, not retried in a loop.
    /// When no new instance comes, it counts toward the cap.
    #[tokio::test]
    async fn a_restart_answered_too_late_is_still_watched() {
        for comes in [true, false] {
            let mut f = fixture(false).await;
            f.owner.heal_every = Duration::from_millis(1000);
            f.owner.restart = RestartTimes {
                call: Duration::from_millis(200),
                wait: Duration::from_millis(500),
                absent: Duration::from_millis(700),
            };
            let mut r = reader(Reading::Grab);
            r.held = false;
            f.owner
                .follow(&mut r, true, OTHER, Reading::None)
                .await
                .unwrap();
            f.start_xfpm().await;
            *f.reply.lock().unwrap() = Duration::from_millis(220);
            f.set_after(if comes {
                After::Late(Duration::from_millis(300))
            } else {
                After::Quits
            });
            f.owner
                .follow(&mut r, true, XFPM, Reading::Grab)
                .await
                .unwrap();
            // The call timed out; the old instance quits and the new one
            // is not there yet.
            tokio::time::sleep(Duration::from_millis(230)).await;
            f.owner
                .follow(&mut r, true, OTHER, Reading::None)
                .await
                .unwrap();
            assert_eq!(f.owner.owned(), Reading::None, "grabbed in the gap");
            assert_eq!(f.restarts().await, [1]);
            if comes {
                assert!(!f.owner.marker.present());
                assert_eq!(f.owner.unconfirmed, 0);
            } else {
                assert!(f.owner.marker.present());
                assert_eq!(f.owner.unconfirmed, 1);
            }
        }
    }

    /// Codex on #521: a restart needs logind's inhibitor first; without it
    /// nothing is restarted, and the backoff tries again.
    #[tokio::test]
    async fn no_restart_without_the_inhibitor() {
        let mut f = fixture(false).await;
        f.owner.heal_every = Duration::from_millis(100);
        // An earlier adapter of this display grabbed and crashed.
        f.owner.marker.set();
        f.owner.heal = true;
        f.start_xfpm().await;
        f.login.lock().unwrap().refuse = true;
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, false, XFPM, Reading::None)
            .await
            .unwrap();
        assert!(f.restarts().await.is_empty());
        assert!(f.owner.marker.present());
        assert_eq!(f.owner.unconfirmed, 0);
        f.login.lock().unwrap().refuse = false;
        tokio::time::sleep(Duration::from_millis(130)).await;
        f.owner
            .follow(&mut r, false, XFPM, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.restarts().await, [1]);
        assert!(!f.owner.marker.present());
    }

    /// Review of #521: `Restart` is never called while the name has no
    /// owner.
    #[tokio::test]
    async fn restart_is_not_called_without_an_owner() {
        let f = fixture(false).await;
        assert_eq!(
            restart_power_manager(&f.xfconf.adapter, RestartTimes::default()).await,
            Restarted::Gone
        );
    }

    /// Review of #521: when xfce4-power-manager's name has no owner any
    /// more at the restart, nothing is restarted and nothing counts as an
    /// unconfirmed restart; the next pass ends the repair.
    #[tokio::test]
    async fn no_restart_when_the_name_is_gone() {
        let mut f = fixture(false).await;
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        // The snapshot still names xfce4-power-manager; it is gone since.
        f.owner
            .follow(&mut r, true, XFPM, Reading::Grab)
            .await
            .unwrap();
        assert!(f.restarts().await.is_empty());
        assert_eq!(f.owner.unconfirmed, 0);
        f.owner
            .follow(&mut r, false, OTHER, Reading::None)
            .await
            .unwrap();
        assert!(!f.owner.marker.present());
    }

    /// Review of #521: a repair that ended resets the backoff, so a new
    /// grab's check after letting go runs at once.
    #[tokio::test]
    async fn a_new_grab_after_a_repair_is_checked_at_once() {
        let mut f = fixture(false).await;
        f.owner.heal_every = Duration::from_secs(10);
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        f.start_xfpm().await;
        f.owner
            .follow(&mut r, true, XFPM, Reading::Grab)
            .await
            .unwrap();
        assert_eq!(f.restarts().await, [1]);
        f.stop_xfpm();
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
        assert_eq!(f.restarts().await.len(), 2);
    }

    /// #519: a stale copy whose start-up restore found no system bus opens
    /// it again in its retry, so the retry holds its inhibitor; the owner
    /// keeps the bus.
    #[tokio::test]
    async fn a_stale_retry_opens_the_system_bus_again() {
        let mut f = fixture(false).await;
        f.owner.system = None;
        f.owner.system_address = Some(f.xfconf.bus.1.clone());
        assert!(xfce_power::take(&f.xfconf.adapter, &f.xfconf.files).await);
        f.xfconf.lock();
        let (bus, files) = (f.xfconf.adapter.clone(), f.xfconf.files.clone());
        // The start's connection failed.
        f.owner.restore_stale(&bus, files, None).await;
        assert!(f.owner.stale.is_some());
        assert!(f.login.lock().unwrap().calls.is_empty());
        f.xfconf.unlock();
        let mut r = reader(Reading::Grab);
        f.owner
            .follow(&mut r, false, OTHER, Reading::None)
            .await
            .unwrap();
        f.owner.stale_retried().await;
        assert_eq!(f.login.lock().unwrap().calls.len(), 1);
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
        f.owner
            .follow(&mut r, false, OTHER, Reading::None)
            .await
            .unwrap();
        assert!(f.owner.stale.is_none());
        assert!(f.owner.system.is_some());
        f.settle(0).await;
    }

    /// Codex on #517, #519: the system XDG autostart restore at an XFCE
    /// login gives a stale copy back under a temporary inhibitor, and asks
    /// logind nothing when there is no copy.
    #[tokio::test]
    async fn the_login_restore_holds_an_inhibitor() {
        let f = fixture(true).await;
        let system = f.xfconf.bus.1.clone();
        let (bus, files) = (&f.xfconf.adapter, &f.xfconf.files);
        assert!(restore_at_login(bus, files, Some(&system)).await);
        assert!(f.login.lock().unwrap().calls.is_empty());
        assert!(xfce_power::take(bus, files).await);
        assert!(restore_at_login(bus, files, Some(&system)).await);
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
        assert_eq!(f.login.lock().unwrap().calls.len(), 1);
        f.settle(0).await;
    }

    /// Codex on #517: a stale copy the start could not give back (here
    /// xfconf refused) is retried off the loop until it succeeds, since an
    /// ordinary XFCE login's restore entry leaves it to a running adapter.
    #[tokio::test]
    async fn a_stale_copy_the_start_could_not_restore_is_retried() {
        let mut f = fixture(false).await;
        assert!(xfce_power::take(&f.xfconf.adapter, &f.xfconf.files).await);
        f.xfconf.lock();
        let system = logind::connect(&f.xfconf.bus.1).await;
        assert!(!release_stale(&f.xfconf.adapter, &f.xfconf.files, Some(&system)).await);
        f.owner.stale = Some((f.xfconf.adapter.clone(), f.xfconf.files.clone()));
        f.owner.stale_every = Duration::from_millis(100);
        let mut r = reader(Reading::Grab);
        // Still refused: no take while the retry runs, and it stays stale.
        f.owner
            .follow(&mut r, false, OTHER, Reading::None)
            .await
            .unwrap();
        f.owner.stale_retried().await;
        f.owner
            .follow(&mut r, false, OTHER, Reading::None)
            .await
            .unwrap();
        assert!(f.owner.stale.is_some());
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(0)));
        // xfconf answers again: the next retry gives the user's value back.
        f.xfconf.unlock();
        tokio::time::sleep(Duration::from_millis(120)).await;
        f.owner
            .follow(&mut r, false, OTHER, Reading::None)
            .await
            .unwrap();
        f.owner.stale_retried().await;
        f.owner
            .follow(&mut r, false, OTHER, Reading::None)
            .await
            .unwrap();
        assert!(f.owner.stale.is_none());
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
    }

    /// Codex on #517: a release xfconf refuses keeps the copy pending with
    /// Arinn's inhibitor; the next passes retry it until the user's value
    /// is back, and only then does the inhibitor go.
    #[tokio::test]
    async fn a_release_xfconf_refused_is_retried() {
        let mut f = fixture(true).await;
        f.owner.stale_every = Duration::from_millis(100);
        let mut r = reader(Reading::Raw);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Raw);
        f.xfconf.lock();
        f.owner
            .follow(&mut r, false, XFCE, Reading::Raw)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(0)));
        assert!(f.owner.stale.is_some());
        assert_eq!(f.open(), 1);
        // xfconf answers again: the next pass gives the user's value back.
        f.xfconf.unlock();
        f.owner
            .follow(&mut r, false, XFCE, Reading::None)
            .await
            .unwrap();
        f.owner.stale_retried().await;
        f.owner
            .follow(&mut r, false, XFCE, Reading::None)
            .await
            .unwrap();
        assert!(f.owner.stale.is_none());
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
        f.settle(0).await;
    }

    /// Codex on #517: the owner keeps the system bus of the start-up
    /// restore, so a retry of a stale copy holds a temporary inhibitor too.
    #[tokio::test]
    async fn a_stale_retry_holds_an_inhibitor() {
        let mut f = fixture(false).await;
        let system = f.owner.system.take();
        assert!(xfce_power::take(&f.xfconf.adapter, &f.xfconf.files).await);
        f.xfconf.lock();
        let (bus, files) = (f.xfconf.adapter.clone(), f.xfconf.files.clone());
        f.owner.restore_stale(&bus, files, system).await;
        assert!(f.owner.stale.is_some());
        assert_eq!(f.login.lock().unwrap().calls.len(), 1);
        f.xfconf.unlock();
        let mut r = reader(Reading::Grab);
        f.owner
            .follow(&mut r, false, OTHER, Reading::None)
            .await
            .unwrap();
        f.owner.stale_retried().await;
        assert_eq!(f.login.lock().unwrap().calls.len(), 2);
        assert_eq!(f.xfconf.value(), Some(xfce_power::Saved::Uint(3)));
        f.settle(0).await;
    }

    /// No take while a stale retry runs, and no retry while Arinn holds
    /// the key.
    #[tokio::test]
    async fn a_stale_retry_and_a_take_never_overlap() {
        let mut f = fixture(false).await;
        f.owner.stale = Some((f.xfconf.adapter.clone(), f.xfconf.files.clone()));
        f.owner.stale_retry = Some(tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            (false, None)
        }));
        let mut r = reader(Reading::Grab);
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::None);
        assert!(r.calls.is_empty());
        f.owner.stale_retried().await;
        f.owner.stale = None;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert_eq!(f.owner.owned(), Reading::Grab);
        f.owner.stale = Some((f.xfconf.adapter.clone(), f.xfconf.files.clone()));
        f.owner.retry_stale();
        assert!(f.owner.stale_retry.is_none());
    }

    /// Review of #517: a crash loop restarts xfce4-power-manager at most
    /// once per grab. The adapter that grabbed crashes; the next start
    /// repairs xfce4-power-manager and clears the marker; the starts after
    /// it, which never grabbed, restart nothing.
    #[tokio::test]
    async fn a_crash_loop_restarts_xfce4_power_manager_once() {
        let mut f = fixture(false).await;
        f.start_xfpm().await;
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, OTHER, Reading::None)
            .await
            .unwrap();
        assert!(f.owner.marker.present());
        // Crash: no give-back; each restart of the adapter is a new owner
        // with the same display's marker.
        let marker = f.owner.marker.clone();
        for _ in 0..4 {
            f.owner = PowerOwner::new(
                false,
                Some(f.xfconf.adapter.clone()),
                None,
                f.owner.system.clone(),
            )
            .with_marker(marker.clone());
            f.owner.restart.wait = Duration::from_millis(50);
            f.owner
                .follow(&mut r, true, XFPM, Reading::None)
                .await
                .unwrap();
            f.owner.restarted().await;
        }
        assert_eq!(f.restarts().await.len(), 1);
        assert!(!marker.present());
    }

    /// Review of #516: without this display's grab marker an adapter start
    /// never restarts xfce4-power-manager, even when no power key is
    /// grabbed (the ordinary XFCE desktop, or no power key mapped).
    #[tokio::test]
    async fn without_a_grab_of_this_display_nothing_is_restarted() {
        let mut f = fixture(false).await;
        f.start_xfpm().await;
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert!(f.restarts().await.is_empty());
        // Another display's marker does not count either.
        let other = GrabMarker::at(f.owner.marker.path.clone(), ":3".into());
        other.set();
        assert!(!f.owner.marker.present());
    }

    /// A crashed adapter may have held the grab: the next adapter's first
    /// pass restarts an xfce4-power-manager whose grab is missing, and
    /// leaves one that holds it alone (the dedicated session, the ordinary
    /// XFCE desktop).
    #[tokio::test]
    async fn the_next_adapter_heals_a_power_manager_a_crash_left_without_its_grab() {
        let mut f = fixture(false).await;
        // An earlier adapter of this display grabbed and crashed.
        f.owner.marker.set();
        f.owner.heal = f.owner.marker.present();
        f.start_xfpm().await;
        let mut r = reader(Reading::Grab);
        r.held = false;
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        // A temporary inhibitor covers the restart at a start.
        assert_eq!(f.restarts().await, [1]);
        assert!(!f.owner.marker.present());
        let mut f = fixture(true).await;
        f.start_xfpm().await;
        let mut r = reader(Reading::Raw);
        f.owner
            .follow(&mut r, true, XFCE, Reading::None)
            .await
            .unwrap();
        assert!(f.restarts().await.is_empty());
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
