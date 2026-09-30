// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Seek thumbnails from a separate ffmpeg process. The main player is never
//! seeked. At most one extraction runs; a newer request cancels the older one.

use crate::paths::ensure_private_dir;
use arinn_protocol::{media::Thumbnail, validation::ErrorCode};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::{process::Command, sync::Mutex};

const INTERVAL: Duration = Duration::from_millis(200);
const DEADLINE: Duration = Duration::from_secs(4);

struct Slot {
    cancel: tokio::sync::watch::Sender<bool>,
}

pub struct Extractor {
    slot: Mutex<Option<Slot>>,
    last_start: Mutex<Instant>,
}

impl Default for Extractor {
    fn default() -> Self {
        Self {
            slot: Mutex::new(None),
            last_start: Mutex::new(Instant::now() - INTERVAL),
        }
    }
}

impl Extractor {
    pub async fn extract(
        &self,
        ffmpeg: &Path,
        directory: &Path,
        source: &Path,
        position_us: i64,
    ) -> Result<Thumbnail, ErrorCode> {
        if !ffmpeg.is_file() || position_us < 0 {
            return Err(ErrorCode::Unsupported);
        }
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        {
            let mut slot = self.slot.lock().await;
            if let Some(previous) = slot.replace(Slot { cancel: cancel_tx }) {
                let _ = previous.cancel.send(true);
            }
        }
        let wait = {
            let last = *self.last_start.lock().await;
            INTERVAL.saturating_sub(last.elapsed())
        };
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = cancel_rx.changed() => return Err(ErrorCode::Cancelled),
        }
        if *cancel_rx.borrow() {
            return Err(ErrorCode::Cancelled);
        }
        *self.last_start.lock().await = Instant::now();
        ensure_private_dir(directory).map_err(|_| ErrorCode::Unsupported)?;
        let asset = uuid::Uuid::new_v4().to_string();
        let output = directory.join(format!("{asset}.jpg"));
        let mut child = Command::new(ffmpeg)
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-ss")
            .arg(crate::parse::format_seconds(position_us))
            .arg("-i")
            .arg(source)
            .arg("-frames:v")
            .arg("1")
            .arg("-vf")
            .arg("scale=320:-2")
            .arg("-f")
            .arg("image2")
            .arg("-y")
            .arg(&output)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| ErrorCode::Unsupported)?;
        let status = tokio::select! {
            status = child.wait() => status.map_err(|_| ErrorCode::Unsupported)?,
            _ = cancel_rx.changed() => {
                let _ = child.kill().await;
                let _ = std::fs::remove_file(&output);
                return Err(ErrorCode::Cancelled);
            }
            _ = tokio::time::sleep(DEADLINE) => {
                let _ = child.kill().await;
                let _ = std::fs::remove_file(&output);
                return Err(ErrorCode::Unsupported);
            }
        };
        if !status.success() {
            let _ = std::fs::remove_file(&output);
            return Err(ErrorCode::Unsupported);
        }
        let metadata = std::fs::metadata(&output).map_err(|_| ErrorCode::Unsupported)?;
        if metadata.len() == 0 {
            let _ = std::fs::remove_file(&output);
            return Err(ErrorCode::Unsupported);
        }
        let _ = std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o600));
        Ok(Thumbnail {
            asset,
            timestamp_us: position_us,
        })
    }
}

pub fn ffmpeg_binary() -> Option<PathBuf> {
    let path = PathBuf::from("/usr/bin/ffmpeg");
    path.is_file().then_some(path)
}

pub fn cache_dir() -> Result<PathBuf, ErrorCode> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .ok_or(ErrorCode::Unavailable)?;
    let dir = base.join("arinn/mpv");
    ensure_private_dir(&dir)?;
    Ok(dir)
}
