// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Api, Collection, Error, parse};
use arinn_music_common::Track;
use arinn_protocol::music::Entity;
use reqwest::Method;
use serde_json::Value;
impl Api {
    /// OAuth identity is not Music API eligibility. Probe both the account and a
    /// music-library endpoint before the native source can report connected.
    pub(crate) async fn check_music_access(&self) -> Result<(), Error> {
        let uid = self.account_uid().await?;
        let library = self
            .request(Method::GET, &["users", &uid, "likes", "tracks"], &[])
            .await?;
        if !library["library"]["tracks"].is_array() {
            return Err(Error::InvalidResponse);
        }
        Ok(())
    }
    pub async fn account_uid(&self) -> Result<String, Error> {
        let value = self
            .request(Method::GET, &["account", "status"], &[])
            .await?;
        let uid = &value["account"]["uid"];
        let uid = uid
            .as_str()
            .map(str::to_owned)
            .or_else(|| uid.as_u64().map(|n| n.to_string()))
            .ok_or(Error::InvalidResponse)?;
        if !parse::digits(&uid) {
            return Err(Error::InvalidResponse);
        }
        Ok(uid)
    }
    pub async fn liked_tracks(&self, uid: &str) -> Result<Vec<Track>, Error> {
        if !parse::digits(uid) {
            return Err(Error::InvalidInput);
        }
        let value = self
            .request(Method::GET, &["users", uid, "likes", "tracks"], &[])
            .await?;
        let values = value["library"]["tracks"]
            .as_array()
            .ok_or(Error::InvalidResponse)?;
        if values.len() > 10_000 {
            return Err(Error::TooLarge);
        }
        let mut ids = Vec::new();
        // The like time of each track, by its bare track ID: when the track
        // joined the user's library (ADR-0142 MH-9, #813).
        let mut liked = std::collections::HashMap::new();
        for track in values {
            let id = track["id"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| track["id"].as_u64().map(|n| n.to_string()))
                .ok_or(Error::InvalidResponse)?;
            if !parse::track_id(&id) {
                return Err(Error::InvalidResponse);
            }
            let at = parse::timestamp(track.get("timestamp"));
            if at > 0 {
                let bare = id.split(':').next().unwrap_or(&id).to_owned();
                let known: &mut u64 = liked.entry(bare).or_default();
                *known = (*known).max(at);
            }
            ids.push(id);
        }
        let mut tracks = Vec::new();
        for chunk in ids.chunks(100) {
            append_tracks(&mut tracks, self.tracks(chunk).await?)?;
        }
        for track in &mut tracks {
            let bare = track.id.split(':').next().unwrap_or(&track.id);
            track.added_at = liked.get(bare).copied().unwrap_or(0);
        }
        Ok(tracks)
    }
    pub async fn playlist_tracks(&self, uid: &str, kind: &str) -> Result<Vec<Track>, Error> {
        if !parse::digits(uid) || !parse::digits(kind) {
            return Err(Error::InvalidInput);
        }
        let value = self
            .request(Method::GET, &["users", uid, "playlists", kind], &[])
            .await?;
        let values = value["tracks"].as_array().ok_or(Error::InvalidResponse)?;
        if values.len() > 10_000 {
            return Err(Error::TooLarge);
        }
        let mut tracks = Vec::new();
        for chunk in values.chunks(200) {
            append_tracks(&mut tracks, parse::tracks(&Value::Array(chunk.to_vec()))?)?;
        }
        Ok(tracks)
    }
    pub async fn album_tracks(&self, id: &str) -> Result<Vec<Track>, Error> {
        if !parse::digits(id) {
            return Err(Error::InvalidInput);
        }
        let value = self
            .request(Method::GET, &["albums", id, "with-tracks"], &[])
            .await?;
        let volumes = value["volumes"]
            .as_array()
            .filter(|v| v.len() <= 100)
            .ok_or(Error::InvalidResponse)?;
        let mut tracks = Vec::new();
        for volume in volumes {
            let values = volume.as_array().ok_or(Error::InvalidResponse)?;
            for chunk in values.chunks(200) {
                append_tracks(&mut tracks, parse::tracks(&Value::Array(chunk.to_vec()))?)?;
            }
            if tracks.len() > 10_000 {
                return Err(Error::TooLarge);
            }
        }
        Ok(tracks)
    }
    pub async fn artist_tracks(&self, id: &str) -> Result<Vec<Track>, Error> {
        if !parse::digits(id) {
            return Err(Error::InvalidInput);
        }
        let mut tracks = Vec::new();
        for page in 0..50 {
            let value = self
                .request(
                    Method::GET,
                    &["artists", id, "tracks"],
                    &[("page", page.to_string()), ("page-size", "100".into())],
                )
                .await?;
            let entries = parse::tracks(value.get("tracks").ok_or(Error::InvalidResponse)?)?;
            let count = entries.len();
            append_tracks(&mut tracks, entries)?;
            if count < 100 {
                return Ok(tracks);
            }
        }
        Err(Error::TooLarge)
    }
    pub async fn search_entities(&self, query: &str, kind: &str) -> Result<Vec<Entity>, Error> {
        if query.trim().is_empty() || query.len() > 1024 || !["album", "artist"].contains(&kind) {
            return Err(Error::InvalidInput);
        }
        let mut entries = Vec::new();
        for page in 0..50 {
            let value = self
                .request(
                    Method::GET,
                    &["search"],
                    &[
                        ("text", query.into()),
                        ("type", kind.into()),
                        ("page", page.to_string()),
                        ("page-size", "100".into()),
                    ],
                )
                .await?;
            let items = value[format!("{kind}s")]["results"]
                .as_array()
                .filter(|v| v.len() <= 200)
                .ok_or(Error::InvalidResponse)?;
            for item in items {
                let id = item["id"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| item["id"].as_u64().map(|n| n.to_string()))
                    .ok_or(Error::InvalidResponse)?;
                if !parse::digits(&id) {
                    return Err(Error::InvalidResponse);
                }
                entries.push(Entity {
                    id: format!("{kind}:{id}"),
                    kind: kind.into(),
                    // Its own image, when the result names one (#825).
                    artwork: if parse::cover_reference(item).is_some() {
                        format!("{kind}-cover:{id}")
                    } else {
                        String::new()
                    },
                    title: arinn_music_common::text(
                        item[if kind == "artist" { "name" } else { "title" }]
                            .as_str()
                            .unwrap_or(""),
                    ),
                    available: true,
                    ..Entity::default()
                });
            }
            if items.len() < 100 {
                return Ok(entries);
            }
        }
        Err(Error::TooLarge)
    }
    pub async fn rate(&self, uid: &str, id: &str, like: bool) -> Result<(), Error> {
        if !parse::digits(uid) || !parse::track_id(id) {
            return Err(Error::InvalidInput);
        }
        let bare = id.split(':').next().ok_or(Error::InvalidInput)?;
        let method = if like { "likes" } else { "dislikes" };
        self.request(
            Method::POST,
            &["users", uid, method, "tracks", "add-multiple"],
            &[("track-ids", bare.into())],
        )
        .await
        .map(|_| ())
    }
    pub async fn user_playlists(&self, uid: &str) -> Result<Vec<Collection>, Error> {
        self.playlists(uid).await
    }
}

/// Bound cumulative metadata while fetching pages, before the snapshot cache.
pub(crate) fn append_tracks(target: &mut Vec<Track>, next: Vec<Track>) -> Result<(), Error> {
    if target.len().saturating_add(next.len()) > 10_000 {
        return Err(Error::TooLarge);
    }
    let bytes = target
        .iter()
        .chain(next.iter())
        .try_fold(0usize, |bytes, track| {
            let size = serde_json::to_vec(track)
                .map_err(|_| Error::InvalidResponse)?
                .len();
            let total = bytes.saturating_add(size);
            if total > 8 * 1024 * 1024 {
                Err(Error::TooLarge)
            } else {
                Ok(total)
            }
        })?;
    let _ = bytes;
    target.extend(next);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn materialization_budget_preserves_previous_tracks_on_overflow() {
        let mut tracks = vec![Track {
            id: "123".into(),
            ..Track::default()
        }];
        let huge = Track {
            id: "124".into(),
            title: "x".repeat(8 * 1024 * 1024),
            ..Track::default()
        };
        assert!(matches!(
            append_tracks(&mut tracks, vec![huge]),
            Err(Error::TooLarge)
        ));
        assert_eq!(tracks.len(), 1);
    }
}
