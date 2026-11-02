// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native account, catalogue and playback. Credentials, stream links and
//! station identity stay in this process.
use crate::{
    Api, Batch, Feedback, RadioSession,
    auth::{self, CredentialStore, DeviceAuth},
    secrets::SecretService,
};
use arinn_music_common::{
    Track, paths,
    playback::{Playback, Resolver, Station, StationEvent, entity},
    player::{Player, Source},
    storage::Storage,
};
use arinn_plugin_sdk::runtime::{MusicCatalogue, MusicHandler, MusicService, run_music_on};
use arinn_protocol::{
    music::{Command, Entity, Prompt, Query, State},
    validation::ErrorCode,
};
use std::{
    collections::VecDeque,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, OnceLock},
};
use tokio::sync::Mutex;

/// The personal station every account has.
const MY_WAVE: &str = "user:onyourwave";
/// Facets of a connected account.
const FACETS: [&str; 3] = ["tracks", "playlists", "selections"];

struct Account {
    state: State,
    epoch: u64,
}
/// The API client of the connected account, for one account epoch.
struct Connected {
    epoch: u64,
    api: Arc<Api>,
    uid: String,
}
struct Service<S: CredentialStore + 'static = SecretService> {
    auth: Option<Arc<DeviceAuth<S>>>,
    account: Arc<Mutex<Account>>,
    connected: Arc<Mutex<Option<Connected>>>,
    /// The liked tracks last listed, so a liked track plays with the rest.
    liked: Arc<Mutex<Vec<Track>>>,
    playback: Arc<OnceLock<Arc<Playback>>>,
}
impl<S: CredentialStore + 'static> Clone for Service<S> {
    fn clone(&self) -> Self {
        Self {
            auth: self.auth.clone(),
            account: self.account.clone(),
            connected: self.connected.clone(),
            liked: self.liked.clone(),
            playback: self.playback.clone(),
        }
    }
}
fn api_error(error: crate::Error) -> ErrorCode {
    match error {
        crate::Error::Authentication | crate::Error::Forbidden => ErrorCode::PermissionDenied,
        crate::Error::InvalidInput => ErrorCode::InvalidArgument,
        crate::Error::InvalidResponse => ErrorCode::Internal,
        _ => ErrorCode::Unavailable,
    }
}
impl<S: CredentialStore + 'static> Service<S> {
    fn new(auth: Option<Arc<DeviceAuth<S>>>) -> Self {
        let configured = auth.is_some();
        Self {
            auth,
            connected: Arc::new(Mutex::new(None)),
            liked: Arc::new(Mutex::new(Vec::new())),
            playback: Arc::new(OnceLock::new()),
            account: Arc::new(Mutex::new(Account {
                epoch: 0,
                state: State {
                    backend: if configured {
                        "available"
                    } else {
                        "unavailable"
                    }
                    .into(),
                    account: if configured {
                        "disconnected"
                    } else {
                        "unconfigured"
                    }
                    .into(),
                    actions: if configured {
                        vec!["connect".into()]
                    } else {
                        vec![]
                    },
                    ..State::default()
                },
            })),
        }
    }
    async fn snapshot(&self) -> State {
        let account = self.account.lock().await.state.clone();
        let Some(playback) = self.playback.get() else {
            return account;
        };
        if account.account != "connected" {
            return account;
        }
        // A connected account shows its catalogue and its playback.
        let mut state = playback.state().await;
        state.revision = state.revision.saturating_add(account.revision);
        state.catalogue_revision = account.catalogue_revision;
        state.backend = account.backend;
        state.account = account.account;
        state.prompt = None;
        state.facets = FACETS.iter().map(|f| (*f).to_owned()).collect();
        state.actions.push("radio".into());
        state.actions.push("disconnect".into());
        if state.error.is_none() {
            state.error = account.error;
        }
        state
    }
    /// The API client of the connected account, built once per epoch.
    async fn client(&self) -> Result<(Arc<Api>, String), ErrorCode> {
        let auth = self.auth.as_ref().ok_or(ErrorCode::Unsupported)?;
        let epoch = {
            let a = self.account.lock().await;
            if a.state.account != "connected" {
                return Err(ErrorCode::PermissionDenied);
            }
            a.epoch
        };
        let mut connected = self.connected.lock().await;
        if let Some(c) = connected.as_ref().filter(|c| c.epoch == epoch) {
            return Ok((c.api.clone(), c.uid.clone()));
        }
        let api = Arc::new(auth.api().await.map_err(|_| ErrorCode::PermissionDenied)?);
        let uid = api.account_uid().await.map_err(api_error)?;
        *connected = Some(Connected {
            epoch,
            api: api.clone(),
            uid: uid.clone(),
        });
        Ok((api, uid))
    }
    /// Forgets the account's client and stops its playback.
    async fn forget(&self) {
        *self.connected.lock().await = None;
        self.liked.lock().await.clear();
        if let Some(playback) = self.playback.get() {
            let _ = playback.stop().await;
        }
    }
    async fn catalogue_entries(&self, query: &Query) -> Result<Vec<Entity>, ErrorCode> {
        let (api, uid) = self.client().await?;
        let search = query.search.trim().to_lowercase();
        let matches = |e: &Entity| {
            search.is_empty()
                || format!("{} {} {}", e.title, e.artist, e.album)
                    .to_lowercase()
                    .contains(&search)
        };
        Ok(match (query.facet.as_str(), query.parent.as_str()) {
            ("tracks", _) if !search.is_empty() => api
                .search_tracks(query.search.trim(), 0)
                .await
                .map_err(api_error)?
                .iter()
                .map(entity)
                .collect(),
            ("tracks", _) => {
                let ids = api.liked_track_ids(&uid).await.map_err(api_error)?;
                let tracks = api.tracks_batched(&ids).await.map_err(api_error)?;
                let entries = tracks.iter().map(entity).collect();
                *self.liked.lock().await = tracks;
                entries
            }
            ("playlists", "") => api
                .playlists(&uid)
                .await
                .map_err(api_error)?
                .into_iter()
                .map(|p| Entity {
                    id: format!("playlist:{}", p.id),
                    kind: "playlist".into(),
                    title: p.title,
                    available: p.track_count > 0,
                    ..Entity::default()
                })
                .filter(|e| matches(e))
                .collect(),
            ("playlists", parent) => {
                let kind = parent
                    .strip_prefix("playlist:")
                    .ok_or(ErrorCode::NotFound)?;
                api.playlist_tracks(&uid, kind)
                    .await
                    .map_err(api_error)?
                    .iter()
                    .map(entity)
                    .filter(|e| matches(e))
                    .collect()
            }
            ("selections", _) => [Entity {
                id: format!("station:{MY_WAVE}"),
                kind: "station".into(),
                title: "My Wave".into(),
                available: true,
                ..Entity::default()
            }]
            .into_iter()
            .filter(|e| matches(e))
            .collect(),
            _ => return Err(ErrorCode::Unsupported),
        })
    }
    /// Starts what an entity names: a station, a playlist or a track (a
    /// liked track plays with the liked tracks after it).
    async fn play(&self, id: &str) -> Result<(), ErrorCode> {
        let playback = self.playback.get().ok_or(ErrorCode::Unavailable)?.clone();
        if let Some(seed) = id.strip_prefix("station:") {
            return self.radio(seed).await;
        }
        let (api, uid) = self.client().await?;
        let tracks = if let Some(kind) = id.strip_prefix("playlist:") {
            api.playlist_tracks(&uid, kind).await.map_err(api_error)?
        } else {
            let liked = self.liked.lock().await.clone();
            match liked.iter().position(|t| t.id == id) {
                Some(index) => liked[index..].to_vec(),
                None => api.tracks(&[id.to_owned()]).await.map_err(api_error)?,
            }
        };
        let tracks: Vec<Track> = tracks.into_iter().filter(|t| t.available).collect();
        if tracks.is_empty() {
            return Err(ErrorCode::NotFound);
        }
        playback.play(tracks).await.map(|_| ())
    }
    /// Starts a station: My Wave, or a radio seeded by a track.
    async fn radio(&self, seed: &str) -> Result<(), ErrorCode> {
        let playback = self.playback.get().ok_or(ErrorCode::Unavailable)?.clone();
        let (api, _) = self.client().await?;
        let seed = if seed == MY_WAVE || seed.starts_with("track:") {
            seed.to_owned()
        } else {
            format!("track:{seed}")
        };
        let session = api.start_radio(&seed).await.map_err(api_error)?;
        let first: Vec<Track> = session.batch.tracks.clone();
        if first.is_empty() {
            return Err(ErrorCode::NotFound);
        }
        let station = Arc::new(YandexStation {
            api,
            batches: std::sync::Mutex::new(VecDeque::from([session.batch.clone()])),
            session,
        });
        playback
            .play_station(seed, first, station)
            .await
            .map(|_| ())
    }
    /// An attempt can be accepted in the instant before a Cancel, reset or
    /// newer state change hides it (review of #710). Whenever the account is
    /// not shown as connected or connecting, an accepted session is removed
    /// again, so the UI never says "disconnected" over a stored token.
    async fn reconcile(&self) {
        let Some(auth) = &self.auth else {
            return;
        };
        let shown = self.account.lock().await.state.account.clone();
        if shown == "connected" || shown == "connecting" || auth.api().await.is_err() {
            return;
        }
        if let Err(error) = auth.disconnect().await {
            let mut a = self.account.lock().await;
            a.state.error = Some(normalize(error));
            a.state.revision += 1;
        }
    }
    async fn restore(&self, start: Option<auth::Start>) {
        let Some(auth) = &self.auth else {
            return;
        };
        let Some(start) = start else {
            return;
        };
        let result = auth.restore_prepared(start).await;
        let mut account = self.account.lock().await;
        if account.epoch != 0 {
            drop(account);
            if result == Ok(true) {
                self.reconcile().await;
            }
            return;
        }
        match result {
            Ok(true) => {
                account.state.account = "connected".into();
                account.state.actions = vec!["disconnect".into()];
                account.state.catalogue_revision += 1;
            }
            Ok(false) => {}
            Err(error) => account.state.error = Some(normalize(error)),
        }
        account.state.revision += 1;
    }
    async fn connect(&self) -> Result<(), ErrorCode> {
        let auth = self.auth.clone().ok_or(ErrorCode::Unsupported)?;
        let (epoch, start) = {
            let mut a = self.account.lock().await;
            if a.state.account == "connected" {
                return Err(ErrorCode::Conflict);
            }
            auth.cancel();
            a.epoch += 1;
            a.state.revision += 1;
            a.state.account = "connecting".into();
            a.state.prompt = None;
            a.state.actions = vec!["cancel".into()];
            a.state.error = None;
            (a.epoch, auth.prepare())
        };
        let this = self.clone();
        tokio::spawn(async move {
            let result = async {
                let attempt = auth.start_prepared(start).await?;
                {
                    let mut a = this.account.lock().await;
                    if a.epoch != epoch {
                        return Err(auth::Error::Cancelled);
                    }
                    a.state.prompt = Some(Prompt {
                        verification_url: attempt.prompt.verification_url.clone(),
                        user_code: attempt.prompt.user_code.clone(),
                        expires_in: attempt.prompt.expires_in,
                    });
                    a.state.revision += 1;
                }
                auth.complete(attempt).await
            }
            .await;
            let mut a = this.account.lock().await;
            if a.epoch != epoch {
                drop(a);
                if result.is_ok() {
                    this.reconcile().await;
                }
                return;
            }
            a.state.prompt = None;
            a.state.revision += 1;
            match result {
                Ok(()) => {
                    a.state.account = "connected".into();
                    a.state.actions = vec!["disconnect".into()];
                    a.state.error = None;
                    a.state.catalogue_revision += 1;
                }
                Err(error) => {
                    a.state.account = if error == auth::Error::Expired {
                        "expired"
                    } else {
                        "disconnected"
                    }
                    .into();
                    a.state.actions = vec!["connect".into()];
                    a.state.error = Some(normalize(error));
                }
            }
            // A failed newer attempt must not leave an older accepted one.
            drop(a);
            this.reconcile().await;
        });
        Ok(())
    }
    async fn cancel(&self) {
        let mut a = self.account.lock().await;
        if a.state.account != "connecting" {
            return;
        }
        if let Some(auth) = &self.auth {
            auth.cancel();
        }
        a.epoch += 1;
        a.state.revision += 1;
        a.state.prompt = None;
        a.state.account = "disconnected".into();
        a.state.actions = vec!["connect".into()];
        a.state.error = None;
    }
}
/// Resolves tracks to stream links and records ratings for the account.
struct YandexResolver<S: CredentialStore + 'static>(Service<S>);
impl<S: CredentialStore + 'static> Resolver for YandexResolver<S> {
    fn resolve(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            let (api, _) = self.0.client().await?;
            let url = api.stream_url(&id).await.map_err(api_error)?;
            Ok(Source::Stream {
                url,
                headers: vec![],
            })
        })
    }
    fn rates(&self) -> bool {
        true
    }
    fn rate(
        &self,
        id: String,
        like: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            let (api, uid) = self.0.client().await?;
            api.rate(&uid, &id, like).await.map_err(api_error)
        })
    }
}

/// A Yandex radio session. Feedback names the batch a track came from.
struct YandexStation {
    api: Arc<Api>,
    session: RadioSession,
    /// The last batches received, newest last.
    batches: std::sync::Mutex<VecDeque<Batch>>,
}
const KEPT_BATCHES: usize = 8;
impl Station for YandexStation {
    fn next(
        &self,
        after: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Track>, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            let after = after.ok_or(ErrorCode::NotFound)?;
            let batch = self
                .api
                .next_batch(&self.session, &after)
                .await
                .map_err(api_error)?;
            let tracks = batch.tracks.clone();
            let mut batches = self.batches.lock().map_err(|_| ErrorCode::Internal)?;
            batches.push_back(batch);
            while batches.len() > KEPT_BATCHES {
                batches.pop_front();
            }
            Ok(tracks)
        })
    }
    fn feedback(
        &self,
        event: StationEvent,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            let track = match &event {
                StationEvent::Started(t)
                | StationEvent::Finished(t, _)
                | StationEvent::Skipped(t, _) => t.clone(),
            };
            let batch = {
                let batches = self.batches.lock().map_err(|_| ErrorCode::Internal)?;
                batches
                    .iter()
                    .rev()
                    .find(|b| b.tracks.iter().any(|t| t.id == track))
                    .cloned()
                    .ok_or(ErrorCode::NotFound)?
            };
            let feedback = match &event {
                StationEvent::Started(t) => Feedback::TrackStarted(t),
                StationEvent::Finished(t, us) => Feedback::TrackFinished(t, us / 1000),
                StationEvent::Skipped(t, us) => Feedback::Skip(t, us / 1000),
            };
            self.api
                .feedback(&self.session, &batch, feedback)
                .await
                .map_err(api_error)
        })
    }
}

fn normalize(error: auth::Error) -> ErrorCode {
    match error {
        auth::Error::Denied | auth::Error::Locked => ErrorCode::PermissionDenied,
        auth::Error::InvalidInput | auth::Error::ClientRejected => ErrorCode::InvalidArgument,
        auth::Error::Cancelled => ErrorCode::Conflict,
        _ => ErrorCode::Unavailable,
    }
}
impl<S: CredentialStore + 'static> MusicHandler for Service<S> {
    fn state(&self) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>> {
        Box::pin(async move { Ok(self.snapshot().await) })
    }
    fn catalogue(
        &self,
        query: Query,
    ) -> Pin<Box<dyn Future<Output = Result<MusicCatalogue, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            let entries = self.catalogue_entries(&query).await?;
            let revision = self.account.lock().await.state.catalogue_revision.max(1);
            Ok(MusicCatalogue { revision, entries })
        })
    }
    fn command(
        &self,
        command: Command,
    ) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            match command.action.as_str() {
                "connect" => self.connect().await?,
                "cancel" => self.cancel().await,
                "disconnect" => {
                    let auth = self.auth.as_ref().ok_or(ErrorCode::Unsupported)?;
                    {
                        let mut a = self.account.lock().await;
                        auth.cancel();
                        a.epoch += 1;
                        a.state.prompt = None;
                        a.state.account = "disconnected".into();
                        a.state.actions = vec!["connect".into()];
                        a.state.revision += 1;
                    }
                    self.forget().await;
                    if let Err(error) = auth.disconnect().await {
                        let mut a = self.account.lock().await;
                        a.state.error = Some(normalize(error));
                        a.state.revision += 1;
                        return Err(normalize(error));
                    }
                }
                "play" => self.play(&command.entity).await?,
                // The protocol requires the entity: a track or a station.
                "radio" => {
                    let seed = &command.entity;
                    self.radio(seed.strip_prefix("station:").unwrap_or(seed))
                        .await?;
                }
                _ => {
                    let playback = self.playback.get().ok_or(ErrorCode::Unsupported)?;
                    if self.account.lock().await.state.account != "connected" {
                        return Err(ErrorCode::PermissionDenied);
                    }
                    playback.command(&command).await?;
                }
            }
            Ok(self.snapshot().await)
        })
    }
    fn reset(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.cancel().await;
        })
    }
}

/// Yandex Music Android client identity (upstream yandex-music-api).
const DEFAULT_CLIENT_ID: &str = "23cabbbdc6cd418abb4b39c32c41195d";
const DEFAULT_CLIENT_SECRET: &str = "53bc75238f0c4d08a118e51fe9203300";

pub async fn serve(connection: zbus::Connection) -> Result<(), ErrorCode> {
    let provider = std::env::var("ARINN_PROVIDER_ID").map_err(|_| ErrorCode::InvalidArgument)?;
    // The OAuth client identity of Yandex Music's Android app, as published
    // by the upstream yandex-music-api library, is bundled by owner decision
    // (ADR-0134, 2026-11-02). A deployment may set its own identity with
    // ARINN_YANDEX_CLIENT_ID and ARINN_YANDEX_CLIENT_SECRET.
    let configured = |name: &str| {
        std::env::var(name)
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    let (client_id, client_secret) = match configured("ARINN_YANDEX_CLIENT_ID") {
        Some(id) => (id, configured("ARINN_YANDEX_CLIENT_SECRET")),
        None => (
            DEFAULT_CLIENT_ID.to_owned(),
            Some(DEFAULT_CLIENT_SECRET.to_owned()),
        ),
    };
    let client_id = Some(client_id);
    let bus = connection.clone();
    let auth = tokio::task::spawn_blocking(move || {
        let Some(client_id) = client_id else {
            return Ok(None);
        };
        let directory = paths::provider_state(&provider)?;
        let identity =
            Storage::open(&directory.join("oauth-device")).map_err(|_| ErrorCode::Unavailable)?;
        let device = match identity.read().map_err(|_| ErrorCode::Unavailable)? {
            Some(bytes) => String::from_utf8(bytes).map_err(|_| ErrorCode::Unavailable)?,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                identity
                    .write(id.as_bytes())
                    .map_err(|_| ErrorCode::Unavailable)?;
                id
            }
        };
        let reference = paths::group("account", &provider).replace(':', "-");
        let store = SecretService::new(bus, &reference, &directory.join("oauth-approval"))
            .map_err(normalize)?;
        DeviceAuth::new(&client_id, client_secret.as_deref(), &device, store)
            .map(|auth| Some(Arc::new(auth)))
            .map_err(normalize)
    })
    .await
    .map_err(|_| ErrorCode::Internal)?;
    // Invalid local configuration or state is reported, not fatal: the
    // provider still registers and shows why it cannot connect (review of
    // #710). A corrupt approval journal is still never reset silently.
    let (auth, failure) = match auth {
        Ok(auth) => (auth, None),
        Err(error) => (None, Some(error)),
    };
    let handler: Service = Service::new(auth);
    if let Some(error) = failure {
        let mut a = handler.account.lock().await;
        a.state.backend = "unavailable".into();
        a.state.error = Some(error);
        a.state.revision += 1;
    }
    // An audio-only player owned by this provider, like the local one.
    let player = Player::new(&paths::runtime()?, PathBuf::from("mpv"))?;
    let _ = handler.playback.set(Playback::new(
        player,
        Arc::new(YandexResolver(handler.clone())),
    ));
    let restore_start = handler.auth.as_ref().map(|auth| auth.prepare());
    let restore = handler.clone();
    tokio::spawn(async move {
        restore.restore(restore_start).await;
    });
    let service = MusicService::new(handler.clone());
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| ErrorCode::Unavailable)?;
    let result = tokio::select! {result=run_music_on(connection,service)=>result.map_err(|_|ErrorCode::Unavailable),_=terminate.recv()=>Ok(()),_=tokio::signal::ctrl_c()=>Ok(())};
    handler.cancel().await;
    if let Some(playback) = handler.playback.get() {
        let _ = playback.stop().await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn missing_client_identity_has_no_connect_action_or_secret_prompt() {
        let service: Service = Service::new(None);
        service.restore(None).await;
        service.cancel().await;
        let state = service.snapshot().await;
        assert_eq!(state.account, "unconfigured");
        assert!(state.actions.is_empty());
        assert!(state.prompt.is_none());
        assert!(state.validate().is_ok());
        assert_eq!(service.connect().await, Err(ErrorCode::Unsupported));
    }
    /// Review of #710: an attempt accepted in the instant before Cancel left
    /// the token stored and the session active while the screen said
    /// "disconnected"; later connects then failed. The hidden session is now
    /// removed again.
    #[tokio::test]
    async fn an_accepted_session_hidden_by_a_cancel_is_removed_again() {
        use crate::auth::tests::{MemoryStore, Mock, code, reply, token};
        let mock = Mock::new(vec![reply(200, code()), reply(200, token())]);
        let store = MemoryStore::default();
        let service = Service::new(Some(Arc::new(mock.client(store.clone()))));
        service.connect().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while service.snapshot().await.account != "connected" {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let auth = service.auth.clone().unwrap();
        assert!(auth.api().await.is_ok());
        assert!(store.bytes.lock().unwrap().is_some());
        // What a Cancel landing just after acceptance used to leave behind.
        {
            let mut a = service.account.lock().await;
            a.epoch += 1;
            a.state.account = "disconnected".into();
            a.state.actions = vec!["connect".into()];
        }
        service.reconcile().await;
        assert!(auth.api().await.is_err());
        assert!(store.bytes.lock().unwrap().is_none());
        // A shown connection is left alone.
        let state = service.snapshot().await;
        assert_eq!(state.account, "disconnected");
        mock.finish();
    }

    use crate::tests::{Mock as ApiMock, Recorder, response};
    use serde_json::json;
    /// A service whose account is connected to a loopback API.
    async fn connected(
        responses: Vec<(u16, String)>,
    ) -> (Service<crate::auth::tests::MemoryStore>, Recorder) {
        use crate::auth::tests::{MemoryStore, Mock};
        let auth = Mock::new(vec![]);
        let service = Service::new(Some(Arc::new(auth.client(MemoryStore::default()))));
        auth.finish();
        let (api, recorder) = ApiMock::new(responses).split();
        let epoch = {
            let mut a = service.account.lock().await;
            a.state.account = "connected".into();
            a.state.actions = vec!["disconnect".into()];
            a.epoch
        };
        *service.connected.lock().await = Some(Connected {
            epoch,
            api,
            uid: "1001".into(),
        });
        (service, recorder)
    }
    fn query(facet: &str, parent: &str) -> Query {
        Query {
            facet: facet.into(),
            parent: parent.into(),
            search: String::new(),
            token: String::new(),
        }
    }
    fn synthetic(id: u64, album: u64) -> serde_json::Value {
        json!({"id":id,"title":format!("Synthetic {id}"),"available":true,"albums":[{"id":album}]})
    }
    /// The connected account lists its liked tracks, its playlists and their
    /// tracks, and My Wave; a disconnected one lists nothing.
    #[tokio::test]
    async fn a_connected_account_has_a_catalogue() {
        let (service, recorder) = connected(vec![
            response(json!({"library":{"tracks":[
                {"id":"11","albumId":"21"},{"id":"12","albumId":"22"}]}})),
            response(json!([synthetic(11, 21), synthetic(12, 22)])),
            response(json!([{"kind":3,"title":"Synthetic list","trackCount":1}])),
            response(json!({"tracks":[{"track":synthetic(13, 23)}]})),
        ])
        .await;
        let tracks = service.catalogue(query("tracks", "")).await.unwrap();
        let ids: Vec<_> = tracks.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["11:21", "12:22"]);
        let lists = service.catalogue(query("playlists", "")).await.unwrap();
        assert_eq!(lists.entries[0].id, "playlist:3");
        assert_eq!(lists.entries[0].kind, "playlist");
        let list = service
            .catalogue(query("playlists", "playlist:3"))
            .await
            .unwrap();
        assert_eq!(list.entries[0].id, "13:23");
        let selections = service.catalogue(query("selections", "")).await.unwrap();
        assert_eq!(selections.entries[0].id, "station:user:onyourwave");
        assert!(selections.entries[0].available);
        for entry in [&tracks, &lists, &list, &selections]
            .iter()
            .flat_map(|c| c.entries.iter())
        {
            assert!(entry.validate().is_ok());
        }
        let requests = recorder.finish();
        assert!(requests[0].0.starts_with("GET /users/1001/likes/tracks "));
        assert!(requests[1].0.starts_with("POST /tracks "));
        assert!(requests[2].0.starts_with("GET /users/1001/playlists/list "));
        assert!(requests[3].0.starts_with("GET /users/1001/playlists/3 "));
        assert_eq!(
            service
                .catalogue(query("playlists", "../secret"))
                .await
                .err(),
            Some(ErrorCode::NotFound)
        );
        service.account.lock().await.state.account = "disconnected".into();
        assert_eq!(
            service.catalogue(query("tracks", "")).await.err(),
            Some(ErrorCode::PermissionDenied)
        );
    }
    /// Playing a liked track plays it and the liked tracks after it, and the
    /// connected state shows its facets and the radio action.
    #[tokio::test]
    async fn a_liked_track_plays_with_the_liked_tracks_after_it() {
        let (service, recorder) = connected(vec![
            response(json!({"library":{"tracks":[
                {"id":"11","albumId":"21"},{"id":"12","albumId":"22"},
                {"id":"14","albumId":"24"}]}})),
            response(json!([
                synthetic(11, 21),
                synthetic(12, 22),
                synthetic(14, 24)
            ])),
        ])
        .await;
        let directory = std::env::temp_dir().join(format!(
            "ay-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&directory)
                .unwrap();
        }
        let player = Player::new(&directory, directory.join("missing-player")).unwrap();
        let _ = service.playback.set(Playback::new(
            player,
            Arc::new(YandexResolver(service.clone())),
        ));
        service.catalogue(query("tracks", "")).await.unwrap();
        recorder.finish();
        let state = service
            .command(Command {
                request_id: "r1".into(),
                action: "play".into(),
                session: String::new(),
                entity: "12:22".into(),
                occurrence: String::new(),
                position_us: 0,
                replaces_provider: String::new(),
                replaces_session: String::new(),
            })
            .await
            .unwrap();
        assert_eq!(state.current.as_ref().unwrap().track.id, "12:22");
        let upcoming: Vec<_> = state.upcoming.iter().map(|e| e.track.id.as_str()).collect();
        assert_eq!(upcoming, ["14:24"]);
        assert_eq!(state.facets, FACETS);
        assert!(state.actions.contains(&"radio".to_owned()));
        assert!(state.actions.contains(&"disconnect".to_owned()));
        assert_eq!(state.account, "connected");
        assert!(state.validate().is_ok());
        service.playback.get().unwrap().stop().await.unwrap();
        let _ = std::fs::remove_dir_all(&directory);
    }
    /// A like and a dislike are recorded with the account.
    #[tokio::test]
    async fn ratings_are_recorded_with_the_account() {
        let (service, recorder) = connected(vec![
            response(json!({"revision":2})),
            response(json!({"revision":3})),
        ])
        .await;
        let resolver = YandexResolver(service.clone());
        assert!(resolver.rates());
        resolver.rate("12:22".into(), true).await.unwrap();
        resolver.rate("12:22".into(), false).await.unwrap();
        let requests = recorder.finish();
        assert!(
            requests[0]
                .0
                .starts_with("POST /users/1001/likes/tracks/add-multiple ")
        );
        assert!(
            requests[1]
                .0
                .starts_with("POST /users/1001/dislikes/tracks/add-multiple ")
        );
        assert_eq!(
            resolver.rate("../1".into(), true).await,
            Err(ErrorCode::InvalidArgument)
        );
    }
    /// Station feedback names the batch the track came from, in
    /// milliseconds, and the station asks for more after the given track.
    #[tokio::test]
    async fn station_feedback_names_the_batch_of_its_track() {
        let mock = ApiMock::new(vec![
            response(json!({"radioSessionId":"session-a","batchId":"batch-a",
                "sequence":[{"type":"track","track":synthetic(11, 21)}]})),
            response(json!(true)),
            response(json!({"batchId":"batch-b",
                "sequence":[{"type":"track","track":synthetic(12, 22)}]})),
            response(json!(true)),
            response(json!(true)),
        ]);
        let (api, recorder) = mock.split();
        let session = api.start_radio("user:onyourwave").await.unwrap();
        let station = YandexStation {
            api,
            batches: std::sync::Mutex::new(VecDeque::from([session.batch.clone()])),
            session,
        };
        let more = station.next(Some("11:21".into())).await.unwrap();
        assert_eq!(more[0].id, "12:22");
        station
            .feedback(StationEvent::Finished("11:21".into(), 2_500_000))
            .await
            .unwrap();
        station
            .feedback(StationEvent::Skipped("12:22".into(), 1_000_000))
            .await
            .unwrap();
        assert_eq!(
            station
                .feedback(StationEvent::Started("99:99".into()))
                .await,
            Err(ErrorCode::NotFound)
        );
        let requests = recorder.finish();
        assert_eq!(requests[2].1["queue"], json!(["11"]));
        assert_eq!(requests[3].1["batch_id"], "batch-a");
        assert_eq!(requests[3].1["event"]["totalPlayedSeconds"], 2.5);
        assert_eq!(requests[4].1["batch_id"], "batch-b");
        assert_eq!(requests[4].1["event"]["type"], "skip");
    }
}
