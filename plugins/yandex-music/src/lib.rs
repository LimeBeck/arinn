// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider-private catalogue/radio and device-code connection backends.
//! Native D-Bus account lifecycle is available; playback and live client
//! provisioning qualification remain separate integration work.
mod artwork;
pub mod auth;
mod browser;
pub mod catalogue;
mod compatibility;
mod parse;
pub mod runtime;
pub mod secrets;
pub mod service;
mod settings;
pub mod stream;
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

const MAX_RESPONSE: usize = 1024 * 1024;

/// Errors deliberately contain no request URL, token, query or response body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    /// HTTP 404: the service does not know the requested item (#811).
    NotFound,
    Authentication,
    Forbidden,
    RateLimited,
    Unavailable,
    Transport,
    InvalidResponse,
    TooLarge,
}

/// Does not implement Debug: the client owns a sensitive authorization header.
#[derive(Clone)]
pub struct Api {
    client: Client,
    origin: Url,
    downloads: Client,
    expired: std::sync::Arc<std::sync::atomic::AtomicBool>,
    lease: Option<std::sync::Arc<std::sync::atomic::AtomicU8>>,
    expires_at: Option<u64>,
    deadline: Option<tokio::time::Instant>,
    /// Whether each rotor station's info names an image, per connection:
    /// a new account gets a new `Api` and asks again (#843).
    station_images:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Option<String>>>>,
    /// One lookup in flight per station: a second caller waits for it and
    /// reads its answer (Codex review of #849).
    station_lookups: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>,
    >,
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
    pub fn expired(&self) -> bool {
        self.expired.load(std::sync::atomic::Ordering::Acquire)
            || self
                .lease
                .as_ref()
                .is_some_and(|l| l.load(std::sync::atomic::Ordering::Acquire) != auth::ACCEPTED)
            || self.expires_at.is_some_and(|at| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(true, |t| t.as_secs() >= at)
            })
            || self
                .deadline
                .is_some_and(|at| tokio::time::Instant::now() >= at)
    }
    pub(crate) fn leased(
        token: &str,
        expires_at: Option<u64>,
        lease: std::sync::Arc<std::sync::atomic::AtomicU8>,
    ) -> Result<Self, Error> {
        let mut api = Self::new(token)?;
        api.expires_at = expires_at;
        api.lease = Some(lease);
        api.deadline = expires_at.map(|at| {
            tokio::time::Instant::now()
                + Duration::from_secs(
                    at.saturating_sub(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(at, |t| t.as_secs()),
                    ),
                )
        });
        Ok(api)
    }
    pub(crate) async fn guard<T>(
        &self,
        work: impl std::future::Future<Output = Result<T, Error>>,
    ) -> Result<T, Error> {
        if self.expired() {
            return Err(Error::Authentication);
        }
        let expiry = async {
            let mut tick = tokio::time::interval(Duration::from_millis(100));
            loop {
                tick.tick().await;
                if self.expired() {
                    return;
                }
            }
        };
        tokio::select! {biased;_=expiry=>Err(Error::Authentication),result=work=>{if self.expired(){Err(Error::Authentication)}else{result}}}
    }
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
        let downloads = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .user_agent("arinn-yandex-provider")
            .build()
            .map_err(|_| Error::Transport)?;
        Ok(Self {
            downloads,
            expired: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            lease: None,
            expires_at: None,
            deadline: None,
            station_images: Default::default(),
            station_lookups: Default::default(),
            client,
            origin: Url::parse("https://api.music.yandex.net/").map_err(|_| Error::InvalidInput)?,
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
        self.guard(self.send_inner(builder)).await
    }
    async fn send_inner(&self, builder: reqwest::RequestBuilder) -> Result<Value, Error> {
        let mut response = builder.send().await.map_err(|_| Error::Transport)?;
        match response.status() {
            StatusCode::UNAUTHORIZED => {
                self.expired
                    .store(true, std::sync::atomic::Ordering::Release);
                return Err(Error::Authentication);
            }
            StatusCode::FORBIDDEN => return Err(Error::Forbidden),
            StatusCode::NOT_FOUND => return Err(Error::NotFound),
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

    /// What the connection already knows about a station's image, without a
    /// request: `None` when it has not asked yet (#843).
    pub(crate) fn known_station_image(&self, key: &str) -> Option<Option<String>> {
        self.station_images.lock().ok()?.get(key).cloned()
    }

    /// The image reference a rotor station's info names, or `None` (#843).
    /// An answer, image or none, is kept for this connection; a failure is
    /// not, so a later listing asks again.
    pub(crate) async fn station_image(&self, key: &str) -> Result<Option<String>, Error> {
        if parse::station_seed(key).ok().as_deref() != Some(key) {
            return Err(Error::InvalidInput);
        }
        if let Some(known) = self.known_station_image(key) {
            return Ok(known);
        }
        let lookup = {
            let mut lookups = self
                .station_lookups
                .lock()
                .map_err(|_| Error::Unavailable)?;
            // Only idle locks are dropped, so a waiting caller keeps its own.
            if lookups.len() >= 1024 {
                lookups.retain(|_, lock| std::sync::Arc::strong_count(lock) > 1);
            }
            lookups.entry(key.to_owned()).or_default().clone()
        };
        let _turn = lookup.lock().await;
        // The lookup that held the turn may have answered meanwhile.
        if let Some(known) = self.known_station_image(key) {
            return Ok(known);
        }
        let info = self
            .request(Method::GET, &["rotor", "station", key, "info"], &[])
            .await?;
        // Only a reference the cover route accepts is kept: an allowed host
        // and at most 16 KiB. Anything else could never be shown and counts
        // as no image, so the memo stays small (Codex review of #849).
        let image = parse::station_image(&info)?
            .filter(|reference| stream::cover_url(reference).is_ok())
            .map(str::to_owned);
        let mut known = self.station_images.lock().map_err(|_| Error::Unavailable)?;
        // A bounded memo with room for every pin (256), My Wave and the
        // radios started in one connection; past that only one entry makes
        // way, never the whole memo (Codex review of #849).
        if known.len() >= 1024
            && !known.contains_key(key)
            && let Some(other) = known.keys().next().cloned()
        {
            known.remove(&other);
        }
        known.insert(key.into(), image.clone());
        Ok(image)
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
        if !completed_track.is_empty() && !parse::track_id(completed_track) {
            return Err(Error::InvalidInput);
        }
        let result = self
            .request_json(
                &["rotor", "session", &session.id, "tracks"],
                json!({
                    "queue":if completed_track.is_empty(){Vec::<&str>::new()}else{vec![completed_track.split(':').next().unwrap_or(completed_track)]},
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
#[derive(Clone)]
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
