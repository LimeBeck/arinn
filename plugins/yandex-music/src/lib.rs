// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider-private catalogue/radio and device-code connection backends.
//! Native D-Bus account lifecycle is available; playback and live client
//! provisioning qualification remain separate integration work.
pub mod auth;
mod parse;
pub mod secrets;
pub mod service;
#[cfg(test)]
mod tests;
use arinn_music_common::Track;
pub use parse::{Batch, Collection};
use reqwest::{
    Client, Method, StatusCode, Url,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde_json::{Value, json};
use std::time::Duration;

/// Liked-track lists and playlists of several thousand tracks fit.
const MAX_RESPONSE: usize = 8 * 1024 * 1024;
/// The bounded download-info XML of one track.
const MAX_DOWNLOAD_INFO: usize = 64 * 1024;
/// The stream-link signing salt of Yandex's own clients, as published by the
/// upstream yandex-music-api library. Bundled by owner decision (ADR-0134,
/// 2026-11-02); `ARINN_YANDEX_SIGN_SALT` overrides it.
pub const DEFAULT_SIGN_SALT: &str = "XGRlBW9FXlekgbPrRHuSiA";
/// Upper bound on liked tracks read for the catalogue.
const MAX_LIKED: usize = 10_000;

/// Errors deliberately contain no request URL, token, query or response body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    Authentication,
    Forbidden,
    RateLimited,
    Unavailable,
    Transport,
    InvalidResponse,
    TooLarge,
}

/// Does not implement Debug: the client owns a sensitive authorization header.
pub struct Api {
    client: Client,
    origin: Url,
    salt: String,
}

fn authorization_header(token: &str) -> Result<HeaderValue, Error> {
    if token.is_empty() || token.len() > 8192 || token.chars().any(char::is_control) {
        return Err(Error::InvalidInput);
    }
    let mut value =
        HeaderValue::from_str(&format!("OAuth {token}")).map_err(|_| Error::InvalidInput)?;
    value.set_sensitive(true);
    Ok(value)
}

impl Api {
    /// The caller obtains and stores a user-authorized token outside shell/QML.
    /// No third-party application client ID or secret is bundled here.
    pub fn new(token: &str) -> Result<Self, Error> {
        let value = authorization_header(token)?;
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, value);
        let client = Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .user_agent("arinn-yandex-provider")
            .build()
            .map_err(|_| Error::Transport)?;
        Ok(Self {
            client,
            origin: Url::parse("https://api.music.yandex.net/").map_err(|_| Error::InvalidInput)?,
            salt: std::env::var("ARINN_YANDEX_SIGN_SALT")
                .ok()
                .map(|s| s.trim().to_owned())
                .filter(|s| {
                    !s.is_empty() && s.len() <= 256 && s.bytes().all(|b| b.is_ascii_graphic())
                })
                .unwrap_or_else(|| DEFAULT_SIGN_SALT.to_owned()),
        })
    }

    async fn request(
        &self,
        method: Method,
        segments: &[&str],
        fields: &[(&str, String)],
    ) -> Result<Value, Error> {
        let mut url = self.origin.clone();
        url.path_segments_mut()
            .map_err(|_| Error::InvalidInput)?
            .clear()
            .extend(segments);
        let mut builder = self.client.request(method.clone(), url);
        if method == Method::GET {
            builder = builder.query(fields);
        } else {
            builder = builder.form(fields);
        }
        self.send(builder).await
    }

    async fn request_json(&self, segments: &[&str], payload: Value) -> Result<Value, Error> {
        let mut url = self.origin.clone();
        url.path_segments_mut()
            .map_err(|_| Error::InvalidInput)?
            .clear()
            .extend(segments);
        self.send(self.client.post(url).json(&payload)).await
    }

    async fn send(&self, builder: reqwest::RequestBuilder) -> Result<Value, Error> {
        let mut response = builder.send().await.map_err(|_| Error::Transport)?;
        match response.status() {
            StatusCode::UNAUTHORIZED => return Err(Error::Authentication),
            StatusCode::FORBIDDEN => return Err(Error::Forbidden),
            StatusCode::TOO_MANY_REQUESTS => return Err(Error::RateLimited),
            status if status.is_redirection() || !status.is_success() => {
                return Err(Error::Unavailable);
            }
            _ => {}
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_RESPONSE as u64)
        {
            return Err(Error::TooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if chunk.len() > MAX_RESPONSE.saturating_sub(body.len()) {
                return Err(Error::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&body).map_err(|_| Error::InvalidResponse)?;
        if value.get("error").is_some_and(|v| !v.is_null()) {
            return Err(Error::Unavailable);
        }
        value.get("result").cloned().ok_or(Error::InvalidResponse)
    }

    /// The account's numeric user ID.
    pub async fn account_uid(&self) -> Result<String, Error> {
        let result = self
            .request(Method::GET, &["account", "status"], &[])
            .await?;
        let uid = result
            .pointer("/account/uid")
            .and_then(|v| {
                v.as_u64()
                    .map(|n| n.to_string())
                    .or_else(|| v.as_str().map(str::to_owned))
            })
            .ok_or(Error::InvalidResponse)?;
        if !parse::digits(&uid) {
            return Err(Error::InvalidResponse);
        }
        Ok(uid)
    }

    /// The account's liked track IDs, most recent first (`id:albumId`).
    pub async fn liked_track_ids(&self, uid: &str) -> Result<Vec<String>, Error> {
        if !parse::digits(uid) {
            return Err(Error::InvalidInput);
        }
        let result = self
            .request(Method::GET, &["users", uid, "likes", "tracks"], &[])
            .await?;
        let items = result
            .pointer("/library/tracks")
            .and_then(Value::as_array)
            .ok_or(Error::InvalidResponse)?;
        Ok(items
            .iter()
            .filter_map(|item| {
                let id = item.get("id").and_then(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .or_else(|| v.as_u64().map(|n| n.to_string()))
                })?;
                let album = item.get("albumId").and_then(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .or_else(|| v.as_u64().map(|n| n.to_string()))
                });
                let id = match album {
                    Some(album) if !album.is_empty() => format!("{id}:{album}"),
                    _ => id,
                };
                parse::track_id(&id).then_some(id)
            })
            .take(MAX_LIKED)
            .collect())
    }

    /// Track metadata for any number of IDs, read in batches of 100.
    pub async fn tracks_batched(&self, ids: &[String]) -> Result<Vec<Track>, Error> {
        let mut tracks = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(100) {
            tracks.extend(self.tracks(chunk).await?);
        }
        Ok(tracks)
    }

    /// The tracks of one of the user's playlists.
    pub async fn playlist_tracks(&self, uid: &str, kind: &str) -> Result<Vec<Track>, Error> {
        if !parse::digits(uid) || !parse::digits(kind) {
            return Err(Error::InvalidInput);
        }
        let result = self
            .request(Method::GET, &["users", uid, "playlists", kind], &[])
            .await?;
        parse::tracks_bounded(
            result.get("tracks").ok_or(Error::InvalidResponse)?,
            MAX_LIKED,
        )
    }

    /// Records a like or a dislike for a track with the service.
    pub async fn rate(&self, uid: &str, track: &str, like: bool) -> Result<(), Error> {
        if !parse::digits(uid) || !parse::track_id(track) {
            return Err(Error::InvalidInput);
        }
        let list = if like { "likes" } else { "dislikes" };
        self.request(
            Method::POST,
            &["users", uid, list, "tracks", "add-multiple"],
            &[("track-ids", track.to_owned())],
        )
        .await
        .map(|_| ())
    }

    /// A time-limited HTTPS link to the track's audio. The link and the
    /// steps that build it never leave this process.
    pub async fn stream_url(&self, track: &str) -> Result<String, Error> {
        if !parse::track_id(track) {
            return Err(Error::InvalidInput);
        }
        let result = self
            .request(Method::GET, &["tracks", track, "download-info"], &[])
            .await?;
        let info = parse::download_info(&result)?;
        let xml = self.fetch_download_info(&info).await?;
        parse::direct_link(&xml, &self.salt)
    }

    /// Reads the download-info document from Yandex's storage host only.
    async fn fetch_download_info(&self, url: &str) -> Result<String, Error> {
        let url = Url::parse(url).map_err(|_| Error::InvalidResponse)?;
        if !parse::storage_host(&url) {
            return Err(Error::InvalidResponse);
        }
        let mut response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| Error::Transport)?;
        if !response.status().is_success() {
            return Err(Error::Unavailable);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if chunk.len() > MAX_DOWNLOAD_INFO.saturating_sub(body.len()) {
                return Err(Error::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        String::from_utf8(body).map_err(|_| Error::InvalidResponse)
    }

    pub async fn search_tracks(&self, query: &str, page: u32) -> Result<Vec<Track>, Error> {
        if query.trim().is_empty() || query.len() > 1024 || page > 100_000 {
            return Err(Error::InvalidInput);
        }
        let result = self
            .request(
                Method::GET,
                &["search"],
                &[
                    ("text", query.into()),
                    ("type", "track".into()),
                    ("page", page.to_string()),
                ],
            )
            .await?;
        parse::tracks(
            result
                .pointer("/tracks/results")
                .ok_or(Error::InvalidResponse)?,
        )
    }

    pub async fn tracks(&self, ids: &[String]) -> Result<Vec<Track>, Error> {
        if ids.is_empty() || ids.len() > 100 || ids.iter().any(|id| !parse::track_id(id)) {
            return Err(Error::InvalidInput);
        }
        let result = self
            .request(Method::POST, &["tracks"], &[("track-ids", ids.join(","))])
            .await?;
        parse::tracks(&result)
    }

    pub async fn playlists(&self, uid: &str) -> Result<Vec<Collection>, Error> {
        if !parse::digits(uid) {
            return Err(Error::InvalidInput);
        }
        let result = self
            .request(Method::GET, &["users", uid, "playlists", "list"], &[])
            .await?;
        parse::collections(&result)
    }

    pub async fn start_radio(&self, seed: &str) -> Result<RadioSession, Error> {
        let station = parse::station_seed(seed)?;
        let result = self
            .request_json(
                &["rotor", "session", "new"],
                json!({
                    "seeds":[station], "includeTracksInResponse":true,
                    "includeWaveModel":true, "interactive":true,
                }),
            )
            .await?;
        let batch = parse::batch(&result)?;
        let session = RadioSession {
            id: parse::opaque_id(result.get("radioSessionId"))?,
            batch,
        };
        self.feedback(&session, &session.batch, Feedback::RadioStarted)
            .await?;
        Ok(session)
    }

    pub async fn next_batch(
        &self,
        session: &RadioSession,
        completed_track: &str,
    ) -> Result<Batch, Error> {
        if !parse::track_id(completed_track) {
            return Err(Error::InvalidInput);
        }
        let result = self
            .request_json(
                &["rotor", "session", &session.id, "tracks"],
                json!({
                    "queue":[completed_track.split(':').next().unwrap_or(completed_track)],
                }),
            )
            .await?;
        let batch = parse::batch(&result)?;
        Ok(batch)
    }

    pub async fn feedback(
        &self,
        session: &RadioSession,
        batch: &Batch,
        feedback: Feedback<'_>,
    ) -> Result<(), Error> {
        if let Feedback::TrackStarted(id) | Feedback::TrackFinished(id, _) | Feedback::Skip(id, _) =
            feedback
        {
            let bare = id.split(':').next().unwrap_or(id);
            if !batch
                .tracks
                .iter()
                .any(|track| track.id.split(':').next() == Some(bare))
            {
                return Err(Error::InvalidInput);
            }
        }
        let mut event = match feedback {
            Feedback::RadioStarted => json!({"type":"radioStarted", "from":"arinn"}),
            Feedback::TrackStarted(id) => {
                if !parse::track_id(id) {
                    return Err(Error::InvalidInput);
                }
                json!({"type":"trackStarted", "trackId":id})
            }
            Feedback::TrackFinished(id, played_ms) | Feedback::Skip(id, played_ms) => {
                if !parse::track_id(id) || played_ms > 31_536_000_000 {
                    return Err(Error::InvalidInput);
                }
                json!({"type":if matches!(feedback, Feedback::Skip(..)) {"skip"} else {"trackFinished"}, "trackId":id, "totalPlayedSeconds":played_ms as f64 / 1000.0})
            }
        };
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| Error::InvalidInput)?
            .as_secs();
        event["timestamp"] = json!(timestamp);
        self.request_json(
            &["rotor", "session", &session.id, "feedback"],
            json!({
                "event":event, "batch_id":batch.id,
            }),
        )
        .await
        .map(|_| ())
    }
}

/// Provider-private identifiers, deliberately not logged or exported to QML.
pub struct RadioSession {
    id: String,
    pub batch: Batch,
}
#[derive(Clone, Copy)]
pub enum Feedback<'a> {
    RadioStarted,
    TrackStarted(&'a str),
    TrackFinished(&'a str, u64),
    Skip(&'a str, u64),
}
