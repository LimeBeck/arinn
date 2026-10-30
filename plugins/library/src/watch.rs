// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Serves `LibraryProvider1`, `LibraryResume1`, `LibraryImport1`,
//! `LibraryRoots1` and `ProviderSettings1`. Scanning runs beside the snapshot
//! and does not hold the D-Bus reply path.

use crate::store::{Settings, Store};
use arinn_plugin_sdk::runtime::{
    LibraryExtensions, LibraryHandler, LibraryImportHandler, LibraryResumeHandler, LibraryService,
    SettingsHandler, run_library_on, serve_settings,
};
use arinn_protocol::{
    library::{Addition, Collection, ImportCandidate, Item, Resume, RootState},
    media::PlaybackSource,
    settings::Setting,
    validation::ErrorCode,
};
use serde_json::{Map, Value};
use std::{
    collections::HashSet,
    future::Future,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::unix::AsyncFd,
    sync::{Mutex, Notify},
};

#[derive(Clone)]
struct Bridge {
    store: Arc<std::sync::Mutex<Store>>,
    service: Arc<Mutex<Option<LibraryService>>>,
    publication: Arc<Mutex<()>>,
    settings: Arc<std::sync::Mutex<Settings>>,
    /// The configured roots as written, with the path the scan uses.
    written: Arc<Vec<(String, PathBuf)>>,
}

// A snapshot and its publication permit travel together, so no producer can
// publish an older snapshot after a newer one. Slow discovery stays outside.
struct Publication {
    _permit: tokio::sync::OwnedMutexGuard<()>,
    collections: Vec<Collection>,
    items: Vec<Item>,
    resumes: Vec<Resume>,
    additions: Vec<Addition>,
}

pub async fn serve(connection: zbus::Connection) -> Result<(), ErrorCode> {
    let provider_id = std::env::var("ARINN_PROVIDER_ID").unwrap_or_else(|_| "library".into());
    let database = crate::paths::state_database(&provider_id)?;
    let text = match std::env::var("ARINN_PROVIDER_SETTINGS") {
        Ok(text) => Some(text),
        Err(std::env::VarError::NotPresent) => Some("{}".into()),
        Err(std::env::VarError::NotUnicode(_)) => None,
    };
    let loaded = text
        .as_deref()
        .ok_or(ErrorCode::InvalidArgument)
        .and_then(Settings::parse);
    let backend_ok = loaded.is_ok();
    let settings = loaded.unwrap_or_default();
    // Settings that do not load leave no roots to scan or report.
    let written = match (&text, backend_ok) {
        (Some(text), true) => crate::store::written_roots(text),
        _ => Vec::new(),
    };
    let store = Store::open(&database)?;
    let bridge = Bridge {
        store: Arc::new(std::sync::Mutex::new(store)),
        service: Arc::new(Mutex::new(None)),
        publication: Arc::new(Mutex::new(())),
        settings: Arc::new(std::sync::Mutex::new(settings)),
        written: Arc::new(written),
    };
    let (collections, items, resumes, additions) = {
        let store = bridge.store.lock().map_err(|_| ErrorCode::Internal)?;
        let (collections, items) = store.snapshot()?;
        (collections, items, store.resumes()?, store.additions()?)
    };
    let service = LibraryService::with_extensions(
        bridge.clone(),
        collections.clone(),
        items.clone(),
        if backend_ok {
            "available"
        } else {
            "unavailable"
        },
        LibraryExtensions::default()
            .resume(bridge.clone())
            .import(bridge.clone())
            .roots()
            .additions(),
    )?;
    service
        .replace_with_additions(
            collections,
            items,
            if backend_ok {
                "available"
            } else {
                "unavailable"
            },
            additions,
        )
        .await?;
    *bridge.service.lock().await = Some(service.clone());
    service.replace_resumes(resumes).await?;
    serve_settings(&connection, bridge.clone(), service.registrations())
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    let scanner = bridge.clone();
    let running = tokio::spawn(async move {
        if backend_ok {
            let _ = scanner.scan_and_publish().await;
        }
        scanner.scan_loop().await
    });
    tokio::select! {
        result = running => result.map_err(|_| ErrorCode::Internal)?,
        result = run_library_on(connection, service) => result.map_err(|_| ErrorCode::Unavailable),
    }
}

impl Bridge {
    fn roots(&self) -> Vec<PathBuf> {
        self.settings
            .lock()
            .map(|settings| settings.roots.clone())
            .unwrap_or_default()
    }

    async fn read_publication(&self) -> Result<Publication, ErrorCode> {
        let permit = self.publication.clone().lock_owned().await;
        let store = self.store.clone();
        let (collections, items, resumes, additions) = tokio::task::spawn_blocking(move || {
            let store = store.lock().map_err(|_| ErrorCode::Internal)?;
            let (collections, items) = store.snapshot()?;
            Ok::<_, ErrorCode>((collections, items, store.resumes()?, store.additions()?))
        })
        .await
        .map_err(|_| ErrorCode::Internal)??;
        Ok(Publication {
            _permit: permit,
            collections,
            items,
            resumes,
            additions,
        })
    }

    async fn publish_current(&self) -> Result<LibraryService, ErrorCode> {
        let Publication {
            _permit,
            collections,
            items,
            resumes,
            additions,
        } = self.read_publication().await?;
        let service = self
            .service
            .lock()
            .await
            .clone()
            .ok_or(ErrorCode::Unavailable)?;
        service
            .replace_with_additions(collections, items, "available", additions)
            .await?;
        service.replace_resumes(resumes).await?;
        Ok(service)
    }

    async fn scan_and_publish(&self) -> Result<(), ErrorCode> {
        let store = self.store.clone();
        let settings = self.settings.clone();
        let scanned = tokio::task::spawn_blocking(move || {
            let settings = settings.lock().map_err(|_| ErrorCode::Internal)?.clone();
            crate::store::scan_shared(&store, &settings)
        })
        .await
        .map_err(|_| ErrorCode::Internal)?;
        let readable = match scanned {
            Ok(readable) => readable,
            Err(error) => {
                log_error("scan", error);
                return Ok(());
            }
        };
        let service = self.publish_current().await?;
        // Every configured root as written, with what this scan found (ADR-0071).
        if let Err(error) = service.replace_roots(self.root_states(&readable)).await {
            log_error("roots", error);
        }
        Ok(())
    }

    fn root_states(&self, readable: &[(PathBuf, bool)]) -> Vec<RootState> {
        self.written
            .iter()
            .map(|(text, path)| RootState {
                path: text.clone(),
                available: readable.iter().any(|(scanned, ok)| *ok && scanned == path),
            })
            .collect()
    }

    async fn scan_loop(self) -> Result<(), ErrorCode> {
        let mut hup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .map_err(|_| ErrorCode::Unavailable)?;
        let mut interval = tokio::time::interval(scan_interval());
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval.tick().await;
        let hints = HintWatch::spawn(self.roots());
        loop {
            let autoscan = self
                .settings
                .lock()
                .map_err(|_| ErrorCode::Internal)?
                .autoscan;
            tokio::select! {
                _ = hup.recv() => {
                    hints.rewatch(self.roots());
                    let _ = self.scan_and_publish().await;
                }
                _ = interval.tick(), if autoscan => {
                    let _ = self.scan_and_publish().await;
                }
                _ = hints.notified() => {
                    tokio::time::sleep(hint_debounce()).await;
                    let _ = self.scan_and_publish().await;
                }
            }
        }
    }
}

fn log_error(operation: &str, error: ErrorCode) {
    eprintln!(
        "{{\"level\":\"error\",\"component\":\"arinn-plugin-library\",\"operation\":\"{operation}\",\"error\":\"{error}\"}}"
    );
}

impl SettingsHandler for Bridge {
    fn schema(&self) -> Vec<Setting> {
        crate::settings::schema()
    }

    fn apply(
        &self,
        values: Map<String, Value>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        // Settings are read once at start (ADR-0056): a valid change is
        // `Unsupported` and applies at the next start.
        Box::pin(async move { crate::settings::check(&values) })
    }
}

impl LibraryHandler for Bridge {
    fn launch(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<PlaybackSource, ErrorCode>> + Send + '_>> {
        let store = self.store.clone();
        let service = self.service.clone();
        Box::pin(async move {
            let (source, resumes) = tokio::task::spawn_blocking(move || {
                let store = store.lock().map_err(|_| ErrorCode::Internal)?;
                let (source, restarted) = store.launch_for_playback(&id)?;
                // A rewatch cleared completion: publish the changed resume.
                let resumes = if restarted {
                    Some(store.resumes()?)
                } else {
                    None
                };
                Ok::<_, ErrorCode>((source, resumes))
            })
            .await
            .map_err(|_| ErrorCode::Internal)??;
            if let Some(resumes) = resumes
                && let Some(service) = service.lock().await.clone()
            {
                service.replace_resumes(resumes).await?;
            }
            Ok(source)
        })
    }
}

impl LibraryResumeHandler for Bridge {
    fn set_progress(
        &self,
        id: String,
        position_us: i64,
        duration_us: i64,
        ended: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        let store = self.store.clone();
        let service = self.service.clone();
        Box::pin(async move {
            let resumes = tokio::task::spawn_blocking(move || {
                let mut store = store.lock().map_err(|_| ErrorCode::Internal)?;
                store.set_progress(&id, position_us, duration_us, ended)?;
                store.resumes()
            })
            .await
            .map_err(|_| ErrorCode::Internal)??;
            let service = service.lock().await.clone().ok_or(ErrorCode::Unavailable)?;
            service.replace_resumes(resumes).await?;
            Ok(())
        })
    }

    fn restart(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        let store = self.store.clone();
        let service = self.service.clone();
        Box::pin(async move {
            let resumes = tokio::task::spawn_blocking(move || {
                let mut store = store.lock().map_err(|_| ErrorCode::Internal)?;
                store.restart(&id)?;
                store.resumes()
            })
            .await
            .map_err(|_| ErrorCode::Internal)??;
            let service = service.lock().await.clone().ok_or(ErrorCode::Unavailable)?;
            service.replace_resumes(resumes).await?;
            Ok(())
        })
    }
}

impl LibraryImportHandler for Bridge {
    fn import(
        &self,
        candidates: Vec<ImportCandidate>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>, ErrorCode>> + Send + '_>> {
        let store = self.store.clone();
        let settings = self.settings.clone();
        Box::pin(async move {
            let ids = tokio::task::spawn_blocking(move || {
                let roots = settings
                    .lock()
                    .map_err(|_| ErrorCode::Internal)?
                    .roots
                    .clone();
                let mut store = store.lock().map_err(|_| ErrorCode::Internal)?;
                store.set_configured_roots(&roots);
                store.import(&candidates)
            })
            .await
            .map_err(|_| ErrorCode::Internal)??;
            self.publish_current().await?;
            Ok(ids)
        })
    }
}

fn scan_interval() -> Duration {
    std::env::var("ARINN_LIBRARY_INTERVAL_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(600))
}

fn hint_debounce() -> Duration {
    std::env::var("ARINN_LIBRARY_DEBOUNCE_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(2))
}

struct HintWatch {
    notify: Arc<Notify>,
    roots: tokio::sync::watch::Sender<Vec<PathBuf>>,
}

impl HintWatch {
    fn spawn(roots: Vec<PathBuf>) -> Self {
        let (tx, rx) = tokio::sync::watch::channel(roots);
        let notify = Arc::new(Notify::new());
        let flag = notify.clone();
        tokio::spawn(async move { hint_loop(rx, flag).await });
        Self { notify, roots: tx }
    }

    fn rewatch(&self, roots: Vec<PathBuf>) {
        let _ = self.roots.send(roots);
    }

    async fn notified(&self) {
        self.notify.notified().await;
    }
}

async fn hint_loop(mut roots: tokio::sync::watch::Receiver<Vec<PathBuf>>, notify: Arc<Notify>) {
    loop {
        let current = roots.borrow().clone();
        let Some(mut watch) = Inotify::open(&current) else {
            if roots.changed().await.is_err() {
                return;
            }
            continue;
        };
        loop {
            tokio::select! {
                changed = roots.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    break;
                }
                _ = watch.ready() => notify.notify_one(),
            }
        }
    }
}

struct Inotify {
    fd: AsyncFd<OwnedFd>,
    roots: Vec<PathBuf>,
    seen: HashSet<PathBuf>,
    left: usize,
}

impl Inotify {
    fn open(roots: &[PathBuf]) -> Option<Self> {
        let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw < 0 {
            return None;
        }
        let mut seen = HashSet::new();
        let mut left = 4096usize;
        for root in roots {
            add_tree(raw, root, &mut left, &mut seen);
        }
        let owned = unsafe { OwnedFd::from_raw_fd(raw) };
        // SAFETY: OwnedFd keeps this nonblocking inotify descriptor open and
        // stable for the entire AsyncFd lifetime; it is not closed elsewhere.
        unsafe { AsyncFd::register(owned) }.ok().map(|fd| Self {
            fd,
            roots: roots.to_vec(),
            seen,
            left,
        })
    }

    fn rewalk(&mut self) {
        let fd = self.fd.get_ref().as_raw_fd();
        for root in self.roots.clone() {
            add_tree(fd, &root, &mut self.left, &mut self.seen);
        }
    }

    async fn ready(&mut self) {
        loop {
            let Ok(mut guard) = self.fd.readable().await else {
                return;
            };
            let fd = self.fd.get_ref().as_raw_fd();
            let mut buf = [0u8; 4096];
            let read = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
            if read < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    guard.clear_ready();
                    continue;
                }
                return;
            }
            if read == 0 {
                guard.clear_ready();
                return;
            }
            self.rewalk();
            return;
        }
    }
}

fn add_tree(fd: i32, dir: &std::path::Path, left: &mut usize, seen: &mut HashSet<PathBuf>) {
    let Ok(meta) = std::fs::symlink_metadata(dir) else {
        return;
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return;
    }
    if !seen.contains(dir) {
        if *left == 0 {
            return;
        }
        let Ok(name) =
            std::ffi::CString::new(std::os::unix::ffi::OsStrExt::as_bytes(dir.as_os_str()))
        else {
            return;
        };
        let mask = libc::IN_CREATE
            | libc::IN_DELETE
            | libc::IN_MOVED_FROM
            | libc::IN_MOVED_TO
            | libc::IN_CLOSE_WRITE
            | libc::IN_ATTRIB
            | libc::IN_DELETE_SELF
            | libc::IN_MOVE_SELF
            | libc::IN_UNMOUNT;
        if unsafe { libc::inotify_add_watch(fd, name.as_ptr(), mask) } < 0 {
            return;
        }
        seen.insert(dir.to_path_buf());
        *left -= 1;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if std::fs::symlink_metadata(&path)
            .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
        {
            add_tree(fd, &path, left, seen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn queued_publication_reads_commits_made_while_waiting() {
        let scratch =
            std::env::temp_dir().join(format!("arinn-publication-{}", uuid::Uuid::new_v4()));
        let root = scratch.join("root");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("First.mkv"), b"first").unwrap();
        let settings = Settings {
            roots: vec![root.clone()],
            probe: false,
            ..Settings::default()
        };
        let mut store = Store::open(&scratch.join("library.sqlite3")).unwrap();
        store.use_mount_tables(crate::mounts::MountTables::default());
        store.scan(&settings).unwrap();
        let bridge = Bridge {
            store: Arc::new(std::sync::Mutex::new(store)),
            service: Arc::new(Mutex::new(None)),
            publication: Arc::new(Mutex::new(())),
            settings: Arc::new(std::sync::Mutex::new(settings.clone())),
            written: Arc::new(Vec::new()),
        };
        let held = bridge.publication.lock().await;
        let queued = bridge.read_publication();
        tokio::pin!(queued);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut queued)
                .await
                .is_err()
        );
        // An import/scan can commit while publication is blocked. Its new item
        // must be in the queued publisher's catalogue AND additions snapshot.
        std::fs::write(root.join("Second.mkv"), b"second").unwrap();
        bridge.store.lock().unwrap().scan(&settings).unwrap();
        drop(held);
        let publication = queued.await.unwrap();
        assert_eq!(publication.items.len(), 2);
        assert_eq!(publication.additions.len(), 2);
        assert!(bridge.publication.try_lock().is_err());
        drop(publication);
        assert!(bridge.publication.try_lock().is_ok());
        let _ = std::fs::remove_dir_all(scratch);
    }

    #[test]
    fn library_methods_do_not_block_the_async_runtime() {
        let scratch = std::env::temp_dir().join(format!(
            "arinn-library-runtime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&scratch).unwrap();
        let mut store = Store::open(&scratch.join("library.sqlite3")).unwrap();
        store.use_mount_tables(crate::mounts::MountTables::default());
        let bridge = Bridge {
            store: Arc::new(std::sync::Mutex::new(store)),
            service: Arc::new(Mutex::new(None)),
            publication: Arc::new(Mutex::new(())),
            settings: Arc::new(std::sync::Mutex::new(Settings::default())),
            written: Arc::new(Vec::new()),
        };
        let hold = bridge.store.lock().unwrap();
        let progressed = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&progressed);
        let bridge = bridge.clone();
        let worker = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let launch = bridge.launch("missing".into());
                let progress = bridge.set_progress("missing".into(), 0, -1, false);
                let restart = bridge.restart("missing".into());
                let import = bridge.import(vec![arinn_protocol::library::ImportCandidate {
                    source: arinn_protocol::production::EntityRef {
                        provider_id: "downloads".into(),
                        capability: "download".into(),
                        local_id: "job".into(),
                    },
                    file_uri: "file:///tmp/arinn-library-missing.mkv".into(),
                    completion_id: "done".into(),
                }]);
                tokio::pin!(launch, progress, restart, import);
                let pause = tokio::time::sleep(Duration::from_millis(200));
                tokio::pin!(pause);
                tokio::select! {
                    _ = &mut pause => flag.store(true, Ordering::SeqCst),
                    _ = &mut launch => {}
                    _ = &mut progress => {}
                    _ = &mut restart => {}
                    _ = &mut import => {}
                }
                if flag.load(Ordering::SeqCst) {
                    let _ = tokio::join!(launch, progress, restart, import);
                }
            });
        });
        std::thread::sleep(Duration::from_millis(800));
        let saw_progress = progressed.load(Ordering::SeqCst);
        drop(hold);
        worker.join().unwrap();
        let _ = std::fs::remove_dir_all(&scratch);
        assert!(saw_progress, "async runtime blocked on the catalogue mutex");
    }

    #[tokio::test]
    async fn a_new_subdirectory_is_watched() {
        let scratch = std::env::temp_dir().join(format!(
            "arinn-library-inotify-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let root = scratch.join("root");
        std::fs::create_dir_all(&root).unwrap();
        let mut watch = Inotify::open(std::slice::from_ref(&root)).expect("inotify");
        std::fs::create_dir(root.join("extra")).unwrap();
        tokio::time::timeout(Duration::from_secs(2), watch.ready())
            .await
            .expect("directory create");
        std::fs::write(root.join("extra/Clip (2001).mkv"), b"x").unwrap();
        tokio::time::timeout(Duration::from_secs(2), watch.ready())
            .await
            .expect("file in a new subdirectory was not watched");
        let _ = std::fs::remove_dir_all(&scratch);
    }
}
