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
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
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
        // A pinned station the service no longer knows (#811).
        crate::Error::NotFound => ErrorCode::NotFound,
        _ => ErrorCode::Unavailable,
    }
}
fn track_entity(track: &Track) -> Entity {
    let mut value = arinn_music_common::playback::entity(track);
    value.artwork = format!("cover:{}", track.id);
    value
}
/// One of the account's playlists; its cover only when the API names one
/// (#825).
fn playlist(uid: &str, p: crate::Collection) -> Entity {
    Entity {
        artwork: if p.cover && crate::parse::digits(uid) && crate::parse::digits(&p.id) {
            format!("playlist-cover:{uid}:{}", p.id)
        } else {
            String::new()
        },
        id: format!("playlist:{}", p.id),
        kind: "playlist".into(),
        title: p.title,
        available: true,
        added_at: p.added_at,
        ..Entity::default()
    }
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
/// A station's artwork ID when its rotor info names an image, else empty:
/// no image, no artwork (#843). A slow or failed lookup leaves it empty and
/// never holds up the station.
async fn station_artwork(api: &Api, id: &str) -> String {
    station_artworks(api, &[id.to_owned()])
        .await
        .remove(id)
        .unwrap_or_default()
}
/// The artwork IDs of several stations, asked at most eight at a time under
/// one shared 3-second deadline: however many stations are slow, the
/// listing waits no longer, and those stations go without artwork (Codex
/// review of #849).
async fn station_artworks(api: &Api, ids: &[String]) -> HashMap<String, String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let slots = Arc::new(Semaphore::new(8));
    let mut lookups = tokio::task::JoinSet::new();
    for id in ids {
        let Ok(key) = seed(id) else {
            continue;
        };
        let (api, slots, id) = (api.clone(), slots.clone(), id.clone());
        lookups.spawn(async move {
            let _slot = slots.acquire_owned().await;
            let image = api.station_image(&key).await;
            (
                id,
                matches!(image, Ok(Some(_))).then(|| format!("station-cover:{key}")),
            )
        });
    }
    let mut artworks = HashMap::new();
    while let Ok(Some(done)) = tokio::time::timeout_at(deadline, lookups.join_next()).await {
        if let Ok((id, Some(artwork))) = done {
            artworks.insert(id, artwork);
        }
    }
    // Dropping the set cancels the lookups still running.
    artworks
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
        // A track's, an album's, an artist's or a playlist's cover (#825).
        let cover = crate::parse::cover(id).ok_or(ErrorCode::NotFound)?;
        let id = id.to_owned();
        let api = self
            .source
            .read()
            .await
            .clone()
            .ok_or(ErrorCode::Unavailable)?;
        let epoch = api.epoch;
        let owner = self.clone();
        let request = cover;
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
            let pins = self.data.lock().await.pins.clone();
            for mut pin in pins {
                if !entries.iter().any(|e| e.id == pin.id) {
                    if pin.id == "station:wave" {
                        pin.title = station_title("user:onyourwave", &message_locale()).into();
                    }
                    entries.push(pin);
                }
            }
            // Each station's cover as its rotor info names it now, also for
            // a pin stored before stations carried one (#843).
            let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
            let mut artworks = station_artworks(api, &ids).await;
            for entry in &mut entries {
                entry.artwork = artworks.remove(&entry.id).unwrap_or_default();
            }
            entries.extend(
                api.user_playlists(&uid)
                    .await
                    .map_err(error)?
                    .into_iter()
                    .map(|p| playlist(&uid, p)),
            );
            return Ok((entries, vec![]));
        } else if query.facet == "playlists" {
            return Ok((
                api.user_playlists(&uid)
                    .await
                    .map_err(error)?
                    .into_iter()
                    .map(|p| playlist(&uid, p))
                    .collect(),
                vec![],
            ));
        } else {
            api.liked_tracks(&uid).await.map_err(error)?
        };
        let entries = if query.parent.is_empty()
            && ["albums", "artists"].contains(&query.facet.as_str())
        {
            let mut groups = BTreeMap::new();
            for t in &tracks {
                if query.facet == "artists" {
                    for id in &t.artist_ids {
                        let group = groups.entry(format!("artist:{id}")).or_insert(Entity {
                            id: format!("artist:{id}"),
                            kind: "artist".into(),
                            title: t.artist.clone(),
                            available: t.available,
                            // The artist's own picture (#825).
                            artwork: if crate::parse::digits(id) {
                                format!("artist-cover:{id}")
                            } else {
                                String::new()
                            },
                            ..Entity::default()
                        });
                        // A group takes its newest track's date (MH-9).
                        group.added_at = group.added_at.max(t.added_at);
                    }
                }
                if query.facet == "albums" && !t.album_id.is_empty() {
                    let group = groups
                        .entry(format!("album:{}", t.album_id))
                        .or_insert(Entity {
                            id: format!("album:{}", t.album_id),
                            kind: "album".into(),
                            title: t.album.clone(),
                            artist: t.album_artist.clone(),
                            available: t.available,
                            // A track's cover is its album's: no new
                            // request (#825, as music-local).
                            artwork: format!("cover:{}", t.id),
                            ..Entity::default()
                        });
                    group.added_at = group.added_at.max(t.added_at);
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
        let mut playing = station(&station_id, title);
        playing.artwork = station_artwork(api, &station_id).await;
        let mut d = self.data.lock().await;
        if d.epoch != epoch {
            return Err(ErrorCode::Conflict);
        }
        let state = self
            .playback
            .radio(
                session.batch.tracks.clone(),
                session.batch.id.clone(),
                playing,
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
    fn response_with(value: serde_json::Value) -> (u16, String) {
        (200, json!({ "result": value }).to_string())
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
    /// #813 (ADR-0142 MH-9): liked tracks carry their like time, groups take
    /// their newest track's date, a track without a like time and a release
    /// date never count, and playlists carry their created date.
    #[tokio::test]
    async fn library_dates_come_from_likes_and_playlists_only() {
        let fixture = Fixture::new();
        let dated = |id: u64, title: &str, album: u64| {
            json!({"id":id,"title":title,"available":true,"year":2001,
                   "artists":[{"id":9,"name":"Fixture artist"}],
                   "albums":[{"id":album,"title":"Fixture album","releaseDate":"2001-01-01T00:00:00+00:00"}]})
        };
        let likes = json!({"result":{"library":{"tracks":[
            {"id":"123","albumId":"42","timestamp":"2024-05-01T12:00:00+00:00"},
            {"id":"124","albumId":"42","timestamp":"2024-06-01T12:00:00+00:00"},
            {"id":"125","albumId":"43"}
        ]}}});
        let tracks = json!({"result":[dated(123,"Older",42), dated(124,"Newer",42), dated(125,"Undated",43)]});
        let account = json!({"result":{"account":{"uid":5}}});
        for (facet, expected) in [
            (
                "tracks",
                vec![
                    ("Older", 1_714_564_800),
                    ("Newer", 1_717_243_200),
                    ("Undated", 0),
                ],
            ),
            ("albums", vec![("album:42", 1_717_243_200), ("album:43", 0)]),
            ("artists", vec![("artist:9", 1_717_243_200)]),
        ] {
            let mock = Mock::new(vec![
                (200, account.to_string()),
                (200, likes.to_string()),
                (200, tracks.to_string()),
            ]);
            let (entries, _) = fixture
                .runtime
                .fetch(&mock.api, &query(facet, "", ""))
                .await
                .unwrap();
            let got: Vec<(String, u64)> = entries
                .iter()
                .map(|e| {
                    (
                        if facet == "tracks" {
                            e.title.clone()
                        } else {
                            e.id.clone()
                        },
                        e.added_at,
                    )
                })
                .collect();
            let expected: Vec<(String, u64)> =
                expected.into_iter().map(|(k, v)| (k.into(), v)).collect();
            assert_eq!(got, expected, "{facet}");
            assert!(entries.iter().all(|e| e.validate().is_ok()));
            mock.finish();
        }
        let mock =
            Mock::new(vec![
            (200, account.to_string()),
            (200, json!({"result":[
                {"kind":3,"title":"Dated","trackCount":2,"created":"2024-05-01T12:00:00+00:00"},
                {"kind":4,"title":"Undated","trackCount":1}
            ]}).to_string()),
        ]);
        let (entries, _) = fixture
            .runtime
            .fetch(&mock.api, &query("playlists", "", ""))
            .await
            .unwrap();
        assert_eq!(
            entries.iter().map(|e| e.added_at).collect::<Vec<_>>(),
            [1_714_564_800, 0]
        );
        mock.finish();
    }
    /// #825: grouped albums take their first track's cover, grouped artists
    /// and search results their own, playlists theirs only when the API
    /// names one; every ID passes the Entity artwork rules.
    #[tokio::test]
    async fn albums_artists_and_playlists_carry_their_covers() {
        let fixture = Fixture::new();
        let account = json!({"result":{"account":{"uid":5}}});
        let likes = json!({"result":{"library":{"tracks":[{"id":"123","albumId":"42"}]}}});
        let tracks = json!({"result":[track(123,"One")]});
        for (facet, expected) in [("albums", "cover:123:42"), ("artists", "artist-cover:9")] {
            let mock = Mock::new(vec![
                (200, account.to_string()),
                (200, likes.to_string()),
                (200, tracks.to_string()),
            ]);
            let (entries, _) = fixture
                .runtime
                .fetch(&mock.api, &query(facet, "", ""))
                .await
                .unwrap();
            assert_eq!(entries.len(), 1, "{facet}");
            assert_eq!(entries[0].artwork, expected, "{facet}");
            mock.finish();
        }
        let searches = [
            (
                "albums",
                json!({"albums":{"results":[
                    {"id":7,"title":"Pictured","coverUri":"avatars.yandex.net/get-music-content/1/a/%%"},
                    {"id":8,"title":"Bare"}
                ]}}),
                vec!["album-cover:7", ""],
            ),
            (
                "artists",
                json!({"artists":{"results":[
                    {"id":9,"name":"Pictured","cover":{"type":"from-album-cover","uri":"avatars.yandex.net/get-music-content/2/b/%%"}},
                    {"id":10,"name":"Shared","ogImage":"avatars.yandex.net/get-music-content/3/c/%%"},
                    {"id":11,"name":"Bare","cover":{"type":"mosaic"}}
                ]}}),
                vec!["artist-cover:9", "artist-cover:10", ""],
            ),
        ];
        for (facet, result, expected) in searches {
            let mock = Mock::new(vec![
                (200, account.to_string()),
                (200, json!({ "result": result }).to_string()),
            ]);
            let (entries, _) = fixture
                .runtime
                .fetch(&mock.api, &query(facet, "", "fixture"))
                .await
                .unwrap();
            let got: Vec<&str> = entries.iter().map(|e| e.artwork.as_str()).collect();
            assert_eq!(got, expected, "{facet}");
            assert!(entries.iter().all(|e| e.validate().is_ok()));
            mock.finish();
        }
        let mock = Mock::new(vec![
            (200, account.to_string()),
            (200, json!({"result":[
                {"kind":3,"title":"Pictured","trackCount":2,"cover":{"type":"pic","uri":"avatars.yandex.net/get-yapic/3/c/%%"}},
                {"kind":4,"title":"Bare","trackCount":1,"cover":{"type":"mosaic","itemsUri":[]}}
            ]}).to_string()),
        ]);
        let (entries, _) = fixture
            .runtime
            .fetch(&mock.api, &query("playlists", "", ""))
            .await
            .unwrap();
        let got: Vec<&str> = entries.iter().map(|e| e.artwork.as_str()).collect();
        assert_eq!(got, ["playlist-cover:5:3", ""]);
        assert!(entries.iter().all(|e| e.validate().is_ok()));
        mock.finish();
    }
    /// #825: an album cover on a host outside the avatars CDN is refused
    /// before any download; an ID that is not this provider's is not found
    /// and sends nothing.
    #[tokio::test]
    async fn a_cover_on_a_foreign_host_is_refused() {
        let fixture = Fixture::new();
        let mock = Mock::new(vec![response_with(
            json!({"id":7,"coverUri":"images.example.com/get-music-content/1/a/%%"}),
        )]);
        fixture.runtime.connected(mock.api.clone()).await;
        assert_eq!(
            fixture.runtime.artwork("album-cover:7").await,
            Err(ErrorCode::Unavailable)
        );
        for id in [
            "album-cover:7/8",
            "playlist-cover:5",
            "artist-cover:x",
            "station:wave",
        ] {
            assert_eq!(
                fixture.runtime.artwork(id).await,
                Err(ErrorCode::NotFound),
                "{id}"
            );
        }
        let requests = mock.finish();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].0.starts_with("GET /albums/7 "),
            "{}",
            requests[0].0
        );
    }
    /// #843: a station carries `station-cover:<key>` when its rotor info
    /// names an image, and no artwork otherwise, also for a pin stored
    /// before stations had covers; the answer is kept for the connection.
    #[tokio::test]
    async fn stations_carry_their_rotor_covers() {
        let fixture = Fixture::new();
        fixture.runtime.data.lock().await.pins = vec![
            station("station:track:123", "Fixture radio"),
            station("station:track:456", "Bare radio"),
        ];
        let account = json!({"result":{"account":{"uid":5}}});
        let wave = json!({"result":[{"station":{"id":{"type":"user","tag":"onyourwave"},
            "icon":{"backgroundColor":"#000000","imageUrl":"avatars.yandex.net/get-music-misc/1/wave/%%"}}}]});
        let track = json!({"result":[{"station":{"id":{"type":"track","tag":"123"},
            "icon":{"imageUrl":""},"fullImageUrl":"avatars.yandex.net/get-music-content/2/b/%%"}}]});
        let bare = json!({"result":[{"station":{"id":{"type":"track","tag":"456"},"icon":{}}}]});
        let mock = Mock::new(vec![
            (200, account.to_string()),
            (200, wave.to_string()),
            (200, track.to_string()),
            (200, bare.to_string()),
            (200, json!({"result":[]}).to_string()),
            (200, account.to_string()),
            (200, json!({"result":[]}).to_string()),
        ]);
        for _ in 0..2 {
            let (entries, _) = fixture
                .runtime
                .fetch(&mock.api, &query("selections", "", ""))
                .await
                .unwrap();
            let got: Vec<(&str, &str)> = entries
                .iter()
                .map(|e| (e.id.as_str(), e.artwork.as_str()))
                .collect();
            assert_eq!(
                got,
                [
                    ("station:wave", "station-cover:user:onyourwave"),
                    ("station:track:123", "station-cover:track:123"),
                    ("station:track:456", ""),
                ]
            );
            assert!(entries.iter().all(|e| e.validate().is_ok()));
        }
        let requests = mock.finish();
        let paths: Vec<&str> = requests
            .iter()
            .filter_map(|r| r.0.split(' ').nth(1))
            .filter(|p| p.starts_with("/rotor/"))
            .collect();
        assert_eq!(
            paths,
            [
                "/rotor/station/user:onyourwave/info",
                "/rotor/station/track:123/info",
                "/rotor/station/track:456/info",
            ]
        );
    }
    /// #843: a station cover is resolved through the rotor info with the
    /// same allow-list as the other covers; an image on another host is
    /// refused before any download, and an ID that is not a station key in
    /// its own form is not found and sends nothing.
    #[tokio::test]
    async fn a_station_cover_on_a_foreign_host_is_refused() {
        let fixture = Fixture::new();
        let mock = Mock::new(vec![response_with(json!([{"station":{
            "icon":{"imageUrl":"images.example.com/get-music-misc/1/wave/%%"}}}]))]);
        fixture.runtime.connected(mock.api.clone()).await;
        assert_eq!(
            fixture
                .runtime
                .artwork("station-cover:user:onyourwave")
                .await,
            Err(ErrorCode::Unavailable)
        );
        for id in [
            "station-cover:user:other",
            "station-cover:track:123:45",
            "station-cover:123",
            "station-cover:genre:rock",
            "station-cover:",
        ] {
            assert_eq!(
                fixture.runtime.artwork(id).await,
                Err(ErrorCode::NotFound),
                "{id}"
            );
        }
        let requests = mock.finish();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0]
                .0
                .starts_with("GET /rotor/station/user:onyourwave/info "),
            "{}",
            requests[0].0
        );
    }
    /// Codex review of #849: a full station memo makes room for one more
    /// station by dropping a single entry, never the whole memo.
    #[tokio::test]
    async fn a_full_station_memo_keeps_its_answers() {
        let mock = Mock::new(vec![response_with(json!([{"station":{
            "icon":{"imageUrl":"avatars.yandex.net/get-music-misc/1/x/%%"}}}]))]);
        {
            let mut known = mock.api.station_images.lock().unwrap();
            for n in 0..1024 {
                known.insert(format!("track:{n}"), None);
            }
        }
        assert!(
            mock.api
                .station_image("track:5000")
                .await
                .unwrap()
                .is_some()
        );
        {
            let known = mock.api.station_images.lock().unwrap();
            assert_eq!(known.len(), 1024);
            assert!(known.contains_key("track:5000"));
        }
        // A known station is answered from the memo, with no request.
        assert!(
            mock.api
                .station_image("track:5000")
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(mock.finish().len(), 1);
    }
    /// Codex review of #849: slow station lookups share one deadline, so
    /// twelve stalled stations cost about three seconds, not three each,
    /// and come back without artwork.
    #[tokio::test]
    async fn slow_station_covers_share_one_deadline() {
        // A service that accepts every request and never answers.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut api = Mock::new(vec![]).api.clone();
        api.origin =
            reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        listener.set_nonblocking(true).unwrap();
        let held = std::thread::spawn(move || {
            let mut sockets = Vec::new();
            let until = std::time::Instant::now() + Duration::from_secs(6);
            while std::time::Instant::now() < until {
                if let Ok((socket, _)) = listener.accept() {
                    sockets.push(socket);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let ids: Vec<String> = (1..=12).map(|n| format!("station:track:{n}")).collect();
        let started = std::time::Instant::now();
        let artworks = station_artworks(&api, &ids).await;
        assert!(artworks.is_empty());
        assert!(
            started.elapsed() < Duration::from_secs(6),
            "{:?}",
            started.elapsed()
        );
        held.join().unwrap();
    }
    /// #825: the epoch guard covers the new kinds: a reconnect while an
    /// artist cover is being resolved ends it with a conflict.
    #[tokio::test]
    async fn a_reconnect_ends_a_cover_in_flight() {
        let fixture = Fixture::new();
        // A service that accepts the request and never answers it.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut api = Mock::new(vec![]).api.clone();
        api.origin =
            reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let held = std::thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_secs(2));
            drop(socket);
        });
        fixture.runtime.connected(api).await;
        let next = Mock::new(vec![]);
        let (result, ()) = tokio::join!(fixture.runtime.artwork("artist-cover:9"), async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            fixture.runtime.connected(next.api.clone()).await;
        });
        assert_eq!(result, Err(ErrorCode::Conflict));
        held.join().unwrap();
        assert!(next.finish().is_empty());
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
    /// Review of #815: a stored station the service no longer knows must
    /// fail as NotFound, not as a transient outage.
    #[tokio::test]
    async fn a_vanished_station_fails_as_not_found() {
        let fixture = Fixture::new();
        let mock = Mock::new(vec![(404, "{}".into())]);
        fixture.runtime.connected(mock.api.clone()).await;
        let api = fixture.runtime.source.read().await.clone().unwrap();
        assert_eq!(
            fixture.runtime.start_radio(&api, "station:track:123").await,
            Err(ErrorCode::NotFound)
        );
        mock.finish();
    }
}
