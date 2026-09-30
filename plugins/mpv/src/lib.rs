// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! External mpv as an Arinn `media` provider (spec §7.4, ADR-0024, ADR-0057).
//! File paths and titles are not logged.

mod ipc;
mod parse;
mod paths;
mod player;
mod scope;
mod thumbnail;

pub use paths::{ensure_private_dir, file_path, validate_runtime};
pub use player::{Launch, Player, Settings, View};
pub use scope::{UNIT, arguments, ensure_unit, socket_is_private};
pub use thumbnail::Extractor;

use arinn_plugin_sdk::runtime::{
    MediaAction, MediaExtensions, MediaHandler, MediaOpenHandler, MediaService,
    MediaTimelineHandler, MediaTracksHandler, run_media_on,
};
use arinn_protocol::{
    media::{MediaSource, Thumbnail, Timeline, Track},
    validation::ErrorCode,
};
use std::{future::Future, pin::Pin, time::Duration};
use zbus::Connection;

/// One JSON line for a provider failure. `operation` is a fixed token.
pub fn log_line(operation: &str, error: impl std::fmt::Display) -> String {
    format!(
        "{{\"level\":\"error\",\"component\":\"arinn-plugin-mpv\",\"operation\":\"{operation}\",\"error\":\"{error}\"}}"
    )
}

/// Serve `MediaProvider1`, `MediaOpen1`, `MediaTracks1` and `MediaTimeline1`.
pub async fn serve(connection: Connection) -> Result<(), ErrorCode> {
    let socket = paths::socket_path()?;
    let state_path = socket
        .parent()
        .map(|dir| dir.join("session"))
        .ok_or(ErrorCode::Unavailable)?;
    let launch = match scope::mpv_binary() {
        Ok(binary) => Launch::Systemd { binary },
        Err(ErrorCode::Unavailable) => Launch::Missing,
        Err(error) => return Err(error),
    };
    let backend = if matches!(launch, Launch::Missing) {
        "unavailable"
    } else {
        "available"
    };
    let thumb_dir = thumbnail::cache_dir().ok();
    let ffmpeg = thumb_dir.as_ref().and_then(|_| thumbnail::ffmpeg_binary());
    let player = Player::start(Settings {
        socket,
        state_path,
        launch,
        publish_interval: Duration::from_secs(1),
        ffmpeg,
        thumb_dir: thumb_dir.unwrap_or_default(),
        backend: backend.to_string(),
    })?;
    let bridge = Bridge(player.clone());
    let service = MediaService::with_extensions(
        bridge.clone(),
        Vec::new(),
        backend,
        MediaExtensions::default()
            .open(bridge.clone())
            .tracks(bridge.clone())
            .timeline(bridge),
    )?;
    let mut notes = player.subscribe();
    let publish = publish(service.clone(), &mut notes);
    tokio::pin!(publish);
    tokio::select! {
        result = run_media_on(connection, service) => result.map_err(|_| ErrorCode::Unavailable),
        result = &mut publish => result,
        _ = tokio::signal::ctrl_c() => Ok(()),
    }
}

async fn publish(
    service: MediaService,
    notes: &mut tokio::sync::watch::Receiver<View>,
) -> Result<(), ErrorCode> {
    let mut registrations = service.registrations();
    let mut seen_snapshot = 0u64;
    let mut seen_touch = 0u64;
    loop {
        if registrations.borrow().is_none() {
            registrations
                .changed()
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            continue;
        }
        let current = notes.borrow().clone();
        if current.snapshot != seen_snapshot {
            service
                .replace(current.sessions.clone(), &current.backend)
                .await?;
            seen_snapshot = current.snapshot;
            seen_touch = current.touch;
        } else if current.touch != seen_touch {
            match service.touch().await {
                Ok(()) => seen_touch = current.touch,
                Err(ErrorCode::Unavailable) => {}
                Err(error) => return Err(error),
            }
        }
        tokio::select! {
            changed = notes.changed() => {
                changed.map_err(|_| ErrorCode::Unavailable)?;
            }
            changed = registrations.changed() => {
                changed.map_err(|_| ErrorCode::Unavailable)?;
            }
        }
    }
}

#[derive(Clone)]
struct Bridge(Player);

impl Bridge {
    fn player(&self) -> Player {
        self.0.clone()
    }
}

impl MediaHandler for Bridge {
    fn control(
        &self,
        action: MediaAction,
        id: String,
        position_us: i64,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        let player = self.player().clone();
        Box::pin(async move { player.control(action, id, position_us).await })
    }
}

impl MediaOpenHandler for Bridge {
    fn open(
        &self,
        source: MediaSource,
    ) -> Pin<Box<dyn Future<Output = Result<String, ErrorCode>> + Send + '_>> {
        let player = self.player().clone();
        Box::pin(async move { player.open(source).await })
    }
}

impl MediaTracksHandler for Bridge {
    fn list(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Track>, ErrorCode>> + Send + '_>> {
        let player = self.player().clone();
        Box::pin(async move { player.tracks(id).await })
    }
    fn select(
        &self,
        id: String,
        kind: String,
        track_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        let player = self.player().clone();
        Box::pin(async move { player.select(id, kind, track_id).await })
    }
    fn disable_subtitles(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        let player = self.player().clone();
        Box::pin(async move { player.disable_subtitles(id).await })
    }
}

impl MediaTimelineHandler for Bridge {
    fn timeline(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Timeline, ErrorCode>> + Send + '_>> {
        let player = self.player().clone();
        Box::pin(async move { player.timeline(id).await })
    }
    fn thumbnail(
        &self,
        id: String,
        position_us: i64,
    ) -> Pin<Box<dyn Future<Output = Result<Thumbnail, ErrorCode>> + Send + '_>> {
        let player = self.player().clone();
        Box::pin(async move { player.thumbnail(id, position_us).await })
    }
}
