// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Source-owned catalogue materialization, finite audio and continuous sessions.
use crate::{Api, Batch, Feedback, RadioSession};
use arinn_music_common::{
    Track,
    playback::{EventKind, Playback, Resolver},
    player::{Player, Source},
    storage::Storage,
};
use arinn_plugin_sdk::runtime::MusicCatalogue;
use arinn_protocol::{
    music::{Command, Entity, Query, State},
    validation::ErrorCode,
};
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    future::Future,
    pin::Pin,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{Mutex, RwLock, Semaphore};
#[derive(Clone)]
struct Authorized {
    epoch: u64,
    api: Arc<Api>,
}
impl std::ops::Deref for Authorized {
    type Target = Api;
    fn deref(&self) -> &Api {
        &self.api
    }
}
struct SourceResolver(
    Arc<RwLock<Option<Authorized>>>,
    Arc<std::sync::atomic::AtomicUsize>,
);
impl Resolver for SourceResolver {
    fn prefetch_capacity(&self) -> usize {
        self.1.load(std::sync::atomic::Ordering::Relaxed).min(5)
    }
    fn resolve(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            let api = self.0.read().await.clone().ok_or(ErrorCode::Unavailable)?;
            api.audio(&id).await.map_err(error)
        })
    }
    fn prefetch(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            let api = self.0.read().await.clone().ok_or(ErrorCode::Unavailable)?;
            api.prebuffer(&id).await.map_err(error)
        })
    }
}
struct Cache {
    key: String,
    revision: u64,
    entries: Vec<Entity>,
    tracks: Vec<Track>,
    error: Option<ErrorCode>,
    bytes: usize,
}
struct Radio {
    wire: String,
    session: RadioSession,
    batches: VecDeque<Batch>,
    request: u64,
    pending: bool,
}
struct Data {
    epoch: u64,
    revision: u64,
    pending: HashSet<String>,
    cache: VecDeque<Cache>,
    radio: Option<Radio>,
    ratings: BTreeMap<String, String>,
    pins: Vec<Entity>,
    feedback_error: Option<ErrorCode>,
    expired_stopped: bool,
}
pub struct Runtime {
    source: Arc<RwLock<Option<Authorized>>>,
    data: Mutex<Data>,
    pub playback: Arc<Playback>,
    catalogue_slots: Arc<Semaphore>,
    pins: Arc<std::sync::Mutex<Storage>>,
    #[cfg(test)]
    assets: Arc<Semaphore>,
    artwork_cache: Arc<crate::artwork::Artwork>,
    prefetch_tracks: Arc<std::sync::atomic::AtomicUsize>,
    epochs: tokio::sync::watch::Sender<u64>,
}
fn error(error: crate::Error) -> ErrorCode {
    match error {
        crate::Error::InvalidInput => ErrorCode::InvalidArgument,
        crate::Error::Authentication | crate::Error::Forbidden => ErrorCode::PermissionDenied,
        _ => ErrorCode::Unavailable,
    }
}
fn track_entity(track: &Track) -> Entity {
    let mut value = arinn_music_common::playback::entity(track);
    value.artwork = format!("cover:{}", track.id);
    value
}
fn station_title(seed: &str, locale: &str) -> &'static str {
    let russian = locale.split(['_', '-', '.']).next() == Some("ru");
    match (seed == "user:onyourwave", russian) {
        (true, true) => "Моя волна",
        (false, true) => "Радио по треку",
        (true, false) => "My Wave",
        (false, false) => "Track radio",
    }
}
fn message_locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default()
}
// Every station here derives its ID from the service's own station key
// (`user:onyourwave`, `track:<id>`), so the ID survives a provider restart
// and never names another station: it may be pinned (ADR-0142 MH-6, #811).
fn station(id: &str, title: &str) -> Entity {
    Entity {
        id: id.into(),
        kind: "station".into(),
        title: title.into(),
        available: true,
        pinnable: true,
        ..Entity::default()
    }
}
fn seed(entity: &str) -> Result<String, ErrorCode> {
    if entity == "station:wave" {
        Ok("user:onyourwave".into())
    } else {
        let id = entity.strip_prefix("station:track:").unwrap_or(entity);
        if !crate::parse::track_id(id) {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(format!(
            "track:{}",
            id.split(':').next().ok_or(ErrorCode::InvalidArgument)?
        ))
    }
}
fn feedback_batch(batch: &Batch) -> Batch {
    Batch {
        id: batch.id.clone(),
        tracks: batch
            .tracks
            .iter()
            .map(|t| Track {
                id: t.id.clone(),
                ..Track::default()
            })
            .collect(),
    }
}

impl Runtime {
    pub fn new(player: Player, pins: Storage) -> Result<Arc<Self>, ErrorCode> {
        let mut saved: Vec<Entity> = pins
            .read()
            .map_err(|_| ErrorCode::Unavailable)?
            .map(|b| serde_json::from_slice(&b).map_err(|_| ErrorCode::Unavailable))
            .transpose()?
            .unwrap_or_default();
        // Pins stored before #811 lack the mark; every stored pin is one of
        // this provider's stations, so it carries it.
        for pin in &mut saved {
            pin.pinnable = pin.kind == "station";
        }
        if saved.len() > 256
            || saved
                .iter()
                .any(|e| e.kind != "station" || e.validate().is_err() || seed(&e.id).is_err())
        {
            return Err(ErrorCode::InvalidArgument);
        }
        let source = Arc::new(RwLock::new(None));
        let prefetch_tracks = Arc::new(std::sync::atomic::AtomicUsize::new(5));
        let playback = Playback::new(
            player,
            Arc::new(SourceResolver(source.clone(), prefetch_tracks.clone())),
        );
        let assets = Arc::new(Semaphore::new(4));
        let this = Arc::new(Self {
            source,
            data: Mutex::new(Data {
                epoch: 0,
                revision: 1,
                pending: HashSet::new(),
                cache: VecDeque::new(),
                radio: None,
                ratings: BTreeMap::new(),
                pins: saved,
                feedback_error: None,
                expired_stopped: false,
            }),
            playback,
            catalogue_slots: Arc::new(Semaphore::new(2)),
            pins: Arc::new(std::sync::Mutex::new(pins)),
            artwork_cache: crate::artwork::Artwork::new(assets.clone()),
            prefetch_tracks,
            #[cfg(test)]
            assets,
            epochs: tokio::sync::watch::channel(0).0,
        });
        let weak = Arc::downgrade(&this);
        let mut events = this.playback.subscribe();
        tokio::spawn(async move {
            loop {
                let event = match events.recv().await {
                    Ok(e) => e,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    Err(_) => {
                        if let Some(this) = weak.upgrade() {
                            let mut d = this.data.lock().await;
                            d.revision += 1;
                            d.feedback_error = Some(ErrorCode::Unavailable);
                        }
                        continue;
                    }
                };
                let Some(this) = weak.upgrade() else {
                    return;
                };
                let Some((epoch, radio, batch, api)) = this.feedback_context(&event).await else {
                    continue;
                };
                let feedback = match event.kind {
                    EventKind::Started => Feedback::TrackStarted(&event.track),
                    EventKind::Finished => {
                        Feedback::TrackFinished(&event.track, event.played_us / 1000)
                    }
                    EventKind::Skipped => Feedback::Skip(&event.track, event.played_us / 1000),
                };
                if tokio::time::timeout(
                    Duration::from_secs(4),
                    this.guard(epoch, async {
                        api.feedback(&radio, &batch, feedback).await.map_err(error)
                    }),
                )
                .await
                .map_or(true, |r| r.is_err())
                {
                    let mut d = this.data.lock().await;
                    if d.epoch == epoch {
                        d.revision += 1;
                        d.feedback_error = Some(ErrorCode::Unavailable);
                    }
                }
            }
        });
        let weak = Arc::downgrade(&this);
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(Duration::from_secs(1));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                timer.tick().await;
                let Some(this) = weak.upgrade() else {
                    return;
                };
                if this.expired().await {
                    this.stop_expired().await;
                    continue;
                }
                let state = this.playback.state().await;
                if state.continuous && state.refill == "idle" && state.upcoming.len() < 8 {
                    let _ = this.refill(false).await;
                }
            }
        });
        Ok(this)
    }
    pub async fn set_prefetch_tracks(&self, count: usize) -> Result<(), ErrorCode> {
        if count > 5 {
            return Err(ErrorCode::InvalidArgument);
        }
        self.prefetch_tracks
            .store(count, std::sync::atomic::Ordering::Relaxed);
        self.playback.refresh_prefetch().await;
        Ok(())
    }
    pub async fn connected(&self, api: Api) {
        let mut d = self.data.lock().await;
        d.epoch += 1;
        *self.source.write().await = Some(Authorized {
            epoch: d.epoch,
            api: Arc::new(api),
        });
        self.epochs.send_replace(d.epoch);
        d.revision += 1;
        d.pending.clear();
        d.cache.clear();
        d.radio = None;
        d.ratings.clear();
        d.feedback_error = None;
        d.expired_stopped = false;
        {
            let epoch = *self.epochs.borrow();
            self.artwork_cache.clear(epoch).await;
        }
    }
    pub async fn disconnected(&self) {
        {
            let mut d = self.data.lock().await;
            d.epoch += 1;
            *self.source.write().await = None;
            self.epochs.send_replace(d.epoch);
            d.revision += 1;
            d.pending.clear();
            d.cache.clear();
            d.radio = None;
            d.ratings.clear();
            d.feedback_error = None;
            d.expired_stopped = false;
        }
        {
            let epoch = *self.epochs.borrow();
            self.artwork_cache.clear(epoch).await;
        }
        let _ = self.playback.stop().await;
    }
    async fn feedback_context(
        &self,
        event: &arinn_music_common::playback::PlaybackEvent,
    ) -> Option<(u64, RadioSession, Batch, Authorized)> {
        let d = self.data.lock().await;
        let radio = d.radio.as_ref().filter(|r| r.wire == event.session)?;
        let batch = radio.batches.iter().find(|b| b.id == event.batch)?.clone();
        let api = self
            .source
            .read()
            .await
            .clone()
            .filter(|api| api.epoch == d.epoch)?;
        Some((d.epoch, radio.session.clone(), batch, api))
    }
    async fn guard<T>(
        &self,
        epoch: u64,
        work: impl Future<Output = Result<T, ErrorCode>>,
    ) -> Result<T, ErrorCode> {
        let mut changes = self.epochs.subscribe();
        if *changes.borrow() != epoch {
            return Err(ErrorCode::Conflict);
        }
        tokio::select! {biased;_=changes.changed()=>Err(ErrorCode::Conflict),result=work=>result}
    }
    async fn stop_expired(&self) {
        let mut d = self.data.lock().await;
        if d.expired_stopped {
            return;
        }
        d.expired_stopped = true;
        d.epoch += 1;
        d.revision += 1;
        d.radio = None;
        d.pending.clear();
        d.cache.clear();
        d.ratings.clear();
        if let Some(source) = self.source.write().await.as_mut() {
            source.epoch = d.epoch;
        }
        self.epochs.send_replace(d.epoch);
        drop(d);
        let _ = self.playback.stop().await;
    }
    pub async fn artwork(
        self: &Arc<Self>,
        id: &str,
    ) -> Result<arinn_protocol::assets::Asset, ErrorCode> {
        let id = id
            .strip_prefix("cover:")
            .filter(|id| crate::parse::track_id(id))
            .ok_or(ErrorCode::NotFound)?
            .to_owned();
        let api = self
            .source
            .read()
            .await
            .clone()
            .ok_or(ErrorCode::Unavailable)?;
        let epoch = api.epoch;
        let owner = self.clone();
        let request = id.clone();
        self.guard(
            epoch,
            self.artwork_cache.get(epoch, id, async move {
                owner
                    .guard(epoch, async { api.artwork(&request).await.map_err(error) })
                    .await
            }),
        )
        .await
    }
    pub async fn expired(&self) -> bool {
        self.source
            .read()
            .await
            .as_ref()
            .is_some_and(|api| api.expired())
    }
    pub async fn state(&self) -> State {
        let mut state = self.playback.state().await;
        let expired = self.expired().await;
        let d = self.data.lock().await;
        if let Some(station) = &mut state.station
            && station.id == "station:wave"
        {
            station.title = station_title("user:onyourwave", &message_locale()).into();
        }
        state.account = if expired { "expired" } else { "connected" }.into();
        state.revision = state.revision.saturating_add(d.revision);
        state.catalogue_revision = d.revision;
        state.catalogue_loading = !d.pending.is_empty();
        state.error = state.error.or(d.feedback_error);
        state.facets = vec![
            "selections".into(),
            "tracks".into(),
            "albums".into(),
            "artists".into(),
            "playlists".into(),
        ];
        state.actions.retain(|a| a != "remove" || !state.continuous);
        state.actions.extend(["scan".into(), "radio".into()]);
        if let Some(current) = &mut state.current {
            current.track.artwork = format!("cover:{}", current.track.id);
            state.feedback = d
                .ratings
                .get(&current.track.id)
                .cloned()
                .unwrap_or_default();
            state.actions.extend(["like".into(), "dislike".into()]);
        }
        for entry in state.history.iter_mut().chain(&mut state.upcoming) {
            entry.track.artwork = format!("cover:{}", entry.track.id);
        }
        if let Some(station) = &state.station {
            state.pinned = d.pins.iter().any(|p| p.id == station.id);
            state.actions.push("pin".into());
        }
        if expired {
            state.actions = vec!["connect".into()];
            state.error = Some(ErrorCode::PermissionDenied);
        }
        state
    }
    pub async fn catalogue(self: &Arc<Self>, query: Query) -> Result<MusicCatalogue, ErrorCode> {
        let key = serde_json::to_string(&query.without_token())
            .map_err(|_| ErrorCode::InvalidArgument)?;
        let mut d = self.data.lock().await;
        if let Some(cache) = d.cache.iter().find(|c| c.key == key) {
            if let Some(error) = cache.error {
                return Err(error);
            }
            return Ok(MusicCatalogue {
                revision: cache.revision,
                entries: cache.entries.clone(),
            });
        }
        if !d.pending.contains(&key) {
            let permit = self
                .catalogue_slots
                .clone()
                .try_acquire_owned()
                .map_err(|_| ErrorCode::Unavailable)?;
            let api = self
                .source
                .read()
                .await
                .clone()
                .ok_or(ErrorCode::Unavailable)?;
            let epoch = d.epoch;
            d.pending.insert(key.clone());
            let this = self.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let result =
                    tokio::time::timeout(Duration::from_secs(30), this.fetch(&api, &query))
                        .await
                        .unwrap_or(Err(ErrorCode::Timeout));
                let mut d = this.data.lock().await;
                if d.epoch != epoch {
                    return;
                }
                d.pending.remove(&key);
                d.revision += 1;
                let (mut entries, mut tracks, mut error) = match result {
                    Ok((e, t)) => (e, t, None),
                    Err(e) => (vec![], vec![], Some(e)),
                };
                let mut bytes = serde_json::to_vec(&entries).map_or(16 * 1024 * 1024, |b| b.len())
                    + serde_json::to_vec(&tracks).map_or(16 * 1024 * 1024, |b| b.len());
                if bytes > 16 * 1024 * 1024 {
                    entries.clear();
                    tracks.clear();
                    error = Some(ErrorCode::Unavailable);
                    bytes = 0;
                }
                while !d.cache.is_empty()
                    && (d.cache.len() >= 8
                        || d.cache.iter().map(|c| c.bytes).sum::<usize>() + bytes
                            > 16 * 1024 * 1024)
                {
                    d.cache.pop_front();
                }
                let revision = d.revision;
                if bytes <= 16 * 1024 * 1024 {
                    d.cache.push_back(Cache {
                        key,
                        revision,
                        entries,
                        tracks,
                        error,
                        bytes,
                    });
                }
            });
        }
        Err(ErrorCode::Unavailable)
    }
    async fn fetch(
        &self,
        api: &Api,
        query: &Query,
    ) -> Result<(Vec<Entity>, Vec<Track>), ErrorCode> {
        let uid = api.account_uid().await.map_err(error)?;
        let tracks = if let Some(id) = query.parent.strip_prefix("playlist:") {
            api.playlist_tracks(&uid, id).await.map_err(error)?
        } else if let Some(id) = query.parent.strip_prefix("album:") {
            api.album_tracks(id).await.map_err(error)?
        } else if let Some(id) = query.parent.strip_prefix("artist:") {
            api.artist_tracks(id).await.map_err(error)?
        } else if !query.search.trim().is_empty()
            && ["albums", "artists"].contains(&query.facet.as_str())
        {
            let kind = if query.facet == "albums" {
                "album"
            } else {
                "artist"
            };
            return Ok((
                api.search_entities(&query.search, kind)
                    .await
                    .map_err(error)?,
                vec![],
            ));
        } else if !query.search.trim().is_empty() {
            let mut tracks = Vec::new();
            for page in 0..50 {
                let next = api
                    .search_tracks(&query.search, page)
                    .await
                    .map_err(error)?;
                let count = next.len();
                crate::catalogue::append_tracks(&mut tracks, next).map_err(error)?;
                if count < 20 {
                    break;
                }
                if page == 49 {
                    return Err(ErrorCode::Unavailable);
                }
            }
            tracks
        } else if query.facet == "selections" {
            let mut entries = vec![station(
                "station:wave",
                station_title("user:onyourwave", &message_locale()),
            )];
            for pin in &self.data.lock().await.pins {
                if !entries.iter().any(|e| e.id == pin.id) {
                    let mut pin = pin.clone();
                    if pin.id == "station:wave" {
                        pin.title = station_title("user:onyourwave", &message_locale()).into();
                    }
                    entries.push(pin);
                }
            }
            entries.extend(
                api.user_playlists(&uid)
                    .await
                    .map_err(error)?
                    .into_iter()
                    .map(|p| Entity {
                        id: format!("playlist:{}", p.id),
                        kind: "playlist".into(),
                        title: p.title,
                        available: true,
                        ..Entity::default()
                    }),
            );
            return Ok((entries, vec![]));
        } else if query.facet == "playlists" {
            return Ok((
                api.user_playlists(&uid)
                    .await
                    .map_err(error)?
                    .into_iter()
                    .map(|p| Entity {
                        id: format!("playlist:{}", p.id),
                        kind: "playlist".into(),
                        title: p.title,
                        available: true,
                        ..Entity::default()
                    })
                    .collect(),
                vec![],
            ));
        } else {
            api.liked_tracks(&uid).await.map_err(error)?
        };
        let entries =
            if query.parent.is_empty() && ["albums", "artists"].contains(&query.facet.as_str()) {
                let mut groups = BTreeMap::new();
                for t in &tracks {
                    if query.facet == "artists" {
                        for id in &t.artist_ids {
                            groups.entry(format!("artist:{id}")).or_insert(Entity {
                                id: format!("artist:{id}"),
                                kind: "artist".into(),
                                title: t.artist.clone(),
                                available: t.available,
                                ..Entity::default()
                            });
                        }
                    }
                    if query.facet == "albums" && !t.album_id.is_empty() {
                        groups
                            .entry(format!("album:{}", t.album_id))
                            .or_insert(Entity {
                                id: format!("album:{}", t.album_id),
                                kind: "album".into(),
                                title: t.album.clone(),
                                artist: t.album_artist.clone(),
                                available: t.available,
                                ..Entity::default()
                            });
                    }
                }
                groups.into_values().collect()
            } else {
                let mut seen = HashSet::new();
                tracks
                    .iter()
                    .filter(|t| seen.insert(&t.id))
                    .filter(|t| {
                        query.parent.is_empty()
                            || query.search.trim().is_empty()
                            || format!("{} {} {}", t.title, t.artist, t.album)
                                .to_lowercase()
                                .contains(&query.search.to_lowercase())
                    })
                    .map(track_entity)
                    .collect()
            };
        Ok((entries, tracks))
    }
    async fn selected(&self, id: &str) -> Result<Vec<Track>, ErrorCode> {
        let d = self.data.lock().await;
        for cache in d.cache.iter().rev() {
            if serde_json::from_str::<Query>(&cache.key).is_ok_and(|q| q.parent == id) {
                return Ok(cache.tracks.clone());
            }
            if let Some(index) = cache.tracks.iter().position(|t| t.id == id) {
                return Ok(cache.tracks[index..].to_vec());
            }
        }
        Err(ErrorCode::NotFound)
    }
    pub async fn command(self: &Arc<Self>, command: &Command) -> Result<(), ErrorCode> {
        let api = self
            .source
            .read()
            .await
            .clone()
            .ok_or(ErrorCode::Unavailable)?;
        match command.action.as_str() {
            "play" => {
                let tracks = self.selected(&command.entity).await?;
                let mut d = self.data.lock().await;
                if d.epoch != api.epoch {
                    return Err(ErrorCode::Conflict);
                }
                self.playback.play(tracks).await?;
                d.radio = None;
            }
            "radio" => self.start_radio(&api, &command.entity).await?,
            "retry" => {
                if self.playback.state().await.session != command.session {
                    return Err(ErrorCode::Conflict);
                }
                self.refill(false).await?;
            }
            "like" | "dislike" => {
                let state = self.playback.state().await;
                if state.session != command.session {
                    return Err(ErrorCode::Conflict);
                }
                let track = state.current.ok_or(ErrorCode::Conflict)?.track.id;
                tokio::time::timeout(
                    Duration::from_secs(3),
                    self.guard(api.epoch, async {
                        let uid = api.account_uid().await.map_err(error)?;
                        api.rate(&uid, &track, command.action == "like")
                            .await
                            .map_err(error)
                    }),
                )
                .await
                .map_err(|_| ErrorCode::Timeout)??;
                let mut d = self.data.lock().await;
                let current = self.playback.state().await;
                if d.epoch != api.epoch
                    || current.session != state.session
                    || current.current.as_ref().map(|c| &c.track.id) != Some(&track)
                {
                    return Err(ErrorCode::Conflict);
                }
                if d.ratings.len() >= 512 && !d.ratings.contains_key(&track) {
                    d.ratings.pop_first();
                }
                d.ratings.insert(
                    track,
                    if command.action == "like" {
                        "liked"
                    } else {
                        "disliked"
                    }
                    .into(),
                );
                d.revision += 1;
                drop(d);
                if command.action == "dislike" && state.continuous {
                    self.refill(true).await?;
                }
            }
            "pin" => self.pin(&command.entity).await?,
            "stop" => {
                self.data.lock().await.radio = None;
                self.playback.command(command).await?;
            }
            "scan" => {
                let mut d = self.data.lock().await;
                d.cache.clear();
                d.revision += 1;
            }
            _ => {
                self.playback.command(command).await?;
            }
        }
        Ok(())
    }
    async fn start_radio(self: &Arc<Self>, api: &Authorized, id: &str) -> Result<(), ErrorCode> {
        let epoch = api.epoch;
        let seed = seed(id)?;
        let session = self
            .prepare_radio(epoch, async { api.start_radio(&seed).await.map_err(error) })
            .await?;
        let title = station_title(&seed, &message_locale());
        let station_id = if seed == "user:onyourwave" {
            "station:wave".into()
        } else {
            format!("station:{seed}")
        };
        let mut d = self.data.lock().await;
        if d.epoch != epoch {
            return Err(ErrorCode::Conflict);
        }
        let state = self
            .playback
            .radio(
                session.batch.tracks.clone(),
                session.batch.id.clone(),
                station(&station_id, title),
            )
            .await?;
        let batches = VecDeque::from([feedback_batch(&session.batch)]);
        d.radio = Some(Radio {
            wire: state.session,
            session,
            batches,
            request: 0,
            pending: false,
        });
        d.revision += 1;
        Ok(())
    }
    async fn prepare_radio(
        &self,
        epoch: u64,
        work: impl Future<Output = Result<RadioSession, ErrorCode>>,
    ) -> Result<RadioSession, ErrorCode> {
        // Session creation and initial feedback each have a 15-second HTTP limit.
        // Keep one finite preparation deadline and cancel both on epoch changes.
        tokio::time::timeout(Duration::from_secs(30), self.guard(epoch, work))
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
    async fn refill(self: &Arc<Self>, replace: bool) -> Result<(), ErrorCode> {
        let mut d = self.data.lock().await;
        let api = self
            .source
            .read()
            .await
            .clone()
            .ok_or(ErrorCode::Unavailable)?;
        let epoch = d.epoch;
        if api.epoch != epoch {
            return Err(ErrorCode::Conflict);
        }
        let radio = d.radio.as_mut().ok_or(ErrorCode::Unsupported)?;
        if radio.pending && !replace {
            return Ok(());
        }
        let state = self.playback.state().await;
        if state.session != radio.wire {
            return Err(ErrorCode::Conflict);
        }
        let request = self.playback.begin_radio_refill(replace).await?;
        radio.request += 1;
        radio.pending = true;
        let serial = radio.request;
        let wire = radio.wire.clone();
        let private = radio.session.clone();
        let current = state.current.map(|c| c.track.id).unwrap_or_default();
        drop(d);
        let this = self.clone();
        tokio::spawn(async move {
            let result = tokio::time::timeout(
                Duration::from_secs(20),
                this.guard(epoch, async {
                    api.next_batch(&private, &current).await.map_err(error)
                }),
            )
            .await
            .unwrap_or(Err(ErrorCode::Timeout));
            let mut d = this.data.lock().await;
            if d.epoch != epoch {
                return;
            }
            let Some(radio) = d
                .radio
                .as_mut()
                .filter(|r| r.wire == wire && r.request == serial)
            else {
                return;
            };
            radio.pending = false;
            match result {
                Ok(batch) => {
                    radio.batches.push_back(feedback_batch(&batch));
                    let needed = this.playback.batch_ids().await;
                    while radio.batches.len() > 256 {
                        if let Some(index) =
                            radio.batches.iter().position(|b| !needed.contains(&b.id))
                        {
                            radio.batches.remove(index);
                        } else {
                            break;
                        }
                    }

                    let _ = this
                        .playback
                        .apply_radio_refill(request, batch.tracks, batch.id)
                        .await;
                }
                Err(e) => this.playback.fail_radio_refill(request, e).await,
            }
            d.revision += 1;
        });
        Ok(())
    }
    async fn pin(self: &Arc<Self>, id: &str) -> Result<(), ErrorCode> {
        self.pin_with(id, |storage, bytes| storage.write(bytes))
            .await
    }
    async fn pin_with<F>(self: &Arc<Self>, id: &str, write: F) -> Result<(), ErrorCode>
    where
        F: FnOnce(&Storage, &[u8]) -> Result<(), arinn_music_common::storage::Error>
            + Send
            + 'static,
    {
        let this = self.clone();
        let id = id.to_owned();
        // Own the write and publication together if the caller times out.
        tokio::spawn(async move {
            let mut d = this.data.lock().await;
            let state = this.playback.state().await;
            let station = state
                .station
                .filter(|s| s.id == id)
                .ok_or(ErrorCode::InvalidArgument)?;
            let mut pins = d.pins.clone();
            if let Some(index) = pins.iter().position(|p| p.id == id) {
                pins.remove(index);
            } else {
                if pins.len() >= 256 {
                    return Err(ErrorCode::Unavailable);
                }
                pins.push(station);
            }
            let storage = this.pins.clone();
            let bytes = serde_json::to_vec(&pins).map_err(|_| ErrorCode::Internal)?;
            let result = tokio::task::spawn_blocking(move || {
                let storage = storage
                    .lock()
                    .map_err(|_| arinn_music_common::storage::Error::Unavailable)?;
                write(&storage, &bytes)
            })
            .await
            .map_err(|_| ErrorCode::Internal)?;
            let durability = matches!(result, Err(arinn_music_common::storage::Error::Durability));
            if result.is_err() && !durability {
                return Err(ErrorCode::Unavailable);
            }
            d.pins = pins;
            d.cache.clear();
            d.revision += 1;
            if durability {
                d.feedback_error = Some(ErrorCode::Unavailable);
                Err(ErrorCode::Unavailable)
            } else {
                Ok(())
            }
        })
        .await
        .map_err(|_| ErrorCode::Internal)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::Mock;
    use serde_json::json;
    struct Fixture {
        directory: std::path::PathBuf,
        runtime: Arc<Runtime>,
    }
    impl Fixture {
        fn new() -> Self {
            use std::os::unix::fs::DirBuilderExt;
            let directory =
                std::env::temp_dir().join(format!("arinn-yandex-runtime-{}", uuid::Uuid::new_v4()));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&directory)
                .unwrap();
            let pins = Storage::open(&directory.join("pins")).unwrap();
            let runtime = Runtime::new(
                Player::new(&directory, "/nonexistent-arinn-test-player".into()).unwrap(),
                pins,
            )
            .unwrap();
            Self { directory, runtime }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
    fn track(id: u64, title: &str) -> serde_json::Value {
        json!({"id":id,"title":title,"available":true,"artists":[{"id":9,"name":"Fixture artist"}],"albums":[{"id":42,"title":"Fixture album"}]})
    }
    fn query(facet: &str, parent: &str, search: &str) -> Query {
        Query {
            facet: facet.into(),
            parent: parent.into(),
            search: search.into(),
            token: String::new(),
        }
    }
    fn radio_response() -> RadioSession {
        RadioSession {
            id: "session-a".into(),
            batch: crate::parse::batch(&json!({"batchId":"batch-a", "sequence":[
                {"track":{"id":123,"available":true}}
            ]}))
            .unwrap(),
        }
    }
    #[tokio::test(start_paused = true)]
    async fn radio_start_accepts_session_and_initial_feedback_after_three_seconds() {
        let fixture = Fixture::new();
        let epoch = fixture.runtime.data.lock().await.epoch;
        let started = tokio::time::Instant::now();
        let result = fixture
            .runtime
            .prepare_radio(epoch, async {
                tokio::time::sleep(Duration::from_secs(9)).await;
                let session = radio_response();
                // Initial feedback is part of the same bounded preparation.
                tokio::time::sleep(Duration::from_secs(8)).await;
                Ok(session)
            })
            .await
            .unwrap();
        assert_eq!(result.id, "session-a");
        assert_eq!(
            tokio::time::Instant::now() - started,
            Duration::from_secs(17)
        );
        assert!(fixture.runtime.data.lock().await.radio.is_none());
    }
    #[tokio::test(start_paused = true)]
    async fn radio_start_deadline_cancels_unfinished_preparation() {
        let fixture = Fixture::new();
        let epoch = fixture.runtime.data.lock().await.epoch;
        let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
        let started = tokio::time::Instant::now();
        let result = fixture
            .runtime
            .prepare_radio(epoch, async move {
                let _sender = sender;
                std::future::pending::<Result<RadioSession, ErrorCode>>().await
            })
            .await;
        assert_eq!(result.err(), Some(ErrorCode::Timeout));
        assert_eq!(
            tokio::time::Instant::now() - started,
            Duration::from_secs(30)
        );
        assert!(receiver.await.is_err());
        assert!(fixture.runtime.data.lock().await.radio.is_none());
    }
    #[tokio::test(start_paused = true)]
    async fn reconnect_cancels_radio_preparation_before_the_longer_deadline() {
        let fixture = Fixture::new();
        let epoch = fixture.runtime.data.lock().await.epoch;
        let mock = Mock::new(vec![]);
        let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
        let (result, ()) = tokio::join!(
            fixture.runtime.prepare_radio(epoch, async move {
                let _sender = sender;
                tokio::time::sleep(Duration::from_secs(9)).await;
                Ok(radio_response())
            }),
            async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                fixture.runtime.connected(mock.api.clone()).await;
            }
        );
        assert_eq!(result.err(), Some(ErrorCode::Conflict));
        assert!(receiver.await.is_err());
        assert!(fixture.runtime.data.lock().await.radio.is_none());
        assert!(mock.finish().is_empty());
    }
    #[tokio::test]
    async fn artists_and_parent_search_use_real_provider_metadata_without_losing_group_order() {
        let fixture = Fixture::new();
        let mock = Mock::new(vec![
            (200, json!({"result":{"account":{"uid":5}}}).to_string()),
            (
                200,
                json!({"result":{"library":{"tracks":[{"id":123}]}}}).to_string(),
            ),
            (200, json!({"result":[track(123,"Song")]}).to_string()),
        ]);
        let (artists, _) = fixture
            .runtime
            .fetch(&mock.api, &query("artists", "", ""))
            .await
            .unwrap();
        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0].id, "artist:9");
        assert_eq!(artists[0].title, "Fixture artist");
        assert_eq!(mock.finish().len(), 3);
        let mock=Mock::new(vec![(200,json!({"result":{"account":{"uid":5}}}).to_string()),(200,json!({"result":{"tracks":[{"track":track(123,"Morning")},{"track":track(124,"Quiet evening")}]}}).to_string())]);
        let (rows, all) = fixture
            .runtime
            .fetch(&mock.api, &query("playlists", "playlist:8", "quiet"))
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Quiet evening");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].title, "Morning");
        assert_eq!(mock.finish().len(), 2);
    }
    #[tokio::test]
    async fn stale_feedback_cannot_use_a_new_accounts_client() {
        let fixture = Fixture::new();
        let old = Mock::new(vec![]);
        let new = Mock::new(vec![]);
        fixture.runtime.connected(old.api.clone()).await;
        let batch = Batch {
            id: "batch-A".into(),
            tracks: vec![Track {
                id: "123".into(),
                ..Track::default()
            }],
        };
        {
            let mut d = fixture.runtime.data.lock().await;
            d.radio = Some(Radio {
                wire: "wire-A".into(),
                session: RadioSession {
                    id: "radio-A".into(),
                    batch: batch.clone(),
                },
                batches: VecDeque::from([batch]),
                request: 0,
                pending: false,
            });
        }
        let event = arinn_music_common::playback::PlaybackEvent {
            session: "wire-A".into(),
            batch: "batch-A".into(),
            track: "123".into(),
            occurrence: 1,
            played_us: 0,
            kind: EventKind::Started,
        };
        let (epoch, radio, batch, client) = fixture.runtime.feedback_context(&event).await.unwrap();
        fixture.runtime.connected(new.api.clone()).await;
        assert!(fixture.runtime.feedback_context(&event).await.is_none());
        assert_eq!(
            fixture
                .runtime
                .guard(epoch, async {
                    client
                        .feedback(&radio, &batch, Feedback::TrackStarted("123"))
                        .await
                        .map_err(error)
                })
                .await,
            Err(ErrorCode::Conflict)
        );
        assert!(old.finish().is_empty());
        assert!(new.finish().is_empty());
    }
    #[tokio::test]
    async fn known_token_expiry_stops_the_session_without_an_http_401() {
        let fixture = Fixture::new();
        let mut mock = Mock::new(vec![]);
        mock.api.deadline = Some(tokio::time::Instant::now());
        fixture.runtime.connected(mock.api.clone()).await;
        fixture
            .runtime
            .playback
            .radio(
                vec![],
                "empty".into(),
                station("station:wave", "Fixture wave"),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                let stopped = fixture.runtime.data.lock().await.expired_stopped;
                if stopped && !fixture.runtime.playback.state().await.continuous {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let state = fixture.runtime.state().await;
        assert_eq!(state.account, "expired");
        assert!(!state.continuous);
        assert!(state.current.is_none());
        assert!(state.actions.contains(&"connect".into()));
        assert!(mock.finish().is_empty());
    }
    #[test]
    fn built_in_station_titles_follow_message_locale_without_changing_identity() {
        assert_eq!(station_title("user:onyourwave", "ru_RU.UTF-8"), "Моя волна");
        assert_eq!(station_title("track:123", "ru-RU"), "Радио по треку");
        assert_eq!(station_title("user:onyourwave", "en_US.UTF-8"), "My Wave");
        assert_eq!(station_title("user:onyourwave", "C"), "My Wave");
    }
    #[tokio::test]
    async fn artwork_admission_is_checked_before_sending_any_http_request() {
        let fixture = Fixture::new();
        let mock = Mock::new(vec![
            (
                200,
                json!({"result":[{"id":123,"coverUri":"invalid"}]}).to_string(),
            ),
            (200, json!({"result":{"account":{"uid":5}}}).to_string()),
        ]);
        fixture.runtime.connected(mock.api.clone()).await;
        let permit = fixture.runtime.assets.acquire_many(4).await.unwrap();
        assert_eq!(
            fixture.runtime.artwork("cover:123").await,
            Err(ErrorCode::Unavailable)
        );
        drop(permit);
        assert!(fixture.runtime.artwork("cover:123").await.is_err());
        assert_eq!(mock.api.account_uid().await.unwrap(), "5");
        assert_eq!(mock.finish().len(), 2);
    }
    #[tokio::test]
    async fn old_account_cannot_start_radio_after_reconnect() {
        let fixture = Fixture::new();
        let old = Mock::new(vec![]);
        let new = Mock::new(vec![]);
        fixture.runtime.connected(old.api.clone()).await;
        let stale = fixture.runtime.source.read().await.clone().unwrap();
        fixture.runtime.connected(new.api.clone()).await;
        assert_eq!(
            fixture.runtime.start_radio(&stale, "station:wave").await,
            Err(ErrorCode::Conflict)
        );
        assert!(old.finish().is_empty());
        assert!(new.finish().is_empty());
    }
    #[tokio::test]
    async fn invalid_finite_selection_preserves_existing_radio_context() {
        let fixture = Fixture::new();
        let mock = Mock::new(vec![]);
        fixture.runtime.connected(mock.api.clone()).await;
        fixture.runtime.data.lock().await.radio = Some(Radio {
            wire: "wire".into(),
            session: RadioSession {
                id: "radio".into(),
                batch: Batch {
                    id: "batch".into(),
                    tracks: vec![],
                },
            },
            batches: VecDeque::new(),
            request: 0,
            pending: false,
        });
        let command = Command {
            request_id: uuid::Uuid::new_v4().to_string(),
            action: "play".into(),
            entity: "unknown".into(),
            session: String::new(),
            occurrence: String::new(),
            position_us: 0,
            replaces_provider: String::new(),
            replaces_session: String::new(),
            text: String::new(),
        };
        assert_eq!(
            fixture.runtime.command(&command).await,
            Err(ErrorCode::NotFound)
        );
        assert_eq!(
            fixture
                .runtime
                .data
                .lock()
                .await
                .radio
                .as_ref()
                .unwrap()
                .wire,
            "wire"
        );
        assert!(mock.finish().is_empty());
    }
    #[tokio::test]
    async fn fresh_connected_catalogue_advertises_station_activation() {
        let fixture = Fixture::new();
        let mock = Mock::new(vec![]);
        fixture.runtime.connected(mock.api.clone()).await;
        let state = fixture.runtime.state().await;
        assert!(state.current.is_none());
        assert!(state.actions.contains(&"radio".into()));
        assert!(!state.actions.contains(&"like".into()));
        assert!(mock.finish().is_empty());
    }
    #[tokio::test]
    async fn post_rename_pin_sync_failure_publishes_committed_metadata() {
        let fixture = Fixture::new();
        fixture
            .runtime
            .playback
            .radio(
                vec![],
                "batch".into(),
                station("station:wave", "Fixture wave"),
            )
            .await
            .unwrap();
        let result = fixture
            .runtime
            .pin_with("station:wave", |storage, bytes| {
                storage.write(bytes)?;
                Err(arinn_music_common::storage::Error::Durability)
            })
            .await;
        assert_eq!(result, Err(ErrorCode::Unavailable));
        assert!(fixture.runtime.state().await.pinned);
        let stored: Vec<Entity> = serde_json::from_slice(
            &fixture
                .runtime
                .pins
                .lock()
                .unwrap()
                .read()
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(stored.len(), 1);
        fixture.runtime.pin("station:wave").await.unwrap();
        assert!(!fixture.runtime.state().await.pinned);
        let stored: Vec<Entity> = serde_json::from_slice(
            &fixture
                .runtime
                .pins
                .lock()
                .unwrap()
                .read()
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert!(stored.is_empty());
    }
    /// #811: a station's ID and its pinnable mark are the same after a
    /// provider restart, also for a pin stored before the mark existed.
    #[tokio::test]
    async fn station_ids_survive_a_provider_restart() {
        let fixture = Fixture::new();
        fixture
            .runtime
            .playback
            .radio(
                vec![],
                "batch".into(),
                station("station:wave", "Fixture wave"),
            )
            .await
            .unwrap();
        let current = fixture.runtime.state().await.station.unwrap();
        assert_eq!(current.id, "station:wave");
        assert!(current.pinnable && current.validate().is_ok());
        fixture.runtime.pin("station:wave").await.unwrap();
        // Add a pin stored before #811, without the mark.
        let mut stored: Vec<serde_json::Value> = serde_json::from_slice(
            &fixture
                .runtime
                .pins
                .lock()
                .unwrap()
                .read()
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        stored.push(
            json!({"id":"station:track:123","kind":"station","title":"Fixture track radio",
            "artist":"","album":"","artwork":"","duration_us":0,"available":true}),
        );
        let bytes = serde_json::to_vec(&stored).unwrap();
        // The next provider start reads the same stored state. The first
        // runtime keeps its storage locked, so the restart reads a copy.
        let restarted = Fixture::new();
        let storage = Storage::open(&restarted.directory.join("pins-copy")).unwrap();
        storage.write(&bytes).unwrap();
        let again = Runtime::new(
            Player::new(
                &restarted.directory,
                "/nonexistent-arinn-test-player".into(),
            )
            .unwrap(),
            storage,
        )
        .unwrap();
        let pins = again.data.lock().await.pins.clone();
        let ids: Vec<_> = pins.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["station:wave", "station:track:123"]);
        assert!(pins.iter().all(|p| p.pinnable && p.validate().is_ok()));
        // A station started again from its stored ID resolves to the same ID.
        assert_eq!(seed("station:wave").unwrap(), "user:onyourwave");
        assert_eq!(seed("station:track:123").unwrap(), "track:123");
    }
}
