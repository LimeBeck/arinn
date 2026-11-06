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
    fn prefetch_capacity(&self) -> usize {
        1
    }
    fn prefetch(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
        self.resolve(id)
    }
}
struct Prepared {
    session: String,
    occurrence: u64,
    created: tokio::time::Instant,
    work: Option<tokio::task::JoinHandle<Result<Source, ErrorCode>>>,
    finished: tokio::sync::watch::Receiver<bool>,
}
impl Drop for Prepared {
    fn drop(&mut self) {
        if let Some(work) = self.work.take() {
            work.abort();
        }
    }
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
    prepared: Vec<Prepared>,
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
    refill_error: Option<ErrorCode>,
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
    load_changes: tokio::sync::watch::Sender<u64>,
    prefetch_limit: Arc<tokio::sync::Semaphore>,
    events: tokio::sync::broadcast::Sender<PlaybackEvent>,
}
impl Playback {
    pub fn new(player: Player, resolver: Arc<dyn Resolver>) -> Arc<Self> {
        let this = Arc::new(Self {
            data: Mutex::new(Data {
                prepared: Vec::new(),
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
                refill_error: None,
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
            load_changes: tokio::sync::watch::channel(0).0,
            prefetch_limit: Arc::new(tokio::sync::Semaphore::new(1)),
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
            error: d.error.or(d.refill_error),
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
            self.load_changes.send_replace(d.token);
            d.prepared.clear();
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
            d.refill_error = None;
            d.revision += 1;
            d.token
        };
        let owner = self.clone();
        tokio::spawn(async move {
            owner.load_current(token).await;
        });
        Ok(self.state().await)
    }
    /// Replaces queued tracks' metadata with `fresh` values of the same ID,
    /// as after a rescan of retagged files (review of #815). Returns whether
    /// anything changed.
    pub async fn refresh_tracks(&self, fresh: &std::collections::BTreeMap<String, Track>) -> bool {
        let mut d = self.data.lock().await;
        let mut changed = false;
        for (id, track) in d.tracks.iter_mut() {
            if let Some(new) = fresh.get(id)
                && new != track
            {
                *track = new.clone();
                changed = true;
            }
        }
        if changed {
            d.revision += 1;
        }
        changed
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
            d.refill_error = Some(error);
            d.revision += 1;
        }
    }
    pub async fn apply_radio_refill(
        self: &Arc<Self>,
        refill: RadioRefill,
        tracks: Vec<Track>,
        batch: String,
    ) -> Result<(), ErrorCode> {
        self.apply_refill(refill, tracks, batch, true).await
    }
    /// A refill whose tracks may repeat those queued or recently played: a
    /// provider's own station over a small library (see
    /// `Queue::complete_refill_repeating`).
    pub async fn apply_radio_refill_repeating(
        self: &Arc<Self>,
        refill: RadioRefill,
        tracks: Vec<Track>,
        batch: String,
    ) -> Result<(), ErrorCode> {
        self.apply_refill(refill, tracks, batch, false).await
    }
    async fn apply_refill(
        self: &Arc<Self>,
        refill: RadioRefill,
        tracks: Vec<Track>,
        batch: String,
        dedupe: bool,
    ) -> Result<(), ErrorCode> {
        let mut d = self.data.lock().await;
        if d.session != refill.session {
            return Err(ErrorCode::Conflict);
        }
        let before: std::collections::HashSet<_> =
            d.queue.entries().iter().map(|e| e.occurrence).collect();
        let ids = tracks.iter().take(64).map(|t| t.id.clone()).collect();
        let result = if dedupe {
            d.queue.complete_refill(refill.request, ids, refill.replace)
        } else {
            d.queue
                .complete_refill_repeating(refill.request, ids, refill.replace)
        };
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
        d.refill_error = None;
        d.revision += 1;
        let resume = d.waiting_end && (d.queue.upcoming() > 0 || before.is_empty());
        let token = if resume {
            if !before.is_empty() {
                d.queue.advance().map_err(|_| ErrorCode::Conflict)?;
            }
            d.token += 1;
            self.load_changes.send_replace(d.token);
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
    pub async fn refresh_prefetch(&self) {
        self.prepare_next().await;
    }
    async fn prepare_next(&self) {
        let mut d = self.data.lock().await;
        if !d.loaded && d.status != "playing" && d.status != "paused" {
            d.prepared.clear();
            return;
        }
        let capacity = self.resolver.prefetch_capacity().min(5);
        let next: Vec<_> = d
            .queue
            .current()
            .map(|current| {
                d.queue
                    .entries()
                    .iter()
                    .skip_while(|e| e.occurrence != current.occurrence)
                    .skip(1)
                    .take(capacity)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let session = d.session.clone();
        d.prepared
            .retain(|p| p.session == session && next.iter().any(|e| e.occurrence == p.occurrence));
        if d.prepared.first().is_some_and(|p| {
            next.first()
                .is_some_and(|first| first.occurrence != p.occurrence)
        }) {
            // Going back inserts a new immediate successor before older queued work.
            d.prepared.clear();
        }
        for next in next {
            if d.prepared.iter().any(|p| p.occurrence == next.occurrence) {
                continue;
            }
            let resolver = self.resolver.clone();
            let limit = self.prefetch_limit.clone();
            let predecessor = d.prepared.last().map(|p| p.finished.clone());
            let (done, finished) = tokio::sync::watch::channel(false);
            d.prepared.push(Prepared {
                session: session.clone(),
                occurrence: next.occurrence,
                created: tokio::time::Instant::now(),
                finished,
                work: Some(tokio::spawn(async move {
                    // Explicit predecessor order, independent of Tokio task scheduling.
                    // A cancelled predecessor closes its channel and unblocks the survivor.
                    if let Some(mut previous) = predecessor {
                        let _ = previous.wait_for(|ready| *ready).await;
                    }
                    // At most five queued preparations and one network download.
                    let _permit = limit
                        .acquire_owned()
                        .await
                        .map_err(|_| ErrorCode::Unavailable)?;
                    let result = tokio::time::timeout(
                        std::time::Duration::from_secs(30),
                        resolver.prefetch(next.track),
                    )
                    .await
                    .unwrap_or(Err(ErrorCode::Timeout));
                    done.send_replace(true);
                    result
                })),
            });
        }
    }
    async fn load_current(self: &Arc<Self>, mut token: u64) {
        loop {
            // Bound each immediate retry window, then yield so a refilling
            // provider cannot create a hot loop or strand queued work.
            for _ in 0..=crate::queue::MAX_UPCOMING {
                let Some(next) = self.load_current_once(token).await else {
                    return;
                };
                token = next;
            }
            let mut changes = self.load_changes.subscribe();
            {
                let d = self.data.lock().await;
                if d.token != token {
                    return;
                }
                changes.borrow_and_update();
            }
            tokio::select! {
                biased;
                _ = changes.changed() => return,
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
            }
        }
    }
    async fn load_current_once(self: &Arc<Self>, token: u64) -> Option<u64> {
        let mut changes = self.load_changes.subscribe();
        let (id, prepared) = {
            let mut d = self.data.lock().await;
            if d.token != token {
                return None;
            }
            let entry = d.queue.current()?;
            let id = entry.track.clone();
            let occurrence = entry.occurrence;
            let prepared = d
                .prepared
                .iter()
                .position(|p| p.occurrence == occurrence)
                .map(|i| d.prepared.remove(i))
                .filter(|p| p.session == d.session);
            changes.borrow_and_update();
            (id, prepared)
        };
        {
            let _io = self.io.lock().await;
            if self.data.lock().await.token != token {
                return None;
            }
            if let Err(error) = self.player.stop().await {
                let mut d = self.data.lock().await;
                if d.token == token {
                    d.status = "stopped".into();
                    d.error = Some(error);
                    d.revision += 1;
                }
                return None;
            }
        }
        let resolving = async {
            match prepared {
                Some(mut prepared) => match prepared.work.as_mut().unwrap().await {
                    Ok(Ok(Source::Stream { .. }))
                        if prepared.created.elapsed() > std::time::Duration::from_secs(45) =>
                    {
                        self.resolver.resolve(id).await
                    }
                    Ok(Ok(source)) => Ok(source),
                    _ => self.resolver.resolve(id).await,
                },
                None => self.resolver.resolve(id).await,
            }
        };
        let source =
            tokio::select! {biased;_=changes.changed()=>return None,result=resolving=>result};

        let _io = self.io.lock().await;
        if self.data.lock().await.token != token {
            return None;
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
            return None;
        }
        let next = match result {
            Ok(()) => {
                d.loaded = true;
                None
            }
            Err(error) => {
                d.error = Some(error);
                self.advance_after_failure(&mut d)
            }
        };
        d.revision += 1;
        drop(d);
        drop(_io);
        if next.is_none() {
            self.prepare_next().await;
        }
        next
    }
    fn advance_after_failure(&self, d: &mut Data) -> Option<u64> {
        if let Some(skipped) = event(d, EventKind::Skipped) {
            let _ = self.events.send(skipped);
            d.started = None;
        }
        d.loaded = false;
        d.status = "stopped".into();
        d.waiting_end = d.station.is_some();
        if d.station.is_none() || d.queue.advance().is_err() {
            return None;
        }
        d.token += 1;
        self.load_changes.send_replace(d.token);
        d.started = None;
        d.played_us = 0;
        d.clock = tokio::time::Instant::now();
        d.waiting_end = false;
        d.position = 0;
        d.duration = 0;
        d.seek_end = None;
        d.status = "loading".into();
        prune_radio(d);
        Some(d.token)
    }
    pub async fn stop(&self) -> Result<State, ErrorCode> {
        let token = {
            let mut d = self.data.lock().await;
            d.token += 1;
            self.load_changes.send_replace(d.token);
            d.status = "loading".into();
            d.loaded = false;
            d.token
        };
        let _io = self.io.lock().await;
        let stopped = self.player.stop().await;
        let mut d = self.data.lock().await;
        if d.token == token {
            d.prepared.clear();
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
            d.refill_error = None;
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
                        && d.station.is_none()
                    {
                        d.tail.push_front(track);
                    }
                    if let Some(event) = skipped {
                        let _ = self.events.send(event);
                    }
                    d.token += 1;
                    self.load_changes.send_replace(d.token);
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
                let next = if snapshot.failed {
                    d.error = Some(ErrorCode::Unavailable);
                    self.advance_after_failure(&mut d)
                } else {
                    None
                };
                d.revision += 1;
                drop(d);
                drop(_io);
                if let Some(token) = next {
                    let owner = self.clone();
                    tokio::spawn(async move {
                        owner.load_current(token).await;
                    });
                }
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
                        // A later track plays: an error kept from a skipped
                        // one no longer describes this session, and must
                        // not come back at its natural end (#810).
                        d.error = None;
                        if let Some(event) = event(&d, EventKind::Started) {
                            let _ = this.events.send(event);
                        }
                    }
                    if snapshot.failed {
                        d.error = Some(ErrorCode::Unavailable);
                        advance = this.advance_after_failure(&mut d).is_some();
                        d.revision += 1;
                    } else if let Some(eof) = snapshot.eof {
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
                                this.load_changes.send_replace(d.token);
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
            if !advance {
                this.prepare_next().await;
            }
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
        added_at: t.added_at,
        pinnable: false,
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
    struct PrefetchDrop(Arc<std::sync::atomic::AtomicBool>);
    impl Drop for PrefetchDrop {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
    struct BlockResolver {
        started: Arc<std::sync::atomic::AtomicBool>,
        dropped: Arc<std::sync::atomic::AtomicBool>,
    }
    impl Resolver for BlockResolver {
        fn resolve(
            &self,
            _id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
            Box::pin(async { Err(ErrorCode::Unavailable) })
        }
        fn prefetch(
            &self,
            _id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
            Box::pin(async move {
                let _drop = PrefetchDrop(self.dropped.clone());
                self.started
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                std::future::pending().await
            })
        }
    }
    #[tokio::test]
    async fn stop_or_replacement_cancels_even_a_preload_consumed_by_load_current() {
        use std::sync::atomic::{AtomicBool, Ordering};
        for replace in [false, true] {
            let started = Arc::new(AtomicBool::new(false));
            let dropped = Arc::new(AtomicBool::new(false));
            let resolver = Arc::new(BlockResolver {
                started: started.clone(),
                dropped: dropped.clone(),
            });
            let playback = Playback::new(
                Player::new(
                    std::path::Path::new("/tmp"),
                    "/nonexistent-arinn-prefetch-player".into(),
                )
                .unwrap(),
                resolver,
            );
            {
                let mut d = playback.data.lock().await;
                d.queue
                    .start(vec!["one".into(), "two".into()], None)
                    .unwrap();
                d.session = "fixture".into();
                d.status = "playing".into();
                d.duration = 60_000_000;
                d.position = 45_000_000;
            }
            playback.prepare_next().await;
            tokio::time::timeout(std::time::Duration::from_secs(1), async {
                while !started.load(Ordering::SeqCst) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            {
                playback.data.lock().await.queue.advance().unwrap();
            }
            let owner = playback.clone();
            let loading = tokio::spawn(async move {
                owner.load_current(0).await;
            });
            tokio::time::timeout(std::time::Duration::from_secs(1), async {
                while !playback.data.lock().await.prepared.is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            if replace {
                playback
                    .play(vec![Track {
                        id: "new".into(),
                        available: true,
                        ..Track::default()
                    }])
                    .await
                    .unwrap();
            } else {
                playback.stop().await.unwrap();
            }
            tokio::time::timeout(std::time::Duration::from_secs(1), loading)
                .await
                .unwrap()
                .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(1), async {
                while !dropped.load(Ordering::SeqCst) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(playback.prefetch_limit.available_permits(), 1);
        }
    }
    struct CountResolver {
        resolves: std::sync::atomic::AtomicUsize,
        preparations: std::sync::atomic::AtomicUsize,
        capacity: std::sync::atomic::AtomicUsize,
        order: std::sync::Mutex<Vec<String>>,
    }
    impl Resolver for CountResolver {
        fn prefetch_capacity(&self) -> usize {
            self.capacity.load(std::sync::atomic::Ordering::Relaxed)
        }
        fn resolve(
            &self,
            _id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
            self.resolves
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async { Ok(Source::File(std::fs::File::open("/dev/null").unwrap())) })
        }
        fn prefetch(
            &self,
            _id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
            self.order.lock().unwrap().push(_id);
            self.preparations
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async { Ok(Source::File(std::fs::File::open("/dev/null").unwrap())) })
        }
    }
    async fn prepared_fixture() -> (Arc<Playback>, Arc<CountResolver>) {
        let resolver = Arc::new(CountResolver {
            resolves: 0.into(),
            preparations: 0.into(),
            capacity: 1.into(),
            order: std::sync::Mutex::new(Vec::new()),
        });
        let player = Player::new(
            std::path::Path::new("/tmp"),
            "/nonexistent-arinn-prefetch-player".into(),
        )
        .unwrap();
        let playback = Playback::new(player, resolver.clone());
        {
            let mut d = playback.data.lock().await;
            d.queue
                .start(vec!["one".into(), "two".into(), "three".into()], None)
                .unwrap();
            d.session = "fixture-session".into();
            d.status = "playing".into();
            d.duration = 60_000_000;
            d.position = 45_000_000;
        }
        (playback, resolver)
    }
    #[tokio::test]
    async fn configured_queue_starts_immediately_and_is_bounded_to_five() {
        let (playback, resolver) = prepared_fixture().await;
        {
            let mut d = playback.data.lock().await;
            d.queue
                .start((0..9).map(|i| format!("fixture-{i}")).collect(), None)
                .unwrap();
            d.position = 0;
            d.duration = 300_000_000;
        }
        resolver
            .capacity
            .store(5, std::sync::atomic::Ordering::Relaxed);
        playback.prepare_next().await;
        assert_eq!(playback.data.lock().await.prepared.len(), 5);
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while resolver.order.lock().unwrap().len() < 5 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            *resolver.order.lock().unwrap(),
            vec![
                "fixture-1",
                "fixture-2",
                "fixture-3",
                "fixture-4",
                "fixture-5"
            ]
        );
        resolver
            .capacity
            .store(2, std::sync::atomic::Ordering::Relaxed);
        playback.refresh_prefetch().await;
        assert_eq!(playback.data.lock().await.prepared.len(), 2);
        resolver
            .capacity
            .store(0, std::sync::atomic::Ordering::Relaxed);
        playback.refresh_prefetch().await;
        assert!(playback.data.lock().await.prepared.is_empty());
    }
    #[tokio::test]
    async fn next_track_is_prepared_once_without_replacing_current_and_reused_at_boundary() {
        let (playback, resolver) = prepared_fixture().await;
        playback.prepare_next().await;
        tokio::task::yield_now().await;
        playback.prepare_next().await;
        tokio::task::yield_now().await;
        assert_eq!(
            resolver
                .preparations
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        {
            let mut d = playback.data.lock().await;
            assert_eq!(d.queue.current().unwrap().track, "one");
            assert_eq!(d.status, "playing");
            assert_eq!(d.position, 45_000_000);
            d.queue.advance().unwrap();
        }
        playback.load_current(0).await;
        // Fake player cannot start, but no second resolver call is needed.
        assert_eq!(
            resolver.resolves.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }
    #[tokio::test]
    async fn expired_changed_and_removed_next_occurrences_never_reuse_prepared_audio() {
        let (playback, resolver) = prepared_fixture().await;
        playback.prepare_next().await;
        tokio::task::yield_now().await;
        {
            let mut d = playback.data.lock().await;
            d.prepared.first_mut().unwrap().created -= std::time::Duration::from_secs(46);
            d.prepared.first_mut().unwrap().work = Some(tokio::spawn(async {
                Ok(Source::Stream {
                    url: "https://fixture.invalid/audio".into(),
                    headers: vec![],
                })
            }));
            d.queue.advance().unwrap();
        }
        playback.load_current(0).await;
        assert_eq!(
            resolver.resolves.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        let (playback, resolver) = prepared_fixture().await;
        playback.prepare_next().await;
        tokio::task::yield_now().await;
        {
            let mut d = playback.data.lock().await;
            let next = d.prepared.first().unwrap().occurrence;
            d.queue.remove(next).unwrap();
            d.queue.advance().unwrap();
        }
        playback.load_current(0).await;
        assert_eq!(
            resolver.resolves.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }
    #[tokio::test]
    async fn preload_starts_as_soon_as_current_is_loaded_and_stop_discards_it() {
        let (playback, _) = prepared_fixture().await;
        playback.prepare_next().await;
        {
            playback.data.lock().await.position = 1_000_000;
        }
        playback.prepare_next().await;
        assert_eq!(playback.data.lock().await.prepared.len(), 1);
        {
            playback.data.lock().await.position = 45_000_000;
        }
        playback.prepare_next().await;
        playback.stop().await.unwrap();
        let d = playback.data.lock().await;
        assert!(d.prepared.is_empty());
        assert!(d.queue.current().is_none());
        assert_eq!(d.status, "stopped");
    }
    /// Resolves every track but `bad`, which fails as an unplayable file.
    struct FailingResolver;
    impl Resolver for FailingResolver {
        fn resolve(
            &self,
            id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
            Box::pin(async move {
                if id == "bad" {
                    Err(ErrorCode::Unavailable)
                } else {
                    Ok(Source::File(std::fs::File::open("/dev/null").unwrap()))
                }
            })
        }
    }

    struct LoadFixture {
        directory: std::path::PathBuf,
        playback: Arc<Playback>,
    }
    impl LoadFixture {
        fn new(resolver: Arc<dyn Resolver>) -> Self {
            use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
            let directory =
                std::env::temp_dir().join(format!("am-{}", uuid::Uuid::new_v4().simple()));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&directory)
                .unwrap();
            let executable = directory.join("fixture.py");
            // Readiness and EOF are explicit so a fast player cannot hide the
            // skipped failure before the test observes the next loading track.
            std::fs::write(&executable, r##"#!/usr/bin/python3
import json,pathlib,socket,sys
root=pathlib.Path(__file__).parent
path=next(x.split('=',1)[1] for x in sys.argv if x.startswith('--input-ipc-server='))
server=socket.socket(socket.AF_UNIX);server.bind(path);server.listen(1)
connection,_=server.accept();stream=connection.makefile('rwb',buffering=0)
loaded=False;ready=False;ended=False;failed=False
for line in stream:
 request=json.loads(line);command=request['command'];data=None;error='success'
 if command[0]=='loadfile':
  if (root/'reject-once').exists():
   (root/'reject-once').unlink();error='loading failed'
  else:
   loaded=True
   stream.write(b'{"event":"start-file","playlist_entry_id":7}\n')
   if (root/'reject-async-once').exists() or (root/'reject-async-always').exists():
    (root/'reject-async-once').unlink(missing_ok=True);failed=True
    stream.write(b'{"event":"end-file","playlist_entry_id":7,"reason":"error"}\n')
 if loaded and command[:3]==['set_property','pause',True] and (root/'reject-on-command').exists():
  (root/'reject-on-command').unlink();failed=True
  stream.write(b'{"event":"end-file","playlist_entry_id":7,"reason":"error"}\n')
 if loaded and not failed and not ready and (root/'ready').exists():
  ready=True;stream.write(b'{"event":"file-loaded"}\n')
 if ready and not failed and (root/'reject-after-ready').exists():
  (root/'reject-after-ready').unlink();failed=True
  stream.write(b'{"event":"end-file","playlist_entry_id":7,"reason":"error"}\n')
 if ready and not ended and (root/'end').exists():
  ended=True;stream.write(b'{"event":"end-file","playlist_entry_id":7,"reason":"eof"}\n')
 if command[0]=='get_property':
  prop=command[1]
  if prop=='playlist/0/id':data=7
  elif prop=='idle-active':data=ended or failed
  elif prop=='pause':data=False
  elif prop=='time-pos':data=0
  elif prop=='duration':data=10
 stream.write((json.dumps({'request_id':request['request_id'],'data':data,'error':error})+'\n').encode())
"##).unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self {
                playback: Playback::new(Player::new(&directory, executable).unwrap(), resolver),
                directory,
            }
        }
        fn flag(&self, name: &str) {
            std::fs::write(self.directory.join(name), []).unwrap();
        }
        async fn start(&self, station: bool, ids: &[&str]) -> State {
            let tracks = ids
                .iter()
                .map(|id| Track {
                    id: (*id).into(),
                    available: true,
                    ..Track::default()
                })
                .collect();
            if station {
                self.playback
                    .radio(
                        tracks,
                        "batch".into(),
                        Entity {
                            id: "station".into(),
                            kind: "station".into(),
                            ..Entity::default()
                        },
                    )
                    .await
                    .unwrap()
            } else {
                self.playback.play(tracks).await.unwrap()
            }
        }
        async fn wait(&self, ready: impl Fn(&State) -> bool) -> State {
            tokio::time::timeout(std::time::Duration::from_secs(8), async {
                loop {
                    let state = self.playback.state().await;
                    if ready(&state) {
                        return state;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap()
        }
        async fn close(self) {
            self.playback.stop().await.unwrap();
        }
    }
    impl Drop for LoadFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    // #818: failures are visible while the next occurrence loads. #810 still
    // clears them once native readiness confirms recovery, including at EOF.
    #[tokio::test]
    async fn a_station_skips_failed_tracks_until_native_recovery() {
        for failure in ["resolve", "load", "native"] {
            let resolver: Arc<dyn Resolver> = if failure == "resolve" {
                Arc::new(FailingResolver)
            } else {
                Arc::new(FixtureResolver)
            };
            let fixture = LoadFixture::new(resolver);
            if failure == "load" {
                fixture.flag("reject-once");
            } else if failure == "native" {
                fixture.flag("reject-async-once");
            }
            let mut events = fixture.playback.subscribe();
            let started = fixture.start(true, &["bad", "good"]).await;
            let next = fixture
                .wait(|state| {
                    state
                        .current
                        .as_ref()
                        .is_some_and(|entry| entry.track.id == "good")
                        && state.playback == "loading"
                        && state.error.is_some()
                })
                .await;
            assert!(next.continuous);
            assert_eq!(next.session, started.session);
            assert_eq!(next.error, Some(ErrorCode::Unavailable));
            assert_eq!(next.history[0].track.id, "bad");
            assert!(
                events.try_recv().is_err(),
                "an unready track emitted feedback"
            );

            fixture.flag("ready");
            let playing = fixture.wait(|state| state.playback == "playing").await;
            assert_eq!(playing.current.unwrap().track.id, "good");
            assert_eq!(playing.error, None);
            let event = events.recv().await.unwrap();
            assert_eq!(event.kind, EventKind::Started);
            assert_eq!(event.track, "good");
            assert_eq!(event.batch, "batch");
            fixture.flag("end");
            let ended = fixture.wait(|state| state.playback == "ended").await;
            assert_eq!(ended.error, None);
            let event = events.recv().await.unwrap();
            assert_eq!(event.kind, EventKind::Finished);
            assert_eq!(event.track, "good");
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn a_finite_queue_still_stops_on_resolve_or_load_failure() {
        for failure in ["resolve", "load", "native"] {
            let resolver: Arc<dyn Resolver> = if failure == "resolve" {
                Arc::new(FailingResolver)
            } else {
                Arc::new(FixtureResolver)
            };
            let fixture = LoadFixture::new(resolver);
            if failure == "load" {
                fixture.flag("reject-once");
            } else if failure == "native" {
                fixture.flag("reject-async-once");
            }
            fixture.start(false, &["bad", "good"]).await;
            let state = fixture
                .wait(|state| state.playback == "stopped" && state.error.is_some())
                .await;
            assert_eq!(state.error, Some(ErrorCode::Unavailable));
            assert_eq!(state.current.unwrap().track.id, "bad");
            assert_eq!(state.upcoming[0].track.id, "good");
            assert!(state.history.is_empty());
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn an_exhausted_failed_station_can_recover_from_a_later_refill() {
        let fixture = LoadFixture::new(Arc::new(FailingResolver));
        let mut events = fixture.playback.subscribe();
        fixture
            .start(true, &vec!["bad"; crate::queue::MAX_UPCOMING + 1])
            .await;
        let request = fixture.playback.begin_radio_refill(false).await.unwrap();
        let state = fixture
            .wait(|state| state.playback == "stopped" && state.upcoming.is_empty())
            .await;
        assert_eq!(state.error, Some(ErrorCode::Unavailable));
        assert_eq!(state.history.len(), crate::queue::MAX_HISTORY);
        assert_eq!(state.current.unwrap().track.id, "bad");
        assert!(events.try_recv().is_err());
        fixture.flag("ready");
        fixture
            .playback
            .apply_radio_refill(
                request,
                vec![Track {
                    id: "good".into(),
                    available: true,
                    ..Track::default()
                }],
                "refill-batch".into(),
            )
            .await
            .unwrap();
        let playing = fixture.wait(|state| state.playback == "playing").await;
        assert_eq!(playing.error, None);
        assert_eq!(playing.current.unwrap().track.id, "good");
        let event = events.recv().await.unwrap();
        assert_eq!(event.kind, EventKind::Started);
        assert_eq!(event.batch, "refill-batch");
        fixture.close().await;
    }

    #[tokio::test]
    async fn native_failures_exhaust_a_station_without_feedback_then_allow_refill() {
        let fixture = LoadFixture::new(Arc::new(FixtureResolver));
        fixture.flag("reject-async-always");
        let mut events = fixture.playback.subscribe();
        fixture.start(true, &["one", "two", "three"]).await;
        let request = fixture.playback.begin_radio_refill(false).await.unwrap();
        let failed = fixture
            .wait(|state| state.playback == "stopped" && state.upcoming.is_empty())
            .await;
        assert_eq!(failed.current.unwrap().track.id, "three");
        assert_eq!(failed.history.len(), 2);
        assert_eq!(failed.error, Some(ErrorCode::Unavailable));
        assert!(events.try_recv().is_err());

        std::fs::remove_file(fixture.directory.join("reject-async-always")).unwrap();
        fixture.flag("ready");
        fixture
            .playback
            .apply_radio_refill(
                request,
                vec![Track {
                    id: "good".into(),
                    available: true,
                    ..Track::default()
                }],
                "refill-batch".into(),
            )
            .await
            .unwrap();
        let playing = fixture.wait(|state| state.playback == "playing").await;
        assert_eq!(playing.current.unwrap().track.id, "good");
        assert_eq!(playing.error, None);
        let event = events.recv().await.unwrap();
        assert_eq!(event.kind, EventKind::Started);
        assert_eq!(event.track, "good");
        assert_eq!(event.batch, "refill-batch");
        fixture.close().await;
    }

    #[tokio::test]
    async fn a_native_failure_observed_by_a_transport_command_also_advances() {
        let fixture = LoadFixture::new(Arc::new(FixtureResolver));
        let mut events = fixture.playback.subscribe();
        let initial = fixture.start(true, &["bad", "good"]).await;
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            while !fixture.playback.data.lock().await.loaded {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        fixture.flag("reject-on-command");
        fixture
            .playback
            .command(&radio_command("pause", &initial.session))
            .await
            .unwrap();
        let next = fixture
            .wait(|state| state.current.as_ref().is_some_and(|e| e.track.id == "good"))
            .await;
        assert_eq!(next.session, initial.session);
        assert_eq!(next.error, Some(ErrorCode::Unavailable));
        assert_eq!(next.history[0].track.id, "bad");
        assert!(events.try_recv().is_err());
        fixture.flag("ready");
        let playing = fixture.wait(|state| state.playback == "playing").await;
        assert_eq!(playing.error, None);
        assert_eq!(playing.current.unwrap().track.id, "good");
        fixture.close().await;
    }

    #[tokio::test]
    async fn a_ready_track_that_fails_emits_one_skip_with_played_time() {
        for (command, exhausted) in [(false, false), (true, false), (false, true), (true, true)] {
            let fixture = LoadFixture::new(Arc::new(FixtureResolver));
            fixture.flag("ready");
            let mut events = fixture.playback.subscribe();
            let ids: &[&str] = if exhausted {
                &["bad"]
            } else {
                &["bad", "good"]
            };
            let initial = fixture.start(true, ids).await;
            fixture.wait(|state| state.playback == "playing").await;
            let started = events.recv().await.unwrap();
            assert_eq!(started.kind, EventKind::Started);
            assert_eq!(started.track, "bad");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            if command {
                fixture.flag("reject-on-command");
                fixture
                    .playback
                    .command(&radio_command("pause", &initial.session))
                    .await
                    .unwrap();
            } else {
                fixture.flag("reject-after-ready");
            }
            let after = fixture
                .wait(|state| {
                    if exhausted {
                        state.playback == "stopped" && state.error.is_some()
                    } else {
                        state.playback == "playing"
                            && state.current.as_ref().is_some_and(|e| e.track.id == "good")
                    }
                })
                .await;
            let skipped = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(skipped.kind, EventKind::Skipped);
            assert_eq!(skipped.occurrence, started.occurrence);
            assert_eq!(skipped.batch, started.batch);
            assert_eq!(skipped.track, "bad");
            assert!(skipped.played_us > 0);
            if exhausted {
                let request = fixture.playback.begin_radio_refill(false).await.unwrap();
                fixture
                    .playback
                    .apply_radio_refill(
                        request,
                        vec![Track {
                            id: "good".into(),
                            available: true,
                            ..Track::default()
                        }],
                        "refill-batch".into(),
                    )
                    .await
                    .unwrap();
                fixture.wait(|state| state.playback == "playing").await;
            } else {
                assert_eq!(after.error, None);
            }
            let recovered = events.recv().await.unwrap();
            assert_eq!(recovered.kind, EventKind::Started);
            assert_eq!(recovered.track, "good");
            assert_eq!(
                recovered.batch,
                if exhausted { "refill-batch" } else { "batch" }
            );
            assert!(events.try_recv().is_err());
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn refills_clear_their_own_error_but_keep_playback_failure_until_readiness() {
        for failure in ["playback", "refill", "both"] {
            let playback_failed = failure != "refill";
            let fixture = LoadFixture::new(Arc::new(FailingResolver));
            if playback_failed {
                fixture.start(true, &["bad", "good"]).await;
                fixture
                    .wait(|state| state.current.as_ref().is_some_and(|e| e.track.id == "good"))
                    .await;
            } else {
                fixture.start(true, &["good"]).await;
            }
            if failure != "playback" {
                let request = fixture.playback.begin_radio_refill(false).await.unwrap();
                fixture
                    .playback
                    .fail_radio_refill(request, ErrorCode::Timeout)
                    .await;
                if !playback_failed {
                    assert_eq!(
                        fixture.playback.state().await.error,
                        Some(ErrorCode::Timeout)
                    );
                }
            }
            let request = fixture.playback.begin_radio_refill(false).await.unwrap();
            fixture
                .playback
                .apply_radio_refill(
                    request,
                    vec![Track {
                        id: "later".into(),
                        available: true,
                        ..Track::default()
                    }],
                    "refill-batch".into(),
                )
                .await
                .unwrap();
            let loading = fixture.playback.state().await;
            assert_eq!(loading.playback, "loading");
            assert_eq!(loading.current.unwrap().track.id, "good");
            assert_eq!(
                loading.error,
                playback_failed.then_some(ErrorCode::Unavailable)
            );
            fixture.flag("ready");
            let playing = fixture.wait(|state| state.playback == "playing").await;
            assert_eq!(playing.error, None);
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn native_readiness_preserves_a_failed_refill_until_its_retry_succeeds() {
        let fixture = LoadFixture::new(Arc::new(FixtureResolver));
        fixture.start(true, &["good"]).await;
        let failed = fixture.playback.begin_radio_refill(false).await.unwrap();
        fixture
            .playback
            .fail_radio_refill(failed, ErrorCode::Timeout)
            .await;
        fixture.flag("ready");
        let playing = fixture.wait(|state| state.playback == "playing").await;
        assert_eq!(playing.error, Some(ErrorCode::Timeout));
        assert_eq!(playing.refill, "failed");

        let retry = fixture.playback.begin_radio_refill(false).await.unwrap();
        assert_eq!(
            fixture.playback.state().await.error,
            Some(ErrorCode::Timeout)
        );
        fixture
            .playback
            .apply_radio_refill(
                retry,
                vec![Track {
                    id: "later".into(),
                    available: true,
                    ..Track::default()
                }],
                "refill-batch".into(),
            )
            .await
            .unwrap();
        assert_eq!(fixture.playback.state().await.error, None);
        fixture.close().await;
    }

    struct SlowAfterFailure {
        started: tokio::sync::Notify,
        dropped: Arc<std::sync::atomic::AtomicBool>,
        fail_resolve: bool,
    }
    impl Resolver for SlowAfterFailure {
        fn resolve(
            &self,
            id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
            Box::pin(async move {
                if id == "slow" {
                    let _drop = PrefetchDrop(self.dropped.clone());
                    self.started.notify_one();
                    std::future::pending().await
                } else if self.fail_resolve {
                    FailingResolver.resolve(id).await
                } else {
                    FixtureResolver.resolve(id).await
                }
            })
        }
    }
    #[tokio::test]
    async fn stop_or_replacement_cancels_a_pending_station_skip() {
        use std::sync::atomic::{AtomicBool, Ordering};
        for (replace, native) in [(false, false), (true, false), (false, true), (true, true)] {
            let resolver = Arc::new(SlowAfterFailure {
                started: tokio::sync::Notify::new(),
                dropped: Arc::new(AtomicBool::new(false)),
                fail_resolve: !native,
            });
            let fixture = LoadFixture::new(resolver.clone());
            fixture.flag("ready");
            if native {
                fixture.flag("reject-async-once");
            }
            let old = fixture.start(true, &["bad", "slow", "never"]).await;
            tokio::time::timeout(
                std::time::Duration::from_secs(2),
                resolver.started.notified(),
            )
            .await
            .unwrap();
            let loading = fixture
                .wait(|state| state.current.as_ref().is_some_and(|e| e.track.id == "slow"))
                .await;
            assert_eq!(loading.current.unwrap().track.id, "slow");
            assert_eq!(loading.error, Some(ErrorCode::Unavailable));
            if replace {
                fixture.start(false, &["new"]).await;
            } else {
                fixture.playback.stop().await.unwrap();
            }
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while !resolver.dropped.load(Ordering::SeqCst) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let state = if replace {
                fixture.wait(|state| state.playback == "playing").await
            } else {
                fixture.playback.state().await
            };
            assert_ne!(state.session, old.session);
            assert!(!state.continuous);
            assert_eq!(state.error, None);
            assert_eq!(
                state.current.map(|entry| entry.track.id),
                replace.then(|| "new".into())
            );
            fixture.close().await;
        }
    }

    struct RefillingFailure {
        playback: std::sync::Mutex<Weak<Playback>>,
        attempts: std::sync::atomic::AtomicUsize,
        times: std::sync::Mutex<Vec<tokio::time::Instant>>,
        recover_after: Option<usize>,
    }
    impl Resolver for RefillingFailure {
        fn prefetch_capacity(&self) -> usize {
            0
        }
        fn resolve(
            &self,
            id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
            Box::pin(async move {
                if id != "bad" {
                    return FixtureResolver.resolve(id).await;
                }
                let attempt = self
                    .attempts
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                    + 1;
                self.times.lock().unwrap().push(tokio::time::Instant::now());
                if self.recover_after.is_some_and(|limit| attempt > limit) {
                    return FixtureResolver.resolve(id).await;
                }
                let playback = self.playback.lock().unwrap().upgrade().unwrap();
                if playback.state().await.upcoming.len() <= 8 {
                    let request = playback.begin_radio_refill(false).await.unwrap();
                    playback
                        .apply_radio_refill_repeating(
                            request,
                            vec![
                                Track {
                                    id: "bad".into(),
                                    available: true,
                                    ..Track::default()
                                };
                                8
                            ],
                            "batch".into(),
                        )
                        .await
                        .unwrap();
                }
                Err(ErrorCode::Unavailable)
            })
        }
    }
    #[tokio::test]
    async fn a_station_continues_queued_work_after_a_paced_failure_window() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let resolver = Arc::new(RefillingFailure {
            playback: std::sync::Mutex::new(Weak::new()),
            attempts: AtomicUsize::new(0),
            times: std::sync::Mutex::new(Vec::new()),
            recover_after: Some(crate::queue::MAX_UPCOMING + 1),
        });
        let fixture = LoadFixture::new(resolver.clone());
        *resolver.playback.lock().unwrap() = Arc::downgrade(&fixture.playback);
        fixture.flag("ready");
        let initial = fixture.start(true, &["bad"]).await;
        let playing = fixture.wait(|state| state.playback == "playing").await;
        assert_eq!(playing.session, initial.session);
        assert_ne!(
            playing.current.unwrap().occurrence,
            initial.current.unwrap().occurrence
        );
        assert!(playing.upcoming.len() >= 8);
        assert_eq!(playing.error, None);
        assert_eq!(
            resolver.attempts.load(Ordering::SeqCst),
            crate::queue::MAX_UPCOMING + 2
        );
        {
            let times = resolver.times.lock().unwrap();
            assert!(
                times[crate::queue::MAX_UPCOMING + 1] - times[crate::queue::MAX_UPCOMING]
                    >= std::time::Duration::from_millis(500)
            );
        }
        fixture.close().await;
    }
    #[tokio::test]
    async fn stop_or_replacement_cancels_a_paused_failure_window() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        for replace in [false, true] {
            let resolver = Arc::new(RefillingFailure {
                playback: std::sync::Mutex::new(Weak::new()),
                attempts: AtomicUsize::new(0),
                times: std::sync::Mutex::new(Vec::new()),
                recover_after: None,
            });
            let fixture = LoadFixture::new(resolver.clone());
            *resolver.playback.lock().unwrap() = Arc::downgrade(&fixture.playback);
            fixture.flag("ready");
            let initial = fixture.start(true, &["bad"]).await;
            let first: usize = initial.current.unwrap().occurrence.parse().unwrap();
            let paused = fixture
                .wait(|state| {
                    let attempts = resolver.attempts.load(Ordering::SeqCst);
                    state.playback == "loading"
                        && attempts > 0
                        && attempts.is_multiple_of(crate::queue::MAX_UPCOMING + 1)
                        && state.current.as_ref().is_some_and(|e| {
                            e.occurrence.parse::<usize>().unwrap() == first + attempts
                        })
                })
                .await;
            assert!(paused.upcoming.len() >= 8);
            assert_eq!(paused.error, Some(ErrorCode::Unavailable));
            if replace {
                fixture.start(false, &["new"]).await;
                fixture.wait(|state| state.playback == "playing").await;
            } else {
                fixture.playback.stop().await.unwrap();
            }
            let attempts = resolver.attempts.load(Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            assert_eq!(resolver.attempts.load(Ordering::SeqCst), attempts);
            let after = fixture.playback.state().await;
            assert_ne!(after.session, initial.session);
            assert_eq!(after.error, None);
            assert!(!after.continuous);
            assert_eq!(
                after.current.map(|e| e.track.id),
                replace.then(|| "new".into())
            );
            fixture.close().await;
        }
    }
    /// #810: the error of a skipped track was kept while the next track
    /// played, and came back when the queue ended naturally.
    #[tokio::test]
    async fn a_skipped_tracks_error_is_cleared_once_a_later_track_plays() {
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
            Arc::new(FailingResolver),
        );
        let track = |id: &str| Track {
            id: id.into(),
            available: true,
            ..Track::default()
        };
        let state = playback
            .play(vec![track("bad"), track("good")])
            .await
            .unwrap();
        let wait = |want: &'static str, error: bool| {
            let playback = playback.clone();
            async move {
                tokio::time::timeout(std::time::Duration::from_secs(8), async {
                    loop {
                        let state = playback.state().await;
                        if state.playback == want && state.error.is_some() == error {
                            return state;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                })
                .await
                .unwrap()
            }
        };
        let failed = wait("stopped", true).await;
        assert_eq!(failed.current.unwrap().track.id, "bad");
        playback
            .command(&Command {
                request_id: uuid::Uuid::new_v4().to_string(),
                action: "next".into(),
                session: state.session.clone(),
                entity: String::new(),
                occurrence: String::new(),
                position_us: 0,
                replaces_provider: String::new(),
                replaces_session: String::new(),
                text: String::new(),
            })
            .await
            .unwrap();
        // The good track plays to its natural end: ended, with no error.
        let ended = wait("ended", false).await;
        assert_eq!(ended.current.unwrap().track.id, "good");
        assert_eq!(ended.error, None);
        playback.stop().await.unwrap();
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
    // These tests exercise queue edits, not failed-track playback. Keep the
    // current load pending so automatic failure skipping cannot move it.
    struct PendingResolver;
    impl Resolver for PendingResolver {
        fn resolve(
            &self,
            _id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
            Box::pin(std::future::pending())
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
                Arc::new(PendingResolver),
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
