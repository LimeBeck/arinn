// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Trusted platform bridge, isolated from capability routing.
use arinn_protocol::{
    Status, production,
    validation::{self, ErrorCode},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, mpsc};
use zbus::{Connection, message::Header};
#[derive(Default)]
struct State {
    home: bool,
    target: String,
    active: String,
    class: String,
    fullscreen: bool,
    home_output: String,
    active_output: String,
    seen: Option<Instant>,
    /// Unique bus name of the adapter connection that last polled.
    adapter: String,
}
#[derive(Clone)]
pub struct Platform {
    state: Arc<Mutex<State>>,
    tx: mpsc::Sender<String>,
    rx: Arc<Mutex<mpsc::Receiver<String>>>,
    /// The quick-menu keys written to KDE (ADR-0106).
    pub quick_menu: crate::global_home::QuickMenuKeys,
}
impl Default for Platform {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel(8);
        Self {
            state: Arc::default(),
            tx,
            rx: Arc::new(Mutex::new(rx)),
            quick_menu: Default::default(),
        }
    }
}
impl Platform {
    pub async fn status(&self, epoch: &str, available: bool) -> Status {
        let s = self.state.lock().await;
        Status {
            epoch: epoch.into(),
            provider_available: available,
            generation: epoch.into(),
            revision: 0,
            home_visible: s.home,
            return_window: s.target.clone(),
            active_window: s.active.clone(),
            active_class: s.class.clone(),
            active_fullscreen: s.fullscreen,
            platform_connected: s
                .seen
                .is_some_and(|t| t.elapsed() < Duration::from_secs(30)),
            home_output: s.home_output.clone(),
            active_output: s.active_output.clone(),
        }
    }
    pub(crate) async fn authorize(c: &Connection, h: &Header<'_>) -> production::Result<()> {
        let dbus = zbus::fdo::DBusProxy::new(c)
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let owner = dbus
            .get_name_owner(
                adapter_service()
                    .ok_or(ErrorCode::PermissionDenied)?
                    .try_into()
                    .unwrap(),
            )
            .await
            .map_err(|_| ErrorCode::PermissionDenied)?;
        if h.sender().map(|s| s.as_str()) != Some(owner.as_str()) {
            return Err(ErrorCode::PermissionDenied.into());
        }
        Ok(())
    }
    pub async fn queue(&self, command: &str) -> production::Result<()> {
        if self
            .state
            .lock()
            .await
            .seen
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(30))
        {
            return Err(ErrorCode::Unavailable.into());
        }
        self.tx
            .try_send(command.into())
            .map_err(|_| ErrorCode::Unavailable.into())
    }
    /// Authorizes an adapter poll and returns whether it comes from a new
    /// adapter connection, which then needs its connection-time setup.
    pub async fn poll(&self, c: &Connection, h: &Header<'_>) -> production::Result<bool> {
        Self::authorize(c, h).await?;
        let sender = h.sender().map(|s| s.to_string()).unwrap_or_default();
        let mut s = self.state.lock().await;
        s.seen = Some(Instant::now());
        Ok(std::mem::replace(&mut s.adapter, sender.clone()) != sender)
    }
    /// The next queued command after an authorized `poll`, or `idle`.
    pub async fn next_command(&self) -> String {
        tokio::time::timeout(Duration::from_secs(20), self.rx.lock().await.recv())
            .await
            .ok()
            .flatten()
            .unwrap_or("idle".into())
    }
    pub async fn take_command(&self, c: &Connection, h: Header<'_>) -> production::Result<String> {
        self.poll(c, &h).await?;
        Ok(self.next_command().await)
    }
    // Caller authenticates and validates before acquiring the inventory lock.
    pub(crate) async fn commit_home(&self, visible: bool, target: &str, output: &str) {
        let mut s = self.state.lock().await;
        s.home = visible;
        s.target = target.into();
        s.home_output = output.into();
    }
    /// The output of the last committed Home report.
    pub async fn home_output(&self) -> String {
        self.state.lock().await.home_output.clone()
    }
    pub async fn report_window(
        &self,
        c: &Connection,
        h: Header<'_>,
        id: &str,
        class: &str,
        fullscreen: bool,
        output: &str,
    ) -> production::Result<()> {
        Self::authorize(c, &h).await?;
        for value in [id, class, output] {
            validation::text(value)?;
        }
        let mut s = self.state.lock().await;
        s.active = id.into();
        s.class = class.into();
        s.fullscreen = fullscreen;
        s.active_output = output.into();
        Ok(())
    }
}

/// Decides whether a KDE input method process is the one KWin started:
/// `(input method pid, KWin pid) -> trusted`. Production uses
/// [`is_direct_child`]; tests replace it, because a test bus cannot fork the
/// fake KWin (ADR-0105).
pub type ChildCheck = fn(u32, u32) -> bool;

/// The parent of `pid` from `/proc/<pid>/stat`, if the process exists.
pub fn parent_pid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name is in parentheses and may contain spaces or ')'.
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(1)?.parse().ok()
}

/// Whether `child` is a direct child process of `parent`.
pub fn is_direct_child(child: u32, parent: u32) -> bool {
    child != parent && parent_pid(child) == Some(parent)
}

/// Authorizes the keyboard deliverer of ADR-0105 and returns its unique
/// name: on EWMH the owner of `org.arinn.X11`; on KDE the owner of
/// `org.arinn.InputMethod` whose process KWin (the `org.kde.KWin` owner)
/// started itself.
pub async fn authorize_deliverer(
    c: &Connection,
    h: &Header<'_>,
    child: ChildCheck,
) -> production::Result<String> {
    let sender = h
        .sender()
        .map(|s| s.to_string())
        .ok_or(ErrorCode::PermissionDenied)?;
    let dbus = zbus::fdo::DBusProxy::new(c)
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    let owner_of = |name: &'static str| {
        let dbus = dbus.clone();
        async move {
            dbus.get_name_owner(name.try_into().unwrap())
                .await
                .map(|o| o.to_string())
                .map_err(|_| ErrorCode::PermissionDenied)
        }
    };
    match adapter_service().ok_or(ErrorCode::PermissionDenied)? {
        "org.arinn.X11" => {
            if owner_of("org.arinn.X11").await? != sender {
                return Err(ErrorCode::PermissionDenied.into());
            }
        }
        _ => {
            if owner_of(arinn_protocol::keyboard::INPUT_METHOD_SERVICE).await? != sender {
                return Err(ErrorCode::PermissionDenied.into());
            }
            let kwin = owner_of("org.kde.KWin").await?;
            let pid = |name: String| {
                let dbus = dbus.clone();
                async move {
                    dbus.get_connection_unix_process_id(name.as_str().try_into().unwrap())
                        .await
                        .map_err(|_| ErrorCode::PermissionDenied)
                }
            };
            if !child(pid(sender.clone()).await?, pid(kwin).await?) {
                return Err(ErrorCode::PermissionDenied.into());
            }
        }
    }
    Ok(sender)
}

/// One backend per daemon process; never accept both adapter owners at once.
pub fn adapter_service() -> Option<&'static str> {
    match std::env::var("ARINN_PLATFORM_BACKEND") {
        Ok(value) => selected_service(Some(&value)),
        Err(std::env::VarError::NotPresent) => selected_service(None),
        Err(std::env::VarError::NotUnicode(_)) => None,
    }
}
fn selected_service(value: Option<&str>) -> Option<&'static str> {
    match value {
        None | Some("kde") => Some("org.kde.KWin"),
        Some("ewmh") => Some("org.arinn.X11"),
        // Invalid configuration fails closed instead of trusting KWin implicitly.
        _ => None,
    }
}
#[cfg(test)]
mod selection_tests {
    #[test]
    fn explicit_backend_selects_one_trusted_owner() {
        use super::selected_service;
        assert_eq!(selected_service(None), Some("org.kde.KWin"));
        assert_eq!(selected_service(Some("kde")), Some("org.kde.KWin"));
        assert_eq!(selected_service(Some("ewmh")), Some("org.arinn.X11"));
        assert_eq!(selected_service(Some("auto")), None);
    }
    #[test]
    fn parent_pid_reads_proc() {
        use super::{is_direct_child, parent_pid};
        let me = std::process::id();
        assert_eq!(parent_pid(me), Some(std::os::unix::process::parent_id()));
        assert!(is_direct_child(me, std::os::unix::process::parent_id()));
        assert!(!is_direct_child(me, me));
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap();
        assert!(is_direct_child(child.id(), me));
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(parent_pid(u32::MAX), None);
    }
}
