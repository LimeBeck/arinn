// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{
    catalogue::Root,
    library::{Library, Playlist},
    probe::Ffprobe,
};
use arinn_music_common::{
    Track, paths,
    playback::{Playback, Resolver, entity},
    player::{Player, Source},
};
use arinn_plugin_sdk::runtime::{
    MusicCatalogue, MusicHandler, MusicService, SettingsHandler, run_music_on, serve_settings,
    startup_settings,
};
use arinn_protocol::{
    music::{Command, Entity, Query, State},
    settings::{Setting, Text, Translations},
    validation::ErrorCode,
};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, HashSet},
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tokio::sync::RwLock;

struct View {
    degraded: bool,
    revision: u64,
    tracks: Vec<Track>,
    playlists: Vec<Playlist>,
    catalogue: Arc<crate::catalogue::Catalogue>,
}
/// The station over the whole library (music Hearth light brief, *Stations*).
/// A fixed ID: it names this station after every provider start and never
/// another one, so the shell may pin it (ADR-0142 MH-6, #811).
pub const LIBRARY_STATION: &str = "station:library";
fn library_station(available: bool) -> Entity {
    let russian = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))
        .is_some_and(|locale| locale.starts_with("ru"));
    Entity {
        id: LIBRARY_STATION.into(),
        kind: "station".into(),
        title: if russian {
            "Вся моя музыка вперемешку"
        } else {
            "All my music, shuffled"
        }
        .into(),
        available,
        pinnable: true,
        ..Entity::default()
    }
}
/// Fisher–Yates with a xorshift generator; the seed comes from a random UUID.
fn shuffle<T>(items: &mut [T], mut seed: u64) {
    seed |= 1;
    for i in (1..items.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        items.swap(i, (seed % (i as u64 + 1)) as usize);
    }
}
pub struct Local {
    library: Arc<StdMutex<Library>>,
    view: Arc<RwLock<View>>,
    playback: Arc<Playback>,
    scanning: AtomicBool,
    scan_revision: AtomicU64,
    shutdown: Arc<AtomicBool>,
    assets: Arc<tokio::sync::Semaphore>,
    /// The library station's next tracks: the library shuffled, dealt in
    /// order and shuffled again once used up.
    deck: tokio::sync::Mutex<Vec<Track>>,
}
struct LocalResolver(Arc<RwLock<View>>);
impl Resolver for LocalResolver {
    fn resolve(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
        let view = self.0.clone();
        Box::pin(async move {
            let catalogue = view.read().await.catalogue.clone();
            tokio::task::spawn_blocking(move || {
                catalogue
                    .open_track(&id)
                    .map(Source::File)
                    .map_err(|_| ErrorCode::Unavailable)
            })
            .await
            .map_err(|_| ErrorCode::Internal)?
        })
    }
}
impl Local {
    pub async fn open(provider_id: &str, roots: Vec<PathBuf>) -> Result<Arc<Self>, ErrorCode> {
        let directory = paths::provider_state(provider_id)?.join("music");
        Self::open_in(directory, &paths::runtime()?, PathBuf::from("mpv"), roots).await
    }
    /// Opens the provider on its private state directory.
    pub async fn open_in(
        directory: PathBuf,
        runtime: &std::path::Path,
        player: PathBuf,
        roots: Vec<PathBuf>,
    ) -> Result<Arc<Self>, ErrorCode> {
        let (library, view) = tokio::task::spawn_blocking(move || {
            let mut library = Library::open(&directory).map_err(|_| ErrorCode::Unavailable)?;
            let configured = distinct_roots(roots)?
                .into_iter()
                .map(|path| {
                    let id = library
                        .roots()
                        .iter()
                        .find(|r| r.path == path)
                        .map(|r| r.id.clone())
                        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    Ok(Root { id, path })
                })
                .collect::<Result<Vec<_>, _>>()?;
            library
                .configure_roots(configured)
                .map_err(|_| ErrorCode::InvalidArgument)?;
            let view = View {
                degraded: !library.roots().is_empty(),
                revision: library.revision().max(1),
                tracks: library.tracks(),
                playlists: library.playlists(),
                catalogue: Arc::new(library.catalogue_snapshot()),
            };
            Ok::<_, ErrorCode>((library, view))
        })
        .await
        .map_err(|_| ErrorCode::Internal)??;
        let library = Arc::new(StdMutex::new(library));
        let view = Arc::new(RwLock::new(view));
        let player = Player::new(runtime, player)?;
        let playback = Playback::new(player, Arc::new(LocalResolver(view.clone())));
        let local = Arc::new(Self {
            library,
            view,
            playback,
            scanning: AtomicBool::new(false),
            scan_revision: AtomicU64::new(0),
            shutdown: Arc::new(AtomicBool::new(false)),
            assets: Arc::new(tokio::sync::Semaphore::new(2)),
            deck: tokio::sync::Mutex::new(Vec::new()),
        });
        // The library station never runs dry: like a service's station it
        // tops up its queue while it plays.
        let weak = Arc::downgrade(&local);
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(std::time::Duration::from_secs(1));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                timer.tick().await;
                let Some(local) = weak.upgrade() else {
                    return;
                };
                let state = local.playback.state().await;
                if state
                    .station
                    .as_ref()
                    .is_some_and(|s| s.id == LIBRARY_STATION)
                    && state.refill == "idle"
                    && state.upcoming.len() < 8
                {
                    let _ = local.refill_station().await;
                }
            }
        });
        Ok(local)
    }
    /// The next `count` tracks of the shuffled library.
    async fn deal(&self, count: usize) -> Vec<Track> {
        let available: Vec<Track> = self
            .view
            .read()
            .await
            .tracks
            .iter()
            .filter(|t| t.available)
            .cloned()
            .collect();
        if available.is_empty() {
            return vec![];
        }
        let mut deck = self.deck.lock().await;
        while deck.len() < count {
            let mut round = available.clone();
            shuffle(&mut round, uuid::Uuid::new_v4().as_u64_pair().0);
            deck.extend(round);
        }
        deck.drain(..count).collect()
    }
    /// Starts the library station; tracks of `seed` (a track, album,
    /// artist or playlist) play first.
    async fn start_station(&self, seed: &str) -> Result<(), ErrorCode> {
        let mut first: Vec<Track> = if seed == LIBRARY_STATION {
            vec![]
        } else {
            let tracks: Vec<Track> = self
                .selected(seed)
                .await
                .into_iter()
                .filter(|t| t.available)
                .take(32)
                .collect();
            if tracks.is_empty() {
                return Err(ErrorCode::NotFound);
            }
            tracks
        };
        self.deck.lock().await.clear();
        // The seed plays first and is not dealt again right after it.
        let seeded: HashSet<String> = first.iter().map(|t| t.id.clone()).collect();
        let dealt = self.deal(64 - first.len()).await;
        first.extend(dealt.into_iter().filter(|t| !seeded.contains(&t.id)));
        if first.is_empty() {
            return Err(ErrorCode::NotFound);
        }
        self.playback
            .radio(
                first,
                uuid::Uuid::new_v4().to_string(),
                library_station(true),
            )
            .await?;
        Ok(())
    }
    async fn refill_station(&self) -> Result<(), ErrorCode> {
        let request = self.playback.begin_radio_refill(false).await?;
        let tracks = self.deal(32).await;
        self.playback
            .apply_radio_refill(request, tracks, uuid::Uuid::new_v4().to_string())
            .await
    }
    pub fn rescan(self: &Arc<Self>) {
        if self.shutdown.load(Ordering::Acquire) || self.scanning.swap(true, Ordering::AcqRel) {
            return;
        }
        self.scan_revision.fetch_add(1, Ordering::AcqRel);
        let owner = self.clone();
        tokio::spawn(async move {
            let library = owner.library.clone();
            let shutdown = owner.shutdown.clone();
            let result = tokio::task::spawn_blocking(move || {
                let mut library = library.lock().map_err(|_| ErrorCode::Internal)?;
                // A scan whose snapshot could not be written is still
                // published from memory, marked degraded.
                let (statuses, persisted) = match library
                    .scan_cancelled(&Ffprobe::default(), &|| shutdown.load(Ordering::Acquire))
                {
                    Ok(statuses) => (statuses, true),
                    Err(_) => (None, false),
                };
                Ok::<_, ErrorCode>(View {
                    degraded: !persisted
                        || statuses.is_none_or(|statuses| {
                            statuses.iter().any(|s| !s.available || s.incomplete)
                        }),
                    revision: library.revision().max(1),
                    tracks: library.tracks(),
                    playlists: library.playlists(),
                    catalogue: Arc::new(library.catalogue_snapshot()),
                })
            })
            .await;
            if let Ok(Ok(view)) = result
                && !owner.shutdown.load(Ordering::Acquire)
            {
                owner.publish(view).await;
            } else if !owner.shutdown.load(Ordering::Acquire) {
                owner.view.write().await.degraded = true;
            }
            owner.scanning.store(false, Ordering::Release);
            owner.scan_revision.fetch_add(1, Ordering::AcqRel);
        });
    }
    async fn publish(&self, view: View) {
        let mut current = self.view.write().await;
        if view.revision >= current.revision {
            *current = view;
        }
    }
    async fn playlist(&self, command: &Command) -> Result<(), ErrorCode> {
        let tracks = if command.action == "save_playlist" {
            if command.entity.is_empty() {
                self.playback.remaining(&command.session).await?
            } else {
                self.selected(&command.entity).await
            }
        } else {
            vec![]
        };
        let degraded = self.view.read().await.degraded;
        let library = self.library.clone();
        let name = command.text.trim().to_owned();
        let id = command.entity.clone();
        let action = command.action.clone();
        let published = self.view.clone();
        tokio::spawn(async move {
            let view = tokio::task::spawn_blocking(move || {
                let mut library = library.try_lock().map_err(|_| ErrorCode::Unavailable)?;
                if action == "save_playlist" {
                    library
                        .create_playlist(&name, tracks.into_iter().map(|t| t.id).collect())
                        .map_err(|_| ErrorCode::InvalidArgument)?;
                } else {
                    library
                        .remove_playlist(&id)
                        .map_err(|_| ErrorCode::NotFound)?;
                }
                Ok::<_, ErrorCode>(View {
                    degraded,
                    revision: library.revision(),
                    tracks: library.tracks(),
                    playlists: library.playlists(),
                    catalogue: Arc::new(library.catalogue_snapshot()),
                })
            })
            .await
            .map_err(|_| ErrorCode::Internal)??;
            let mut current = published.write().await;
            if view.revision >= current.revision {
                *current = view;
            }
            Ok(())
        })
        .await
        .map_err(|_| ErrorCode::Internal)?
    }
    async fn selected(&self, id: &str) -> Vec<Track> {
        select(&*self.view.read().await, id)
    }
}
/// Configured roots as distinct normalized paths: `/m` and `/m/` are one
/// root, and a root inside another one is dropped because its files are
/// already scanned. A duplicate used to fail every provider start (review
/// of #710).
fn distinct_roots(roots: Vec<PathBuf>) -> Result<Vec<PathBuf>, ErrorCode> {
    let mut distinct: Vec<PathBuf> = Vec::new();
    for path in roots {
        if !paths::valid_root(&path) {
            return Err(ErrorCode::InvalidArgument);
        }
        let path: PathBuf = path.components().collect();
        if distinct.iter().any(|kept| path.starts_with(kept)) {
            continue;
        }
        distinct.retain(|kept| !kept.starts_with(&path));
        distinct.push(path);
    }
    Ok(distinct)
}
fn select(view: &View, id: &str) -> Vec<Track> {
    if let Some(playlist) = view.playlists.iter().find(|p| p.id == id) {
        let tracks: BTreeMap<&str, &Track> =
            view.tracks.iter().map(|t| (t.id.as_str(), t)).collect();
        return playlist
            .tracks
            .iter()
            .filter_map(|id| tracks.get(id.as_str()).map(|t| (*t).clone()))
            .collect();
    }
    view.tracks
        .iter()
        .filter(|t| {
            t.id == id
                || format!("album:{}", t.album_id) == id
                || paths::group("artist", &t.artist) == id
        })
        .cloned()
        .collect()
}
#[derive(Clone)]
struct LocalService(Arc<Local>);
impl std::ops::Deref for LocalService {
    type Target = Local;
    fn deref(&self) -> &Local {
        &self.0
    }
}
impl MusicHandler for LocalService {
    fn supports_assets(&self) -> bool {
        true
    }
    fn asset(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<arinn_protocol::assets::Asset, ErrorCode>> + Send + '_>>
    {
        Box::pin(async move {
            let _permit = self
                .assets
                .clone()
                .try_acquire_owned()
                .map_err(|_| ErrorCode::Unavailable)?;
            let catalogue = self.view.read().await.catalogue.clone();
            tokio::task::spawn_blocking(move || crate::artwork::load(&catalogue, &id))
                .await
                .map_err(|_| ErrorCode::Internal)?
        })
    }
    fn state(&self) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            let mut state = self.playback.state().await;
            state.catalogue_revision = self.view.read().await.revision;
            state.revision = state
                .revision
                .saturating_add(self.scan_revision.load(Ordering::Acquire))
                .saturating_add(state.catalogue_revision);
            state.actions.extend([
                "scan".into(),
                "save_playlist".into(),
                "delete_playlist".into(),
            ]);
            // The library station (#811).
            state.facets.push("selections".into());
            if self.view.read().await.tracks.iter().any(|t| t.available) {
                state.actions.push("radio".into());
            }
            let scanning = self.scanning.load(Ordering::Acquire);
            if scanning || self.view.read().await.degraded {
                state.backend = "degraded".into();
                if !scanning {
                    state.error = Some(ErrorCode::Unavailable);
                }
            }
            Ok(state)
        })
    }
    fn catalogue(
        &self,
        query: Query,
    ) -> Pin<Box<dyn Future<Output = Result<MusicCatalogue, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            let view = self.view.read().await;
            let search = query.search.to_lowercase();
            let mut entries: Vec<Entity> = if !query.parent.is_empty() {
                let tracks = select(&view, &query.parent);
                let mut seen = HashSet::new();
                return Ok(MusicCatalogue {
                    revision: view.revision,
                    entries: tracks
                        .iter()
                        .filter(|t| seen.insert(t.id.clone()))
                        .filter(|t| {
                            search.is_empty()
                                || format!("{} {} {}", t.title, t.artist, t.album)
                                    .to_lowercase()
                                    .contains(&search)
                        })
                        .map(entity)
                        .collect(),
                });
            } else {
                match query.facet.as_str() {
                    "tracks" => view.tracks.iter().map(entity).collect(),
                    "selections" => vec![library_station(view.tracks.iter().any(|t| t.available))],
                    "albums" | "artists" => groups(&view.tracks, &query.facet),
                    "playlists" => {
                        view.playlists
                            .iter()
                            .map(|p| Entity {
                                id: p.id.clone(),
                                kind: "playlist".into(),
                                title: p.title.clone(),
                                available: p.tracks.iter().any(|id| {
                                    view.tracks.iter().any(|t| &t.id == id && t.available)
                                }),
                                ..Entity::default()
                            })
                            .collect()
                    }
                    _ => return Err(ErrorCode::Unsupported),
                }
            };
            entries.retain(|e| {
                search.is_empty()
                    || format!("{} {} {}", e.title, e.artist, e.album)
                        .to_lowercase()
                        .contains(&search)
            });
            Ok(MusicCatalogue {
                revision: view.revision,
                entries,
            })
        })
    }
    fn command(
        &self,
        command: Command,
    ) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            match command.action.as_str() {
                "play" => {
                    self.playback
                        .play(self.selected(&command.entity).await)
                        .await?;
                }
                "scan" => self.0.rescan(),
                "radio" => self.start_station(&command.entity).await?,
                "retry"
                    if self
                        .playback
                        .state()
                        .await
                        .station
                        .is_some_and(|s| s.id == LIBRARY_STATION) =>
                {
                    self.refill_station().await?;
                }
                "save_playlist" | "delete_playlist" => self.playlist(&command).await?,
                _ => {
                    self.playback.command(&command).await?;
                }
            }
            self.state().await
        })
    }
    fn reset(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            let _ = self.playback.stop().await;
        })
    }
}
pub fn schema() -> Vec<Setting> {
    vec![Setting {
        id: "roots".into(),
        category: "media".into(),
        title: "Music folders".into(),
        description: "Folders with local audio. Removing a folder never deletes its files.".into(),
        kind: "path".into(),
        value: "[]".into(),
        constraints: r#"{"directory":true,"multiple":true,"max_items":64}"#.into(),
        enabled: true,
    }]
}
struct Settings;
impl SettingsHandler for Settings {
    fn schema(&self) -> Vec<Setting> {
        schema()
    }
    fn translations(&self) -> Translations {
        Translations::default().with(
            "ru",
            "roots",
            Text {
                title: Some("Папки с музыкой".into()),
                description: Some(
                    "Папки с аудиозаписями. Удаление папки из списка не удаляет файлы.".into(),
                ),
                ..Text::default()
            },
        )
    }
    fn apply(
        &self,
        values: Map<String, Value>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            let roots = values
                .get("roots")
                .and_then(Value::as_array)
                .ok_or(ErrorCode::InvalidArgument)?;
            if roots.iter().any(|p| {
                p.as_str()
                    .is_none_or(|s| !paths::valid_root(std::path::Path::new(s)))
            }) {
                return Err(ErrorCode::InvalidArgument);
            }
            Err(ErrorCode::Unsupported)
        })
    }
}
pub async fn serve(connection: zbus::Connection) -> Result<(), ErrorCode> {
    let id = std::env::var("ARINN_PROVIDER_ID").map_err(|_| ErrorCode::InvalidArgument)?;
    let values = startup_settings(&schema());
    let roots = values
        .get("roots")
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(Value::as_str)
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default();
    let local = Local::open(&id, roots).await?;
    let service = MusicService::new(LocalService(local.clone()));
    serve_settings(&connection, Settings, service.registrations())
        .await
        .map_err(|_| ErrorCode::Internal)?;
    local.rescan();
    let scan = Arc::downgrade(&local);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(600));
        tick.tick().await;
        loop {
            tick.tick().await;
            let Some(local) = scan.upgrade() else {
                return;
            };
            local.rescan();
        }
    });
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| ErrorCode::Unavailable)?;
    let result = tokio::select! {result=run_music_on(connection,service)=>result.map_err(|_|ErrorCode::Unavailable),_=terminate.recv()=>Ok(()),_=tokio::signal::ctrl_c()=>Ok(())};
    local.shutdown.store(true, Ordering::Release);
    let _ = local.playback.stop().await;
    result
}

/// Albums or artists built from tracks. A group is available when any of its
/// tracks is, and as new as its newest track (#806).
fn groups(tracks: &[Track], facet: &str) -> Vec<Entity> {
    let mut groups = BTreeMap::new();
    for t in tracks {
        let (id, title, kind) = if facet == "albums" {
            (format!("album:{}", t.album_id), t.album.clone(), "album")
        } else {
            (
                paths::group("artist", &t.artist),
                t.artist.clone(),
                "artist",
            )
        };
        let e = groups.entry(id.clone()).or_insert_with(|| Entity {
            id,
            kind: kind.into(),
            title,
            artist: if kind == "album" {
                if t.album_artist.is_empty() {
                    t.artist.clone()
                } else {
                    t.album_artist.clone()
                }
            } else {
                String::new()
            },
            album: String::new(),
            artwork: if kind == "album" {
                format!("cover:{}", t.id)
            } else {
                String::new()
            },
            duration_us: 0,
            available: false,
            added_at: 0,
            pinnable: false,
        });
        e.available |= t.available;
        e.added_at = e.added_at.max(t.added_at);
    }
    groups.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::{Error, Probe};
    struct Synthetic;
    impl Probe for Synthetic {
        fn inspect(&self, _path: &std::path::Path) -> Result<Track, Error> {
            Ok(Track {
                title: "Fixture track".into(),
                artist: "Fixture artist".into(),
                album: "Fixture album".into(),
                available: true,
                ..Track::default()
            })
        }
    }
    /// #806: an album or artist is as new as its newest track, and a group
    /// whose tracks have no date has none.
    #[test]
    fn groups_take_their_newest_track_date() {
        let track = |id: &str, album: &str, artist: &str, added_at: u64| Track {
            id: id.into(),
            album: album.into(),
            album_id: album.into(),
            artist: artist.into(),
            available: true,
            added_at,
            ..Track::default()
        };
        let tracks = [
            track("1", "A", "X", 1_700_000_000),
            track("2", "A", "Y", 1_750_000_000),
            track("3", "B", "Y", 0),
        ];
        let albums = groups(&tracks, "albums");
        let date = |entries: &[Entity], title: &str| {
            entries.iter().find(|e| e.title == title).unwrap().added_at
        };
        assert_eq!(date(&albums, "A"), 1_750_000_000);
        assert_eq!(date(&albums, "B"), 0);
        let artists = groups(&tracks, "artists");
        assert_eq!(date(&artists, "X"), 1_700_000_000);
        assert_eq!(date(&artists, "Y"), 1_750_000_000);
    }
    /// Review of #710: `/m` and `/m/` got the same root ID and every
    /// provider start failed; a nested root scanned its files twice.
    #[test]
    fn configured_roots_are_normalized_and_distinct() {
        let roots = distinct_roots(vec![
            PathBuf::from("/srv/music/"),
            PathBuf::from("/srv/music"),
            PathBuf::from("/srv/music/rock"),
            PathBuf::from("/data/./audio"),
            PathBuf::from("/data"),
        ])
        .unwrap();
        assert_eq!(roots, [PathBuf::from("/srv/music"), PathBuf::from("/data")]);
        assert_eq!(
            distinct_roots(vec![PathBuf::from("relative")]),
            Err(ErrorCode::InvalidArgument)
        );
    }
    #[tokio::test]
    async fn published_playback_snapshot_does_not_wait_for_library_scan_lock() {
        let directory =
            std::env::temp_dir().join(format!("arinn-local-snapshot-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let root = directory.join("audio");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("fixture.wav"), b"synthetic-probe-input").unwrap();
        let mut library = Library::open(&directory.join("library")).unwrap();
        library
            .configure_roots(vec![Root {
                id: "root".into(),
                path: root,
            }])
            .unwrap();
        library.scan(&Synthetic).unwrap();
        let tracks = library.tracks();
        let id = tracks[0].id.clone();
        let revision = library.revision();
        assert!(
            library
                .scan_cancelled(&Synthetic, &|| true)
                .unwrap()
                .is_none()
        );
        assert_eq!(library.revision(), revision);
        assert_eq!(library.tracks(), tracks);
        let view = Arc::new(RwLock::new(View {
            degraded: false,
            revision,
            tracks,
            playlists: vec![],
            catalogue: Arc::new(library.catalogue_snapshot()),
        }));
        let library = Arc::new(StdMutex::new(library));
        let locked = library.clone();
        let (ready, started) = std::sync::mpsc::channel();
        let (release, finish) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _guard = locked.lock().unwrap();
            ready.send(()).unwrap();
            finish.recv().unwrap();
        });
        started.recv().unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            LocalResolver(view).resolve(id),
        )
        .await;
        release.send(()).unwrap();
        worker.join().unwrap();
        assert!(matches!(result, Ok(Ok(Source::File(_)))));
        drop(library);
        std::fs::remove_dir_all(directory).unwrap();
    }
    /// A scanned synthetic library in a private directory.
    fn scanned(directory: &std::path::Path, files: &[&str]) -> PathBuf {
        let root = directory.join("audio");
        std::fs::create_dir_all(&root).unwrap();
        for file in files {
            std::fs::write(root.join(file), b"synthetic-probe-input").unwrap();
        }
        let mut library = Library::open(&directory.join("music")).unwrap();
        library
            .configure_roots(vec![Root {
                id: "root".into(),
                path: root.clone(),
            }])
            .unwrap();
        library.scan(&Synthetic).unwrap();
        root
    }
    /// The provider's rescan, with the synthetic probe.
    async fn rescanned(local: &Arc<Local>) {
        let view = {
            let mut library = local.library.lock().unwrap();
            library.scan(&Synthetic).unwrap();
            View {
                degraded: false,
                revision: library.revision().max(1),
                tracks: library.tracks(),
                playlists: library.playlists(),
                catalogue: Arc::new(library.catalogue_snapshot()),
            }
        };
        local.publish(view).await;
    }
    fn query(facet: &str) -> Query {
        Query {
            facet: facet.into(),
            parent: String::new(),
            search: String::new(),
            token: String::new(),
        }
    }
    /// #811: the library station has the same ID and is pinnable after
    /// every provider start, so a pin stored by the shell starts it again.
    #[tokio::test]
    async fn the_library_station_keeps_its_id_across_provider_starts() {
        let directory =
            std::env::temp_dir().join(format!("arinn-local-station-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let root = scanned(&directory, &["one.wav"]);
        let mut seen = vec![];
        for _ in 0..2 {
            let local = Local::open_in(
                directory.join("music"),
                &directory,
                PathBuf::from("/nonexistent-arinn-test-player"),
                vec![root.clone()],
            )
            .await
            .unwrap();
            rescanned(&local).await;
            let service = LocalService(local);
            let state = service.state().await.unwrap();
            assert!(state.facets.contains(&"selections".into()));
            assert!(state.actions.contains(&"radio".into()));
            let page = service.catalogue(query("selections")).await.unwrap();
            assert_eq!(page.entries.len(), 1);
            let station = page.entries[0].clone();
            assert!(station.pinnable && station.available && station.validate().is_ok());
            seen.push(station.id);
        }
        assert_eq!(seen, [LIBRARY_STATION, LIBRARY_STATION]);
        std::fs::remove_dir_all(directory).unwrap();
    }
    /// #811: `radio` starts the library station, seed first, as a
    /// continuous queue that the provider tops up.
    #[tokio::test]
    async fn the_library_station_plays_its_seed_first_and_refills() {
        let directory =
            std::env::temp_dir().join(format!("arinn-local-radio-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        // More tracks than one station batch, so a refill has new ones.
        let files: Vec<String> = (0..80).map(|i| format!("track-{i}.wav")).collect();
        let root = scanned(
            &directory,
            &files.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        let local = Local::open_in(
            directory.join("music"),
            &directory,
            PathBuf::from("/nonexistent-arinn-test-player"),
            vec![root],
        )
        .await
        .unwrap();
        rescanned(&local).await;
        let seed = local.view.read().await.tracks[1].id.clone();
        let service = LocalService(local.clone());
        let radio = |entity: &str| Command {
            request_id: uuid::Uuid::new_v4().to_string(),
            action: "radio".into(),
            session: String::new(),
            entity: entity.into(),
            occurrence: String::new(),
            position_us: 0,
            replaces_provider: String::new(),
            replaces_session: String::new(),
            text: String::new(),
        };
        let state = service.command(radio(&seed)).await.unwrap();
        let station = state.station.clone().unwrap();
        assert_eq!(station.id, LIBRARY_STATION);
        assert!(station.pinnable && state.continuous);
        assert_eq!(state.current.unwrap().track.id, seed);
        // The shuffled library, with no track twice.
        let queued: HashSet<_> = state.upcoming.iter().map(|e| e.track.id.clone()).collect();
        assert!(state.upcoming.len() >= 32);
        assert_eq!(queued.len(), state.upcoming.len());
        assert!(!queued.contains(&seed));
        let before = local.playback.state().await.upcoming.len();
        local.refill_station().await.unwrap();
        let after = local.playback.state().await;
        assert!(after.upcoming.len() > before);
        assert_eq!(after.refill, "idle");
        // An unknown seed starts nothing.
        assert_eq!(
            service.command(radio("track:missing")).await.err(),
            Some(ErrorCode::NotFound)
        );
        drop(service);
        drop(local);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
