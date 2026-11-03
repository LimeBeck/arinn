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
pub struct Local {
    library: Arc<StdMutex<Library>>,
    view: Arc<RwLock<View>>,
    playback: Arc<Playback>,
    scanning: AtomicBool,
    scan_revision: AtomicU64,
    shutdown: Arc<AtomicBool>,
    assets: Arc<tokio::sync::Semaphore>,
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
        let provider_id = provider_id.to_owned();
        let (library, view) = tokio::task::spawn_blocking(move || {
            let directory = paths::provider_state(&provider_id)?.join("music");
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
        let player = Player::new(&paths::runtime()?, PathBuf::from("mpv"))?;
        let playback = Playback::new(player, Arc::new(LocalResolver(view.clone())));
        Ok(Arc::new(Self {
            library,
            view,
            playback,
            scanning: AtomicBool::new(false),
            scan_revision: AtomicU64::new(0),
            shutdown: Arc::new(AtomicBool::new(false)),
            assets: Arc::new(tokio::sync::Semaphore::new(2)),
        }))
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
                    "albums" | "artists" => {
                        let mut groups = BTreeMap::new();
                        for t in &view.tracks {
                            let (id, title, kind) = if query.facet == "albums" {
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
                            });
                            e.available |= t.available;
                        }
                        groups.into_values().collect()
                    }
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
}
