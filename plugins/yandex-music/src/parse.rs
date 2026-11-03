// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::Error;
use arinn_music_common::{Track, text};
use serde_json::Value;

#[derive(Clone, PartialEq, Eq)]
pub struct Batch {
    pub id: String,
    pub tracks: Vec<Track>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collection {
    pub id: String,
    pub title: String,
    pub track_count: u32,
}

pub(crate) fn digits(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.bytes().all(|c| c.is_ascii_digit())
}
pub(crate) fn track_id(value: &str) -> bool {
    if value.len() > 129 {
        return false;
    }
    let parts: Vec<_> = value.split(':').collect();
    (1..=2).contains(&parts.len()) && parts.iter().all(|part| digits(part))
}
fn id(value: Option<&Value>) -> Result<String, Error> {
    let value = value.ok_or(Error::InvalidResponse)?;
    let id = value
        .as_str()
        .map(String::from)
        .or_else(|| value.as_u64().map(|v| v.to_string()))
        .ok_or(Error::InvalidResponse)?;
    if !digits(&id) {
        return Err(Error::InvalidResponse);
    }
    Ok(id)
}
pub(crate) fn opaque_id(value: Option<&Value>) -> Result<String, Error> {
    let value = value
        .and_then(Value::as_str)
        .ok_or(Error::InvalidResponse)?;
    if value.is_empty()
        || value.len() > 512
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return Err(Error::InvalidResponse);
    }
    Ok(value.into())
}

pub(crate) fn station_seed(value: &str) -> Result<String, Error> {
    if value == "user:onyourwave" {
        return Ok(value.into());
    }
    let value = value.strip_prefix("track:").unwrap_or(value);
    if !track_id(value) {
        return Err(Error::InvalidInput);
    }
    Ok(format!(
        "track:{}",
        value.split(':').next().unwrap_or(value)
    ))
}

pub(crate) fn tracks(value: &Value) -> Result<Vec<Track>, Error> {
    let items = value.as_array().ok_or(Error::InvalidResponse)?;
    if items.len() > 200 {
        return Err(Error::TooLarge);
    }
    items.iter().map(track).collect()
}

fn track(value: &Value) -> Result<Track, Error> {
    let value = value.get("track").unwrap_or(value);
    let track_id = id(value.get("id"))?;
    let artists = value.get("artists").and_then(Value::as_array);
    if artists.is_some_and(|a| a.len() > 32) {
        return Err(Error::TooLarge);
    }
    let artist = text(
        &artists
            .into_iter()
            .flatten()
            .filter_map(|a| a.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(", "),
    );
    let album = value
        .get("albums")
        .and_then(Value::as_array)
        .and_then(|v| v.first());
    let album_id = album.and_then(|a| id(a.get("id")).ok()).unwrap_or_default();
    let position = album.and_then(|a| a.get("trackPosition"));
    Ok(Track {
        id: if album_id.is_empty() {
            track_id
        } else {
            format!("{track_id}:{album_id}")
        },
        title: text(value.get("title").and_then(Value::as_str).unwrap_or("")),
        artist: artist.clone(),
        artist_ids: artists
            .into_iter()
            .flatten()
            .filter_map(|a| id(a.get("id")).ok())
            .collect(),
        album_artist: artist,
        album: text(
            album
                .and_then(|a| a.get("title"))
                .and_then(Value::as_str)
                .unwrap_or(""),
        ),
        album_id,
        disc: position
            .and_then(|a| a.get("volume"))
            .and_then(Value::as_u64)
            .and_then(|v| v.try_into().ok())
            .unwrap_or(0),
        number: position
            .and_then(|a| a.get("index"))
            .and_then(Value::as_u64)
            .and_then(|v| v.try_into().ok())
            .unwrap_or(0),
        duration_us: value
            .get("durationMs")
            .and_then(Value::as_u64)
            .filter(|v| *v <= 31_536_000_000)
            .and_then(|v| v.checked_mul(1000))
            .unwrap_or(0),
        // Missing entitlement is unknown/unavailable, never optimistic Play.
        available: value.get("available").and_then(Value::as_bool) == Some(true),
        ..Track::default()
    })
}

pub(crate) fn collections(value: &Value) -> Result<Vec<Collection>, Error> {
    let items = value.as_array().ok_or(Error::InvalidResponse)?;
    if items.len() > 200 {
        return Err(Error::TooLarge);
    }
    items
        .iter()
        .map(|v| {
            Ok(Collection {
                id: id(v.get("kind"))?,
                title: text(v.get("title").and_then(Value::as_str).unwrap_or("")),
                track_count: v
                    .get("trackCount")
                    .and_then(Value::as_u64)
                    .and_then(|v| v.try_into().ok())
                    .ok_or(Error::InvalidResponse)?,
            })
        })
        .collect()
}

pub(crate) fn batch(value: &Value) -> Result<Batch, Error> {
    let tracks = tracks(
        value
            .get("sequence")
            .or_else(|| value.get("tracks"))
            .ok_or(Error::InvalidResponse)?,
    )?;
    Ok(Batch {
        id: opaque_id(value.get("batchId"))?,
        tracks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn seeds_do_not_confuse_album_qualified_tracks_with_stations() {
        assert_eq!(station_seed("123:456"), Ok("track:123".into()));
        assert_eq!(
            station_seed("user:onyourwave"),
            Ok("user:onyourwave".into())
        );
        assert!(station_seed("../../private").is_err());
        assert!(station_seed("track:123/other").is_err());
    }
    #[test]
    fn metadata_keeps_unknown_entitlement_unavailable_and_bounds_duration() {
        let tracks = tracks(&json!([{"track":{"id":123,"title":"Synthetic","artists":[{"name":"Fixture"}],"albums":[{"id":456,"title":"Album"}],"durationMs":1234}}, {"id":789,"durationMs":u64::MAX,"available":false}])).unwrap();
        assert_eq!(tracks[0].id, "123:456");
        assert_eq!(tracks[0].duration_us, 1_234_000);
        assert!(!tracks[0].available);
        assert_eq!(tracks[1].duration_us, 0);
    }
    #[test]
    fn batches_reject_unbounded_or_invalid_identifiers_without_echoing_them() {
        assert!(batch(&json!({"batchId":"../../secret", "tracks":[]})).is_err());
        assert_eq!(
            tracks(&json!(vec![json!({"id":123}); 201])).err(),
            Some(Error::TooLarge)
        );
    }
}
