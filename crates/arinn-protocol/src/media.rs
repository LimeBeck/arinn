// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-stable media contract (ADR-0013, ADR-0024, ADR-0052).
//!
//! Session titles and file URIs travel to the daemon and, for titles, the
//! shell. URIs never reach the shell and neither is logged (spec §13).
use crate::{
    production::EntityRef,
    validation::{self, ErrorCode, MAX_BODY_BYTES, MAX_PAGE_ENTRIES},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zbus::zvariant::{LE, OwnedObjectPath, Type, serialized::Context, to_bytes};

pub const PROVIDER_INTERFACE: &str = "org.arinn.MediaProvider1";
pub const INTERFACE: &str = "org.arinn.Media1";
pub const OPEN_INTERFACE: &str = "org.arinn.MediaOpen1";
pub const TRACKS_INTERFACE: &str = "org.arinn.MediaTracks1";
pub const TIMELINE_INTERFACE: &str = "org.arinn.MediaTimeline1";

/// Feature flags a provider may register. Absence means the interface is not
/// exported. MPRIS does not have to set any of them.
pub const FEATURE_OPEN: &str = "MediaOpen1";
pub const FEATURE_TRACKS: &str = "MediaTracks1";
pub const FEATURE_TIMELINE: &str = "MediaTimeline1";

pub const ACTION_PLAY: &str = "play";
pub const ACTION_PAUSE: &str = "pause";
pub const ACTION_PLAY_PAUSE: &str = "play-pause";
pub const ACTION_SEEK: &str = "seek";
pub const ACTION_NEXT: &str = "next";
pub const ACTION_PREVIOUS: &str = "previous";
pub const ACTION_STOP: &str = "stop";

/// Playback states the shell interpolates. Any other token is shown as
/// unsupported and does not advance the position (spec §9.8).
pub const STATES: &[&str] = &["playing", "paused", "stopped"];
pub const TRACK_AUDIO: &str = "audio";
pub const TRACK_SUBTITLE: &str = "subtitle";
pub const MAX_ACTIONS: usize = 16;
pub const MAX_TRACKS: usize = 64;
pub const MAX_CHAPTERS: usize = 256;
pub const MAX_RANGES: usize = 64;
const MAX_TOKEN: usize = 32;

/// True only for `playing`. Paused, stopped and unknown states stay put.
pub fn advances(state: &str) -> bool {
    state == "playing"
}

fn token(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty()
        || value.len() > MAX_TOKEN
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        Err(ErrorCode::InvalidArgument)
    } else {
        Ok(())
    }
}

/// Empty, or a short BCP 47-like tag (`en`, `en-us`). Not a free string.
pub fn language_tag(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty() {
        return Ok(());
    }
    if value.len() > MAX_TOKEN
        || !value.starts_with(|c: char| c.is_ascii_alphabetic())
        || value.ends_with('-')
        || value.contains("--")
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        Err(ErrorCode::InvalidArgument)
    } else {
        Ok(())
    }
}

/// Absolute `file:///` URI for a local or already-mounted file.
/// Hosts, queries, dot segments and control characters are rejected.
/// The value is never logged by this function.
pub fn file_uri(value: &str) -> Result<(), ErrorCode> {
    if value.len() > validation::MAX_URL_BYTES
        || !value.starts_with("file:///")
        || value.contains(['?', '#', '\\'])
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(ErrorCode::InvalidArgument);
    }
    let path = decode_percent(&value["file://".len()..])?;
    let segments: Vec<&str> = path.split('/').collect();
    if segments.first() != Some(&"")
        || segments.len() < 2
        || segments[1..]
            .iter()
            .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(())
}

fn decode_percent(value: &str) -> Result<String, ErrorCode> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(ErrorCode::InvalidArgument);
            }
            let byte = (hex(bytes[index + 1])? << 4) | hex(bytes[index + 2])?;
            if byte == 0 {
                return Err(ErrorCode::InvalidArgument);
            }
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ErrorCode::InvalidArgument)
}

fn hex(byte: u8) -> Result<u8, ErrorCode> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ErrorCode::InvalidArgument),
    }
}

/// `duration` is −1 when unknown and otherwise non-negative.
pub fn duration_us(value: i64) -> Result<(), ErrorCode> {
    if value >= -1 {
        Ok(())
    } else {
        Err(ErrorCode::InvalidArgument)
    }
}

/// Absolute position. When `duration` is known, the position cannot pass it.
pub fn position_us(position: i64, duration: i64) -> Result<(), ErrorCode> {
    duration_us(duration)?;
    if position < 0 || (duration >= 0 && position > duration) {
        Err(ErrorCode::InvalidArgument)
    } else {
        Ok(())
    }
}

fn asset_ref(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty() {
        Ok(())
    } else {
        validation::id(value)
    }
}

fn entity(value: &EntityRef, required: bool) -> Result<(), ErrorCode> {
    let empty =
        value.provider_id.is_empty() && value.capability.is_empty() && value.local_id.is_empty();
    if !required {
        return if empty {
            Ok(())
        } else {
            Err(ErrorCode::InvalidArgument)
        };
    }
    validation::id(&value.provider_id)?;
    validation::capability(&value.capability)?;
    validation::id(&value.local_id)
}

/// One MPRIS-compatible session. `anchor_us` is `CLOCK_MONOTONIC` in
/// microseconds at the moment `position_us` was sampled. The shell reads the
/// same clock and advances the position only while [`advances`] is true, at
/// most reconciling with a new anchor once per second while playing.
/// `artwork` is an opaque asset id, or empty. `content` is set for a session
/// opened from a library item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub artwork: String,
    pub state: String,
    pub duration_us: i64,
    pub position_us: i64,
    pub anchor_us: u64,
    pub actions: Vec<String>,
    pub has_content: bool,
    pub content: EntityRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SessionPage {
    pub generation: String,
    pub revision: u64,
    pub sessions: Vec<Session>,
    pub next_token: String,
}

/// What [`OPEN_INTERFACE`] receives from the daemon. The URI is not a shell
/// argument and is never logged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MediaSource {
    pub content: EntityRef,
    pub uri: String,
    pub start_position_us: i64,
    pub audio_language: String,
    pub subtitle_language: String,
}

/// Library `Launch` result. The daemon adds the provider id and calls
/// [`MediaSource`] on the selected `MediaOpen1` provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PlaybackSource {
    pub local_id: String,
    pub uri: String,
    pub start_position_us: i64,
    pub audio_language: String,
    pub subtitle_language: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Track {
    pub id: String,
    pub kind: String,
    pub language: String,
    pub label: String,
    pub codec: String,
    pub channels: u32,
    pub selected: bool,
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Chapter {
    pub id: String,
    pub start_us: i64,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct BufferedRange {
    pub start_us: i64,
    pub end_us: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Timeline {
    pub chapters: Vec<Chapter>,
    pub ranges: Vec<BufferedRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Thumbnail {
    pub asset: String,
    pub timestamp_us: i64,
}

impl Session {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        validation::text(&self.title)?;
        validation::text(&self.subtitle)?;
        asset_ref(&self.artwork)?;
        token(&self.state)?;
        position_us(self.position_us, self.duration_us)?;
        if self.actions.len() > MAX_ACTIONS {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut actions = HashSet::new();
        for action in &self.actions {
            token(action)?;
            if !actions.insert(action.as_str()) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        entity(&self.content, self.has_content)
    }
}

impl SessionPage {
    pub fn encoded_len(&self) -> Result<usize, ErrorCode> {
        to_bytes(Context::new_dbus(LE, 0), &(self,))
            .map(|bytes| bytes.len())
            .map_err(|_| ErrorCode::InvalidArgument)
    }

    pub fn validate(&self) -> Result<(), ErrorCode> {
        uuid::Uuid::parse_str(&self.generation).map_err(|_| ErrorCode::InvalidArgument)?;
        if !self.next_token.is_empty() {
            validation::id(&self.next_token)?;
        }
        if self.revision == 0 || self.sessions.len() > MAX_PAGE_ENTRIES {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut ids = HashSet::new();
        for session in &self.sessions {
            session.validate()?;
            if !ids.insert(session.id.as_str()) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        if self.encoded_len()? > MAX_BODY_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}

pub fn validate_sessions(sessions: &[Session]) -> Result<(), ErrorCode> {
    let mut ids = HashSet::new();
    for session in sessions {
        session.validate()?;
        if !ids.insert(session.id.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    Ok(())
}

impl MediaSource {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        entity(&self.content, true)?;
        file_uri(&self.uri)?;
        if self.start_position_us < 0 {
            return Err(ErrorCode::InvalidArgument);
        }
        language_tag(&self.audio_language)?;
        language_tag(&self.subtitle_language)
    }
}

impl PlaybackSource {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.local_id)?;
        file_uri(&self.uri)?;
        if self.start_position_us < 0 {
            return Err(ErrorCode::InvalidArgument);
        }
        language_tag(&self.audio_language)?;
        language_tag(&self.subtitle_language)
    }
}

impl Track {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        if self.kind != TRACK_AUDIO && self.kind != TRACK_SUBTITLE {
            return Err(ErrorCode::InvalidArgument);
        }
        language_tag(&self.language)?;
        validation::text(&self.label)?;
        if !self.codec.is_empty()
            && (self.codec.len() > MAX_TOKEN
                || !self.codec.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'
                }))
        {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}

pub fn validate_tracks(tracks: &[Track]) -> Result<(), ErrorCode> {
    if tracks.len() > MAX_TRACKS {
        return Err(ErrorCode::InvalidArgument);
    }
    let mut ids = HashSet::new();
    let mut selected_audio = 0;
    let mut selected_subtitle = 0;
    for track in tracks {
        track.validate()?;
        if !ids.insert(track.id.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
        if track.selected && track.kind == TRACK_AUDIO {
            selected_audio += 1;
        }
        if track.selected && track.kind == TRACK_SUBTITLE {
            selected_subtitle += 1;
        }
    }
    if selected_audio > 1 || selected_subtitle > 1 {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(())
}

impl Chapter {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        if self.start_us < 0 {
            return Err(ErrorCode::InvalidArgument);
        }
        validation::text(&self.title)
    }
}

impl BufferedRange {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.start_us < 0 || self.end_us < self.start_us {
            Err(ErrorCode::InvalidArgument)
        } else {
            Ok(())
        }
    }
}

impl Timeline {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.chapters.len() > MAX_CHAPTERS || self.ranges.len() > MAX_RANGES {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut ids = HashSet::new();
        for chapter in &self.chapters {
            chapter.validate()?;
            if !ids.insert(chapter.id.as_str()) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        for range in &self.ranges {
            range.validate()?;
        }
        Ok(())
    }
}

impl Thumbnail {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.asset)?;
        if self.timestamp_us < 0 {
            Err(ErrorCode::InvalidArgument)
        } else {
            Ok(())
        }
    }
}

/// `org.arinn.MediaProvider1` at `/org/arinn/Provider1`. Only the daemon calls it.
pub mod provider {
    use super::{Session, SessionPage};
    #[zbus::proxy(
        interface = "org.arinn.MediaProvider1",
        default_path = "/org/arinn/Provider1"
    )]
    pub trait MediaProvider {
        fn get_sessions(&self, token: &str) -> zbus::Result<(SessionPage,)>;
        fn get_session(&self, generation: &str, id: &str) -> zbus::Result<(Session,)>;
        fn backend_state(&self) -> zbus::Result<String>;
        fn features(&self) -> zbus::Result<Vec<String>>;
        fn play(&self, generation: &str, id: &str) -> zbus::Result<()>;
        fn pause(&self, generation: &str, id: &str) -> zbus::Result<()>;
        fn play_pause(&self, generation: &str, id: &str) -> zbus::Result<()>;
        fn seek(&self, generation: &str, id: &str, position_us: i64) -> zbus::Result<()>;
        fn next(&self, generation: &str, id: &str) -> zbus::Result<()>;
        fn previous(&self, generation: &str, id: &str) -> zbus::Result<()>;
        fn stop(&self, generation: &str, id: &str) -> zbus::Result<()>;
        #[zbus(signal, name = "StateChanged")]
        fn provider_state_changed(&self, generation: &str, revision: u64) -> zbus::Result<()>;
    }
}
pub use provider::MediaProviderProxy;

pub mod open {
    use super::MediaSource;
    #[zbus::proxy(
        interface = "org.arinn.MediaOpen1",
        default_path = "/org/arinn/Provider1"
    )]
    pub trait MediaOpen {
        fn open(&self, generation: &str, source: MediaSource) -> zbus::Result<String>;
    }
}
pub use open::MediaOpenProxy;

pub mod tracks {
    use super::Track;
    #[zbus::proxy(
        interface = "org.arinn.MediaTracks1",
        default_path = "/org/arinn/Provider1"
    )]
    pub trait MediaTracks {
        fn list_tracks(&self, generation: &str, id: &str) -> zbus::Result<Vec<Track>>;
        fn select_track(
            &self,
            generation: &str,
            id: &str,
            kind: &str,
            track_id: &str,
        ) -> zbus::Result<()>;
        fn disable_subtitles(&self, generation: &str, id: &str) -> zbus::Result<()>;
    }
}
pub use tracks::MediaTracksProxy;

pub mod timeline {
    use super::{Thumbnail, Timeline};
    #[zbus::proxy(
        interface = "org.arinn.MediaTimeline1",
        default_path = "/org/arinn/Provider1"
    )]
    pub trait MediaTimeline {
        fn get_timeline(&self, generation: &str, id: &str) -> zbus::Result<(Timeline,)>;
        fn request_thumbnail(
            &self,
            generation: &str,
            id: &str,
            position_us: i64,
        ) -> zbus::Result<(Thumbnail,)>;
    }
}
pub use timeline::MediaTimelineProxy;

/// `org.arinn.Media1` on the provider's daemon path. Commands return
/// `Operation1` objects. Extension methods exist on this interface so the
/// shell has one proxy; the daemon returns `Unsupported` unless `Features`
/// lists the flag. The shell never sends a URI.
#[zbus::proxy(interface = "org.arinn.Media1", default_service = "org.arinn.Daemon1")]
pub trait MediaApi {
    fn get_sessions(&self, token: &str) -> zbus::Result<(SessionPage,)>;
    fn get_session(&self, generation: &str, id: &str) -> zbus::Result<(Session,)>;
    fn features(&self) -> zbus::Result<Vec<String>>;
    fn play(&self, generation: &str, id: &str, request_id: &str) -> zbus::Result<OwnedObjectPath>;
    fn pause(&self, generation: &str, id: &str, request_id: &str) -> zbus::Result<OwnedObjectPath>;
    fn play_pause(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
    fn seek(
        &self,
        generation: &str,
        id: &str,
        position_us: i64,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
    fn next(&self, generation: &str, id: &str, request_id: &str) -> zbus::Result<OwnedObjectPath>;
    fn previous(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
    fn stop(&self, generation: &str, id: &str, request_id: &str) -> zbus::Result<OwnedObjectPath>;
    fn list_tracks(&self, generation: &str, id: &str) -> zbus::Result<Vec<Track>>;
    fn select_track(
        &self,
        generation: &str,
        id: &str,
        kind: &str,
        track_id: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
    fn disable_subtitles(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
    fn get_timeline(&self, generation: &str, id: &str) -> zbus::Result<(Timeline,)>;
    fn request_thumbnail(
        &self,
        generation: &str,
        id: &str,
        position_us: i64,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(on: bool) -> EntityRef {
        if on {
            EntityRef {
                provider_id: "library".into(),
                capability: "library".into(),
                local_id: "film-1".into(),
            }
        } else {
            EntityRef {
                provider_id: String::new(),
                capability: String::new(),
                local_id: String::new(),
            }
        }
    }

    fn session() -> Session {
        Session {
            id: "s1".into(),
            title: "Film".into(),
            subtitle: "Artist".into(),
            artwork: "asset-1".into(),
            state: "playing".into(),
            duration_us: 60_000_000,
            position_us: 1_000_000,
            anchor_us: 42,
            actions: vec!["play".into(), "pause".into(), "seek".into()],
            has_content: true,
            content: content(true),
        }
    }

    #[test]
    fn wire_signatures_round_trip() {
        assert_eq!(Session::SIGNATURE.to_string(), "(sssssxxtasb(sss))");
        assert_eq!(
            SessionPage::SIGNATURE.to_string(),
            "(sta(sssssxxtasb(sss))s)"
        );
        assert_eq!(MediaSource::SIGNATURE.to_string(), "((sss)sxss)");
        assert_eq!(PlaybackSource::SIGNATURE.to_string(), "(ssxss)");
        assert_eq!(Track::SIGNATURE.to_string(), "(sssssubb)");
        assert_eq!(Timeline::SIGNATURE.to_string(), "(a(sxs)a(xx))");
        assert_eq!(Thumbnail::SIGNATURE.to_string(), "(sx)");
        let page = SessionPage {
            generation: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            sessions: vec![session()],
            next_token: String::new(),
        };
        page.validate().unwrap();
        let bytes = to_bytes(Context::new_dbus(LE, 0), &(&page,)).unwrap();
        let (decoded,): (SessionPage,) = bytes.deserialize().unwrap().0;
        assert_eq!(decoded, page);
    }

    #[test]
    fn file_uris_are_local_absolute_paths() {
        assert_eq!(file_uri("file:///media/film.mkv"), Ok(()));
        assert_eq!(file_uri("file:///media/my%20film.mkv"), Ok(()));
        for bad in [
            "https://example.org/a.mkv",
            "file://host/media/a.mkv",
            "file:///media/../etc/passwd",
            "file:///media/%2e%2e/passwd",
            "file:///media/a.mkv?x=1",
            "file:///",
            "",
        ] {
            assert_eq!(file_uri(bad), Err(ErrorCode::InvalidArgument), "{bad}");
        }
    }

    #[test]
    fn sessions_reject_bad_positions_duplicate_actions_and_partial_content() {
        let mut item = session();
        item.position_us = item.duration_us + 1;
        assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));
        item.position_us = 0;
        item.duration_us = -1;
        assert_eq!(item.validate(), Ok(()));
        item.actions.push("seek".into());
        assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));
        item.actions.pop();
        item.has_content = false;
        assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));
        item.content = content(false);
        assert_eq!(item.validate(), Ok(()));
        assert!(!advances("paused"));
        assert!(advances("playing"));
        assert_eq!(
            validate_sessions(&[session(), session()]),
            Err(ErrorCode::InvalidArgument)
        );
    }

    #[test]
    fn tracks_and_timeline_are_bounded_and_exclusive() {
        let track = Track {
            id: "a".into(),
            kind: TRACK_AUDIO.into(),
            language: "en".into(),
            label: "Stereo".into(),
            codec: "aac".into(),
            channels: 2,
            selected: true,
            external: false,
        };
        assert_eq!(
            validate_tracks(&[track.clone(), track]),
            Err(ErrorCode::InvalidArgument)
        );
        let timeline = Timeline {
            chapters: vec![Chapter {
                id: "c".into(),
                start_us: 0,
                title: "Start".into(),
            }],
            ranges: vec![BufferedRange {
                start_us: 0,
                end_us: 10,
            }],
        };
        timeline.validate().unwrap();
        assert_eq!(
            BufferedRange {
                start_us: 5,
                end_us: 4
            }
            .validate(),
            Err(ErrorCode::InvalidArgument)
        );
        Thumbnail {
            asset: "thumb".into(),
            timestamp_us: 0,
        }
        .validate()
        .unwrap();
    }
}
