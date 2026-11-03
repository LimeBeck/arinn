// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{
    Track,
    player::{Player, Source},
    queue::Queue,
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
pub trait Resolver: Send + Sync + 'static {
    fn resolve(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Started,
    Finished,
    Skipped,
}
#[derive(Clone)]
pub struct PlaybackEvent {
    pub session: String,
    pub batch: String,
    pub track: String,
    pub occurrence: u64,
    pub played_us: u64,
    pub kind: EventKind,
}
pub struct RadioRefill {
    session: String,
    request: crate::queue::Request,
    replace: bool,
}
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
    seek_end: Option<u64>,
    failures: u8,
    station: Option<Entity>,
    origins: BTreeMap<u64, String>,
    refill: String,
    started: Option<u64>,
    waiting_end: bool,
    played_us: u64,
    clock: tokio::time::Instant,
}
pub struct Playback {
    data: Mutex<Data>,
    io: Mutex<()>,
    player: Player,
    resolver: Arc<dyn Resolver>,
    events: tokio::sync::broadcast::Sender<PlaybackEvent>,
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
                origins: BTreeMap::new(),
                refill: "idle".into(),
                started: None,
                waiting_end: false,
                played_us: 0,
                clock: tokio::time::Instant::now(),
            }),
            io: Mutex::new(()),
            player,
            resolver,
            events: tokio::sync::broadcast::channel(128).0,
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
        if cursor.is_some() || d.station.is_some() {
            actions.push("stop".into());
        }
        if cursor.is_some() {
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
        }
        if d.station.is_some() && ["failed", "exhausted"].contains(&d.refill.as_str()) {
            actions.push("retry".into());
        }
        State {
            continuous: d.station.is_some(),
            station: d.station.clone(),
            refill: d.refill.clone(),
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
            error: d.error,
            ..State::default()
        }
    }
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<PlaybackEvent> {
        self.events.subscribe()
    }
    pub async fn play(self: &Arc<Self>, tracks: Vec<Track>) -> Result<State, ErrorCode> {
        self.start(tracks, None, None).await
    }
    pub async fn radio(
        self: &Arc<Self>,
        tracks: Vec<Track>,
        batch: String,
        station: Entity,
    ) -> Result<State, ErrorCode> {
        if batch.is_empty() || batch.len() > 512 || station.kind != "station" {
            return Err(ErrorCode::InvalidArgument);
        }
        station.validate()?;
        self.start(tracks, Some(batch), Some(station)).await
    }
    async fn start(
        self: &Arc<Self>,
        tracks: Vec<Track>,
        batch: Option<String>,
        station: Option<Entity>,
    ) -> Result<State, ErrorCode> {
        if (tracks.is_empty() && station.is_none()) || tracks.len() > 100_000 {
            return Err(ErrorCode::NotFound);
        }
        let token = {
            let mut d = self.data.lock().await;
            d.token = d.token.checked_add(1).ok_or(ErrorCode::Internal)?;
            d.session = uuid::Uuid::new_v4().to_string();
            d.tail = if station.is_none() {
                tracks.iter().skip(65).map(|t| t.id.clone()).collect()
            } else {
                VecDeque::new()
            };
            d.queue
                .start(
                    tracks.iter().take(65).map(|t| t.id.clone()).collect(),
                    station.as_ref().map(|s| s.id.clone()),
                )
                .map_err(|_| ErrorCode::InvalidArgument)?;
            d.station = station;
            d.origins = d
                .queue
                .entries()
                .iter()
                .map(|e| (e.occurrence, batch.clone().unwrap_or_default()))
                .collect();
            d.refill = "idle".into();
            d.started = None;
            d.played_us = 0;
            d.clock = tokio::time::Instant::now();
            d.waiting_end = d.queue.current().is_none();
            d.tracks = tracks.into_iter().map(|t| (t.id.clone(), t)).collect();
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
    pub async fn remaining(&self, session: &str) -> Result<Vec<Track>, ErrorCode> {
        let d = self.data.lock().await;
        if d.session != session || d.session.is_empty() || d.station.is_some() {
            return Err(ErrorCode::Conflict);
        }
        let cursor = d.queue.current().map(|e| e.occurrence);
        let index = d
            .queue
            .entries()
            .iter()
            .position(|e| Some(e.occurrence) == cursor)
            .unwrap_or(0);
        let ids = d
            .queue
            .entries()
            .iter()
            .skip(index)
            .map(|e| &e.track)
            .chain(d.tail.iter());
        Ok(ids.filter_map(|id| d.tracks.get(id).cloned()).collect())
    }
    pub async fn batch_ids(&self) -> std::collections::HashSet<String> {
        self.data.lock().await.origins.values().cloned().collect()
    }
    pub async fn begin_radio_refill(&self, replace: bool) -> Result<RadioRefill, ErrorCode> {
        let mut d = self.data.lock().await;
        let request = if replace {
            d.queue.begin_replacement()
        } else {
            d.queue.begin_refill()
        }
        .map_err(|_| ErrorCode::Conflict)?;
        d.refill = "loading".into();
        d.revision += 1;
        Ok(RadioRefill {
            session: d.session.clone(),
            request,
            replace,
        })
    }
    pub async fn fail_radio_refill(&self, refill: RadioRefill, error: ErrorCode) {
        let mut d = self.data.lock().await;
        if d.session == refill.session && d.queue.fail_refill(refill.request).is_ok() {
            d.refill = "failed".into();
            d.error = Some(error);
            d.revision += 1;
        }
    }
    pub async fn apply_radio_refill(
        self: &Arc<Self>,
        refill: RadioRefill,
        tracks: Vec<Track>,
        batch: String,
    ) -> Result<(), ErrorCode> {
        let mut d = self.data.lock().await;
        if d.session != refill.session {
            return Err(ErrorCode::Conflict);
        }
        let before: std::collections::HashSet<_> =
            d.queue.entries().iter().map(|e| e.occurrence).collect();
        let result = d.queue.complete_refill(
            refill.request,
            tracks.iter().take(64).map(|t| t.id.clone()).collect(),
            refill.replace,
        );
        if let Err(error) = result {
            if error == crate::queue::Error::Stale {
                return Err(ErrorCode::Conflict);
            }
            d.refill = if error == crate::queue::Error::Empty {
                "exhausted"
            } else {
                "failed"
            }
            .into();
            d.revision += 1;
            return Err(if error == crate::queue::Error::Stale {
                ErrorCode::Conflict
            } else {
                ErrorCode::Unavailable
            });
        }
        for track in tracks {
            d.tracks.insert(track.id.clone(), track);
        }
        let added: Vec<_> = d
            .queue
            .entries()
            .iter()
            .filter(|e| !before.contains(&e.occurrence))
            .map(|e| e.occurrence)
            .collect();
        for occurrence in added {
            d.origins.insert(occurrence, batch.clone());
        }
        prune_radio(&mut d);
        d.refill = "idle".into();
        d.error = None;
        d.revision += 1;
        let resume = d.waiting_end && (d.queue.upcoming() > 0 || before.is_empty());
        let token = if resume {
            if !before.is_empty() {
                d.queue.advance().map_err(|_| ErrorCode::Conflict)?;
            }
            d.token += 1;
            d.loaded = false;
            d.started = None;
            d.played_us = 0;
            d.clock = tokio::time::Instant::now();
            d.waiting_end = false;
            d.seek_end = None;
            d.position = 0;
            d.duration = 0;
            d.status = "loading".into();
            Some(d.token)
        } else {
            None
        };
        drop(d);
        if let Some(token) = token {
            let owner = self.clone();
            tokio::spawn(async move {
                owner.load_current(token).await;
            });
        }
        Ok(())
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
        let source = self.resolver.resolve(id).await;
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
        let stopped = self.player.stop().await;
        let mut d = self.data.lock().await;
        if d.token == token {
            d.queue.stop();
            d.station = None;
            d.origins.clear();
            d.refill = "idle".into();
            d.started = None;
            d.played_us = 0;
            d.clock = tokio::time::Instant::now();
            d.waiting_end = false;
            d.tail.clear();
            d.tracks.clear();
            d.session.clear();
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
            "next" | "previous" => {
                let token = {
                    let mut d = self.data.lock().await;
                    refill(&mut d)?;
                    stamp(&mut d);
                    let skipped = event(&d, EventKind::Skipped);
                    if command.action == "next" {
                        d.queue.advance().map_err(|_| ErrorCode::NotFound)?;
                    } else if let Some(track) =
                        d.queue.previous().map_err(|_| ErrorCode::NotFound)?
                    {
                        if d.station.is_none() {
                            d.tail.push_front(track);
                        }
                    }
                    if let Some(event) = skipped {
                        let _ = self.events.send(event);
                    }
                    d.token += 1;
                    d.started = None;
                    d.played_us = 0;
                    d.clock = tokio::time::Instant::now();
                    d.waiting_end = false;
                    prune_radio(&mut d);
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
                stamp(&mut d);
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
                    stamp(&mut d);
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
                    if snapshot.ready
                        && !snapshot.failed
                        && (!snapshot.idle || snapshot.eof.is_some())
                        && d.started != d.queue.current().map(|e| e.occurrence)
                    {
                        d.started = d.queue.current().map(|e| e.occurrence);
                        if let Some(event) = event(&d, EventKind::Started) {
                            let _ = this.events.send(event);
                        }
                    }
                    if let Some(eof) = snapshot.eof {
                        acknowledge = Some(eof);
                        d.loaded = false;
                        if d.seek_end.is_none() {
                            if let Some(event) = event(&d, EventKind::Finished) {
                                let _ = this.events.send(event);
                            }
                            d.waiting_end = d.station.is_some();
                            let _ = refill(&mut d);
                            if let Some(occurrence) = d.queue.current().map(|e| e.occurrence)
                                && d.queue.confirmed_end(occurrence).is_ok()
                            {
                                d.token += 1;
                                d.started = None;
                                d.played_us = 0;
                                d.clock = tokio::time::Instant::now();
                                d.waiting_end = false;
                                prune_radio(&mut d);
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
        }
    }
}
fn stamp(d: &mut Data) {
    let now = tokio::time::Instant::now();
    if d.started.is_some() && d.status == "playing" {
        d.played_us = d.played_us.saturating_add(
            now.duration_since(d.clock)
                .as_micros()
                .min(u64::MAX as u128) as u64,
        );
    }
    d.clock = now;
}

fn event(d: &Data, kind: EventKind) -> Option<PlaybackEvent> {
    let current = d.queue.current()?;
    if d.station.is_none() || d.started != Some(current.occurrence) {
        return None;
    }
    Some(PlaybackEvent {
        session: d.session.clone(),
        batch: d.origins.get(&current.occurrence)?.clone(),
        track: current.track.clone(),
        occurrence: current.occurrence,
        played_us: d.played_us,
        kind,
    })
}
fn prune_radio(d: &mut Data) {
    if d.station.is_some() {
        let occurrences: std::collections::HashSet<_> =
            d.queue.entries().iter().map(|e| e.occurrence).collect();
        let tracks: std::collections::HashSet<_> =
            d.queue.entries().iter().map(|e| e.track.clone()).collect();
        d.origins.retain(|id, _| occurrences.contains(id));
        d.tracks.retain(|id, _| tracks.contains(id));
    }
}

fn refill(d: &mut Data) -> Result<(), ErrorCode> {
    if d.station.is_some() {
        return Ok(());
    }
    let count = (64 - d.queue.upcoming()).min(d.tail.len());
    let tracks = d.tail.drain(..count).collect();
    d.queue
        .append_finite(tracks)
        .map_err(|_| ErrorCode::Internal)
}
pub fn entity(t: &Track) -> Entity {
    let mut value = Entity {
        id: t.id.clone(),
        kind: "track".into(),
        title: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        artwork: if t.id.len() <= 250 {
            format!("cover:{}", t.id)
        } else {
            String::new()
        },
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
    struct FixtureResolver;
    impl Resolver for FixtureResolver {
        fn resolve(
            &self,
            _id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
            Box::pin(async { Ok(Source::File(std::fs::File::open("/dev/null").unwrap())) })
        }
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
                text: String::new(),
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
    #[tokio::test]
    async fn short_natural_completion_emits_start_then_finish_before_first_poll() {
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
  idle=True
  stream.write(b'{"event":"end-file","playlist_entry_id":7,"reason":"eof"}\n')
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
        let mut events = playback.subscribe();
        playback
            .radio(
                vec![Track {
                    id: "one".into(),
                    available: true,
                    ..Track::default()
                }],
                "batch".into(),
                Entity {
                    id: "station".into(),
                    kind: "station".into(),
                    title: "Fixture station".into(),
                    ..Entity::default()
                },
            )
            .await
            .unwrap();
        let started = tokio::time::timeout(std::time::Duration::from_secs(8), events.recv())
            .await
            .unwrap()
            .unwrap();
        let finished = tokio::time::timeout(std::time::Duration::from_secs(8), events.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(started.kind, EventKind::Started);
        assert_eq!(finished.kind, EventKind::Finished);
        assert_eq!(started.occurrence, finished.occurrence);
        assert_eq!(started.track, "one");
        playback.stop().await.unwrap();
        drop(playback);
        std::fs::remove_dir_all(directory).unwrap();
    }
    struct ClosedResolver;
    impl Resolver for ClosedResolver {
        fn resolve(
            &self,
            _id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
            Box::pin(async { Err(ErrorCode::Unavailable) })
        }
    }
    struct RadioFixture {
        directory: std::path::PathBuf,
        playback: Arc<Playback>,
    }
    impl RadioFixture {
        fn new() -> Self {
            use std::os::unix::fs::DirBuilderExt;
            let directory =
                std::env::temp_dir().join(format!("arinn-radio-{}", uuid::Uuid::new_v4()));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&directory)
                .unwrap();
            let playback = Playback::new(
                Player::new(&directory, "/nonexistent-arinn-test-player".into()).unwrap(),
                Arc::new(ClosedResolver),
            );
            Self {
                directory,
                playback,
            }
        }
        async fn start(&self, ids: &[&str]) -> State {
            self.playback
                .radio(
                    ids.iter()
                        .map(|id| Track {
                            id: (*id).into(),
                            title: (*id).into(),
                            available: true,
                            ..Track::default()
                        })
                        .collect(),
                    "batch-A".into(),
                    Entity {
                        id: "station".into(),
                        kind: "station".into(),
                        title: "Fixture radio".into(),
                        ..Entity::default()
                    },
                )
                .await
                .unwrap()
        }
    }
    impl Drop for RadioFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
    fn radio_command(action: &str, session: &str) -> Command {
        Command {
            request_id: uuid::Uuid::new_v4().to_string(),
            action: action.into(),
            session: session.into(),
            entity: String::new(),
            occurrence: String::new(),
            position_us: 0,
            replaces_provider: String::new(),
            replaces_session: String::new(),
            text: String::new(),
        }
    }
    #[tokio::test]
    async fn stale_append_cannot_mark_the_new_replacement_failed_or_restore_after_stop() {
        let fixture = RadioFixture::new();
        fixture.start(&["A", "B"]).await;
        let old = fixture.playback.begin_radio_refill(false).await.unwrap();
        let replacement = fixture.playback.begin_radio_refill(true).await.unwrap();
        assert_eq!(
            fixture
                .playback
                .apply_radio_refill(
                    old,
                    vec![Track {
                        id: "old".into(),
                        ..Track::default()
                    }],
                    "old".into()
                )
                .await,
            Err(ErrorCode::Conflict)
        );
        assert_eq!(fixture.playback.state().await.refill, "loading");
        fixture
            .playback
            .apply_radio_refill(
                replacement,
                vec![Track {
                    id: "C".into(),
                    ..Track::default()
                }],
                "batch-C".into(),
            )
            .await
            .unwrap();
        let state = fixture.playback.state().await;
        assert_eq!(state.current.unwrap().track.id, "A");
        assert_eq!(state.upcoming[0].track.id, "C");
        let pending = fixture.playback.begin_radio_refill(false).await.unwrap();
        fixture.playback.stop().await.unwrap();
        assert_eq!(
            fixture
                .playback
                .apply_radio_refill(
                    pending,
                    vec![Track {
                        id: "late".into(),
                        ..Track::default()
                    }],
                    "late".into()
                )
                .await,
            Err(ErrorCode::Conflict)
        );
        assert!(!fixture.playback.state().await.continuous);
        assert!(fixture.playback.state().await.current.is_none());
    }
    #[tokio::test]
    async fn empty_station_can_receive_its_first_window() {
        let fixture = RadioFixture::new();
        fixture.start(&[]).await;
        assert!(
            fixture
                .playback
                .state()
                .await
                .actions
                .contains(&"stop".into())
        );
        let refill = fixture.playback.begin_radio_refill(false).await.unwrap();
        fixture
            .playback
            .apply_radio_refill(
                refill,
                vec![Track {
                    id: "first".into(),
                    available: true,
                    ..Track::default()
                }],
                "first-batch".into(),
            )
            .await
            .unwrap();
        let state = fixture.playback.state().await;
        assert!(state.continuous);
        assert_eq!(state.current.unwrap().track.id, "first");
    }
    #[tokio::test]
    async fn failed_navigation_emits_no_skip_but_confirmed_transition_uses_original_batch() {
        let fixture = RadioFixture::new();
        let state = fixture.start(&["A"]).await;
        {
            let mut d = fixture.playback.data.lock().await;
            d.token += 1;
            d.loaded = false;
            d.status = "paused".into();
            d.started = d.queue.current().map(|e| e.occurrence);
            d.played_us = 750_000;
            d.position = 90_000_000;
        }
        let mut events = fixture.playback.subscribe();
        assert!(
            fixture
                .playback
                .command(&radio_command("previous", &state.session))
                .await
                .is_err()
        );
        assert!(
            fixture
                .playback
                .command(&radio_command("next", &state.session))
                .await
                .is_err()
        );
        assert!(matches!(
            events.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
        let refill = fixture.playback.begin_radio_refill(false).await.unwrap();
        fixture
            .playback
            .apply_radio_refill(
                refill,
                vec![Track {
                    id: "B".into(),
                    ..Track::default()
                }],
                "batch-B".into(),
            )
            .await
            .unwrap();
        fixture
            .playback
            .command(&radio_command("next", &state.session))
            .await
            .unwrap();
        let event = events.try_recv().unwrap();
        assert_eq!(event.kind, EventKind::Skipped);
        assert_eq!(event.batch, "batch-A");
        assert_eq!(event.track, "A");
        assert_eq!(event.played_us, 750_000);
    }
    #[tokio::test]
    async fn repeated_feedback_replacement_keeps_the_current_batch_and_track_identity() {
        let fixture = RadioFixture::new();
        fixture.start(&["A", "B"]).await;
        let before = fixture.playback.data.lock().await.token;
        for index in 0..300 {
            let replacement = fixture.playback.begin_radio_refill(true).await.unwrap();
            fixture
                .playback
                .apply_radio_refill(
                    replacement,
                    vec![Track {
                        id: format!("new-{index}"),
                        ..Track::default()
                    }],
                    format!("batch-{index}"),
                )
                .await
                .unwrap();
        }
        let d = fixture.playback.data.lock().await;
        let current = d.queue.current().unwrap();
        assert_eq!(current.track, "A");
        assert_eq!(d.origins[&current.occurrence], "batch-A");
        assert_eq!(d.token, before);
        assert!(d.tracks.len() <= 97);
        assert!(d.origins.len() <= 97);
    }
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
}
