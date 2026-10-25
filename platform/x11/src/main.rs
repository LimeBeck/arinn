// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Qt key codes in this shared module serve the KDE adapter only.
mod background;
#[allow(dead_code)]
#[path = "../../key_sequence.rs"]
mod key_sequence;
mod keyboard;
mod keyboard_lane;
mod native;
#[path = "../../power_inhibitor.rs"]
mod power_inhibitor;
mod power_owner;
mod xfce_power;
use anyhow::{Result, ensure};
use arinn_protocol::{
    kwin::{self, KWinBridgeProxy},
    media_window::{self, KWinMediaProxy},
    production,
    return_association::KWinReturnProxy,
};
use native::{Native, Row, Snapshot};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinSet,
};
use uuid::Uuid;
use zbus::{Connection, proxy::Proxy};

// Existing internal daemon methods; no public provider contract is extended.
#[zbus::proxy(
    interface = "org.arinn.Daemon1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
trait HomeBridge {
    fn take_command(&self) -> zbus::Result<String>;
    fn report_home(&self, visible: bool, target: &str, output: &str) -> zbus::Result<()>;
    fn report_home_request(&self, target: &str, output: &str) -> zbus::Result<()>;
    fn report_quick_menu_request(&self, target: &str, output: &str) -> zbus::Result<()>;
    fn report_power_menu_request(&self, target: &str, output: &str) -> zbus::Result<()>;
    fn report_window(
        &self,
        id: &str,
        class: &str,
        fullscreen: bool,
        output: &str,
    ) -> zbus::Result<()>;
}
#[derive(Clone)]
struct Presentation {
    shell: u32,
    shell_token: String,
    token: String,
    launch: String,
    saved: Vec<u32>,
}
struct Actor {
    native: Native,
    snapshot: Snapshot,
    home: bool,
    target: String,
    presentation: Option<Presentation>,
    media_allowed: bool,
    /// A quick-menu press raises the shell only after its request is
    /// published, so the shell never has focus before it learns the request
    /// came from over an Activity (ADR-0106, review of #502).
    raise_after_publish: bool,
}
/// The global key behind an explicit presentation request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Request {
    Home,
    /// ADR-0106: the same Home state, presented as the quick menu.
    QuickMenu,
    /// ADR-0111: the same Home state, with the power menu open.
    PowerMenu,
}
#[derive(Clone)]
struct Update {
    snapshot: Snapshot,
    home: bool,
    target: String,
    explicit: Option<Request>,
}
#[derive(Clone)]
enum Command {
    Home(String),
    Window(Vec<String>),
    Media(Vec<String>),
}
struct Pending {
    command: Command,
    done: oneshot::Sender<&'static str>,
}
impl Actor {
    fn connect() -> Result<Self> {
        let (home, quick_menu) = shortcuts();
        let mut native = Native::connect(&home, &quick_menu)?;
        // The power key waits for a live shell (ADR-0111, `PowerOwner`).
        native.set_power(false, false)?;
        let snapshot = native.snapshot()?;
        Ok(Self {
            native,
            snapshot,
            home: false,
            target: String::new(),
            presentation: None,
            media_allowed: false,
            raise_after_publish: false,
        })
    }
    fn row(&self, token: &str) -> Option<&Row> {
        self.snapshot.rows.iter().find(|r| r.token == token)
    }
    fn update(&self, explicit: Option<Request>) -> Update {
        let mut snapshot = self.snapshot.clone();
        if !self.media_allowed {
            for row in &mut snapshot.rows {
                row.launch.clear();
            }
        }
        Update {
            snapshot,
            home: self.home,
            target: self.target.clone(),
            explicit,
        }
    }
    fn release(&mut self) -> &'static str {
        let Some(p) = self.presentation.take() else {
            return "";
        };
        if let Some(row) = self.row(&p.token).filter(|r| r.launch == p.launch)
            && self.native.release(row.xid, &p.saved, &p.launch).is_err()
        {
            self.presentation = Some(p);
            return "Unavailable";
        }
        ""
    }
    fn release_live(&mut self) -> Result<()> {
        let Some(p) = self.presentation.as_ref() else {
            return Ok(());
        };
        let same_player = if let Some(row) = self.row(&p.token) {
            self.native.has_launch(row.xid, &p.launch)?
        } else {
            false
        };
        if !same_player {
            self.presentation = None;
        } else if !self.release().is_empty() {
            eprintln!("arinn-platform-x11: presentation restoration failed");
            anyhow::bail!("presentation restoration failed");
        }
        Ok(())
    }
    fn cleanup(&mut self) -> Result<()> {
        // Restore from the saved row before inventory retirement loses its XID.
        let restored = self.release_live();
        // Lower the current shell even if player restoration failed; never use
        // stale shell identity when a fresh inventory cannot be obtained.
        let lowered = self.native.snapshot().and_then(|snapshot| {
            self.snapshot = snapshot;
            if let Some(shell) = self.snapshot.shell {
                self.native.raise_shell(shell, false)?;
            }
            Ok(())
        });
        restored.and(lowered)
    }
    fn present(&mut self, token: &str) -> &'static str {
        let Some(row) = self.row(token).cloned() else {
            return "NotFound";
        };
        if row.launch.is_empty() {
            return "Unsupported";
        }
        let Some(shell) = self.snapshot.shell else {
            return "Unavailable";
        };
        // A hidden (unlisted) shell has no lifetime and cannot be the input
        // layer over the video (ADR-0046).
        if self.snapshot.shell_token.is_empty() {
            return "Unavailable";
        }
        if !self.snapshot.compositor {
            return "Unsupported";
        }
        if self
            .presentation
            .as_ref()
            .is_some_and(|p| p.token == row.token && p.launch == row.launch)
        {
            if self.native.raise_shell(shell, true).is_err() {
                return "Unavailable";
            }
            self.home = false;
            self.target.clear();
            return "";
        }
        if !self.release().is_empty() {
            return "Unavailable";
        }
        match self.native.present(row.xid, shell, &row.launch) {
            Ok(attempt) => {
                self.presentation = Some(Presentation {
                    shell,
                    shell_token: self.snapshot.shell_token.clone(),
                    token: row.token,
                    launch: row.launch,
                    saved: attempt.saved,
                });
                if !attempt.succeeded {
                    let _ = self.release();
                    return "Unavailable";
                }
                self.home = false;
                self.target.clear();
                ""
            }
            Err(_) => "Unavailable",
        }
    }
    fn refresh(&mut self, unlocked: bool) -> Result<Update> {
        let snapshot = self.native.snapshot()?;
        if self.presentation.as_ref().is_some_and(|p| {
            !snapshot
                .rows
                .iter()
                .any(|r| r.token == p.token && r.launch == p.launch)
        }) {
            // Keep the previous row until restoration has used its exact XID.
            // Native release revalidates the launch identity before changing state.
            self.release_live()?;
            if !self.home
                && let Some(shell) = snapshot.shell
            {
                self.native.raise_shell(shell, false)?;
            }
        }
        self.snapshot = snapshot;
        self.media_allowed = unlocked && self.snapshot.compositor && self.snapshot.shell.is_some();
        if let Some(p) = self.presentation.clone() {
            let video = self
                .row(&p.token)
                .filter(|r| r.launch == p.launch)
                .map(|r| r.xid);
            let same_shell =
                self.snapshot.shell == Some(p.shell) && self.snapshot.shell_token == p.shell_token;
            let unrelated_focus = self
                .snapshot
                .active
                .is_some_and(|active| active != p.shell && Some(active) != video);
            if !same_shell || unrelated_focus {
                if !self.release().is_empty() {
                    eprintln!("arinn-platform-x11: presentation restoration failed");
                    anyhow::bail!("presentation restoration failed");
                }
                if same_shell {
                    self.native.raise_shell(p.shell, false)?;
                }
                self.home = false;
                self.target.clear();
            }
        }
        if !self.media_allowed {
            let fallback = self
                .presentation
                .as_ref()
                .and_then(|p| self.row(&p.token))
                .map(|r| r.xid);
            let had_presentation = self.presentation.is_some();
            if !self.release().is_empty() {
                eprintln!("arinn-platform-x11: presentation restoration failed");
                anyhow::bail!("presentation restoration failed");
            }
            if !unlocked || had_presentation {
                if let Some(shell) = self.snapshot.shell {
                    self.native.raise_shell(shell, false)?;
                }
                self.home = false;
                self.target.clear();
                if unlocked && let Some(xid) = fallback {
                    self.native.activate(xid)?;
                }
            }
        }
        let mut explicit = None;
        let key = if self.snapshot.home_key {
            Some(Request::Home)
        } else if self.snapshot.quick_menu_key {
            Some(Request::QuickMenu)
        } else if self.snapshot.power_key {
            Some(Request::PowerMenu)
        } else {
            None
        };
        if let Some(key) = key
            && unlocked
        {
            self.raise_after_publish = key == Request::QuickMenu;
            if self
                .command(&Command::Home("home".into()), unlocked)
                .is_empty()
            {
                explicit = Some(key);
            } else {
                self.raise_after_publish = false;
            }
        }
        if unlocked
            && !self.raise_after_publish
            && (self.home || self.presentation.is_some())
            && let Some(shell) = self.snapshot.shell
        {
            self.native.raise_shell(shell, true)?;
        }
        Ok(self.update(explicit))
    }
    /// Raises the shell held back for a quick-menu request, once the request
    /// is published.
    fn raise_deferred(&mut self) -> Result<()> {
        if !std::mem::take(&mut self.raise_after_publish) || !self.home {
            return Ok(());
        }
        if let Some(shell) = self.snapshot.shell {
            self.native.raise_shell(shell, true)?;
        }
        Ok(())
    }
    fn command(&mut self, command: &Command, unlocked: bool) -> &'static str {
        match command {
            Command::Window(fields) => {
                if kwin::validate_control(fields).is_err() || fields.len() != 3 {
                    return "InvalidArgument";
                }
                let Some(row) = self.row(&fields[2]) else {
                    return "NotFound";
                };
                if !unlocked {
                    return "Unavailable";
                }
                let result = match fields[1].as_str() {
                    "activate" => self.native.activate(row.xid),
                    "close" if row.flags & 8 != 0 => self.native.close(row.xid),
                    _ => return "Unsupported",
                };
                if result.is_err() {
                    return "Unavailable";
                }
                if fields[1] == "activate" {
                    let error = self.release();
                    if !error.is_empty() {
                        return error;
                    }
                    if let Some(shell) = self.snapshot.shell
                        && self.native.raise_shell(shell, false).is_err()
                    {
                        return "Unavailable";
                    }
                    self.home = false;
                    self.target.clear();
                }
                ""
            }
            Command::Media(fields) => {
                if media_window::validate_control(fields).is_err() || fields.len() != 3 {
                    return "InvalidArgument";
                }
                if fields[1] == "release" {
                    if !fields[2].is_empty()
                        && self
                            .presentation
                            .as_ref()
                            .is_some_and(|p| p.token != fields[2])
                    {
                        return "Conflict";
                    }
                    let error = self.release();
                    if !error.is_empty() {
                        return error;
                    }
                    if !self.home
                        && let Some(shell) = self.snapshot.shell
                    {
                        let _ = self.native.raise_shell(shell, false);
                    }
                    ""
                } else if unlocked {
                    self.present(&fields[2])
                } else {
                    "Unavailable"
                }
            }
            Command::Home(command) => match command.as_str() {
                "home" => {
                    if !unlocked {
                        return "Unavailable";
                    }
                    let Some(shell) = self.snapshot.shell else {
                        return "Unavailable";
                    };
                    if !self.home {
                        self.target = self
                            .presentation
                            .as_ref()
                            .map(|p| p.token.clone())
                            .or_else(|| {
                                self.snapshot
                                    .rows
                                    .iter()
                                    .find(|r| Some(r.xid) == self.snapshot.active)
                                    .map(|r| r.token.clone())
                            })
                            .unwrap_or_default();
                    }
                    if !self.raise_after_publish && self.native.raise_shell(shell, true).is_err() {
                        return "Unavailable";
                    }
                    self.home = true;
                    ""
                }
                "return" => {
                    if !self.home {
                        return "";
                    }
                    if !unlocked {
                        return "Unavailable";
                    }
                    let target = self.target.clone();
                    let Some(row) = self.row(&target).cloned() else {
                        self.target.clear();
                        return "NotFound";
                    };
                    if self
                        .presentation
                        .as_ref()
                        .is_some_and(|p| p.token == target)
                    {
                        let result = self.present(&target);
                        if result.is_empty() {
                            self.home = false;
                            self.target.clear();
                        }
                        return result;
                    }
                    if self.native.activate(row.xid).is_err() {
                        return "Unavailable";
                    }
                    let error = self.release();
                    if !error.is_empty() {
                        return error;
                    }
                    if let Some(shell) = self.snapshot.shell {
                        let _ = self.native.raise_shell(shell, false);
                    }
                    self.home = false;
                    self.target.clear();
                    ""
                }
                "dismiss" => {
                    let error = self.release();
                    if !error.is_empty() {
                        return error;
                    }
                    if let Some(shell) = self.snapshot.shell {
                        let _ = self.native.raise_shell(shell, false);
                    }
                    self.home = false;
                    self.target.clear();
                    ""
                }
                "idle" => "",
                _ => "Unsupported",
            },
        }
    }
}
const LOCK_ENDPOINTS: [(&str, &str, &str); 3] = [
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
async fn unlocked(c: &Connection) -> bool {
    tokio::time::timeout(Duration::from_secs(1), async {
        let bus = zbus::fdo::DBusProxy::new(c).await.ok()?;
        for (service, path, interface) in LOCK_ENDPOINTS {
            // Pin calls to an existing owner: never auto-start a screensaver.
            let Ok(owner) = bus.get_name_owner(service.try_into().ok()?).await else {
                continue;
            };
            let p = Proxy::new(c, owner.as_str(), path, interface).await.ok()?;
            match p.call::<_, _, bool>("GetActive", &()).await {
                Ok(active) => {
                    let current = bus.get_name_owner(service.try_into().ok()?).await.ok()?;
                    return Some(current == owner && !active);
                }
                Err(zbus::Error::MethodError(name, _, _))
                    if [
                        "org.freedesktop.DBus.Error.UnknownObject",
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        "org.freedesktop.DBus.Error.UnknownInterface",
                    ]
                    .contains(&name.as_str()) =>
                {
                    continue;
                }
                // An owned but unresponsive/malformed service is unknown, not
                // evidence of unlock from a lower-priority endpoint.
                Err(_) => return None,
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
        == Some(true)
}
async fn refresh(actor: &Arc<Mutex<Actor>>, unlocked: bool) -> Result<Update> {
    let actor = actor.clone();
    tokio::task::spawn_blocking(move || actor.lock().unwrap().refresh(unlocked)).await?
}
async fn raise_deferred(actor: &Arc<Mutex<Actor>>) -> Result<()> {
    let actor = actor.clone();
    tokio::task::spawn_blocking(move || actor.lock().unwrap().raise_deferred()).await?
}
async fn apply(
    actor: &Arc<Mutex<Actor>>,
    command: Command,
    unlocked: bool,
) -> Result<(&'static str, Update)> {
    let actor = actor.clone();
    Ok(tokio::task::spawn_blocking(move || {
        let mut actor = actor.lock().unwrap();
        let result = actor.command(&command, unlocked);
        let explicit = (matches!(&command, Command::Home(s) if s == "home") && result.is_empty())
            .then_some(Request::Home);
        (result, actor.update(explicit))
    })
    .await?)
}
#[allow(clippy::too_many_arguments)] // Four existing transport proxies and one inventory fence.
async fn publish(
    home: &HomeBridgeProxy<'_>,
    bridge: &KWinBridgeProxy<'_>,
    media: &KWinMediaProxy<'_>,
    returns: &KWinReturnProxy<'_>,
    generation: &str,
    revision: &mut i32,
    previous: &mut Vec<Row>,
    previous_home: &mut Option<(bool, String)>,
    update: &Update,
) -> Result<()> {
    let rows = &update.snapshot.rows;
    let inventory_changed = *revision == 0
        || previous.len() != rows.len()
        || previous.iter().zip(rows).any(|(old, row)| {
            old.token != row.token || old.desktop_id != row.desktop_id || old.flags != row.flags
        });
    if inventory_changed {
        *revision = revision
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("inventory revision exhausted"))?;

        bridge
            .begin_inventory(generation, *revision, rows.len() as i32)
            .await?;
        for (index, row) in rows.iter().enumerate() {
            bridge
                .put_window(
                    generation,
                    *revision,
                    index as i32,
                    &row.token,
                    &row.desktop_id,
                    "",
                    row.flags,
                )
                .await?;
        }

        bridge.commit_inventory(generation, *revision).await?;
    }
    for (index, row) in rows.iter().enumerate() {
        if inventory_changed
            || previous
                .get(index)
                .is_none_or(|old| old.token != row.token || old.launch != row.launch)
        {
            media
                .report(generation, *revision, &row.token, &row.launch)
                .await?;
        }
    }
    *previous = rows.clone();
    let active = rows.iter().find(|r| Some(r.xid) == update.snapshot.active);

    home.report_window(
        active.map_or("", |r| r.token.as_str()),
        active.map_or("", |r| r.desktop_id.as_str()),
        active.is_some_and(|r| r.flags & 2 != 0),
        "",
    )
    .await?;
    let state = (update.home, update.target.clone());
    if update.explicit == Some(Request::Home) {
        home.report_home_request(&update.target, background::OUTPUT)
            .await?;
    } else if update.explicit == Some(Request::QuickMenu) {
        // The quick menu arms the Activity background on this output too
        // (ADR-0106).
        home.report_quick_menu_request(&update.target, background::OUTPUT)
            .await?;
    } else if update.explicit == Some(Request::PowerMenu) {
        // The power menu opens over Home, so it arms the background too
        // (ADR-0111).
        home.report_power_menu_request(&update.target, background::OUTPUT)
            .await?;
    } else if previous_home.as_ref() != Some(&state) {
        home.report_home(update.home, &update.target, background::OUTPUT)
            .await?;
    }
    *previous_home = Some(state);
    if update.home {
        let token = rows
            .iter()
            .find(|r| r.token == update.target)
            .map_or("", |r| r.token.as_str());
        returns
            .report(generation, *revision, &update.target, token)
            .await?;
    }
    Ok(())
}
/// The adapter's native layer as the power key's reader.
struct ActorReader<'a>(&'a Arc<Mutex<Actor>>);
impl power_owner::Reader for ActorReader<'_> {
    fn set(&mut self, enabled: bool, raw: bool) -> Result<native::Reading> {
        tokio::task::block_in_place(|| self.0.lock().unwrap().native.set_power(enabled, raw))
    }
    fn held_elsewhere(&mut self) -> Result<bool> {
        tokio::task::block_in_place(|| self.0.lock().unwrap().native.power_held_elsewhere())
    }
    fn arm(&mut self) -> Result<()> {
        tokio::task::block_in_place(|| self.0.lock().unwrap().native.arm_raw())
    }
}
async fn session(power: &mut power_owner::PowerOwner) -> Result<()> {
    let actor = Arc::new(Mutex::new(
        tokio::task::spawn_blocking(Actor::connect).await??,
    ));
    let result = run(&actor, power).await;
    let cleanup = actor.clone();
    end_session(power, result, move || cleanup.lock().unwrap().cleanup()).await
}
/// Ends one session however it ended: the reader is gone with it, so the
/// desktop gets the power key back (xfconf, then the inhibitor) before the
/// X state is restored.
async fn end_session<F>(
    power: &mut power_owner::PowerOwner,
    result: Result<()>,
    cleanup: F,
) -> Result<()>
where
    F: FnOnce() -> Result<()> + Send + 'static,
{
    power.end().await;
    let restored = tokio::task::spawn_blocking(cleanup).await?;
    if restored.is_err() {
        eprintln!("arinn-platform-x11: shutdown restoration failed");
    }
    result.and(restored)
}
async fn run(actor: &Arc<Mutex<Actor>>, power: &mut power_owner::PowerOwner) -> Result<()> {
    let c = zbus::connection::Builder::session()?
        .name("org.arinn.X11")?
        .build()
        .await?;
    let bus = zbus::fdo::DBusProxy::new(&c).await?;
    let owner = bus.get_name_owner(production::SERVICE.try_into()?).await?;
    let destination = owner.to_string();
    let bridge = KWinBridgeProxy::builder(&c)
        .destination(destination.clone())?
        .build()
        .await?;
    let home = HomeBridgeProxy::builder(&c)
        .destination(destination.clone())?
        .build()
        .await?;
    let media = KWinMediaProxy::builder(&c)
        .destination(destination.clone())?
        .build()
        .await?;
    let returns = KWinReturnProxy::builder(&c)
        .destination(destination.clone())?
        .build()
        .await?;
    let generation = bridge.connect(&Uuid::new_v4().to_string()).await.inspect_err(|error| {
        if matches!(error, zbus::Error::MethodError(name, _, _) if name.as_str() == "org.arinn.Error1.PermissionDenied") {
            eprintln!("arinn-platform-x11: daemon rejected adapter; check ARINN_PLATFORM_BACKEND");
        }
    })?;
    // Optional Activity background producer (ADR-0087) on this connection,
    // which owns org.arinn.X11. Its end never ends the session.
    let (windows, current) = tokio::sync::watch::channel(background::Windows::new());
    let producer = tokio::spawn(background::run(
        c.clone(),
        destination.clone(),
        current,
        None,
    ));
    let _producer = Abort(producer);
    let (tx, mut rx) = mpsc::channel::<Pending>(8);
    let mut tasks = JoinSet::<Result<()>>::new();
    let home_poll = home.clone();
    let send = tx.clone();
    tasks.spawn(async move {
        loop {
            let command = home_poll.take_command().await?;
            if command == "idle" {
                continue;
            }
            let (done, wait) = oneshot::channel();
            send.send(Pending {
                command: Command::Home(command),
                done,
            })
            .await?;
            let _ = wait.await?;
        }
    });
    let bridge_poll = bridge.clone();
    let send = tx.clone();
    let g = generation.clone();
    tasks.spawn(async move {
        loop {
            let fields = bridge_poll.next_control(&g).await?;
            if fields.is_empty() {
                continue;
            }
            kwin::validate_control(&fields)?;
            let id = fields[0].clone();
            let (done, wait) = oneshot::channel();
            send.send(Pending {
                command: Command::Window(fields),
                done,
            })
            .await?;
            bridge_poll.complete_control(&g, &id, wait.await?).await?;
        }
    });
    let media_poll = media.clone();
    let send = tx;
    let g = generation.clone();
    tasks.spawn(async move {
        loop {
            let fields = media_poll.next_control(&g).await?;
            if fields.is_empty() {
                continue;
            }
            media_window::validate_control(&fields)?;
            let id = fields[0].clone();
            let (done, wait) = oneshot::channel();
            send.send(Pending {
                command: Command::Media(fields),
                done,
            })
            .await?;
            media_poll.complete_control(&g, &id, wait.await?).await?;
        }
    });
    // The global on-screen keyboard (ADR-0105); optional, so an older
    // daemon without the bridge keeps the rest of the adapter.
    // Home and the quick menu win a key they share with it (ADR-0106).
    let (home_shortcut, quick_menu_shortcut) = shortcuts();
    let (keyboard_keys, fallback, conflict) = key_sequence::keyboard(
        &keyboard_shortcut(),
        &key_sequence::reserved(&home_shortcut, &quick_menu_shortcut),
    );
    if fallback {
        eprintln!("arinn-platform-x11: invalid keyboard_shortcut; using the default");
    }
    if conflict {
        eprintln!("arinn-platform-x11: a keyboard key is also a Home or quick-menu key; left out");
    }
    keyboard_lane::start(
        &c,
        &destination,
        None,
        keyboard_keys,
        |c| Box::pin(async move { unlocked(&c).await }),
        &mut tasks,
    )
    .await?;
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut revision = 0;
    let mut previous = Vec::new();
    let mut previous_home = None;
    loop {
        let (update, completion) = tokio::select! {
            _ = tick.tick() => (refresh(actor, unlocked(&c).await).await?, None),
            pending = rx.recv() => {
                let pending = pending.ok_or_else(|| anyhow::anyhow!("control streams ended"))?;
                // Refresh native objects before every control: queued opaque tokens
                // cannot activate a recycled XID or a removed client.
                let allowed = unlocked(&c).await;
                let fresh = refresh(actor, allowed).await?;
                background::offer(&windows, &fresh.snapshot.rows);
                publish(&home, &bridge, &media, &returns, &generation, &mut revision, &mut previous, &mut previous_home, &fresh).await?;
                if fresh.explicit == Some(Request::QuickMenu) {
                    raise_deferred(actor).await?;
                }
                let (error, update) = apply(actor, pending.command, allowed).await?;
                (update, Some((pending.done, error)))
            },
            ended = tasks.join_next() => { ended.ok_or_else(|| anyhow::anyhow!("pollers ended"))???; anyhow::bail!("poller ended"); },
            _ = shutdown() => return Ok(()),
        };
        // The producer sees the rows before the Home request they belong to.
        background::offer(&windows, &update.snapshot.rows);
        publish(
            &home,
            &bridge,
            &media,
            &returns,
            &generation,
            &mut revision,
            &mut previous,
            &mut previous_home,
            &update,
        )
        .await?;
        if update.explicit == Some(Request::QuickMenu) {
            raise_deferred(actor).await?;
        }
        // The power key follows the shell: only while it owns its name and
        // the key is read (ADR-0111).
        let shell = bus
            .name_has_owner(power_key_shell().try_into()?)
            .await
            .unwrap_or(false);
        // The ordinary XFCE desktop keeps the key; an XFCE session manager
        // or xfce4-power-manager that appears later takes it back within
        // one tick (ADR-0111).
        let xfce = power_owner::xfce_on_bus(&bus).await;
        let reading = update.snapshot.power_reading;
        power
            .follow(&mut ActorReader(actor), shell, xfce, reading)
            .await?;
        if let Some((done, error)) = completion {
            let _ = done.send(error);
        }
    }
}
/// The shell's bus name; its owner is the live shell.
fn power_key_shell() -> &'static str {
    "org.arinn.Shell1"
}
/// Aborts a task when the session ends.
struct Abort<T>(tokio::task::JoinHandle<T>);
impl<T> Drop for Abort<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn shutdown() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("SIGTERM handler");
    tokio::select! { _ = tokio::signal::ctrl_c() => (), _ = term.recv() => () }
}
/// `home_shortcut` (#286) and `quick_menu_shortcut` (ADR-0106) from the
/// daemon's `config.toml`, read only. The daemon owns the file; a missing or
/// unreadable value means the default.
fn shortcuts() -> (String, String) {
    let home = std::env::var_os("HOME");
    let config = std::env::var_os("XDG_CONFIG_HOME");
    let table = config_table(home.as_deref(), config.as_deref());
    (
        shortcut(table.as_ref(), "home_shortcut", key_sequence::DEFAULT_HOME),
        shortcut(
            table.as_ref(),
            "quick_menu_shortcut",
            key_sequence::DEFAULT_QUICK_MENU,
        ),
    )
}
/// `keyboard_shortcut` (ADR-0105) from the same file, read only.
fn keyboard_shortcut() -> String {
    let home = std::env::var_os("HOME");
    let config = std::env::var_os("XDG_CONFIG_HOME");
    shortcut(
        config_table(home.as_deref(), config.as_deref()).as_ref(),
        "keyboard_shortcut",
        key_sequence::DEFAULT_KEYBOARD,
    )
}
fn shortcut(table: Option<&toml::Table>, key: &str, default: &str) -> String {
    table
        .and_then(|table| table.get(key)?.as_str().map(str::to_owned))
        .unwrap_or_else(|| default.to_owned())
}
#[cfg(test)]
fn configured_home(home: Option<&std::ffi::OsStr>, config: Option<&std::ffi::OsStr>) -> String {
    shortcut(
        config_table(home, config).as_ref(),
        "home_shortcut",
        key_sequence::DEFAULT_HOME,
    )
}
fn config_table(
    home: Option<&std::ffi::OsStr>,
    config: Option<&std::ffi::OsStr>,
) -> Option<toml::Table> {
    let path = config
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            home.map(std::path::PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|p| p.join(".config"))
        })
        .map(|root| root.join("arinn/config.toml"));
    path.and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| text.parse::<toml::Table>().ok())
}
/// The files and session bus for xfce4-power-manager's power button.
async fn power_button() -> Option<(Connection, xfce_power::Files)> {
    let files = xfce_power::Files::from_env(
        std::env::var_os("HOME").as_deref(),
        std::env::var_os("XDG_STATE_HOME").as_deref(),
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
    )?;
    Some((Connection::session().await.ok()?, files))
}
#[tokio::main]
async fn main() -> Result<()> {
    let argument = std::env::args_os().nth(1);
    if argument.as_deref() == Some(std::ffi::OsStr::new("--release-power-key")) {
        // ExecStopPost= of arinn-ewmh.service: the desktop's power button
        // setting comes back however the adapter ended (ADR-0111).
        // The adapter is gone and so is any grab it held: if this
        // display's adapter grabbed the power key (its marker) and
        // xfce4-power-manager runs while nobody grabs a mapped power key,
        // its grab failed meanwhile and it is restarted (review of #516).
        if let Ok(bus) = Connection::session().await {
            let files = xfce_power::Files::from_env(
                std::env::var_os("HOME").as_deref(),
                std::env::var_os("XDG_STATE_HOME").as_deref(),
                std::env::var_os("XDG_CONFIG_HOME").as_deref(),
            );
            let system = Connection::system().await.ok();
            power_owner::release_after_adapter(
                &bus,
                files.as_ref(),
                &power_owner::GrabMarker::from_env(),
                || tokio::task::block_in_place(native::power_held_on_display),
                system.as_ref(),
                Duration::from_secs(10),
            )
            .await;
        }
        return Ok(());
    }
    if argument.as_deref() == Some(std::ffi::OsStr::new("--restore-power-button")) {
        // The system XDG autostart entry at every XFCE login: a copy an
        // unclean end of the dedicated session left goes back, unless the
        // adapter runs and holds the key (ADR-0111).
        if let Some((bus, files)) = power_button().await {
            xfce_power::restore_at_login(&bus, &files).await;
        }
        return Ok(());
    }
    ensure!(
        std::env::var("ARINN_PLATFORM_BACKEND").as_deref() == Ok("ewmh"),
        "explicit EWMH backend selection required"
    );
    eprintln!("arinn-platform-x11: selected backend ewmh (org.arinn.X11)");
    let mut power = power_owner::PowerOwner::from_env().await;
    // No native payloads in logs. Reconnect creates fresh tokens and bridge generation.
    loop {
        if session(&mut power).await.is_ok() {
            return Ok(());
        }
        eprintln!("arinn-platform-x11: backend unavailable; retrying");
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(3)) => (),
            _ = shutdown() => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    /// Review of #517: a session that ends, also with an error (a reconnect
    /// follows), gives the power key back: xfconf and the inhibitor go
    /// before the X state is restored, as the reader went with the session.
    #[tokio::test]
    async fn an_ended_session_gives_the_power_key_back() {
        use crate::{native::Reading, power_owner::tests as owner, xfce_power::Saved};
        let mut f = owner::fixture(true).await;
        let mut reader = owner::reader(Reading::Raw);
        f.owner
            .follow(
                &mut reader,
                true,
                power_owner::Xfce {
                    session: true,
                    power_manager: true,
                },
                Reading::None,
            )
            .await
            .unwrap();
        assert_eq!(f.xfconf.value(), Some(Saved::Uint(0)));
        assert_eq!(f.open(), 1);
        let ended = end_session(&mut f.owner, Err(anyhow::anyhow!("lost")), || Ok(())).await;
        assert!(ended.is_err());
        assert_eq!(f.owner.owned(), Reading::None);
        assert_eq!(f.xfconf.value(), Some(Saved::Uint(3)));
        f.settle(0).await;
    }

    use super::*;
    use x11rb::protocol::xproto::ConnectionExt as _;
    #[test]
    fn home_shortcut_comes_from_the_daemon_config_file() {
        let root = std::env::temp_dir().join(format!("arinn-x11-config-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("arinn")).unwrap();
        let read = || configured_home(None, Some(root.as_os_str()));
        assert_eq!(read(), "Meta+F12");
        std::fs::write(
            root.join("arinn/config.toml"),
            "schema_version = 1\nhome_shortcut = \"Ctrl+Alt+H\"\n",
        )
        .unwrap();
        assert_eq!(read(), "Ctrl+Alt+H");
        // XDG_CONFIG_HOME wins over HOME; a relative value is ignored.
        assert_eq!(
            configured_home(
                Some(std::ffi::OsStr::new("/nonexistent")),
                Some(root.as_os_str())
            ),
            "Ctrl+Alt+H"
        );
        assert_eq!(
            configured_home(None, Some(std::ffi::OsStr::new("relative"))),
            "Meta+F12"
        );
        // ADR-0105: keyboard_shortcut comes from the same file.
        let keyboard = || {
            shortcut(
                config_table(None, Some(root.as_os_str())).as_ref(),
                "keyboard_shortcut",
                key_sequence::DEFAULT_KEYBOARD,
            )
        };
        std::fs::write(
            root.join("arinn/config.toml"),
            "schema_version = 1\nkeyboard_shortcut = \"Ctrl+Alt+K\"\n",
        )
        .unwrap();
        assert_eq!(keyboard(), "Ctrl+Alt+K");
        std::fs::write(root.join("arinn/config.toml"), "not toml [").unwrap();
        assert_eq!(read(), "Meta+F12");
        // ADR-0106: quick_menu_shortcut comes from the same file.
        let quick_menu = || {
            shortcut(
                config_table(None, Some(root.as_os_str())).as_ref(),
                "quick_menu_shortcut",
                key_sequence::DEFAULT_QUICK_MENU,
            )
        };
        assert_eq!(quick_menu(), "Meta+F11");
        std::fs::write(
            root.join("arinn/config.toml"),
            "schema_version = 1\nquick_menu_shortcut = \"Ctrl+Alt+Q\"\n",
        )
        .unwrap();
        assert_eq!(quick_menu(), "Ctrl+Alt+Q");
        assert_eq!(read(), "Meta+F12");
        assert_eq!(keyboard(), "Meta+F10");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated server and synthetic EWMH manager"]
    fn missing_shell_makes_home_unavailable() -> Result<()> {
        let desktop = native::fixture::Desktop::new()?;
        let launch = Uuid::new_v4().to_string();
        let video = desktop.client("org.example.Player.desktop", &launch)?;
        desktop.inventory(&[video], video)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let snapshot = native.snapshot()?;
        let mut actor = Actor {
            native,
            snapshot,
            home: false,
            target: String::new(),
            presentation: None,
            media_allowed: true,
            raise_after_publish: false,
        };
        assert_eq!(
            actor.command(&Command::Home("home".into()), true),
            "Unavailable"
        );
        Ok(())
    }
    /// The quick-menu key raises the shell only after its request is
    /// published (`raise_deferred`), so the shell is never active when the
    /// request arrives over an Activity; Home raises at once (review of #502).
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated server and synthetic EWMH manager"]
    fn quick_menu_raises_the_shell_only_after_its_request() -> Result<()> {
        let desktop = native::fixture::Desktop::new()?;
        let shell = desktop.client("org.arinn.Shell", "")?;
        let video = desktop.client("org.example.Player.desktop", "")?;
        desktop.inventory(&[shell, video], video)?;
        let home = crate::key_sequence::home("Ctrl+Alt+H").0;
        let quick_menu = crate::key_sequence::quick_menu("Ctrl+Alt+Q", &home).0;
        let mut native = Native::connect_display_with(Some(&desktop.display), home, quick_menu)?;
        let snapshot = native.snapshot()?;
        let mut actor = Actor {
            snapshot,
            native,
            home: false,
            target: String::new(),
            presentation: None,
            media_allowed: true,
            raise_after_publish: false,
        };
        actor.refresh(true)?;
        let activations = |messages: Vec<x11rb::protocol::xproto::ClientMessageEvent>| {
            messages
                .iter()
                .filter(|m| m.window == shell && m.type_ == desktop.atoms._NET_ACTIVE_WINDOW)
                .count()
        };
        desktop.messages()?;
        desktop.press(&[0xffe3, 0xffe9], u32::from(b'q'))?;
        let update = actor.refresh(true)?;
        assert_eq!(update.explicit, Some(Request::QuickMenu));
        assert!(update.home);
        assert_eq!(activations(desktop.messages()?), 0);
        actor.raise_deferred()?;
        assert_eq!(activations(desktop.messages()?), 1);
        // Nothing is held back afterwards: the next snapshot raises as usual.
        actor.raise_deferred()?;
        assert_eq!(activations(desktop.messages()?), 0);
        actor.refresh(true)?;
        assert_eq!(activations(desktop.messages()?), 1);
        // Home raises in the same snapshot.
        actor.home = false;
        desktop.inventory(&[shell, video], video)?;
        actor.refresh(true)?;
        desktop.messages()?;
        desktop.press(&[0xffe3, 0xffe9], u32::from(b'h'))?;
        let update = actor.refresh(true)?;
        assert_eq!(update.explicit, Some(Request::Home));
        assert!(activations(desktop.messages()?) >= 1);
        Ok(())
    }
    /// ADR-0111, spec §5.7: the power key shows the shell as Home does and
    /// asks for the power menu; on a locked session it does nothing.
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated server and synthetic EWMH manager"]
    fn power_key_opens_the_power_menu_only_when_unlocked() -> Result<()> {
        let desktop = native::fixture::Desktop::new()?;
        let shell = desktop.client("org.arinn.Shell", "")?;
        let video = desktop.client("org.example.Player.desktop", "")?;
        desktop.inventory(&[shell, video], video)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        // PowerOwner's decision (a live shell, the inhibitor held).
        native.set_power(true, false)?;
        let snapshot = native.snapshot()?;
        let mut actor = Actor {
            snapshot,
            native,
            home: false,
            target: String::new(),
            presentation: None,
            media_allowed: true,
            raise_after_publish: false,
        };
        actor.refresh(true)?;
        let activations = |messages: Vec<x11rb::protocol::xproto::ClientMessageEvent>| {
            messages
                .iter()
                .filter(|m| m.window == shell && m.type_ == desktop.atoms._NET_ACTIVE_WINDOW)
                .count()
        };
        // Locked: no request, no Home, the shell stays where it is.
        desktop.messages()?;
        desktop.press(&[], 0x1008_ff2a)?;
        let update = actor.refresh(false)?;
        assert_eq!(update.explicit, None);
        assert!(!update.home);
        assert_eq!(activations(desktop.messages()?), 0);
        // Unlocked: the Home state over the video, raised at once.
        desktop.press(&[], 0x1008_ff2a)?;
        let update = actor.refresh(true)?;
        assert_eq!(update.explicit, Some(Request::PowerMenu));
        assert!(update.home);
        assert!(!update.target.is_empty());
        assert!(activations(desktop.messages()?) >= 1);
        assert_eq!(update.snapshot.power_reading, native::Reading::Grab);
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated server and synthetic EWMH manager"]
    fn hidden_shell_allows_home_but_not_presentation() -> Result<()> {
        let desktop = native::fixture::Desktop::new()?;
        let launch = Uuid::new_v4().to_string();
        let shell = desktop.client("org.arinn.Shell", "")?;
        let video = desktop.client("org.example.Player.desktop", &launch)?;
        desktop.inventory(&[video], video)?;
        desktop.compositing(true)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let snapshot = native.snapshot()?;
        assert_eq!(snapshot.shell, Some(shell));
        assert!(snapshot.shell_token.is_empty());
        let token = snapshot.rows[0].token.clone();
        let mut actor = Actor {
            native,
            snapshot,
            home: false,
            target: String::new(),
            presentation: None,
            media_allowed: true,
            raise_after_publish: false,
        };
        assert_eq!(actor.present(&token), "Unavailable");
        assert!(actor.presentation.is_none());
        assert_eq!(actor.command(&Command::Home("home".into()), true), "");
        assert!(actor.home);
        assert_eq!(actor.target, token);
        // Once the shell maps and is listed, it has a lifetime.
        desktop.inventory(&[shell, video], shell)?;
        actor.refresh(true)?;
        assert!(!actor.snapshot.shell_token.is_empty());
        assert_eq!(actor.command(&Command::Home("return".into()), true), "");
        assert_eq!(actor.present(&token), "");
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; spawns an isolated server and synthetic EWMH manager"]
    fn isolated_home_return_and_video_lifecycle() -> Result<()> {
        let desktop = native::fixture::Desktop::new()?;
        let launch = Uuid::new_v4().to_string();
        let shell = desktop.client("org.arinn.Shell", "")?;
        let video = desktop.client("org.example.Player", &launch)?;
        desktop.inventory(&[shell, video], video)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let snapshot = native.snapshot()?;
        let token = snapshot.rows[0].token.clone();
        let mut actor = Actor {
            native,
            snapshot,
            home: false,
            target: String::new(),
            presentation: None,
            media_allowed: true,
            raise_after_publish: false,
        };
        assert_eq!(
            actor.command(&Command::Home("home".into()), false),
            "Unavailable"
        );
        assert!(!actor.home);
        assert_eq!(actor.command(&Command::Home("home".into()), true), "");
        assert!(actor.home);
        assert_eq!(actor.target, token);
        desktop.inventory(&[shell, video], shell)?;
        actor.refresh(true)?;
        assert_eq!(actor.command(&Command::Home("home".into()), true), "");
        assert_eq!(actor.target, token); // repeated Home cannot overwrite Return evidence
        assert_eq!(actor.command(&Command::Home("return".into()), true), "");
        assert!(!actor.home);
        assert_eq!(actor.present(&token), "Unsupported");
        desktop.compositing(true)?;
        actor.refresh(true)?;
        assert_eq!(actor.present(&token), "");
        assert!(actor.presentation.is_some());
        assert_eq!(actor.command(&Command::Home("home".into()), true), "");
        assert_eq!(actor.target, token);
        assert_eq!(actor.command(&Command::Home("return".into()), true), "");
        assert!(!actor.home);
        let stale_release = vec![
            Uuid::new_v4().to_string(),
            "release".into(),
            "other-token".into(),
        ];
        assert_eq!(
            actor.command(&Command::Media(stale_release), true),
            "Conflict"
        );
        assert!(actor.presentation.is_some());
        desktop.messages()?;
        desktop.compositing(false)?;
        let unavailable = actor.refresh(true)?;
        assert!(actor.presentation.is_none());
        assert!(!unavailable.home);
        assert!(
            unavailable
                .snapshot
                .rows
                .iter()
                .all(|r| r.launch.is_empty())
        );
        let messages = desktop.messages()?;
        assert!(messages.iter().any(|m| m.window == shell
            && m.type_ == desktop.atoms._NET_WM_STATE
            && m.data.as_data32()[0] == 0
            && m.data.as_data32()[1] == desktop.atoms._NET_WM_STATE_ABOVE));
        assert!(
            messages
                .iter()
                .any(|m| m.window == video && m.type_ == desktop.atoms._NET_ACTIVE_WINDOW)
        );
        desktop.compositing(true)?;
        actor.refresh(true)?;
        assert_eq!(actor.present(&token), "");
        actor.refresh(false)?;
        assert!(actor.presentation.is_none());
        assert!(!actor.home);
        desktop.c.destroy_window(video)?.check()?;
        desktop.inventory(&[shell], shell)?;
        actor.refresh(true)?;
        let request = vec![Uuid::new_v4().to_string(), "activate".into(), token];
        assert_eq!(actor.command(&Command::Window(request), true), "NotFound");
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; isolated focus and shell lifetime regressions"]
    fn unrelated_focus_and_shell_loss_restore_exact_presentation() -> Result<()> {
        for scenario in ["focus", "destroy", "replace", "retire", "retire-destroy"] {
            let desktop = native::fixture::Desktop::new()?;
            let launch = Uuid::new_v4().to_string();
            let mut shell = desktop.client("org.arinn.Shell", "")?;
            let video = desktop.client("org.example.Player.desktop", &launch)?;
            let other = desktop.client("org.example.Other.desktop", "")?;
            desktop.inventory(&[shell, video, other], video)?;
            desktop.compositing(true)?;
            let mut native = Native::connect_display(Some(&desktop.display))?;
            let snapshot = native.snapshot()?;
            let token = snapshot
                .rows
                .iter()
                .find(|r| r.launch == launch)
                .unwrap()
                .token
                .clone();
            let mut actor = Actor {
                native,
                snapshot,
                home: false,
                target: String::new(),
                presentation: None,
                media_allowed: true,
                raise_after_publish: false,
            };
            assert_eq!(actor.present(&token), "");
            desktop.messages()?;
            match scenario {
                "focus" => desktop.inventory(&[shell, video, other], other)?,
                "destroy" => {
                    desktop.c.destroy_window(shell)?.check()?;
                    desktop.inventory(&[video, other], video)?;
                }
                "replace" => {
                    desktop.c.destroy_window(shell)?.check()?;
                    let replacement = desktop.client("org.arinn.Shell.desktop", "")?;
                    desktop.inventory(&[replacement, video, other], replacement)?;
                }
                "retire" => {
                    // Inventory removal ends the lifetime even if the XID
                    // returns; the hidden root child is found only for Home.
                    desktop.inventory(&[video, other], video)?;
                    actor.native.snapshot()?;
                    desktop.inventory(&[shell, video, other], shell)?;
                }
                "retire-destroy" => {
                    // A destroyed shell is not found again; a new shell XID
                    // starts a new lifetime.
                    desktop.inventory(&[video, other], video)?;
                    desktop.c.destroy_window(shell)?.check()?;
                    actor.native.snapshot()?;
                    shell = desktop.client("org.arinn.Shell", "")?;
                    desktop.inventory(&[shell, video, other], shell)?;
                }
                _ => unreachable!(),
            }
            actor.refresh(true)?;
            assert!(actor.presentation.is_none(), "{scenario}");
            assert!(!actor.home);
            let messages = desktop.messages()?;
            assert!(
                messages.iter().any(|m| m.window == video
                    && m.type_ == desktop.atoms._NET_WM_STATE
                    && m.data.as_data32()[0] == 0
                    && m.data.as_data32()[1] == desktop.atoms._NET_WM_STATE_FULLSCREEN),
                "{scenario}"
            );
            assert!(
                !messages
                    .iter()
                    .any(|m| m.type_ == desktop.atoms._NET_ACTIVE_WINDOW),
                "{scenario}"
            );
        }
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; isolated inventory disappearance regression"]
    fn disappearing_player_restores_before_retiring_inventory() -> Result<()> {
        let desktop = native::fixture::Desktop::new()?;
        let launch = Uuid::new_v4().to_string();
        let shell = desktop.client("org.arinn.Shell.desktop", "")?;
        let video = desktop.client("org.example.Player.desktop", &launch)?;
        desktop.inventory(&[shell, video], video)?;
        desktop.compositing(true)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let snapshot = native.snapshot()?;
        let token = snapshot.rows[0].token.clone();
        let mut actor = Actor {
            native,
            snapshot,
            home: false,
            target: String::new(),
            presentation: None,
            media_allowed: true,
            raise_after_publish: false,
        };
        assert_eq!(actor.present(&token), "");
        desktop.messages()?;
        // The XID remains alive while the manager temporarily removes the client.
        desktop.inventory(&[shell], shell)?;
        actor.refresh(true)?;
        assert!(actor.presentation.is_none());
        let messages = desktop.messages()?;
        for atom in [
            desktop.atoms._NET_WM_STATE_FULLSCREEN,
            desktop.atoms._NET_WM_STATE_BELOW,
        ] {
            assert!(messages.iter().any(|m| m.window == video
                && m.type_ == desktop.atoms._NET_WM_STATE
                && m.data.as_data32()[0] == 0
                && m.data.as_data32()[1] == atom));
        }
        assert!(
            !messages
                .iter()
                .any(|m| m.type_ == desktop.atoms._NET_ACTIVE_WINDOW)
        );
        desktop.inventory(&[shell, video], video)?;
        actor.refresh(true)?;
        assert_ne!(actor.snapshot.rows[0].token, token);
        assert!(actor.row(&token).is_none());
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; isolated shutdown inventory disappearance regression"]
    fn shutdown_restores_player_missing_from_inventory() -> Result<()> {
        let desktop = native::fixture::Desktop::new()?;
        let launch = Uuid::new_v4().to_string();
        let shell = desktop.client("org.arinn.Shell.desktop", "")?;
        let video = desktop.client("org.example.Player.desktop", &launch)?;
        desktop.inventory(&[shell, video], video)?;
        desktop.compositing(true)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let snapshot = native.snapshot()?;
        let token = snapshot.rows[0].token.clone();
        let mut actor = Actor {
            native,
            snapshot,
            home: false,
            target: String::new(),
            presentation: None,
            media_allowed: true,
            raise_after_publish: false,
        };
        assert_eq!(actor.present(&token), "");
        desktop.messages()?;
        // The XID remains alive while the manager temporarily removes the client.
        desktop.inventory(&[shell], shell)?;
        actor.cleanup()?;
        assert!(actor.presentation.is_none());
        let messages = desktop.messages()?;
        for atom in [
            desktop.atoms._NET_WM_STATE_FULLSCREEN,
            desktop.atoms._NET_WM_STATE_BELOW,
        ] {
            assert!(messages.iter().any(|m| m.window == video
                && m.type_ == desktop.atoms._NET_WM_STATE
                && m.data.as_data32()[0] == 0
                && m.data.as_data32()[1] == atom));
        }
        assert!(
            !messages
                .iter()
                .any(|m| m.type_ == desktop.atoms._NET_ACTIVE_WINDOW)
        );
        desktop.inventory(&[shell, video], video)?;
        actor.refresh(true)?;
        assert_ne!(actor.snapshot.rows[0].token, token);
        assert!(actor.row(&token).is_none());
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; isolated destroyed and changed player identity regression"]
    fn retired_player_identity_does_not_restore_different_window() -> Result<()> {
        use x11rb::{
            protocol::xproto::{AtomEnum, PropMode},
            wrapper::ConnectionExt as _,
        };
        for destroyed in [true, false] {
            let desktop = native::fixture::Desktop::new()?;
            let launch = Uuid::new_v4().to_string();
            let shell = desktop.client("org.arinn.Shell.desktop", "")?;
            let video = desktop.client("org.example.Player.desktop", &launch)?;
            desktop.inventory(&[shell, video], video)?;
            desktop.compositing(true)?;
            let mut native = Native::connect_display(Some(&desktop.display))?;
            let snapshot = native.snapshot()?;
            let token = snapshot.rows[0].token.clone();
            let mut actor = Actor {
                native,
                snapshot,
                home: false,
                target: String::new(),
                presentation: None,
                media_allowed: true,
                raise_after_publish: false,
            };
            assert_eq!(actor.present(&token), "");
            desktop.messages()?;
            if destroyed {
                desktop.c.destroy_window(video)?.check()?;
                desktop.inventory(&[shell], shell)?;
            } else {
                let other = Uuid::new_v4();
                desktop
                    .c
                    .change_property8(
                        PropMode::REPLACE,
                        video,
                        desktop.atoms.WM_CLASS,
                        AtomEnum::STRING,
                        format!("org.arinn.playback.{other}\0mpv\0").as_bytes(),
                    )?
                    .check()?;
                desktop.inventory(&[shell, video], shell)?;
            }
            actor.refresh(true)?;
            assert!(actor.presentation.is_none());
            let messages = desktop.messages()?;
            assert!(!messages.iter().any(|m| m.window == video));
        }
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; isolated cleanup restoration failure regression"]
    fn shutdown_lowers_shell_even_when_player_identity_probe_fails() -> Result<()> {
        use x11rb::{protocol::xproto::PropMode, wrapper::ConnectionExt as _};
        let desktop = native::fixture::Desktop::new()?;
        let launch = Uuid::new_v4().to_string();
        let shell = desktop.client("org.arinn.Shell.desktop", "")?;
        let video = desktop.client("org.example.Player.desktop", &launch)?;
        desktop.inventory(&[shell, video], video)?;
        desktop.compositing(true)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let snapshot = native.snapshot()?;
        let token = snapshot.rows[0].token.clone();
        let mut actor = Actor {
            native,
            snapshot,
            home: false,
            target: String::new(),
            presentation: None,
            media_allowed: true,
            raise_after_publish: false,
        };
        assert_eq!(actor.present(&token), "");
        desktop.messages()?;
        // Malformed metadata prevents proving the live player's launch identity.
        desktop
            .c
            .change_property8(
                PropMode::REPLACE,
                video,
                desktop.atoms.WM_CLASS,
                desktop.atoms.UTF8_STRING,
                b"invalid",
            )?
            .check()?;
        assert!(actor.cleanup().is_err());
        assert!(actor.presentation.is_some());
        let messages = desktop.messages()?;
        assert!(messages.iter().any(|m| m.window == shell
            && m.type_ == desktop.atoms._NET_WM_STATE
            && m.data.as_data32()[0] == 0
            && m.data.as_data32()[1] == desktop.atoms._NET_WM_STATE_ABOVE));
        assert!(!messages.iter().any(|m| m.window == video));
        Ok(())
    }
    #[test]
    #[ignore = "requires Xvfb; isolated regression for partial presentation failure"]
    fn failed_presentation_restores_video_and_reports_restore_failure() -> Result<()> {
        let desktop = native::fixture::Desktop::new()?;
        let launch = Uuid::new_v4().to_string();
        let shell = desktop.client("org.arinn.Shell", "")?;
        let video = desktop.client("org.example.Player", &launch)?;
        desktop.inventory(&[shell, video], video)?;
        desktop.compositing(true)?;
        let mut native = Native::connect_display(Some(&desktop.display))?;
        let snapshot = native.snapshot()?;
        let token = snapshot.rows[0].token.clone();
        let mut actor = Actor {
            native,
            snapshot,
            home: false,
            target: String::new(),
            presentation: None,
            media_allowed: true,
            raise_after_publish: false,
        };
        desktop.c.destroy_window(shell)?.check()?;
        assert_eq!(actor.present(&token), "Unavailable");
        assert!(actor.presentation.is_none());
        let messages = desktop.messages()?;
        assert!(messages.iter().any(|m| m.window == video
            && m.type_ == desktop.atoms._NET_WM_STATE
            && m.data.as_data32()[0] == 1
            && m.data.as_data32()[1] == desktop.atoms._NET_WM_STATE_FULLSCREEN));
        assert!(messages.iter().any(|m| m.window == video
            && m.type_ == desktop.atoms._NET_WM_STATE
            && m.data.as_data32()[0] == 0
            && m.data.as_data32()[1] == desktop.atoms._NET_WM_STATE_FULLSCREEN));
        // If restoration cannot address the exact object, keep its cleanup
        // evidence and report failure rather than acknowledging successful release.
        actor.presentation = Some(Presentation {
            shell,
            shell_token: actor.snapshot.shell_token.clone(),
            token,
            launch,
            saved: Vec::new(),
        });
        desktop.c.destroy_window(video)?.check()?;
        assert_eq!(actor.release(), "Unavailable");
        assert!(actor.presentation.is_some());
        Ok(())
    }
}

#[cfg(test)]
mod lock_tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    pub(super) struct Bus {
        child: Child,
        pub(super) client: Connection,
        pub(super) server: Connection,
        pub(super) address: String,
    }
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
    impl Bus {
        pub(super) async fn new() -> Self {
            let mut child = Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut address = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut address)
                .unwrap();
            async fn connection(address: &str) -> Connection {
                zbus::connection::Builder::address(address.trim())
                    .unwrap()
                    .build()
                    .await
                    .unwrap()
            }
            Self {
                child,
                client: connection(&address).await,
                server: connection(&address).await,
                address,
            }
        }
    }
    #[derive(Clone, Default)]
    struct Lock {
        active: Arc<AtomicBool>,
        calls: Arc<AtomicUsize>,
    }
    #[zbus::interface(name = "org.freedesktop.ScreenSaver")]
    impl Lock {
        fn get_active(&self) -> bool {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.active.load(Ordering::SeqCst)
        }
    }
    struct XfceLock(Lock);
    #[zbus::interface(name = "org.xfce.ScreenSaver")]
    impl XfceLock {
        fn get_active(&self) -> bool {
            self.0.calls.fetch_add(1, Ordering::SeqCst);
            self.0.active.load(Ordering::SeqCst)
        }
    }
    #[tokio::test]
    async fn each_supported_lock_endpoint_checks_active_state_on_a_private_bus() {
        for (service, path, _) in LOCK_ENDPOINTS {
            let bus = Bus::new().await;
            let lock = Lock::default();
            if service == "org.xfce.ScreenSaver" {
                bus.server
                    .object_server()
                    .at(path, XfceLock(lock.clone()))
                    .await
                    .unwrap();
            } else {
                bus.server
                    .object_server()
                    .at(path, lock.clone())
                    .await
                    .unwrap();
            }
            bus.server.request_name(service).await.unwrap();
            assert!(unlocked(&bus.client).await, "{service} {path}");
            lock.active.store(true, Ordering::SeqCst);
            assert!(!unlocked(&bus.client).await, "{service} {path}");
            bus.server.release_name(service).await.unwrap();
            assert!(!unlocked(&bus.client).await);
        }
    }
    #[tokio::test]
    async fn first_answer_wins_and_missing_service_is_never_unlock_evidence() {
        let bus = Bus::new().await;
        assert!(!unlocked(&bus.client).await);
        let primary = Lock::default();
        primary.active.store(true, Ordering::SeqCst);
        let legacy = Lock::default();
        let xfce = Lock::default();
        bus.server
            .object_server()
            .at(LOCK_ENDPOINTS[0].1, primary.clone())
            .await
            .unwrap();
        bus.server
            .object_server()
            .at(LOCK_ENDPOINTS[1].1, legacy.clone())
            .await
            .unwrap();
        bus.server
            .object_server()
            .at(LOCK_ENDPOINTS[2].1, XfceLock(xfce.clone()))
            .await
            .unwrap();
        bus.server.request_name(LOCK_ENDPOINTS[0].0).await.unwrap();
        bus.server.request_name(LOCK_ENDPOINTS[2].0).await.unwrap();
        assert!(!unlocked(&bus.client).await);
        assert_eq!(legacy.calls.load(Ordering::SeqCst), 0);
        assert_eq!(xfce.calls.load(Ordering::SeqCst), 0);
        primary.active.store(false, Ordering::SeqCst);
        assert!(unlocked(&bus.client).await);
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    #[derive(Default)]
    struct Capture {
        current: Option<Vec<(String, String, i32)>>,
        stage: Vec<(String, String, i32)>,
        count: i32,
        revision: i32,
        commits: usize,
        media: Vec<(i32, String, String)>,
    }
    struct Bridge(Arc<Mutex<Capture>>);
    #[zbus::interface(name = "org.arinn.KWinBridge1")]
    impl Bridge {
        fn begin_inventory(&self, _generation: &str, _revision: i32, count: i32) {
            let mut state = self.0.lock().unwrap();
            state.stage.clear();
            state.count = count;
        }
        #[allow(clippy::too_many_arguments)] // Existing scalar transport signature.
        fn put_window(
            &self,
            _generation: &str,
            _revision: i32,
            _index: i32,
            token: &str,
            desktop: &str,
            _output: &str,
            flags: i32,
        ) {
            self.0
                .lock()
                .unwrap()
                .stage
                .push((token.into(), desktop.into(), flags));
        }
        fn commit_inventory(&self, _generation: &str, revision: i32) -> zbus::fdo::Result<()> {
            let mut state = self.0.lock().unwrap();
            // The actual daemon rejects a new revision for identical wire fields.
            if state.current.as_ref() == Some(&state.stage)
                || state.stage.len() != state.count as usize
            {
                return Err(zbus::fdo::Error::Failed(
                    "identical or incomplete inventory".into(),
                ));
            }
            state.current = Some(state.stage.clone());
            state.revision = revision;
            state.commits += 1;
            Ok(())
        }
    }
    struct Media(Arc<Mutex<Capture>>);
    #[zbus::interface(name = "org.arinn.KWinMedia1")]
    impl Media {
        fn report(
            &self,
            _generation: &str,
            revision: i32,
            token: &str,
            launch: &str,
        ) -> zbus::fdo::Result<()> {
            let mut state = self.0.lock().unwrap();
            if revision != state.revision {
                return Err(zbus::fdo::Error::Failed("stale inventory revision".into()));
            }
            state.media.push((revision, token.into(), launch.into()));
            Ok(())
        }
    }
    struct Home;
    #[zbus::interface(name = "org.arinn.Daemon1")]
    impl Home {
        fn report_window(&self, _token: &str, _desktop: &str, _fullscreen: bool, _output: &str) {}
        fn report_home(&self, _visible: bool, _target: &str, _output: &str) {}
    }
    /// Records the Home reports and requests the adapter sends.
    struct Requests(Arc<Mutex<Vec<String>>>);
    #[zbus::interface(name = "org.arinn.Daemon1")]
    impl Requests {
        fn report_window(&self, _token: &str, _desktop: &str, _fullscreen: bool, _output: &str) {}
        fn report_home(&self, visible: bool, _target: &str, _output: &str) {
            self.0.lock().unwrap().push(format!("state {visible}"));
        }
        fn report_home_request(&self, target: &str, output: &str) {
            self.0
                .lock()
                .unwrap()
                .push(format!("home {target} {output}"));
        }
        fn report_quick_menu_request(&self, target: &str, output: &str) {
            self.0
                .lock()
                .unwrap()
                .push(format!("quick-menu {target} {output}"));
        }
        fn report_power_menu_request(&self, target: &str, output: &str) {
            self.0
                .lock()
                .unwrap()
                .push(format!("power-menu {target} {output}"));
        }
    }
    struct Returns;
    #[zbus::interface(name = "org.arinn.KWinReturn1")]
    impl Returns {
        fn report(&self, _generation: &str, _revision: i32, _target: &str, _token: &str) {}
    }
    /// ADR-0106: the quick-menu key is reported as its own request, with the
    /// Return target and the output of an explicit Home; Home keeps its
    /// request. The output arms the Activity background on X11 too (#499).
    /// ADR-0111: the power key likewise, with its own request.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quick_menu_key_reports_its_own_request() -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let bus = lock_tests::Bus::new().await;
            let capture = Arc::new(Mutex::new(Capture::default()));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let path = "/org/arinn/Daemon1";
            let server = zbus::connection::Builder::address(bus.address.trim())?
                .serve_at(path, Bridge(capture.clone()))?
                .serve_at(path, Media(capture.clone()))?
                .serve_at(path, Requests(requests.clone()))?
                .serve_at(path, Returns)?
                .build()
                .await?;
            let owner = server.unique_name().unwrap().clone();
            let home = HomeBridgeProxy::builder(&bus.client)
                .destination(owner.clone())?
                .build()
                .await?;
            let bridge = KWinBridgeProxy::builder(&bus.client)
                .destination(owner.clone())?
                .build()
                .await?;
            let media = KWinMediaProxy::builder(&bus.client)
                .destination(owner.clone())?
                .build()
                .await?;
            let returns = KWinReturnProxy::builder(&bus.client)
                .destination(owner)?
                .build()
                .await?;
            let token = Uuid::new_v4().to_string();
            let mut update = Update {
                snapshot: Snapshot {
                    rows: vec![Row {
                        xid: 42,
                        token: token.clone(),
                        desktop_id: "org.example.Player.desktop".into(),
                        launch: String::new(),
                        flags: 4,
                    }],
                    shell: Some(43),
                    shell_token: Uuid::new_v4().to_string(),
                    active: Some(42),
                    home_key: false,
                    quick_menu_key: true,
                    power_key: false,
                    power_reading: native::Reading::None,
                    compositor: true,
                },
                home: true,
                target: token.clone(),
                explicit: Some(Request::QuickMenu),
            };
            let generation = Uuid::new_v4().to_string();
            let mut revision = 0;
            let mut previous = Vec::new();
            let mut previous_home = None;
            for explicit in [
                Some(Request::QuickMenu),
                Some(Request::QuickMenu),
                None,
                Some(Request::Home),
                Some(Request::PowerMenu),
                Some(Request::PowerMenu),
            ] {
                update.explicit = explicit;
                publish(
                    &home,
                    &bridge,
                    &media,
                    &returns,
                    &generation,
                    &mut revision,
                    &mut previous,
                    &mut previous_home,
                    &update,
                )
                .await?;
            }
            // Every press is a request, identical repeats included; an
            // unchanged state without a press reports nothing.
            let output = background::OUTPUT;
            assert_eq!(
                *requests.lock().unwrap(),
                [
                    format!("quick-menu {token} {output}"),
                    format!("quick-menu {token} {output}"),
                    format!("home {token} {output}"),
                    format!("power-menu {token} {output}"),
                    format!("power-menu {token} {output}"),
                ]
            );
            Ok(())
        })
        .await?
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn media_identity_changes_do_not_resubmit_identical_inventory() -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let bus = lock_tests::Bus::new().await;
            let capture = Arc::new(Mutex::new(Capture::default()));
            let path = "/org/arinn/Daemon1";
            let server = zbus::connection::Builder::address(bus.address.trim())?
                .serve_at(path, Bridge(capture.clone()))?
                .serve_at(path, Media(capture.clone()))?
                .serve_at(path, Home)?
                .build()
                .await?;
            let owner = server.unique_name().unwrap().clone();
            let home = HomeBridgeProxy::builder(&bus.client)
                .destination(owner.clone())?
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await?;
            let bridge = KWinBridgeProxy::builder(&bus.client)
                .destination(owner.clone())?
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await?;
            let media = KWinMediaProxy::builder(&bus.client)
                .destination(owner.clone())?
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await?;
            let returns = KWinReturnProxy::builder(&bus.client)
                .destination(owner)?
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await?;

            let token = Uuid::new_v4().to_string();
            let launch = Uuid::new_v4().to_string();
            let other_launch = Uuid::new_v4().to_string();
            let generation = Uuid::new_v4().to_string();
            let mut update = Update {
                snapshot: Snapshot {
                    rows: vec![Row {
                        xid: 42,
                        token: token.clone(),
                        desktop_id: "org.example.Player.desktop".into(),
                        launch: launch.clone(),
                        flags: 4,
                    }],
                    shell: Some(43),
                    shell_token: Uuid::new_v4().to_string(),
                    active: Some(42),
                    home_key: false,
                    quick_menu_key: false,
                    power_key: false,
                    power_reading: native::Reading::None,
                    compositor: true,
                },
                home: false,
                target: String::new(),
                explicit: None,
            };
            let mut revision = 0;
            let mut previous = Vec::new();
            let mut previous_home = None;
            for identity in [&launch, "", &launch, &other_launch] {
                update.snapshot.rows[0].launch = identity.into();
                tokio::time::timeout(
                    Duration::from_secs(3),
                    publish(
                        &home,
                        &bridge,
                        &media,
                        &returns,
                        &generation,
                        &mut revision,
                        &mut previous,
                        &mut previous_home,
                        &update,
                    ),
                )
                .await??;
                assert_eq!(revision, 1);
            }
            // An unchanged tick emits neither inventory nor media updates.
            publish(
                &home,
                &bridge,
                &media,
                &returns,
                &generation,
                &mut revision,
                &mut previous,
                &mut previous_home,
                &update,
            )
            .await?;
            {
                let state = capture.lock().unwrap();
                assert_eq!(state.commits, 1);
                assert_eq!(
                    state.media,
                    [&launch, "", &launch, &other_launch].map(|id| (
                        1,
                        token.clone(),
                        id.to_string()
                    ))
                );
            }
            // A real wire-field change allocates a revision and re-reports media.
            update.snapshot.rows[0].flags |= 2;
            publish(
                &home,
                &bridge,
                &media,
                &returns,
                &generation,
                &mut revision,
                &mut previous,
                &mut previous_home,
                &update,
            )
            .await?;
            let state = capture.lock().unwrap();
            assert_eq!(state.commits, 2);
            assert_eq!(revision, 2);
            assert_eq!(state.media.last(), Some(&(2, token, other_launch)));
            Ok(())
        })
        .await?
    }
}
