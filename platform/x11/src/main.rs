// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod native;
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
}
#[derive(Clone)]
struct Update {
    snapshot: Snapshot,
    home: bool,
    target: String,
    explicit_home: bool,
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
        let mut native = Native::connect()?;
        let snapshot = native.snapshot()?;
        Ok(Self {
            native,
            snapshot,
            home: false,
            target: String::new(),
            presentation: None,
            media_allowed: false,
        })
    }
    fn row(&self, token: &str) -> Option<&Row> {
        self.snapshot.rows.iter().find(|r| r.token == token)
    }
    fn update(&self, explicit_home: bool) -> Update {
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
            explicit_home,
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
        self.snapshot = self.native.snapshot()?;
        self.media_allowed = unlocked && self.snapshot.compositor;
        if self
            .presentation
            .as_ref()
            .is_some_and(|p| self.row(&p.token).is_none_or(|r| r.launch != p.launch))
        {
            self.presentation = None;
            if !self.home
                && let Some(shell) = self.snapshot.shell
            {
                self.native.raise_shell(shell, false)?;
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
        let mut explicit = false;
        if self.snapshot.home_key && unlocked {
            explicit = self
                .command(&Command::Home("home".into()), unlocked)
                .is_empty();
        }
        if unlocked
            && (self.home || self.presentation.is_some())
            && let Some(shell) = self.snapshot.shell
        {
            self.native.raise_shell(shell, true)?;
        }
        Ok(self.update(explicit))
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
                    if self.native.raise_shell(shell, true).is_err() {
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
async fn apply(
    actor: &Arc<Mutex<Actor>>,
    command: Command,
    unlocked: bool,
) -> Result<(&'static str, Update)> {
    let actor = actor.clone();
    Ok(tokio::task::spawn_blocking(move || {
        let mut actor = actor.lock().unwrap();
        let result = actor.command(&command, unlocked);
        let explicit = matches!(&command, Command::Home(s) if s == "home") && result.is_empty();
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
    if *previous != *rows || *revision == 0 {
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
        for row in rows {
            media
                .report(generation, *revision, &row.token, &row.launch)
                .await?;
        }
        *previous = rows.clone();
    }
    let active = rows.iter().find(|r| Some(r.xid) == update.snapshot.active);
    home.report_window(
        active.map_or("", |r| r.token.as_str()),
        active.map_or("", |r| r.desktop_id.as_str()),
        active.is_some_and(|r| r.flags & 2 != 0),
        "",
    )
    .await?;
    let state = (update.home, update.target.clone());
    if update.explicit_home {
        home.report_home_request(&update.target, "").await?;
    } else if previous_home.as_ref() != Some(&state) {
        home.report_home(update.home, &update.target, "").await?;
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
async fn session() -> Result<()> {
    let actor = Arc::new(Mutex::new(
        tokio::task::spawn_blocking(Actor::connect).await??,
    ));
    let result = run(&actor).await;
    let cleanup = actor.clone();
    tokio::task::spawn_blocking(move || {
        let mut actor = cleanup.lock().unwrap();
        // Refresh identities before restoring presentation state on shutdown.
        if let Ok(snapshot) = actor.native.snapshot() {
            actor.snapshot = snapshot;
            actor.release();
        }
        if let Some(shell) = actor.snapshot.shell {
            let _ = actor.native.raise_shell(shell, false);
        }
    })
    .await?;
    result
}
async fn run(actor: &Arc<Mutex<Actor>>) -> Result<()> {
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
                publish(&home, &bridge, &media, &returns, &generation, &mut revision, &mut previous, &mut previous_home, &fresh).await?;
                let (error, update) = apply(actor, pending.command, allowed).await?;
                (update, Some((pending.done, error)))
            },
            ended = tasks.join_next() => { ended.ok_or_else(|| anyhow::anyhow!("pollers ended"))???; anyhow::bail!("poller ended"); },
            _ = shutdown() => return Ok(()),
        };
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
        if let Some((done, error)) = completion {
            let _ = done.send(error);
        }
    }
}
async fn shutdown() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("SIGTERM handler");
    tokio::select! { _ = tokio::signal::ctrl_c() => (), _ = term.recv() => () }
}
#[tokio::main]
async fn main() -> Result<()> {
    ensure!(
        std::env::var("ARINN_PLATFORM_BACKEND").as_deref() == Ok("ewmh"),
        "explicit EWMH backend selection required"
    );
    eprintln!("arinn-platform-x11: selected backend ewmh (org.arinn.X11)");
    // No native payloads in logs. Reconnect creates fresh tokens and bridge generation.
    loop {
        if session().await.is_ok() {
            return Ok(());
        }
        eprintln!("arinn-platform-x11: backend unavailable; retrying");
        tokio::select! { _ = tokio::time::sleep(Duration::from_secs(3)) => (), _ = shutdown() => return Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use x11rb::protocol::xproto::ConnectionExt as _;
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
    struct Bus {
        child: Child,
        client: Connection,
        server: Connection,
    }
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
    impl Bus {
        async fn new() -> Self {
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
