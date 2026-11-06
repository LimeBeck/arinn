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
    /// When the playlist joined the user's library (its `created` date), in
    /// Unix seconds; 0 when the API does not report it (ADR-0142 MH-9).
    pub added_at: u64,
    /// Whether the API names an image for it (#825).
    pub cover: bool,
}

/// An artwork ID this provider hands out (ADR-0134, #825): the cover of a
/// track, an album, an artist or one of the account's playlists. The shell
/// sees only the opaque ID, never a URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Cover {
    Track(String),
    Album(String),
    Artist(String),
    Playlist { uid: String, kind: String },
}
/// The artwork ID as `Entity.artwork` carries it: no `/`, at most 256 bytes
/// (the music contract's artwork rules); anything else is not this
/// provider's.
pub(crate) fn cover(id: &str) -> Option<Cover> {
    if id.len() > 256 || id.contains('/') {
        return None;
    }
    if let Some(track) = id.strip_prefix("cover:") {
        return track_id(track).then(|| Cover::Track(track.into()));
    }
    if let Some(album) = id.strip_prefix("album-cover:") {
        return digits(album).then(|| Cover::Album(album.into()));
    }
    if let Some(artist) = id.strip_prefix("artist-cover:") {
        return digits(artist).then(|| Cover::Artist(artist.into()));
    }
    let (uid, kind) = id.strip_prefix("playlist-cover:")?.split_once(':')?;
    (digits(uid) && digits(kind)).then(|| Cover::Playlist {
        uid: uid.into(),
        kind: kind.into(),
    })
}
/// The image an API object names: its `coverUri`, its `cover.uri`, or its
/// `ogImage`, in that order; `None` when it names none (#825).
pub(crate) fn cover_reference(value: &Value) -> Option<&str> {
    [
        &value["coverUri"],
        &value["cover"]["uri"],
        &value["ogImage"],
    ]
    .into_iter()
    .filter_map(Value::as_str)
    .find(|reference| !reference.is_empty())
}

/// A library date from the API ("2024-05-01T12:00:00+00:00", optionally
/// with fractional seconds or `Z`) in Unix seconds. Anything else, a date
/// before 1970 or after `MAX_ADDED_AT`, is 0: unknown, never guessed
/// (ADR-0142 MH-9, #813).
pub(crate) fn timestamp(value: Option<&Value>) -> u64 {
    value.and_then(Value::as_str).and_then(rfc3339).unwrap_or(0)
}
fn rfc3339(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    if !(20..=35).contains(&bytes.len()) {
        return None;
    }
    let number = |range: std::ops::Range<usize>| number_in(text, range);
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    let mut rest = &text[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        rest = &fraction[digits..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            if rest.len() != 6 || rest.as_bytes()[3] != b':' {
                return None;
            }
            let (h, m) = (number_in(rest, 1..3)?, number_in(rest, 4..6)?);
            if h > 23 || m > 59 {
                return None;
            }
            sign * (h * 3600 + m * 60)
        }
    };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days_in_month = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month[(month - 1) as usize]
        || hour > 23
        || minute > 59
        // No leap second: a library date is never one, and ":60" turned
        // into ":59" would be a made-up time (review of #814).
        || second > 59
    {
        return None;
    }
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let seconds = days * 86_400 + hour * 3600 + minute * 60 + second - offset;
    u64::try_from(seconds)
        .ok()
        .filter(|s| *s > 0 && *s <= arinn_protocol::music::MAX_ADDED_AT)
}
fn number_in(text: &str, range: std::ops::Range<usize>) -> Option<i64> {
    let part = text.get(range)?;
    if !part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    part.parse().ok()
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

/// One malformed entry (for example a non-numeric user-uploaded track ID)
/// is skipped instead of failing the whole page; only a list where every
/// entry is unusable is an invalid response (review of #710).
pub(crate) fn tracks(value: &Value) -> Result<Vec<Track>, Error> {
    tracks_bounded(value, 200)
}
pub(crate) fn tracks_bounded(value: &Value, limit: usize) -> Result<Vec<Track>, Error> {
    let items = value.as_array().ok_or(Error::InvalidResponse)?;
    if items.len() > limit {
        return Err(Error::TooLarge);
    }
    let mut tracks = Vec::with_capacity(items.len());
    for item in items {
        match track(item) {
            Ok(track) => tracks.push(track),
            Err(Error::TooLarge) => return Err(Error::TooLarge),
            Err(_) => {}
        }
    }
    if tracks.is_empty() && !items.is_empty() {
        return Err(Error::InvalidResponse);
    }
    Ok(tracks)
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
                added_at: timestamp(v.get("created")),
                cover: cover_reference(v).is_some(),
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
    // Batch IDs are JSON data, not session IDs used in URL path segments.
    let id = value
        .get("batchId")
        .and_then(Value::as_str)
        .ok_or(Error::InvalidResponse)?;
    if id.is_empty()
        || id.len() > 512
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    {
        return Err(Error::InvalidResponse);
    }
    Ok(Batch {
        id: id.into(),
        tracks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn radio_batches_accept_dotted_ids_without_relaxing_session_paths() {
        let value = json!({"batchId":"batch.fixture-01_2", "sequence":[
            {"track":{"id":123,"available":true}}
        ]});
        let parsed = batch(&value).unwrap();
        assert_eq!(parsed.id, "batch.fixture-01_2");
        assert_eq!(parsed.tracks[0].id, "123");
        assert!(opaque_id(value.get("batchId")).is_err());
    }
    #[test]
    fn radio_batch_ids_keep_bounds_and_reject_controls_and_path_delimiters() {
        assert!(batch(&json!({"batchId":"a".repeat(512),"tracks":[]})).is_ok());
        for id in [
            "".into(),
            "a".repeat(513),
            "batch\nfixture".into(),
            "batch\u{0}fixture".into(),
            "batch fixture".into(),
            "../fixture".into(),
            "batch\\fixture".into(),
            "batch?fixture".into(),
            "batch#fixture".into(),
        ] {
            assert!(batch(&json!({"batchId":id,"tracks":[]})).is_err());
        }
        for id in [Value::Null, json!(12), json!({}), json!([])] {
            assert!(batch(&json!({"batchId":id,"tracks":[]})).is_err());
        }
        assert!(batch(&json!({"tracks":[]})).is_err());
    }
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
    /// #813: library dates map to Unix seconds; anything that is not a full
    /// date and time is unknown (0), never guessed.
    #[test]
    fn library_dates_parse_strictly() {
        let at = |text: &str| timestamp(Some(&json!(text)));
        assert_eq!(at("2024-05-01T12:00:00+00:00"), 1_714_564_800);
        assert_eq!(at("2024-05-01T12:00:00Z"), 1_714_564_800);
        assert_eq!(at("2024-05-01T15:00:00.123+03:00"), 1_714_564_800);
        assert_eq!(at("2024-05-01T08:30:00-03:30"), 1_714_564_800);
        assert_eq!(at("2024-02-29T00:00:00+00:00"), 1_709_164_800);
        for bad in [
            "",
            "2024",
            "2024-05-01",
            "2024-13-01T00:00:00Z",
            "2023-02-29T00:00:00Z",
            "2024-05-01T24:00:00Z",
            "2024-05-01T12:00:60Z",
            "2024-05-01 12:00:00Z",
            "2024-05-01T12:00:00",
            "2024-05-01T12:00:00+0300",
            "1969-12-31T23:59:59Z",
            "1970-01-01T00:00:00Z",
            "2024-05-01T12:00:00.Z",
            "+2024-05-01T12:00:00Z",
        ] {
            assert_eq!(at(bad), 0, "{bad}");
        }
        assert_eq!(timestamp(Some(&json!(1_714_564_800))), 0);
        assert_eq!(timestamp(None), 0);
    }
    /// #813: a playlist's `created` date is when it joined the library.
    #[test]
    fn playlists_carry_their_created_date() {
        let parsed = collections(&json!([
            {"kind": 3, "title": "Dated", "trackCount": 2, "created": "2024-05-01T12:00:00+00:00"},
            {"kind": 4, "title": "Undated", "trackCount": 1}
        ]))
        .unwrap();
        assert_eq!(parsed[0].added_at, 1_714_564_800);
        assert_eq!(parsed[1].added_at, 0);
    }
    /// #825: artwork IDs are validated strictly, and a cover is read from
    /// `coverUri`, `cover.uri` or `ogImage`.
    #[test]
    fn cover_ids_are_strict_and_read_every_reference() {
        assert_eq!(cover("cover:12:34"), Some(Cover::Track("12:34".into())));
        assert_eq!(cover("album-cover:7"), Some(Cover::Album("7".into())));
        assert_eq!(cover("artist-cover:8"), Some(Cover::Artist("8".into())));
        assert_eq!(
            cover("playlist-cover:5:3"),
            Some(Cover::Playlist {
                uid: "5".into(),
                kind: "3".into()
            })
        );
        for bad in [
            "",
            "cover:",
            "cover:a",
            "album-cover:",
            "album-cover:7:8",
            "album-cover:7/8",
            "artist-cover:x",
            "playlist-cover:5",
            "playlist-cover:5:",
            "playlist-cover:5:3:1",
            "playlist-cover:5/3",
            "station:wave",
            "https://avatars.yandex.net/get-music-content/1/x/%%",
        ] {
            assert_eq!(cover(bad), None, "{bad}");
        }
        let long = format!("album-cover:{}", "1".repeat(64));
        assert_eq!(long.len(), 76);
        assert!(cover(&long).is_some());
        assert_eq!(cover(&format!("album-cover:{}", "1".repeat(65))), None);
        assert_eq!(cover(&"cover:1".repeat(40)), None);
        let reference = |value| cover_reference(&value).map(str::to_owned);
        assert_eq!(
            reference(json!({"coverUri": "a/%%", "ogImage": "b/%%"})),
            Some("a/%%".into())
        );
        assert_eq!(
            reference(json!({"cover": {"type": "pic", "uri": "c/%%"}, "ogImage": "d/%%"})),
            Some("c/%%".into())
        );
        assert_eq!(
            reference(json!({"coverUri": "", "cover": {"type": "mosaic"}, "ogImage": "e/%%"})),
            Some("e/%%".into())
        );
        assert_eq!(reference(json!({"cover": {"type": "mosaic"}})), None);
        assert_eq!(reference(json!({"coverUri": 5})), None);
    }
    /// #825: a playlist records whether the API names an image for it.
    #[test]
    fn playlists_record_whether_they_have_a_cover() {
        let parsed = collections(&json!([
            {"kind": 3, "title": "Pictured", "trackCount": 2, "cover": {"type": "pic", "uri": "a/%%"}},
            {"kind": 4, "title": "Shared", "trackCount": 1, "ogImage": "b/%%"},
            {"kind": 5, "title": "Bare", "trackCount": 1, "cover": {"type": "mosaic", "itemsUri": []}}
        ]))
        .unwrap();
        let covers: Vec<bool> = parsed.iter().map(|p| p.cover).collect();
        assert_eq!(covers, [true, true, false]);
    }
    /// Review of #710: one entry with a non-numeric ID failed a whole search
    /// page or radio batch.
    #[test]
    fn a_malformed_entry_is_skipped_not_fatal() {
        let page = json!([
            {"id": "101", "title": "First"},
            {"id": "user-upload-abc", "title": "Uploaded"},
            {"title": "No id"},
            {"track": {"id": 102, "title": "Second"}}
        ]);
        let parsed = tracks(&page).unwrap();
        let titles: Vec<&str> = parsed.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["First", "Second"]);
        assert_eq!(tracks(&json!([{"id": "bad"}])), Err(Error::InvalidResponse));
        assert_eq!(tracks(&json!([])), Ok(vec![]));
    }
}
