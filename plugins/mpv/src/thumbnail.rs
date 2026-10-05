// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Seek thumbnails from a separate ffmpeg process. The main player is never
//! seeked. At most one extraction runs; a newer request cancels the older one.

use crate::paths::ensure_private_dir;
use arinn_plugin_sdk::runtime::AssetHandler;
use arinn_protocol::{
    assets::{self, Asset, MAX_ASSET_BYTES},
    media::Thumbnail,
    validation::ErrorCode,
};
use std::{
    future::Future,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    pin::Pin,
    time::{Duration, Instant},
};
use tokio::{process::Command, sync::Mutex};

const INTERVAL: Duration = Duration::from_millis(200);
const DEADLINE: Duration = Duration::from_secs(4);
/// Successful frames kept in the cache. Older JPEGs are removed.
pub const KEPT_THUMBNAILS: usize = 8;

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
        let mut output_guard = TemporaryFrame::new(directory.join(format!("{asset}.jpg")));
        let output = output_guard.path().to_path_buf();
        let mut child = Command::new(ffmpeg)
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-protocol_whitelist")
            .arg("file")
            .arg("-ss")
            .arg(crate::parse::format_seconds(position_us))
            .arg("-i")
            .arg(format!("file:{}", source.display()))
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
        validate_frame(&output)?;
        output_guard.keep = true;
        prune(directory, KEPT_THUMBNAILS);
        Ok(Thumbnail {
            asset,
            timestamp_us: position_us,
        })
    }
}

fn prune(directory: &Path, keep: usize) {
    let mut frames = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jpg") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        frames.push((modified, path));
    }
    frames.sort_by_key(|(modified, _)| *modified);
    let extra = frames.len().saturating_sub(keep);
    for (_, path) in frames.into_iter().take(extra) {
        let _ = std::fs::remove_file(path);
    }
}

/// Serves the extracted frames as `AssetProvider1` assets (ADR-0062): the
/// asset id of a thumbnail is the canonical UUID that names its JPEG in the
/// cache. Anything else is `NotFound`, so an id can never name a path.
pub struct ThumbnailAssets {
    directory: PathBuf,
}

impl ThumbnailAssets {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn read(&self, id: &str) -> Result<Asset, ErrorCode> {
        let canonical = uuid::Uuid::parse_str(id)
            .ok()
            .filter(|uuid| !uuid.is_nil() && uuid.to_string() == id)
            .ok_or(ErrorCode::NotFound)?;
        let name = format!("{canonical}.jpg");
        let file = std::fs::File::open(self.directory.join(&name))
            .or_else(|_| std::fs::File::open(poster_dir(&self.directory).join(&name)))
            .map_err(|_| ErrorCode::NotFound)?;
        let mut data = Vec::new();
        // One byte more than allowed tells an oversized file apart.
        file.take(MAX_ASSET_BYTES as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|_| ErrorCode::NotFound)?;
        if data.len() > MAX_ASSET_BYTES || assets::sniff(&data) != Some(assets::MIME_JPEG) {
            return Err(ErrorCode::NotFound);
        }
        Ok(Asset {
            mime: assets::MIME_JPEG.into(),
            data,
        })
    }
}

impl AssetHandler for ThumbnailAssets {
    fn asset(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Asset, ErrorCode>> + Send + '_>> {
        Box::pin(async move { self.read(&id) })
    }
}

/// Whether a session card frame (#292) is due: a new session, playback
/// that moved on by at least `step_us`, or a paused or stopped session at a
/// new position.
pub fn poster_due(
    last: Option<&(String, i64)>,
    id: &str,
    position_us: i64,
    playing: bool,
    step_us: i64,
) -> bool {
    match last {
        Some((last_id, last_position)) if last_id == id => {
            if playing {
                (position_us - last_position).abs() >= step_us
            } else {
                position_us != *last_position
            }
        }
        _ => true,
    }
}

/// Session card frames (#292) live apart from seek thumbnails, so neither
/// pruning removes the other.
pub fn poster_dir(thumb_dir: &Path) -> PathBuf {
    thumb_dir.join("posters")
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

/// Remove captures and interrupted extracts even when their future is dropped.
pub(crate) struct TemporaryFrame {
    path: PathBuf,
    keep: bool,
}
impl TemporaryFrame {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path, keep: false }
    }
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for TemporaryFrame {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
pub(crate) fn validate_frame(path: &Path) -> Result<(), ErrorCode> {
    let metadata = std::fs::metadata(path).map_err(|_| ErrorCode::Unsupported)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ASSET_BYTES as u64 {
        return Err(ErrorCode::Unsupported);
    }
    let bytes = std::fs::read(path).map_err(|_| ErrorCode::Unsupported)?;
    if assets::sniff(&bytes) != Some(assets::MIME_JPEG) {
        return Err(ErrorCode::Unsupported);
    }
    Ok(())
}
