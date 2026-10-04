// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! MPRIS for Arinn-managed playback (ADR-0081).
//!
//! Plasma takes the media keys as global shortcuts and sends them to the
//! current MPRIS player, so Arinn's own mpv must be one: while a session
//! plays or is paused the provider owns `org.mpris.MediaPlayer2.arinn_mpv.instance<pid>`
//! and maps the MPRIS player calls onto the same session commands the shell
//! uses. The `arinn_` name prefix is reserved for Arinn providers; the MPRIS
//! provider skips it, so the session is not listed twice.
use crate::player::{Player, View};
use arinn_plugin_sdk::runtime::MediaAction;
use arinn_protocol::media::{self, Session};
use std::collections::HashMap;
use tokio::sync::watch;
use zbus::{
    Connection, fdo,
    object_server::SignalEmitter,
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value},
};

pub const PATH: &str = "/org/mpris/MediaPlayer2";
/// Reserved prefix for players Arinn providers export.
pub const RESERVED_PREFIX: &str = "org.mpris.MediaPlayer2.arinn_";

pub fn bus_name() -> String {
    format!("{RESERVED_PREFIX}mpv.instance{}", std::process::id())
}

/// MPRIS `PlaybackStatus` for a session state.
pub fn playback_status(state: &str) -> &'static str {
    match state {
        media::STATE_PLAYING => "Playing",
        media::STATE_PAUSED => "Paused",
        _ => "Stopped",
    }
}

/// The track object path for a session id; MPRIS needs a D-Bus path.
pub fn track_id(session: &str) -> OwnedObjectPath {
    let safe: String = session
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    ObjectPath::try_from(format!("/org/arinn/mpv/session/s{safe}"))
        .map(Into::into)
        .unwrap_or_else(|_| {
            ObjectPath::from_static_str_unchecked("/org/mpris/MediaPlayer2/TrackList/NoTrack")
                .into()
        })
}

/// The playback position now, in microseconds: the snapshot position, plus
/// the time since its anchor while playing, clamped to the duration.
pub fn position_now(session: &Session, now_us: u64) -> i64 {
    let mut position = session.position_us.max(0);
    if session.state == media::STATE_PLAYING {
        let elapsed = now_us.saturating_sub(session.anchor_us);
        position = position.saturating_add(i64::try_from(elapsed).unwrap_or(i64::MAX));
    }
    if session.duration_us >= 0 {
        position = position.min(session.duration_us);
    }
    position
}

/// Target of an MPRIS relative `Seek`, clamped to the file.
pub fn seek_target(session: &Session, now_us: u64, offset_us: i64) -> i64 {
    let target = position_now(session, now_us)
        .saturating_add(offset_us)
        .max(0);
    if session.duration_us >= 0 {
        target.min(session.duration_us)
    } else {
        target
    }
}

fn allows(session: Option<&Session>, action: &str) -> bool {
    session.is_some_and(|s| s.actions.iter().any(|a| a == action))
}

/// MPRIS `Metadata` for a session.
pub fn metadata(session: Option<&Session>) -> HashMap<String, OwnedValue> {
    let mut map = HashMap::new();
    let Some(session) = session else {
        return map;
    };
    let mut put = |key: &str, value: Value<'_>| {
        if let Ok(value) = OwnedValue::try_from(value) {
            map.insert(key.to_string(), value);
        }
    };
    put("mpris:trackid", Value::from(track_id(&session.id)));
    if !session.title.is_empty() {
        put("xesam:title", Value::from(session.title.clone()));
    }
    if session.duration_us >= 0 {
        put("mpris:length", Value::from(session.duration_us));
    }
    map
}

fn current(view: &View) -> Option<&Session> {
    view.sessions.first()
}

/// Whether the media keys should reach this player: only a playing or
/// paused session. A stopped or ended one keeps no action the keys use, so
/// the name is released and Plasma picks another player.
pub fn controllable(session: Option<&Session>) -> bool {
    session.is_some_and(|s| s.state == media::STATE_PLAYING || s.state == media::STATE_PAUSED)
}

pub struct Root;

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {}
    fn quit(&self) -> fdo::Result<()> {
        Err(fdo::Error::NotSupported("Quit".into()))
    }
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> &str {
        "Arinn"
    }
    #[zbus(property)]
    fn desktop_entry(&self) -> &str {
        "org.arinn.Shell"
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

pub struct PlayerIface {
    player: Player,
    view: watch::Receiver<View>,
    now: fn() -> u64,
}

impl PlayerIface {
    pub fn new(player: Player, view: watch::Receiver<View>, now: fn() -> u64) -> Self {
        Self { player, view, now }
    }

    fn session(&self) -> Option<Session> {
        current(&self.view.borrow()).cloned()
    }

    async fn run(&self, action: MediaAction, position_us: i64) -> fdo::Result<()> {
        let session = self
            .session()
            .ok_or_else(|| fdo::Error::Failed("no session".into()))?;
        self.player
            .control(action, session.id, position_us)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl PlayerIface {
    async fn play_pause(&self) -> fdo::Result<()> {
        let session = self.session();
        // A player without the toggle still gets the action the key means.
        if allows(session.as_ref(), media::ACTION_PLAY_PAUSE) {
            self.run(MediaAction::PlayPause, 0).await
        } else if session.is_some_and(|s| s.state == media::STATE_PLAYING) {
            self.run(MediaAction::Pause, 0).await
        } else {
            self.run(MediaAction::Play, 0).await
        }
    }
    async fn play(&self) -> fdo::Result<()> {
        self.run(MediaAction::Play, 0).await
    }
    async fn pause(&self) -> fdo::Result<()> {
        self.run(MediaAction::Pause, 0).await
    }
    async fn stop(&self) -> fdo::Result<()> {
        self.run(MediaAction::Stop, 0).await
    }
    async fn next(&self) -> fdo::Result<()> {
        self.run(MediaAction::Next, 0).await
    }
    async fn previous(&self) -> fdo::Result<()> {
        self.run(MediaAction::Previous, 0).await
    }
    async fn seek(
        &self,
        offset: i64,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let session = self
            .session()
            .ok_or_else(|| fdo::Error::Failed("no session".into()))?;
        let target = seek_target(&session, (self.now)(), offset);
        self.run(MediaAction::Seek, target).await?;
        // Clients extrapolate Position; a jump must be announced.
        Self::seeked(&emitter, target).await?;
        Ok(())
    }
    async fn set_position(
        &self,
        track: ObjectPath<'_>,
        position: i64,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let session = self
            .session()
            .ok_or_else(|| fdo::Error::Failed("no session".into()))?;
        // The spec ignores a stale track id or an out-of-range position.
        if track.as_str() != track_id(&session.id).as_str()
            || position < 0
            || (session.duration_us >= 0 && position > session.duration_us)
        {
            return Ok(());
        }
        self.run(MediaAction::Seek, position).await?;
        Self::seeked(&emitter, position).await?;
        Ok(())
    }
    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;
    fn open_uri(&self, _uri: &str) -> fdo::Result<()> {
        Err(fdo::Error::NotSupported("OpenUri".into()))
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        playback_status(self.session().map(|s| s.state).as_deref().unwrap_or("")).into()
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn set_rate(&self, _rate: f64) {}
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        metadata(self.session().as_ref())
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        1.0
    }
    /// Volume is the system's (Plasma owns the volume keys, spec §6.2);
    /// Arinn playback has no player volume, so a write is refused rather
    /// than acknowledged and ignored (review of #326).
    #[zbus(property)]
    fn set_volume(&self, _volume: f64) -> fdo::Result<()> {
        Err(fdo::Error::NotSupported(
            "Volume follows the system volume".into(),
        ))
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.session()
            .map(|s| position_now(&s, (self.now)()))
            .unwrap_or(0)
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        allows(self.session().as_ref(), media::ACTION_NEXT)
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        allows(self.session().as_ref(), media::ACTION_PREVIOUS)
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        let session = self.session();
        allows(session.as_ref(), media::ACTION_PLAY)
            || allows(session.as_ref(), media::ACTION_PLAY_PAUSE)
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        let session = self.session();
        allows(session.as_ref(), media::ACTION_PAUSE)
            || allows(session.as_ref(), media::ACTION_PLAY_PAUSE)
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        allows(self.session().as_ref(), media::ACTION_SEEK)
    }
    #[zbus(property(emits_changed_signal = "const"))]
    fn can_control(&self) -> bool {
        true
    }
}

/// Export the player and keep the bus name only while a session can be
/// controlled (see [`controllable`]).
/// Property changes follow the provider's view.
pub async fn run(connection: Connection, player: Player, now: fn() -> u64) -> zbus::Result<()> {
    let mut notes = player.subscribe();
    connection.object_server().at(PATH, Root).await?;
    connection
        .object_server()
        .at(PATH, PlayerIface::new(player, notes.clone(), now))
        .await?;
    let iface = connection
        .object_server()
        .interface::<_, PlayerIface>(PATH)
        .await?;
    let name = bus_name();
    let mut owned = false;
    loop {
        let has_session = controllable(current(&notes.borrow_and_update()));
        if has_session && !owned {
            connection.request_name(name.as_str()).await?;
            owned = true;
        } else if !has_session && owned {
            connection.release_name(name.as_str()).await?;
            owned = false;
        }
        if owned {
            let emitter = iface.signal_emitter();
            let player = iface.get().await;
            player.playback_status_changed(emitter).await?;
            player.metadata_changed(emitter).await?;
            player.can_play_changed(emitter).await?;
            player.can_pause_changed(emitter).await?;
            player.can_seek_changed(emitter).await?;
            player.can_go_next_changed(emitter).await?;
            player.can_go_previous_changed(emitter).await?;
        }
        if notes.changed().await.is_err() {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arinn_protocol::production::EntityRef;

    fn session(state: &str, position_us: i64, anchor_us: u64, duration_us: i64) -> Session {
        Session {
            id: "3e669532-304b-497b-8845-bf4bea6afe50".into(),
            title: "Clip".into(),
            subtitle: String::new(),
            artwork: String::new(),
            state: state.into(),
            duration_us,
            position_us,
            anchor_us,
            actions: vec![media::ACTION_PLAY_PAUSE.into(), media::ACTION_SEEK.into()],
            has_content: false,
            content: EntityRef {
                provider_id: String::new(),
                capability: String::new(),
                local_id: String::new(),
            },
        }
    }

    #[test]
    fn names_are_reserved_and_states_map() {
        assert!(bus_name().starts_with(RESERVED_PREFIX));
        assert_eq!(playback_status(media::STATE_PLAYING), "Playing");
        assert_eq!(playback_status(media::STATE_PAUSED), "Paused");
        assert_eq!(playback_status(media::STATE_ENDED), "Stopped");
        assert_eq!(playback_status(""), "Stopped");
    }

    #[test]
    fn position_runs_only_while_playing_and_stays_in_the_file() {
        let playing = session(media::STATE_PLAYING, 10_000_000, 1_000_000, 60_000_000);
        assert_eq!(position_now(&playing, 3_000_000), 12_000_000);
        assert_eq!(position_now(&playing, 1_000_000_000), 60_000_000);
        let paused = session(media::STATE_PAUSED, 10_000_000, 1_000_000, 60_000_000);
        assert_eq!(position_now(&paused, 3_000_000), 10_000_000);
        assert_eq!(seek_target(&paused, 0, -30_000_000), 0);
        assert_eq!(seek_target(&paused, 0, 5_000_000), 15_000_000);
        assert_eq!(seek_target(&paused, 0, 90_000_000), 60_000_000);
    }

    // Review of #326: a stopped or ended session must not keep the media keys.
    #[test]
    fn only_a_playing_or_paused_session_keeps_the_name() {
        assert!(controllable(Some(&session(media::STATE_PLAYING, 0, 0, -1))));
        assert!(controllable(Some(&session(media::STATE_PAUSED, 0, 0, -1))));
        assert!(!controllable(Some(&session(
            media::STATE_STOPPED,
            0,
            0,
            -1
        ))));
        assert!(!controllable(Some(&session(media::STATE_ENDED, 0, 0, -1))));
        assert!(!controllable(None));
    }

    #[test]
    fn metadata_has_track_title_and_length() {
        let paused = session(media::STATE_PAUSED, 0, 0, 60_000_000);
        let map = metadata(Some(&paused));
        let track: OwnedObjectPath = map["mpris:trackid"].clone().try_into().unwrap();
        assert_eq!(track, track_id(&paused.id));
        assert!(track.as_str().starts_with("/org/arinn/mpv/session/s"));
        let title: String = map["xesam:title"].clone().try_into().unwrap();
        assert_eq!(title, "Clip");
        let length: i64 = map["mpris:length"].clone().try_into().unwrap();
        assert_eq!(length, 60_000_000);
        assert!(metadata(None).is_empty());
    }
}
