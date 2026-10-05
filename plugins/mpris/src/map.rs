// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure MPRIS → session mapping. No bus names, titles or URLs are logged.
use arinn_protocol::{
    media::{self, Session},
    production::EntityRef,
    validation::{self, MAX_TEXT_BYTES},
};
use std::time::{Duration, Instant};

pub(crate) const NAME_PREFIX: &str = "org.mpris.MediaPlayer2.";
/// Players Arinn providers export themselves (ADR-0081). Their sessions
/// already reach the daemon through that provider.
pub(crate) const RESERVED: &str = "arinn_";
const SEEK_SLACK_US: i64 = 1_500_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Caps {
    pub play: bool,
    pub pause: bool,
    pub seek: bool,
    pub next: bool,
    pub previous: bool,
    pub control: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RawPlayer {
    pub title: String,
    pub subtitle: String,
    pub state: String,
    pub duration_us: i64,
    pub position_us: i64,
    pub actions: Vec<String>,
    pub track_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sample {
    pub anchor_us: u64,
    pub anchored_at: Instant,
    pub state: String,
    pub position_us: i64,
}

pub(crate) fn session_id(bus_name: &str) -> Option<&str> {
    let rest = bus_name.strip_prefix(NAME_PREFIX)?;
    if rest.starts_with(RESERVED) || validation::id(rest).is_err() {
        return None;
    }
    Some(rest)
}

pub(crate) fn playback_state(status: &str) -> &'static str {
    match status {
        "Playing" => "playing",
        "Paused" => "paused",
        "Stopped" => "stopped",
        _ => "unknown",
    }
}

/// Artist when any name is present, otherwise the album. Empty stays empty.
pub(crate) fn subtitle(artists: &[String], album: &str) -> String {
    let names: Vec<&str> = artists
        .iter()
        .map(String::as_str)
        .filter(|name| !name.is_empty())
        .collect();
    let joined = names.join(", ");
    let chosen = if names.is_empty() {
        album
    } else {
        joined.as_str()
    };
    bounded(chosen)
}

/// `CanControl` false clears every flag. `play-pause` is offered only when
/// the player can pause: MPRIS `PlayPause` fails otherwise (ADR-0054).
pub(crate) fn actions(caps: &Caps) -> Vec<String> {
    if !caps.control {
        return Vec::new();
    }
    let mut names = Vec::new();
    if caps.play {
        names.push(media::ACTION_PLAY.to_string());
    }
    if caps.pause {
        names.push(media::ACTION_PAUSE.to_string());
        names.push(media::ACTION_PLAY_PAUSE.to_string());
    }
    if caps.seek {
        names.push(media::ACTION_SEEK.to_string());
    }
    if caps.next {
        names.push(media::ACTION_NEXT.to_string());
    }
    if caps.previous {
        names.push(media::ACTION_PREVIOUS.to_string());
    }
    names.push(media::ACTION_STOP.to_string());
    names
}

pub(crate) fn bounded(value: &str) -> String {
    let mut cleaned = value.replace('\0', "");
    if cleaned.len() <= MAX_TEXT_BYTES {
        return cleaned;
    }
    let mut end = MAX_TEXT_BYTES;
    while end > 0 && !cleaned.is_char_boundary(end) {
        end -= 1;
    }
    cleaned.truncate(end);
    cleaned
}

pub(crate) fn clamp_position(position: i64, duration: i64) -> i64 {
    let position = position.max(0);
    if duration >= 0 && position > duration {
        duration
    } else {
        position
    }
}

/// A new anchor on the first sample, on a state change, on a seek, and at
/// most once per second while `playing` (ADR-0054).
pub(crate) fn refresh_anchor(
    previous: Option<&Sample>,
    state: &str,
    position_us: i64,
    force: bool,
    now: Instant,
) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    if force || previous.state != state {
        return true;
    }
    if state != "playing" {
        return previous.position_us != position_us;
    }
    let elapsed = now.saturating_duration_since(previous.anchored_at);
    if seek_jump(previous.position_us, position_us, elapsed) {
        return true;
    }
    elapsed >= Duration::from_secs(1)
}

fn seek_jump(previous: i64, position: i64, elapsed: Duration) -> bool {
    let elapsed_us = i64::try_from(elapsed.as_micros()).unwrap_or(i64::MAX);
    let expected = previous.saturating_add(elapsed_us);
    (position - expected).saturating_abs() > SEEK_SLACK_US
}

/// Position for a relative seek when a live read is unavailable. While
/// `playing`, advance from the anchor by the time since it was taken.
pub(crate) fn extrapolated_position(sample: &Sample, now: Instant) -> i64 {
    if sample.state != "playing" {
        return sample.position_us;
    }
    let elapsed = now.saturating_duration_since(sample.anchored_at);
    let elapsed_us = i64::try_from(elapsed.as_micros()).unwrap_or(i64::MAX);
    sample.position_us.saturating_add(elapsed_us)
}

pub(crate) fn project(
    id: &str,
    raw: &RawPlayer,
    previous: Option<&Sample>,
    force: bool,
    now: Instant,
    now_us: u64,
) -> (Session, Sample) {
    let refresh = refresh_anchor(previous, &raw.state, raw.position_us, force, now);
    let (anchor_us, anchored_at, position_us) = match (refresh, previous) {
        (false, Some(previous)) => (
            previous.anchor_us,
            previous.anchored_at,
            previous.position_us,
        ),
        _ => (now_us, now, raw.position_us),
    };
    let position_us = clamp_position(position_us, raw.duration_us);
    let session = Session {
        id: id.to_string(),
        title: raw.title.clone(),
        subtitle: raw.subtitle.clone(),
        artwork: String::new(),
        state: raw.state.clone(),
        duration_us: raw.duration_us,
        position_us,
        anchor_us,
        actions: raw.actions.clone(),
        has_content: false,
        content: EntityRef {
            provider_id: String::new(),
            capability: String::new(),
            local_id: String::new(),
        },
    };
    let sample = Sample {
        anchor_us,
        anchored_at,
        state: raw.state.clone(),
        position_us,
    };
    (session, sample)
}

pub(crate) fn monotonic_us() -> u64 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // CLOCK_MONOTONIC is the clock the shell interpolates against (ADR-0054).
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) };
    if rc != 0 {
        return 0;
    }
    (time.tv_sec as u64).saturating_mul(1_000_000) + (time.tv_nsec as u64) / 1_000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(on: bool) -> Caps {
        Caps {
            play: on,
            pause: on,
            seek: on,
            next: on,
            previous: on,
            control: on,
        }
    }

    fn raw(state: &str, position: i64) -> RawPlayer {
        RawPlayer {
            title: "Fixture".into(),
            subtitle: "Artist".into(),
            state: state.into(),
            duration_us: 10_000_000,
            position_us: position,
            actions: actions(&caps(true)),
            track_id: Some("/org/mpris/MediaPlayer2/Track/1".into()),
        }
    }

    #[test]
    fn ids_are_the_instance_suffix() {
        assert_eq!(
            session_id("org.mpris.MediaPlayer2.fixture"),
            Some("fixture")
        );
        assert_eq!(
            session_id("org.mpris.MediaPlayer2.one.two"),
            Some("one.two")
        );
        assert_eq!(session_id("org.mpris.MediaPlayer2."), None);
        // ADR-0081: players of Arinn providers are not listed twice.
        assert_eq!(
            session_id("org.mpris.MediaPlayer2.arinn_mpv.instance42"),
            None
        );
        assert_eq!(
            session_id("org.mpris.MediaPlayer2.arinnplayer"),
            Some("arinnplayer")
        );
        assert_eq!(session_id("org.mpris.MediaPlayer2"), None);
    }

    #[test]
    fn subtitle_prefers_artist_then_album() {
        assert_eq!(
            subtitle(&["Artist".into(), "Guest".into()], "Album"),
            "Artist, Guest"
        );
        assert_eq!(subtitle(&[], "Album"), "Album");
        assert_eq!(subtitle(&[String::new()], "Album"), "Album");
        assert_eq!(subtitle(&[], ""), "");
    }

    #[test]
    fn actions_follow_capability_flags() {
        assert_eq!(
            actions(&caps(true)),
            vec![
                "play",
                "pause",
                "play-pause",
                "seek",
                "next",
                "previous",
                "stop",
            ]
        );
        let mut partial = caps(false);
        partial.play = true;
        partial.pause = true;
        partial.seek = true;
        assert!(actions(&partial).is_empty());
        partial.control = true;
        partial.pause = false;
        assert_eq!(actions(&partial), vec!["play", "seek", "stop"]);
        partial.pause = true;
        assert_eq!(
            actions(&partial),
            vec!["play", "pause", "play-pause", "seek", "stop"]
        );
    }

    #[test]
    fn relative_seek_advances_from_the_anchor_while_playing() {
        let now = Instant::now();
        let sample = Sample {
            anchor_us: 10,
            anchored_at: now,
            state: "playing".into(),
            position_us: 1_000,
        };
        let later = now + Duration::from_millis(400);
        assert_eq!(extrapolated_position(&sample, later), 401_000);
        let paused = Sample {
            state: "paused".into(),
            ..sample
        };
        assert_eq!(extrapolated_position(&paused, later), 1_000);
    }

    #[test]
    fn unknown_playback_status_does_not_advance() {
        assert_eq!(playback_state("Playing"), "playing");
        assert_eq!(playback_state("Paused"), "paused");
        assert_eq!(playback_state("Stopped"), "stopped");
        assert_eq!(playback_state("Buffering"), "unknown");
        assert!(!media::advances(playback_state("Buffering")));
    }

    #[test]
    fn anchors_follow_state_seek_and_the_playing_interval() {
        let now = Instant::now();
        let (session, sample) = project("fixture", &raw("playing", 1_000), None, false, now, 10);
        assert_eq!(session.anchor_us, 10);
        assert!(session.artwork.is_empty());
        assert!(!session.has_content);
        let soon = now + Duration::from_millis(200);
        assert!(!refresh_anchor(
            Some(&sample),
            "playing",
            1_200_000,
            false,
            soon
        ));
        let (_, held) = project(
            "fixture",
            &raw("playing", 1_200_000),
            Some(&sample),
            false,
            soon,
            99,
        );
        assert_eq!(held.anchor_us, 10);
        assert_eq!(held.position_us, 1_000);
        assert!(refresh_anchor(Some(&sample), "paused", 1_000, false, soon));
        assert!(refresh_anchor(
            Some(&sample),
            "playing",
            1_000 + 5_000_000,
            false,
            soon
        ));
        let later = now + Duration::from_millis(1_100);
        assert!(refresh_anchor(
            Some(&sample),
            "playing",
            1_000,
            false,
            later
        ));
        let (_, forced) = project(
            "fixture",
            &raw("playing", 2_000_000),
            Some(&sample),
            true,
            soon,
            50,
        );
        assert_eq!((forced.anchor_us, forced.position_us), (50, 2_000_000));
    }
}
