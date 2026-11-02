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

const MAX_RESPONSE: usize = 1024 * 1024;

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
