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

/// The download-info URL of the best non-preview MP3 variant.
pub(crate) fn download_info(value: &Value) -> Result<String, Error> {
    let items = value.as_array().ok_or(Error::InvalidResponse)?;
    if items.len() > 32 {
        return Err(Error::TooLarge);
    }
    items
        .iter()
        .filter(|v| v.get("codec").and_then(Value::as_str) == Some("mp3"))
        .filter(|v| v.get("preview").and_then(Value::as_bool) != Some(true))
        .max_by_key(|v| v.get("bitrateInKbps").and_then(Value::as_u64).unwrap_or(0))
        .and_then(|v| v.get("downloadInfoUrl").and_then(Value::as_str))
        .filter(|url| url.len() <= 4096)
        .map(str::to_owned)
        .ok_or(Error::Unavailable)
}

/// Download-info documents and audio come from Yandex's storage hosts.
pub(crate) fn storage_host(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url
            .host_str()
            .is_some_and(|host| host.ends_with(".yandex.net") || host.ends_with(".yandex.ru"))
}

fn element<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find(&format!("</{name}>"))?;
    Some(xml[start..end].trim())
}

/// Builds the signed direct link from a download-info document:
/// `https://{host}/get-mp3/{md5(salt + path[1..] + s)}/{ts}{path}`.
pub(crate) fn direct_link(xml: &str, salt: &str) -> Result<String, Error> {
    use md5::{Digest, Md5};
    let host = element(xml, "host").ok_or(Error::InvalidResponse)?;
    let path = element(xml, "path").ok_or(Error::InvalidResponse)?;
    let ts = element(xml, "ts").ok_or(Error::InvalidResponse)?;
    let s = element(xml, "s").ok_or(Error::InvalidResponse)?;
    let token = |v: &str| {
        !v.is_empty()
            && v.len() <= 1024
            && v.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.=/%".contains(&b))
    };
    if !token(path)
        || !path.starts_with('/')
        || path.contains("..")
        || !token(ts)
        || ts.contains('/')
        || !token(s)
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-.".contains(&b))
    {
        return Err(Error::InvalidResponse);
    }
    let mut digest = Md5::new();
    digest.update(salt.as_bytes());
    digest.update(&path.as_bytes()[1..]);
    digest.update(s.as_bytes());
    let sign: String = digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let link = format!("https://{host}/get-mp3/{sign}/{ts}{path}");
    let url = reqwest::Url::parse(&link).map_err(|_| Error::InvalidResponse)?;
    if !storage_host(&url) {
        return Err(Error::InvalidResponse);
    }
    Ok(link)
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
    /// The signed direct link matches the upstream recipe:
    /// md5(salt + path without its leading slash + s).
    #[test]
    fn the_direct_link_is_signed_like_the_upstream_client() {
        let xml = "<?xml version=\"1.0\"?><download-info><host>s123vla.storage.yandex.net</host>\
                   <path>/rmusic/U2FsdGVkX1/abc/def</path><ts>0005f1e2d3c4b5a6</ts>\
                   <region>-1</region><s>0123456789abcdef</s></download-info>";
        assert_eq!(
            direct_link(xml, "XGRlBW9FXlekgbPrRHuSiA").unwrap(),
            "https://s123vla.storage.yandex.net/get-mp3/531e032aa3ad30bf518f6dbdf32fedfb/0005f1e2d3c4b5a6/rmusic/U2FsdGVkX1/abc/def"
        );
        // A document pointing anywhere but Yandex storage is refused.
        let foreign = xml.replace("s123vla.storage.yandex.net", "evil.example");
        assert_eq!(direct_link(&foreign, "salt"), Err(Error::InvalidResponse));
        let traversal = xml.replace("/rmusic/U2FsdGVkX1/abc/def", "/rmusic/../x");
        assert_eq!(direct_link(&traversal, "salt"), Err(Error::InvalidResponse));
        assert_eq!(
            direct_link("<host>a</host>", "salt"),
            Err(Error::InvalidResponse)
        );
    }
    #[test]
    fn download_info_picks_the_best_full_mp3() {
        let info = json!([
            {"codec":"aac","bitrateInKbps":256,"preview":false,"downloadInfoUrl":"https://a.storage.yandex.net/aac"},
            {"codec":"mp3","bitrateInKbps":320,"preview":true,"downloadInfoUrl":"https://a.storage.yandex.net/preview"},
            {"codec":"mp3","bitrateInKbps":192,"preview":false,"downloadInfoUrl":"https://a.storage.yandex.net/192"},
            {"codec":"mp3","bitrateInKbps":320,"preview":false,"downloadInfoUrl":"https://a.storage.yandex.net/320"}
        ]);
        assert_eq!(
            download_info(&info).unwrap(),
            "https://a.storage.yandex.net/320"
        );
        assert_eq!(download_info(&json!([])), Err(Error::Unavailable));
        let good = reqwest::Url::parse("https://s1.storage.yandex.net/x").unwrap();
        assert!(storage_host(&good));
        for bad in [
            "http://s1.storage.yandex.net/x",
            "https://storage.yandex.net.evil.example/x",
            "https://user@s1.storage.yandex.net/x",
            "https://s1.storage.yandex.net:8443/x",
        ] {
            assert!(!storage_host(&reqwest::Url::parse(bad).unwrap()), "{bad}");
        }
    }
}
