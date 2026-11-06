// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider-private domain primitives, not a D-Bus or SDK contract.
//! Stream URLs, headers, tokens and filesystem paths never belong in Track.
pub mod paths;
pub mod playback;
pub mod player;
pub mod queue;
pub mod storage;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub artist_ids: Vec<String>,
    pub album: String,
    pub album_artist: String,
    pub album_id: String,
    pub disc: u32,
    pub number: u32,
    pub duration_us: u64,
    pub available: bool,
    pub embedded_artwork: bool,
    /// When the track joined the library, in Unix seconds; 0 when unknown
    /// (#806). Album and artist groups take their newest track's date.
    #[serde(default)]
    pub added_at: u64,
}

/// Keep provider metadata bounded and single-line even before wire validation.
pub fn text(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(1024)
        .collect::<String>()
        .trim()
        .to_owned()
}
