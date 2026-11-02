// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{
    Track,
    player::{Player, Source},
    queue::{MAX_UPCOMING, Queue},
};
use arinn_protocol::{
    music::{self, Command, Entity, QueueEntry, State},
    validation::ErrorCode,
};
use serde_json::json;
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{Arc, Weak},
};
use tokio::sync::Mutex;
type Reply<'a, T> = Pin<Box<dyn Future<Output = Result<T, ErrorCode>> + Send + 'a>>;
pub trait Resolver: Send + Sync + 'static {
    fn resolve(&self, id: String) -> Reply<'_, Source>;
    /// Whether the provider records likes and dislikes for its tracks.
    fn rates(&self) -> bool {
        false
    }
    /// Records a like (true) or dislike (false) with the service. A reply is
    /// the service's confirmation; nothing is recorded locally.
    fn rate(&self, _id: String, _like: bool) -> Reply<'_, ()> {
        Box::pin(async { Err(ErrorCode::Unsupported) })
    }
}
/// Playback events a continuous source reports back to its service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StationEvent {
    /// The player confirmed the track is playing.
    Started(String),
    /// A natural end; microseconds played.
    Finished(String, u64),
    /// The listener moved on; microseconds played.
    Skipped(String, u64),
}
/// A provider-owned continuous source (a station or radio). Its seed,
/// session and batch identity stay inside the provider.
pub trait Station: Send + Sync + 'static {
    /// The next tracks, after the last one the listener reached (if any).
    /// An empty list means the station has nothing more right now.
    fn next(&self, after: Option<String>) -> Reply<'_, Vec<Track>>;
    /// Best-effort playback feedback; a failure never stops playback.
    fn feedback(&self, event: StationEvent) -> Reply<'_, ()>;
}
/// A continuous queue asks for more tracks when fewer than this many are
/// loaded after the current one.
const LOW_WATER: usize = 3;
struct Data {
    queue: Queue,
    tracks: BTreeMap<String, Track>,
    tail: VecDeque<String>,
    session: String,
    token: u64,
    revision: u64,
    position: u64,
    duration: u64,
    status: String,
    error: Option<ErrorCode>,
    loaded: bool,
    /// The target of a seek to or past the reported duration. The end that
    /// follows is not a natural end; once playback runs a second past the
    /// target the duration was wrong and a later end is natural again.
    seek_end: Option<u64>,
    /// Consecutive failed polls; a single slow reply is not a stop.
    failures: u8,
    /// The continuous source of a station session; None for a finite queue.
    station: Option<Arc<dyn Station>>,
    /// Refill state of a station session (protocol `refill`).
    refill: &'static str,
    /// The occurrence whose start the player confirmed and was reported.
    started: Option<u64>,
}
pub struct Playback {
    data: Mutex<Data>,
    io: Mutex<()>,
    player: Player,
    resolver: Arc<dyn Resolver>,
}
impl Playback {
    pub fn new(player: Player, resolver: Arc<dyn Resolver>) -> Arc<Self> {
        let this = Arc::new(Self {
            data: Mutex::new(Data {
                queue: Queue::default(),
                tracks: BTreeMap::new(),
                tail: VecDeque::new(),
                session: String::new(),
                token: 0,
                revision: 1,
                position: 0,
                duration: 0,
                status: "stopped".into(),
                error: None,
                loaded: false,
                seek_end: None,
                failures: 0,
                station: None,
                refill: "idle",
                started: None,
            }),
            io: Mutex::new(()),
            player,
            resolver,
        });
        let weak = Arc::downgrade(&this);
        tokio::spawn(async move {
            Self::poll_loop(weak).await;
        });
        this
    }
    pub async fn state(&self) -> State {
        let d = self.data.lock().await;
        let entry = |e: &crate::queue::Entry| {
            d.tracks.get(&e.track).map(|t| QueueEntry {
                occurrence: e.occurrence.to_string(),
                track: entity(t),
            })
        };
        let cursor = d.queue.current().map(|e| e.occurrence);
        let index = d
            .queue
            .entries()
            .iter()
            .position(|e| Some(e.occurrence) == cursor)
            .unwrap_or(0);
        let mut actions = vec!["play".into()];
        if cursor.is_some() {
            actions.push("stop".into());
            if d.status == "playing" {
                actions.push("pause".into());
            }
            if d.status == "paused" {
                actions.push("resume".into());
            }
            if d.duration > 0 && d.loaded && ["playing", "paused"].contains(&d.status.as_str()) {
                actions.push("seek".into());
            }
            if index > 0 {
                actions.push("previous".into());
            }
            if d.queue.upcoming() > 0 || !d.tail.is_empty() {
                actions.push("next".into());
                if d.station.is_none() {
                    actions.push("remove".into());
                }
            }
            if d.station.is_some() && d.refill == "failed" {
                actions.push("retry".into());
            }
            if self.resolver.rates() {
                actions.push("like".into());
                actions.push("dislike".into());
            }
        }
        State {
            revision: d.revision,
            backend: "available".into(),
            account: "local".into(),
            facets: music::FACETS[..4].iter().map(|s| s.to_string()).collect(),
            actions,
            session: d.session.clone(),
            playback: d.status.clone(),
            position_us: d.position,
            current: d.queue.current().and_then(entry).map(|mut current| {
                if d.duration > 0 {
                    current.track.duration_us = d.duration;
                }
                current
            }),
            history: d
                .queue
                .entries()
                .iter()
                .take(index)
                .filter_map(entry)
                .collect(),
            upcoming: d
                .queue
                .entries()
                .iter()
                .skip(index + 1)
                .filter_map(entry)
                .collect(),
            continuous: d.station.is_some(),
            refill: d.refill.into(),
            error: d.error,
            ..State::default()
        }
    }
    pub async fn play(self: &Arc<Self>, tracks: Vec<Track>) -> Result<State, ErrorCode> {
        if tracks.is_empty() || tracks.len() > 100_000 {
            return Err(ErrorCode::NotFound);
        }
        let token = {
            let mut d = self.data.lock().await;
            d.token = d.token.checked_add(1).ok_or(ErrorCode::Internal)?;
            d.session = uuid::Uuid::new_v4().to_string();
            d.tail = tracks.iter().skip(65).map(|t| t.id.clone()).collect();
            d.queue
                .start(tracks.iter().take(65).map(|t| t.id.clone()).collect(), None)
                .map_err(|_| ErrorCode::InvalidArgument)?;
            d.tracks = tracks.into_iter().map(|t| (t.id.clone(), t)).collect();
            d.station = None;
            d.refill = "idle";
            d.status = "loading".into();
            d.loaded = false;
            d.position = 0;
            d.duration = 0;
            d.seek_end = None;
            d.error = None;
            d.revision += 1;
            d.token
        };
        let owner = self.clone();
        tokio::spawn(async move {
            owner.load_current(token).await;
        });
        Ok(self.state().await)
    }
    /// Starts a continuous session from its first tracks. More are asked
    /// for whenever fewer than LOW_WATER remain after the current one.
    pub async fn play_station(
        self: &Arc<Self>,
        label: String,
        first: Vec<Track>,
        station: Arc<dyn Station>,
    ) -> Result<State, ErrorCode> {
        if first.is_empty() || first.len() > crate::queue::MAX_UPCOMING + 1 || label.is_empty() {
            return Err(ErrorCode::NotFound);
        }
        let token = {
            let mut d = self.data.lock().await;
            d.token = d.token.checked_add(1).ok_or(ErrorCode::Internal)?;
            d.session = uuid::Uuid::new_v4().to_string();
            d.tail.clear();
            d.queue
                .start(first.iter().map(|t| t.id.clone()).collect(), Some(label))
                .map_err(|_| ErrorCode::InvalidArgument)?;
            d.tracks = first.into_iter().map(|t| (t.id.clone(), t)).collect();
            d.station = Some(station);
            d.refill = "idle";
            d.status = "loading".into();
            d.loaded = false;
            d.position = 0;
            d.duration = 0;
            d.seek_end = None;
            d.error = None;
            d.revision += 1;
            d.token
        };
        let owner = self.clone();
        tokio::spawn(async move {
            owner.load_current(token).await;
        });
        self.top_up();
        Ok(self.state().await)
    }
    /// Asks a station for more tracks when the loaded window runs low. A
    /// reply for a stopped or replaced session is ignored; an ended session
    /// resumes with the first new track.
    fn top_up(self: &Arc<Self>) {
        let owner = self.clone();
        tokio::spawn(async move {
            let (station, request, after, session) = {
                let mut d = owner.data.lock().await;
                let Some(station) = d.station.clone() else {
                    return;
                };
                if d.queue.upcoming() >= LOW_WATER || d.queue.pending() {
                    return;
                }
                let Ok(request) = d.queue.begin_refill() else {
                    return;
                };
                d.refill = "loading";
                d.revision += 1;
                let after = d.queue.entries().back().map(|e| e.track.clone());
                (station, request, after, d.session.clone())
            };
            let result = station.next(after).await;
            let mut d = owner.data.lock().await;
            if d.session != session {
                return;
            }
            match result {
                Ok(tracks) => {
                    let ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
                    for track in tracks {
                        d.tracks.entry(track.id.clone()).or_insert(track);
                    }
                    d.refill = match d.queue.complete_refill(request, ids, false) {
                        Ok(()) => "idle",
                        Err(crate::queue::Error::Empty) => "exhausted",
                        Err(crate::queue::Error::Stale) => return,
                        Err(_) => "failed",
                    };
                }
                Err(_) => {
                    if d.queue.fail_refill(request).is_err() {
                        return;
                    }
                    d.refill = "failed";
                }
            }
            d.revision += 1;
            // The current track already ended while the batch was loading.
            if d.status == "ended" && d.seek_end.is_none() && d.queue.advance().is_ok() {
                d.token += 1;
                d.position = 0;
                d.duration = 0;
                d.status = "loading".into();
                let token = d.token;
                drop(d);
                owner.load_current(token).await;
                owner.top_up();
            }
        });
    }
    async fn current_track(&self) -> Result<String, ErrorCode> {
        let d = self.data.lock().await;
        d.queue
            .current()
            .map(|e| e.track.clone())
            .ok_or(ErrorCode::NotFound)
    }
    /// Sends best-effort feedback for a station session.
    fn report(station: Option<Arc<dyn Station>>, event: StationEvent) {
        if let Some(station) = station {
            tokio::spawn(async move {
                let _ = station.feedback(event).await;
            });
        }
    }
    async fn load_current(self: &Arc<Self>, token: u64) {
        let id = {
            let d = self.data.lock().await;
            if d.token != token {
                return;
            }
            let Some(entry) = d.queue.current() else {
                return;
            };
            entry.track.clone()
        };
        {
            let _io = self.io.lock().await;
            if self.data.lock().await.token != token {
                return;
            }
            if let Err(error) = self.player.stop().await {
                let mut d = self.data.lock().await;
                if d.token == token {
                    d.status = "stopped".into();
                    d.error = Some(error);
                    d.revision += 1;
                }
                return;
            }
        }
        let source = self.resolver.resolve(id.clone()).await;
        let _io = self.io.lock().await;
        if self.data.lock().await.token != token {
            return;
        }
        let result = match source {
            Ok(source) => self.player.load(source).await,
            Err(e) => Err(e),
        };
        if result.is_err() {
            let _ = self.player.stop().await;
        }
        let mut d = self.data.lock().await;
        if d.token != token {
            return;
        }
        match result {
            Ok(()) => {
                d.loaded = true;
            }
            Err(error) => {
                d.loaded = false;
                d.status = "stopped".into();
                d.error = Some(error);
            }
        }
        d.revision += 1;
    }
    pub async fn stop(&self) -> Result<State, ErrorCode> {
        let token = {
            let mut d = self.data.lock().await;
            d.token += 1;
            d.status = "loading".into();
            d.loaded = false;
            d.token
        };
        let _io = self.io.lock().await;
        // Without a player thread no audio process is left running, so the
        // session ends either way; the error is still reported, so a
        // replacement start waits for a confirmed stop.
        let stopped = self.player.stop().await;
        let mut d = self.data.lock().await;
        if d.token == token {
            d.queue.stop();
            d.tail.clear();
            d.tracks.clear();
            d.session.clear();
            d.station = None;
            d.refill = "idle";
            d.status = "stopped".into();
            d.position = 0;
            d.error = stopped.err();
            d.revision += 1;
        }
        drop(d);
        stopped?;
        Ok(self.state().await)
    }
    pub async fn command(self: &Arc<Self>, command: &Command) -> Result<State, ErrorCode> {
        let expected = {
            let d = self.data.lock().await;
            if d.session != command.session || d.session.is_empty() {
                return Err(ErrorCode::Conflict);
            }
            if command.action == "seek" && d.duration == 0 {
                return Err(ErrorCode::Unsupported);
            }
            d.token
        };
        match command.action.as_str() {
            "stop" => return self.stop().await,
            "like" => {
                let id = self.current_track().await?;
                self.resolver.rate(id, true).await?;
            }
            "retry" => {
                if self.data.lock().await.refill != "failed" {
                    return Err(ErrorCode::Unsupported);
                }
                self.top_up();
            }
            "next" | "previous" | "dislike" => {
                // A dislike is recorded first; it then moves on like Next.
                if command.action == "dislike" {
                    let id = self.current_track().await?;
                    self.resolver.rate(id, false).await?;
                }
                let forward = command.action != "previous";
                let token = {
                    let mut d = self.data.lock().await;
                    if forward && let Some(track) = d.queue.current().map(|e| e.track.clone()) {
                        Self::report(d.station.clone(), StationEvent::Skipped(track, d.position));
                    }
                    step(&mut d, forward)?;
                    d.token += 1;
                    d.position = 0;
                    d.duration = 0;
                    d.loaded = false;
                    d.seek_end = None;
                    d.status = "loading".into();
                    d.revision += 1;
                    d.token
                };
                let owner = self.clone();
                tokio::spawn(async move {
                    owner.load_current(token).await;
                });
                self.top_up();
            }
            "remove" => {
                let mut d = self.data.lock().await;
                let occurrence = command
                    .occurrence
                    .parse()
                    .map_err(|_| ErrorCode::InvalidArgument)?;
                d.queue
                    .remove(occurrence)
                    .map_err(|_| ErrorCode::Unsupported)?;
                refill(&mut d)?;
                d.revision += 1;
            }
            "pause" | "resume" | "seek" => {
                let _io = self.io.lock().await;
                let seek_end = {
                    let d = self.data.lock().await;
                    if d.token != expected {
                        return Err(ErrorCode::Conflict);
                    }
                    (command.action == "seek"
                        && d.duration > 0
                        && command.position_us >= d.duration)
                        .then_some(command.position_us)
                };
                let value = if command.action == "seek" {
                    json!([
                        "seek",
                        command.position_us as f64 / 1_000_000.0,
                        "absolute+exact"
                    ])
                } else {
                    json!(["set_property", "pause", command.action == "pause"])
                };
                self.player.command(value).await?;
                let snapshot = self.player.poll().await?;
                let mut d = self.data.lock().await;
                if d.token != expected {
                    return Err(ErrorCode::Conflict);
                }
                d.position = snapshot.position_us;
                if snapshot.failed {
                    d.error = Some(ErrorCode::Unavailable);
                    d.loaded = false;
                }
                if snapshot.duration_us > 0 {
                    d.duration = snapshot.duration_us;
                }
                d.status = if snapshot.failed {
                    "stopped"
                } else if snapshot.idle {
                    "ended"
                } else if !snapshot.ready {
                    "loading"
                } else if snapshot.paused {
                    "paused"
                } else {
                    "playing"
                }
                .into();
                if command.action == "seek" {
                    d.seek_end = seek_end;
                }
                d.revision += 1;
            }
            _ => return Err(ErrorCode::Unsupported),
        }
        Ok(self.state().await)
    }
    async fn poll_loop(owner: Weak<Self>) {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let Some(this) = owner.upgrade() else {
                return;
            };
            let token = {
                let d = this.data.lock().await;
                if !d.loaded {
                    continue;
                }
                d.token
            };
            let io = this.io.lock().await;
            let result = this.player.poll().await;
            let mut d = this.data.lock().await;
            if d.token != token {
                continue;
            }
            let mut advance = false;
            let mut acknowledge = None;
            match result {
                Ok(snapshot) => {
                    d.failures = 0;
                    if d.seek_end.is_some_and(|target| {
                        snapshot.position_us > target.saturating_add(1_000_000)
                    }) {
                        d.seek_end = None;
                    }
                    d.position = snapshot.position_us;
                    if snapshot.failed {
                        d.error = Some(ErrorCode::Unavailable);
                        d.loaded = false;
                    }
                    if snapshot.duration_us > 0 {
                        d.duration = snapshot.duration_us;
                    }
                    let status = if snapshot.failed {
                        "stopped"
                    } else if snapshot.idle {
                        "ended"
                    } else if !snapshot.ready {
                        "loading"
                    } else if snapshot.paused {
                        "paused"
                    } else {
                        "playing"
                    };
                    if d.status != status {
                        d.status = status.into();
                        d.revision += 1;
                    }
                    // Only the player's confirmation counts as a start.
                    let current = d.queue.current().map(|e| (e.occurrence, e.track.clone()));
                    if status == "playing"
                        && let Some((occurrence, track)) = current
                        && d.started != Some(occurrence)
                    {
                        d.started = Some(occurrence);
                        Self::report(d.station.clone(), StationEvent::Started(track));
                    }
                    if let Some(eof) = snapshot.eof {
                        acknowledge = Some(eof);
                        d.loaded = false;
                        if d.seek_end.is_none() {
                            let _ = refill(&mut d);
                            if let Some(track) = d.queue.current().map(|e| e.track.clone()) {
                                let played = d.duration.max(d.position);
                                Self::report(
                                    d.station.clone(),
                                    StationEvent::Finished(track, played),
                                );
                            }
                            if let Some(occurrence) = d.queue.current().map(|e| e.occurrence)
                                && d.queue.confirmed_end(occurrence).is_ok()
                            {
                                d.token += 1;
                                d.position = 0;
                                d.duration = 0;
                                d.status = "loading".into();
                                d.seek_end = None;
                                d.revision += 1;
                                advance = true;
                            }
                        }
                    }
                }
                // One slow or garbled reply is not a stop. After three failed
                // polls in a row the player is stopped, so the reported
                // "stopped" is true and no unobserved audio keeps playing.
                Err(error) => {
                    d.failures = d.failures.saturating_add(1);
                    if d.failures >= 3 {
                        d.failures = 0;
                        d.error = Some(error);
                        d.status = "stopped".into();
                        d.loaded = false;
                        d.revision += 1;
                        drop(d);
                        let _ = this.player.stop().await;
                        drop(io);
                        continue;
                    }
                }
            }
            let next = d.token;
            drop(d);
            if let Some(id) = acknowledge {
                let _ = this.player.acknowledge_eof(id).await;
            }
            drop(io);
            if advance {
                this.load_current(next).await;
            }
            if acknowledge.is_some() {
                this.top_up();
            }
        }
    }
}
/// Moves the finite queue one entry forward or back. The window stays within
/// its bounds: an entry pushed out by stepping back returns to the tail.
fn step(d: &mut Data, forward: bool) -> Result<(), ErrorCode> {
    refill(d)?;
    if forward {
        d.queue.advance().map_err(|_| ErrorCode::NotFound)?;
    } else if let Some(track) = d.queue.previous().map_err(|_| ErrorCode::NotFound)? {
        d.tail.push_front(track);
    }
    Ok(())
}
/// Tops the loaded window up from the finite tail. The tail is drained only
/// after the queue accepted the tracks, so a refused append loses nothing.
fn refill(d: &mut Data) -> Result<(), ErrorCode> {
    let count = MAX_UPCOMING
        .saturating_sub(d.queue.upcoming())
        .min(d.tail.len());
    if count == 0 {
        return Ok(());
    }
    let tracks: Vec<String> = d.tail.iter().take(count).cloned().collect();
    d.queue
        .append_finite(tracks)
        .map_err(|_| ErrorCode::Internal)?;
    d.tail.drain(..count);
    Ok(())
}
pub fn entity(t: &Track) -> Entity {
    let mut value = Entity {
        id: t.id.clone(),
        kind: "track".into(),
        title: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        artwork: String::new(),
        duration_us: t.duration_us,
        available: t.available,
    };
    // Reserve enough budget for all 97 retained queue occurrences, including
    // JSON escaping. Catalogue/private metadata keeps its original text.
    while serde_json::to_vec(&value).map_or(usize::MAX, |b| b.len()) > 2048 {
        let longest = [&mut value.title, &mut value.artist, &mut value.album]
            .into_iter()
            .max_by_key(|s| s.len())
            .expect("three text fields");
        if longest.is_empty() {
            break;
        }
        let end = longest
            .char_indices()
            .nth(longest.chars().count() / 2)
            .map(|(i, _)| i)
            .unwrap_or(0);
        longest.truncate(end);
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escaped_queue_metadata_fits_inside_active_state_envelope() {
        let track = Track {
            id: "x".repeat(256),
            title: "\\\"".repeat(2048),
            artist: "\\\"".repeat(2048),
            album: "\\\"".repeat(2048),
            available: true,
            ..Track::default()
        };
        let entity = entity(&track);
        assert!(serde_json::to_vec(&entity).unwrap().len() <= 2048);
        let entry = |i| QueueEntry {
            occurrence: format!("occurrence-{i}"),
            track: entity.clone(),
        };
        let state = State {
            session: "session".into(),
            current: Some(entry(32)),
            history: (0..32).map(entry).collect(),
            upcoming: (33..97).map(entry).collect(),
            ..State::default()
        };
        state.validate().unwrap();
        let envelope = music::Active {
            provider_id: "x".repeat(256),
            generation: "g".repeat(256),
            state,
        };
        music::encode(&Some(envelope)).unwrap();
    }
    /// Review of #710: stepping back on a full 64-entry window left 65
    /// upcoming entries (an invalid state), and the next refill underflowed
    /// `64 - 65`, panicking in debug builds and dropping the tail in release.
    #[test]
    fn stepping_back_on_a_full_window_keeps_bounds_and_every_track() {
        let ids: Vec<String> = (0..100).map(|i| format!("t{i}")).collect();
        let mut d = Data {
            queue: Queue::default(),
            tracks: BTreeMap::new(),
            tail: ids.iter().skip(65).cloned().collect(),
            session: String::new(),
            token: 0,
            revision: 1,
            position: 0,
            duration: 0,
            status: "stopped".into(),
            error: None,
            loaded: false,
            seek_end: None,
            failures: 0,
            station: None,
            refill: "idle",
            started: None,
        };
        d.queue
            .start(ids.iter().take(65).cloned().collect(), None)
            .unwrap();
        let remaining = |d: &Data| d.queue.upcoming() + d.tail.len();
        assert_eq!(remaining(&d), 99);
        for forward in [true, true, false, false, true, false] {
            step(&mut d, forward).unwrap();
            assert!(d.queue.upcoming() <= MAX_UPCOMING);
        }
        assert_eq!(remaining(&d), 99, "no track was lost");
        // Every track is still reached once, in order, without an error.
        let mut played = Vec::new();
        while step(&mut d, true).is_ok() {
            assert!(d.queue.upcoming() <= MAX_UPCOMING);
            played.push(d.queue.current().unwrap().track.clone());
        }
        assert_eq!(played, ids[1..].to_vec());
    }
    /// A player fixture where every loaded file ends by itself after two
    /// position polls, as a short track would.
    fn ending_player(directory: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let executable = directory.join("fixture.py");
        std::fs::write(&executable, r##"#!/usr/bin/python3
import json,socket,sys
path=next(x.split('=',1)[1] for x in sys.argv if x.startswith('--input-ipc-server='))
server=socket.socket(socket.AF_UNIX);server.bind(path);server.listen(1)
connection,_=server.accept();stream=connection.makefile('rwb',buffering=0)
idle=False;polls=0
for line in stream:
 request=json.loads(line);command=request['command'];data=None;error='success'
 if command[0]=='loadfile':
  idle=False;polls=0
  stream.write(b'{"event":"start-file","playlist_entry_id":7}\n')
  stream.write(b'{"event":"file-loaded"}\n')
 elif command[0]=='get_property':
  prop=command[1]
  if prop=='playlist/0/id':data=7
  elif prop=='idle-active':data=idle
  elif prop=='pause':data=False
  elif prop=='time-pos':
   polls+=1;data=polls
   if polls==2 and not idle:
    idle=True;stream.write(b'{"event":"end-file","playlist_entry_id":7,"reason":"eof"}\n')
  elif prop=='duration':data=3
 stream.write((json.dumps({'request_id':request['request_id'],'data':data,'error':error})+'\n').encode())
"##).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        executable
    }
    fn track(id: &str) -> Track {
        Track {
            id: id.into(),
            title: id.into(),
            available: true,
            ..Track::default()
        }
    }
    #[derive(Default)]
    struct FixtureStation {
        events: std::sync::Mutex<Vec<StationEvent>>,
        batches: std::sync::Mutex<VecDeque<Vec<Track>>>,
        asked: std::sync::Mutex<Vec<Option<String>>>,
    }
    impl Station for FixtureStation {
        fn next(&self, after: Option<String>) -> Reply<'_, Vec<Track>> {
            self.asked.lock().unwrap().push(after);
            let batch = self.batches.lock().unwrap().pop_front().unwrap_or_default();
            Box::pin(async move { Ok(batch) })
        }
        fn feedback(&self, event: StationEvent) -> Reply<'_, ()> {
            self.events.lock().unwrap().push(event);
            Box::pin(async { Ok(()) })
        }
    }
    async fn until(playback: &Arc<Playback>, check: impl Fn(&State) -> bool) -> State {
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                let state = playback.state().await;
                if check(&state) {
                    break state;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("state reached")
    }
    /// A station session refills before its window runs out, keeps playing
    /// across batches, reports confirmed starts, natural ends and skips, and
    /// ends cleanly once the station has nothing more.
    #[tokio::test]
    async fn a_station_refills_continues_and_reports_feedback() {
        use std::os::unix::fs::DirBuilderExt;
        let directory = std::env::temp_dir().join(format!(
            "am-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let playback = Playback::new(
            Player::new(&directory, ending_player(&directory)).unwrap(),
            Arc::new(FixtureResolver),
        );
        let station = Arc::new(FixtureStation::default());
        station
            .batches
            .lock()
            .unwrap()
            .extend([vec![track("s2"), track("s3")], vec![track("s4")]]);
        let state = playback
            .play_station("wave".into(), vec![track("s1")], station.clone())
            .await
            .unwrap();
        assert!(state.continuous);
        // It plays through both batches by itself, then ends.
        let ended = until(&playback, |s| {
            s.playback == "ended" && s.current.as_ref().is_some_and(|c| c.track.id == "s4")
        })
        .await;
        assert!(ended.continuous);
        let state = until(&playback, |s| s.refill == "exhausted").await;
        assert!(!state.actions.contains(&"remove".to_owned()));
        let events = station.events.lock().unwrap().clone();
        for id in ["s1", "s2", "s3", "s4"] {
            assert!(
                events.contains(&StationEvent::Started(id.into())),
                "{id}: {events:?}"
            );
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, StationEvent::Finished(t, _) if t == id)),
                "{id}: {events:?}"
            );
        }
        let asked = station.asked.lock().unwrap().clone();
        assert_eq!(asked.first(), Some(&Some("s1".into())));
        playback.stop().await.unwrap();
        assert!(!playback.state().await.continuous);
        drop(playback);
        std::fs::remove_dir_all(directory).unwrap();
    }
    struct RatingResolver(std::sync::Mutex<Vec<(String, bool)>>);
    impl Resolver for RatingResolver {
        fn resolve(&self, _id: String) -> Reply<'_, Source> {
            Box::pin(async { Ok(Source::File(std::fs::File::open("/dev/null").unwrap())) })
        }
        fn rates(&self) -> bool {
            true
        }
        fn rate(&self, id: String, like: bool) -> Reply<'_, ()> {
            self.0.lock().unwrap().push((id, like));
            Box::pin(async { Ok(()) })
        }
    }
    /// Like records the current track; dislike records it and moves on,
    /// reporting a skip to the station.
    #[tokio::test]
    async fn like_and_dislike_reach_the_service_and_dislike_moves_on() {
        use std::os::unix::fs::DirBuilderExt;
        let directory = std::env::temp_dir().join(format!(
            "am-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let resolver = Arc::new(RatingResolver(Default::default()));
        let playback = Playback::new(
            Player::new(&directory, directory.join("missing-player")).unwrap(),
            resolver.clone(),
        );
        let station = Arc::new(FixtureStation::default());
        let state = playback
            .play_station(
                "wave".into(),
                vec![track("a"), track("b"), track("c"), track("d")],
                station.clone(),
            )
            .await
            .unwrap();
        assert!(state.actions.contains(&"like".to_owned()));
        let command = |action: &str| Command {
            request_id: uuid::Uuid::new_v4().to_string(),
            action: action.into(),
            session: state.session.clone(),
            entity: String::new(),
            occurrence: String::new(),
            position_us: 0,
            replaces_provider: String::new(),
            replaces_session: String::new(),
        };
        playback.command(&command("like")).await.unwrap();
        let after = playback.command(&command("dislike")).await.unwrap();
        assert_eq!(after.current.unwrap().track.id, "b");
        assert_eq!(
            *resolver.0.lock().unwrap(),
            [("a".to_owned(), true), ("a".to_owned(), false)]
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            station
                .events
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, StationEvent::Skipped(t, _) if t == "a"))
        );
        drop(playback);
        std::fs::remove_dir_all(directory).unwrap();
    }
    struct FixtureResolver;
    impl Resolver for FixtureResolver {
        fn resolve(
            &self,
            _id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
            Box::pin(async { Ok(Source::File(std::fs::File::open("/dev/null").unwrap())) })
        }
    }
    /// Review of #710: one poll that missed its 2-second deadline reported
    /// "stopped" while the audio kept playing, and polling stopped for good.
    #[tokio::test]
    async fn a_single_slow_poll_is_not_a_stop() {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let directory = std::env::temp_dir().join(format!(
            "am-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let executable = directory.join("fixture.py");
        std::fs::write(&executable,r##"#!/usr/bin/python3
import json,socket,sys,time
path=next(x.split('=',1)[1] for x in sys.argv if x.startswith('--input-ipc-server='))
server=socket.socket(socket.AF_UNIX);server.bind(path);server.listen(1)
connection,_=server.accept();stream=connection.makefile('rwb',buffering=0)
positions=0
for line in stream:
 request=json.loads(line);command=request['command'];data=None;error='success'
 if command[0]=='loadfile':
  stream.write((json.dumps({'event':'start-file','playlist_entry_id':7})+'\n').encode())
  stream.write(b'{"event":"file-loaded"}\n')
 elif command[0]=='get_property':
  prop=command[1]
  if prop=='playlist/0/id':data=7
  elif prop in ('idle-active','pause'):data=False
  elif prop=='time-pos':
   positions+=1;data=positions
   if positions==3:time.sleep(2.5)
  elif prop=='duration':data=600
 stream.write((json.dumps({'request_id':request['request_id'],'data':data,'error':error})+'\n').encode())
"##).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let playback = Playback::new(
            Player::new(&directory, executable).unwrap(),
            Arc::new(FixtureResolver),
        );
        playback
            .play(vec![Track {
                id: "one".into(),
                available: true,
                ..Track::default()
            }])
            .await
            .unwrap();
        // The slow reply comes on the third poll; wait well past it.
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;
        let state = playback.state().await;
        assert_eq!(state.playback, "playing");
        assert_eq!(state.error, None);
        assert!(state.position_us >= 4_000_000, "polling continued");
        playback.stop().await.unwrap();
        drop(playback);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[tokio::test]
    async fn seeking_to_end_with_immediate_idle_eof_does_not_advance() {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let directory = std::env::temp_dir().join(format!("am-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let executable = directory.join("fixture.py");
        std::fs::write(&executable,r##"#!/usr/bin/python3
import json,socket,sys
path=next(x.split('=',1)[1] for x in sys.argv if x.startswith('--input-ipc-server='))
server=socket.socket(socket.AF_UNIX);server.bind(path);server.listen(1)
connection,_=server.accept();stream=connection.makefile('rwb',buffering=0)
idle=False
for line in stream:
 request=json.loads(line);command=request['command'];data=None;error='success'
 if command[0]=='loadfile':
  stream.write((json.dumps({'event':'start-file','playlist_entry_id':7})+'\n').encode())
  stream.write(b'{"event":"file-loaded"}\n')
 elif command[0]=='seek':
  idle=True
  stream.write(b'{"event":"end-file","playlist_entry_id":7,"reason":"eof"}\n')
 elif command[0]=='get_property':
  prop=command[1]
  if prop=='playlist/0/id':data=7
  elif prop=='idle-active':data=idle
  elif prop=='pause':data=False
  elif prop=='time-pos':data=0
  elif prop=='duration':data=10
 stream.write((json.dumps({'request_id':request['request_id'],'data':data,'error':error})+'\n').encode())
"##).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let playback = Playback::new(
            Player::new(&directory, executable).unwrap(),
            Arc::new(FixtureResolver),
        );
        playback
            .play(vec![
                Track {
                    id: "one".into(),
                    available: true,
                    ..Track::default()
                },
                Track {
                    id: "two".into(),
                    available: true,
                    ..Track::default()
                },
            ])
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            loop {
                if playback.state().await.playback == "playing" {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let session = playback.state().await.session;
        let result = playback
            .command(&Command {
                request_id: uuid::Uuid::new_v4().to_string(),
                action: "seek".into(),
                session,
                entity: String::new(),
                occurrence: String::new(),
                position_us: 10_000_000,
                replaces_provider: String::new(),
                replaces_session: String::new(),
            })
            .await
            .unwrap();
        assert_eq!(result.playback, "ended");
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                if !playback.data.lock().await.loaded {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(playback.state().await.current.unwrap().track.id, "one");
        playback.stop().await.unwrap();
        drop(playback);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
