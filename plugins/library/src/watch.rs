// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Serves `LibraryProvider1`, `LibraryResume1` and `LibraryImport1`.
//! Scanning runs beside the snapshot and does not hold the D-Bus reply path.

use crate::store::{Settings, Store};
use arinn_plugin_sdk::runtime::{
    LibraryExtensions, LibraryHandler, LibraryImportHandler, LibraryResumeHandler, LibraryService,
    run_library_on,
};
use arinn_protocol::{library::ImportCandidate, media::PlaybackSource, validation::ErrorCode};
use std::{
    future::Future,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::{
    io::unix::AsyncFd,
    sync::{Mutex, Notify},
};

#[derive(Clone)]
struct Bridge {
    store: Arc<std::sync::Mutex<Store>>,
    service: Arc<Mutex<Option<LibraryService>>>,
    settings: Arc<std::sync::Mutex<Settings>>,
}

pub async fn serve(connection: zbus::Connection) -> Result<(), ErrorCode> {
    let provider_id = std::env::var("ARINN_PROVIDER_ID").unwrap_or_else(|_| "library".into());
    let database = crate::paths::state_database(&provider_id)?;
    let config = crate::paths::config_file(&provider_id)?;
    let loaded = Settings::load(&config);
    let backend_ok = loaded.is_ok();
    let settings = loaded.unwrap_or_default();
    let store = Store::open(&database)?;
    let bridge = Bridge {
        store: Arc::new(std::sync::Mutex::new(store)),
        service: Arc::new(Mutex::new(None)),
        settings: Arc::new(std::sync::Mutex::new(settings)),
    };
    let (collections, items, resumes) = {
        let store = bridge.store.lock().map_err(|_| ErrorCode::Internal)?;
        let (collections, items) = store.snapshot()?;
        (collections, items, store.resumes()?)
    };
    let service = LibraryService::with_extensions(
        bridge.clone(),
        collections,
        items,
        if backend_ok {
            "available"
        } else {
            "unavailable"
        },
        LibraryExtensions::default()
            .resume(bridge.clone())
            .import(bridge.clone()),
    )?;
    *bridge.service.lock().await = Some(service.clone());
    service.replace_resumes(resumes).await?;
    let scanner = bridge.clone();
    let config_path = config.clone();
    let running = tokio::spawn(async move {
        if backend_ok {
            scanner.scan_and_publish().await?;
        }
        scanner.scan_loop(config_path).await
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

    fn reload(&self, config: &std::path::Path) -> Result<(), ErrorCode> {
        let settings = Settings::load(config)?;
        *self.settings.lock().map_err(|_| ErrorCode::Internal)? = settings;
        Ok(())
    }

    async fn scan_and_publish(&self) -> Result<(), ErrorCode> {
        let settings = self
            .settings
            .lock()
            .map_err(|_| ErrorCode::Internal)?
            .clone();
        let store = self.store.clone();
        let (collections, items, resumes) = tokio::task::spawn_blocking(move || {
            let mut store = store.lock().map_err(|_| ErrorCode::Internal)?;
            store.scan(&settings)?;
            let (collections, items) = store.snapshot()?;
            Ok::<_, ErrorCode>((collections, items, store.resumes()?))
        })
        .await
        .map_err(|_| ErrorCode::Internal)??;
        let service = self
            .service
            .lock()
            .await
            .clone()
            .ok_or(ErrorCode::Unavailable)?;
        service.replace(collections, items, "available").await?;
        service.replace_resumes(resumes).await?;
        Ok(())
    }

    async fn scan_loop(self, config: PathBuf) -> Result<(), ErrorCode> {
        let mut hup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .map_err(|_| ErrorCode::Unavailable)?;
        let mut interval = tokio::time::interval(scan_interval());
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval.tick().await;
        let hints = HintWatch::spawn(self.roots());
        let mut seen = mtime(&config);
        loop {
            let autoscan = self
                .settings
                .lock()
                .map_err(|_| ErrorCode::Internal)?
                .autoscan;
            tokio::select! {
                _ = hup.recv() => {
                    if self.reload(&config).is_ok() {
                        hints.rewatch(self.roots());
                        self.scan_and_publish().await?;
                    }
                }
                _ = interval.tick(), if autoscan => {
                    self.scan_and_publish().await?;
                }
                _ = hints.notified() => {
                    tokio::time::sleep(hint_debounce()).await;
                    self.scan_and_publish().await?;
                }
                _ = tokio::time::sleep(Duration::from_secs(2)) => {
                    let now = mtime(&config);
                    if now != seen {
                        seen = now;
                        if self.reload(&config).is_ok() {
                            hints.rewatch(self.roots());
                            self.scan_and_publish().await?;
                        }
                    }
                }
            }
        }
    }
}

impl LibraryHandler for Bridge {
    fn launch(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<PlaybackSource, ErrorCode>> + Send + '_>> {
        let store = self.store.clone();
        Box::pin(async move {
            let store = store.lock().map_err(|_| ErrorCode::Internal)?;
            store.launch(&id)
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
            let resumes = {
                let mut store = store.lock().map_err(|_| ErrorCode::Internal)?;
                store.set_progress(&id, position_us, duration_us, ended)?;
                store.resumes()?
            };
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
            let resumes = {
                let mut store = store.lock().map_err(|_| ErrorCode::Internal)?;
                store.restart(&id)?;
                store.resumes()?
            };
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
        let service = self.service.clone();
        let settings = self.settings.clone();
        Box::pin(async move {
            let roots = settings
                .lock()
                .map_err(|_| ErrorCode::Internal)?
                .roots
                .clone();
            let (ids, collections, items, resumes) = {
                let mut store = store.lock().map_err(|_| ErrorCode::Internal)?;
                store.set_configured_roots(&roots);
                let ids = store.import(&candidates)?;
                let (collections, items) = store.snapshot()?;
                (ids, collections, items, store.resumes()?)
            };
            let service = service.lock().await.clone().ok_or(ErrorCode::Unavailable)?;
            service.replace(collections, items, "available").await?;
            service.replace_resumes(resumes).await?;
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

fn mtime(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
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
        let Some(watch) = Inotify::open(&current) else {
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
}

impl Inotify {
    fn open(roots: &[PathBuf]) -> Option<Self> {
        let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw < 0 {
            return None;
        }
        let mut left = 4096usize;
        for root in roots {
            add_tree(raw, root, &mut left);
        }
        let owned = unsafe { OwnedFd::from_raw_fd(raw) };
        AsyncFd::new(owned).ok().map(|fd| Self { fd })
    }

    async fn ready(&self) {
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
            return;
        }
    }
}

fn add_tree(fd: i32, dir: &std::path::Path, left: &mut usize) {
    if *left == 0 {
        return;
    }
    let Ok(meta) = std::fs::symlink_metadata(dir) else {
        return;
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return;
    }
    let Ok(name) = std::ffi::CString::new(std::os::unix::ffi::OsStrExt::as_bytes(dir.as_os_str()))
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
    *left -= 1;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if std::fs::symlink_metadata(&path)
            .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
        {
            add_tree(fd, &path, left);
        }
    }
}
