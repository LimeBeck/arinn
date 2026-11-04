// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-stable native music contract (ADR-0134). JSON contains metadata only.
use crate::validation::{self, ErrorCode};
use serde::{Deserialize, Serialize};

pub const INTERFACE: &str = "org.arinn.Music1";
pub const PROVIDER_INTERFACE: &str = "org.arinn.MusicProvider1";
pub const FACETS: &[&str] = &["tracks", "albums", "artists", "playlists", "selections"];
pub const ACTIONS: &[&str] = &[
    "play",
    "pause",
    "resume",
    "stop",
    "previous",
    "next",
    "seek",
    "remove",
    "like",
    "dislike",
    "radio",
    "retry",
    "pin",
    "connect",
    "authorize",
    "cancel",
    "disconnect",
    "scan",
    "save_playlist",
    "delete_playlist",
];

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork: String,
    pub duration_us: u64,
    pub available: bool,
}
impl Entity {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        if !["track", "album", "artist", "playlist", "station"].contains(&self.kind.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
        for value in [&self.title, &self.artist, &self.album] {
            validation::text(value)?;
        }
        // An opaque asset ID: never a filesystem path or a URL the shell
        // could load (music.md). No slash, so neither `/path` nor `scheme://`.
        if !self.artwork.is_empty()
            && (self.artwork.len() > 256
                || !self
                    .artwork
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b)))
        {
            return Err(ErrorCode::InvalidArgument);
        }
        if self.duration_us > 31_536_000_000_000 {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueEntry {
    pub occurrence: String,
    pub track: Entity,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prompt {
    pub verification_url: String,
    pub user_code: String,
    pub expires_in: u64,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub revision: u64,
    pub catalogue_revision: u64,
    pub backend: String,
    pub account: String,
    pub prompt: Option<Prompt>,
    pub facets: Vec<String>,
    pub actions: Vec<String>,
    pub session: String,
    pub playback: String,
    pub position_us: u64,
    pub current: Option<QueueEntry>,
    pub history: Vec<QueueEntry>,
    pub upcoming: Vec<QueueEntry>,
    pub continuous: bool,
    pub refill: String,
    pub error: Option<ErrorCode>,
    #[serde(default)]
    pub station: Option<Entity>,
    #[serde(default)]
    pub catalogue_loading: bool,
    #[serde(default)]
    pub feedback: String,
    #[serde(default)]
    pub pinned: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            revision: 1,
            catalogue_revision: 1,
            backend: "unavailable".into(),
            account: "disconnected".into(),
            prompt: None,
            facets: vec![],
            actions: vec![],
            session: String::new(),
            playback: "stopped".into(),
            position_us: 0,
            current: None,
            history: vec![],
            upcoming: vec![],
            continuous: false,
            refill: "idle".into(),
            error: None,
            station: None,
            catalogue_loading: false,
            feedback: String::new(),
            pinned: false,
        }
    }
}
impl State {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.revision == 0
            || self.catalogue_revision == 0
            || !["available", "degraded", "unavailable"].contains(&self.backend.as_str())
            || ![
                "local",
                "disconnected",
                "connecting",
                "connected",
                "restricted",
                "unavailable",
                "expired",
                "unconfigured",
            ]
            .contains(&self.account.as_str())
            || !["stopped", "loading", "playing", "paused", "ended"]
                .contains(&self.playback.as_str())
            || !["idle", "loading", "failed", "exhausted"].contains(&self.refill.as_str())
            || self.history.len() > 32
            || self.upcoming.len() > 64
            || self.position_us > 31_536_000_000_000
        {
            return Err(ErrorCode::InvalidArgument);
        }
        if !["", "liked", "disliked"].contains(&self.feedback.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
        if let Some(station) = &self.station {
            station.validate()?;
            if station.kind != "station" || !self.continuous {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        known(&self.facets, FACETS)?;
        known(&self.actions, ACTIONS)?;
        if !self.session.is_empty() {
            validation::id(&self.session)?;
        }
        if self.current.is_some() && self.session.is_empty() {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut occurrences = std::collections::HashSet::new();
        for entry in self
            .history
            .iter()
            .chain(self.current.iter())
            .chain(&self.upcoming)
        {
            validation::id(&entry.occurrence)?;
            entry.track.validate()?;
            if entry.track.kind != "track" || !occurrences.insert(&entry.occurrence) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        if let Some(prompt) = &self.prompt {
            if self.account != "connecting"
                || prompt.user_code.is_empty()
                || prompt.user_code.len() > 64
                || prompt.user_code.chars().any(char::is_control)
                || !(1..=1800).contains(&prompt.expires_in)
            {
                return Err(ErrorCode::InvalidArgument);
            }
            crate::web::open_url(&prompt.verification_url)?;
            if !prompt.verification_url.starts_with("https://") {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| ErrorCode::Internal)?
            .len()
            > validation::MAX_BODY_BYTES - 4096
        {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}
fn known(values: &[String], allowed: &[&str]) -> Result<(), ErrorCode> {
    let mut seen = std::collections::HashSet::new();
    if values.len() > allowed.len()
        || values
            .iter()
            .any(|v| !allowed.contains(&v.as_str()) || !seen.insert(v))
    {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(())
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub facet: String,
    pub parent: String,
    pub search: String,
    pub token: String,
}
impl Query {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if !FACETS.contains(&self.facet.as_str())
            || self.search.len() > 1024
            || self.search.chars().any(char::is_control)
            || self.token.len() > 256
        {
            return Err(ErrorCode::InvalidArgument);
        }
        if !self.parent.is_empty() {
            validation::id(&self.parent)?;
        }
        Ok(())
    }
    pub fn without_token(&self) -> Self {
        Self {
            token: String::new(),
            ..self.clone()
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub generation: String,
    pub revision: u64,
    pub entries: Vec<Entity>,
    pub next_token: String,
}
impl Page {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.generation)?;
        if self.revision == 0 || self.entries.len() > 100 || self.next_token.len() > 256 {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut ids = std::collections::HashSet::new();
        for entry in &self.entries {
            entry.validate()?;
            if !ids.insert(&entry.id) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub request_id: String,
    pub action: String,
    pub session: String,
    pub entity: String,
    pub occurrence: String,
    pub position_us: u64,
    #[serde(default)]
    pub replaces_provider: String,
    #[serde(default)]
    pub replaces_session: String,
    #[serde(default)]
    pub text: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Active {
    pub provider_id: String,
    pub generation: String,
    pub state: State,
}
impl Command {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.text.len() > 1024
            || self.text.chars().any(char::is_control)
            || (self.action == "save_playlist" && self.text.trim().is_empty())
        {
            return Err(ErrorCode::InvalidArgument);
        }
        uuid::Uuid::parse_str(&self.request_id).map_err(|_| ErrorCode::InvalidArgument)?;
        if !ACTIONS.contains(&self.action.as_str()) || self.position_us > 31_536_000_000_000 {
            return Err(ErrorCode::InvalidArgument);
        }
        for id in [
            &self.session,
            &self.entity,
            &self.occurrence,
            &self.replaces_provider,
            &self.replaces_session,
        ] {
            if !id.is_empty() {
                validation::id(id)?;
            }
        }
        if ["play", "radio", "pin"].contains(&self.action.as_str()) && self.entity.is_empty() {
            return Err(ErrorCode::InvalidArgument);
        }
        if ![
            "play",
            "radio",
            "pin",
            "connect",
            "authorize",
            "cancel",
            "disconnect",
            "scan",
            "save_playlist",
            "delete_playlist",
        ]
        .contains(&self.action.as_str())
            && self.session.is_empty()
        {
            return Err(ErrorCode::InvalidArgument);
        }
        if self.action == "remove" && self.occurrence.is_empty() {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}
pub fn decode<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, ErrorCode> {
    if json.len() > validation::MAX_BODY_BYTES {
        return Err(ErrorCode::InvalidArgument);
    }
    serde_json::from_str(json).map_err(|_| ErrorCode::InvalidArgument)
}
pub fn encode<T: Serialize>(value: &T) -> Result<String, ErrorCode> {
    let json = serde_json::to_string(value).map_err(|_| ErrorCode::Internal)?;
    if json.len() > validation::MAX_BODY_BYTES {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(json)
}

#[zbus::proxy(
    interface = "org.arinn.MusicProvider1",
    default_path = "/org/arinn/Provider1"
)]
pub trait MusicProvider {
    fn features(&self) -> zbus::Result<Vec<String>>;
    fn get_state(&self, generation: &str) -> zbus::Result<String>;
    fn get_page(&self, generation: &str, client: &str, query: &str) -> zbus::Result<String>;
    fn command(&self, generation: &str, client: &str, command: &str) -> zbus::Result<String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_rejects_secret_fields_duplicate_occurrences_and_unsafe_prompts() {
        let state = State::default();
        assert!(state.validate().is_ok());
        let mut v = serde_json::to_value(&state).unwrap();
        v["access_token"] = serde_json::json!("synthetic");
        assert!(decode::<State>(&v.to_string()).is_err());
        let entry = QueueEntry {
            occurrence: "same".into(),
            track: Entity {
                id: "track".into(),
                kind: "track".into(),
                ..Entity::default()
            },
        };
        let mut state = State {
            session: "session".into(),
            current: Some(entry.clone()),
            upcoming: vec![entry],
            ..State::default()
        };
        assert!(state.validate().is_err());
        state.upcoming.clear();
        assert!(state.validate().is_ok());
        state.account = "connecting".into();
        state.prompt = Some(Prompt {
            verification_url: "file:///synthetic".into(),
            user_code: "ABCD".into(),
            expires_in: 60,
        });
        assert!(state.validate().is_err());
    }
    /// Review of #710: artwork accepted any ID, including paths and URLs.
    #[test]
    fn artwork_is_an_opaque_asset_id_never_a_path_or_url() {
        let entity = |artwork: &str| Entity {
            id: "track".into(),
            kind: "track".into(),
            artwork: artwork.into(),
            ..Entity::default()
        };
        assert!(entity("").validate().is_ok());
        assert!(entity("art:3f2a9c").validate().is_ok());
        for bad in [
            "/home/user/cover.jpg",
            "file:///cover.jpg",
            "https://cdn.example/cover.jpg",
            "../cover",
            "a b",
            &"x".repeat(257),
        ] {
            assert!(entity(bad).validate().is_err(), "{bad}");
        }
    }
}
