// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KWin-specific bridge, isolated from capability routing.
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
}
#[derive(Clone)]
pub struct Platform {
    state: Arc<Mutex<State>>,
    tx: mpsc::Sender<String>,
    rx: Arc<Mutex<mpsc::Receiver<String>>>,
}
impl Default for Platform {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel(8);
        Self {
            state: Arc::default(),
            tx,
            rx: Arc::new(Mutex::new(rx)),
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
    async fn authorize(c: &Connection, h: &Header<'_>) -> production::Result<()> {
        let dbus = zbus::fdo::DBusProxy::new(c)
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let owner = dbus
            .get_name_owner("org.kde.KWin".try_into().unwrap())
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
    pub async fn take_command(&self, c: &Connection, h: Header<'_>) -> production::Result<String> {
        Self::authorize(c, &h).await?;
        self.state.lock().await.seen = Some(Instant::now());
        Ok(
            tokio::time::timeout(Duration::from_secs(20), self.rx.lock().await.recv())
                .await
                .ok()
                .flatten()
                .unwrap_or("idle".into()),
        )
    }
    pub async fn report_home(
        &self,
        c: &Connection,
        h: Header<'_>,
        visible: bool,
        target: &str,
        output: &str,
    ) -> production::Result<()> {
        Self::authorize(c, &h).await?;
        validation::text(target)?;
        validation::text(output)?;
        let mut s = self.state.lock().await;
        s.home = visible;
        s.target = target.into();
        s.home_output = output.into();
        Ok(())
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
