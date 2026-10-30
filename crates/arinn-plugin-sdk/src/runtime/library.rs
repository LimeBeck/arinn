// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `org.arinn.LibraryProvider1` and optional resume/import (ADR-0054) and
//! root availability (ADR-0071).
//! Launch returns a file URI to the daemon only. The SDK does not log it.
use super::{Endpoint, RegistrationContext, backend_state, run_endpoint};
use crate::snapshots::{LibrarySnapshots, ResumeSnapshots};
use arinn_protocol::{
    library::{
        self, Addition, Additions, Collection, FEATURE_ADDITIONS, FEATURE_IMPORT, FEATURE_RESUME,
        FEATURE_ROOTS, ImportCandidate, Item, LibraryPage, Resume, ResumePage, RootState, Roots,
    },
    media::{self, PlaybackSource},
    production::{self, PROVIDER_PATH},
    validation::{self, ErrorCode},
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tokio::sync::Mutex;
use zbus::{message::Header, object_server::SignalEmitter};

pub trait LibraryHandler: Send + Sync + 'static {
    fn launch(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<PlaybackSource, ErrorCode>> + Send + '_>>;
}
pub trait LibraryResumeHandler: Send + Sync + 'static {
    fn set_progress(
        &self,
        id: String,
        position_us: i64,
        duration_us: i64,
        ended: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>>;
    fn restart(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>>;
}
pub trait LibraryImportHandler: Send + Sync + 'static {
    fn import(
        &self,
        candidates: Vec<ImportCandidate>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>, ErrorCode>> + Send + '_>>;
}

#[derive(Clone, Default)]
pub struct LibraryExtensions {
    assets: Option<Arc<dyn super::assets::AssetHandler>>,
    resume: Option<Arc<dyn LibraryResumeHandler>>,
    import: Option<Arc<dyn LibraryImportHandler>>,
    roots: bool,
    additions: bool,
}
impl LibraryExtensions {
    /// Serve bounded, provider-owned discovery times (`LibraryAdditions1`).
    /// Publish them atomically with the catalogue using `replace_with_additions`.
    pub fn additions(mut self) -> Self {
        self.additions = true;
        self
    }
    /// Serve asset bytes for the provider's asset ids (ADR-0062).
    pub fn assets(mut self, handler: impl super::assets::AssetHandler) -> Self {
        self.assets = Some(Arc::new(handler));
        self
    }
    pub fn resume(mut self, handler: impl LibraryResumeHandler) -> Self {
        self.resume = Some(Arc::new(handler));
        self
    }
    pub fn import(mut self, handler: impl LibraryImportHandler) -> Self {
        self.import = Some(Arc::new(handler));
        self
    }
    /// Publish the availability of the configured roots (`LibraryRoots1`)
    /// with [`LibraryService::replace_roots`].
    pub fn roots(mut self) -> Self {
        self.roots = true;
        self
    }
}

struct Data {
    daemon_owner: String,
    generation: String,
    snapshots: Option<LibrarySnapshots>,
    collections: Vec<Collection>,
    items: Vec<Item>,
    backend: String,
    revision: u64,
    resumes: Vec<Resume>,
    resume_snapshots: Option<ResumeSnapshots>,
    resume_revision: u64,
    roots: Vec<RootState>,
    additions: Vec<Addition>,
    roots_revision: u64,
}

#[derive(Clone)]
pub struct LibraryService {
    data: Arc<Mutex<Data>>,
    handler: Arc<dyn LibraryHandler>,
    extensions: LibraryExtensions,
    registrations: tokio::sync::watch::Sender<Option<RegistrationContext>>,
    running: Arc<AtomicBool>,
    updates: tokio::sync::watch::Sender<(String, u64)>,
    resume_updates: tokio::sync::watch::Sender<(String, u64)>,
    roots_updates: tokio::sync::watch::Sender<(String, u64)>,
}

impl LibraryService {
    pub fn new(
        handler: impl LibraryHandler,
        collections: Vec<Collection>,
        items: Vec<Item>,
        backend: &str,
    ) -> Result<Self, ErrorCode> {
        Self::with_extensions(
            handler,
            collections,
            items,
            backend,
            LibraryExtensions::default(),
        )
    }

    pub fn with_extensions(
        handler: impl LibraryHandler,
        collections: Vec<Collection>,
        items: Vec<Item>,
        backend: &str,
        extensions: LibraryExtensions,
    ) -> Result<Self, ErrorCode> {
        library::validate_entries(&collections, &items)?;
        backend_state(backend)?;
        Ok(Self {
            data: Arc::new(Mutex::new(Data {
                daemon_owner: String::new(),
                generation: String::new(),
                snapshots: None,
                collections,
                items,
                backend: backend.into(),
                revision: 0,
                resumes: Vec::new(),
                resume_snapshots: None,
                resume_revision: 0,
                roots: Vec::new(),
                additions: Vec::new(),
                roots_revision: 0,
            })),
            handler: Arc::new(handler),
            extensions,
            registrations: tokio::sync::watch::channel(None).0,
            running: Arc::new(AtomicBool::new(false)),
            updates: tokio::sync::watch::channel((String::new(), 0)).0,
            resume_updates: tokio::sync::watch::channel((String::new(), 0)).0,
            roots_updates: tokio::sync::watch::channel((String::new(), 0)).0,
        })
    }

    pub fn registrations(&self) -> tokio::sync::watch::Receiver<Option<RegistrationContext>> {
        self.registrations.subscribe()
    }

    pub async fn replace(
        &self,
        collections: Vec<Collection>,
        items: Vec<Item>,
        backend: &str,
    ) -> Result<(), ErrorCode> {
        self.replace_snapshot(collections, items, backend, Vec::new())
            .await
    }

    /// Atomically publish discovery metadata and its matching catalogue.
    /// A metadata-only change also advances the catalogue revision.
    pub async fn replace_with_additions(
        &self,
        collections: Vec<Collection>,
        items: Vec<Item>,
        backend: &str,
        additions: Vec<Addition>,
    ) -> Result<(), ErrorCode> {
        if !self.extensions.additions {
            return Err(ErrorCode::Unsupported);
        }
        self.replace_snapshot(collections, items, backend, additions)
            .await
    }

    async fn replace_snapshot(
        &self,
        collections: Vec<Collection>,
        items: Vec<Item>,
        backend: &str,
        additions: Vec<Addition>,
    ) -> Result<(), ErrorCode> {
        library::validate_addition_items(&additions, &items)?;
        library::validate_entries(&collections, &items)?;
        backend_state(backend)?;
        let update = {
            let mut data = self.data.lock().await;
            let backend_changed = data.backend != backend || data.additions != additions;
            let before = data.revision;
            let revision = if let Some(snapshots) = data.snapshots.as_mut() {
                let mut revision = snapshots.publish(collections.clone(), items.clone())?;
                if backend_changed && revision == before {
                    revision = snapshots.touch()?;
                }
                revision
            } else {
                0
            };
            if revision > 0 {
                data.revision = revision;
            }
            data.additions = additions;
            data.collections = collections;
            data.items = items;
            data.backend = backend.into();
            // An unchanged snapshot keeps its revision: no StateChanged.
            (revision > 0 && revision != before).then(|| (data.generation.clone(), revision))
        };
        if let Some(update) = update {
            self.updates.send_replace(update);
        }
        Ok(())
    }

    /// Replace the resume snapshot. No-op storage when `LibraryResume1` is off,
    /// and a new resume revision when it is on. Does not bump the catalogue revision.
    pub async fn replace_resumes(&self, resumes: Vec<Resume>) -> Result<(), ErrorCode> {
        if self.extensions.resume.is_none() {
            return Err(ErrorCode::Unsupported);
        }
        library::validate_resumes(&resumes)?;
        let update = {
            let mut data = self.data.lock().await;
            let before = data.resume_revision;
            let revision = if let Some(snapshots) = data.resume_snapshots.as_mut() {
                snapshots.publish(resumes.clone())?
            } else {
                0
            };
            if revision > 0 {
                data.resume_revision = revision;
            }
            data.resumes = resumes;
            // An unchanged snapshot keeps its revision: no StateChanged.
            (revision > 0 && revision != before).then(|| (data.generation.clone(), revision))
        };
        if let Some(update) = update {
            self.resume_updates.send_replace(update);
        }
        Ok(())
    }

    /// Replace the configured roots and their availability. `Unsupported`
    /// unless the service was built with [`LibraryExtensions::roots`]. A
    /// change bumps the roots revision and emits `StateChanged` on
    /// `LibraryRoots1` once registered; the same list is not a change.
    pub async fn replace_roots(&self, roots: Vec<RootState>) -> Result<(), ErrorCode> {
        if !self.extensions.roots {
            return Err(ErrorCode::Unsupported);
        }
        library::validate_roots(&roots)?;
        let update = {
            let mut data = self.data.lock().await;
            if data.roots == roots {
                return Ok(());
            }
            data.roots = roots;
            if data.roots_revision == 0 {
                None
            } else {
                data.roots_revision += 1;
                Some((data.generation.clone(), data.roots_revision))
            }
        };
        if let Some(update) = update {
            self.roots_updates.send_replace(update);
        }
        Ok(())
    }

    fn feature_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if self.extensions.resume.is_some() {
            names.push(FEATURE_RESUME.into());
        }
        if self.extensions.import.is_some() {
            names.push(FEATURE_IMPORT.into());
        }
        if self.extensions.assets.is_some() {
            names.push(arinn_protocol::assets::FEATURE_ASSETS.into());
        }
        if self.extensions.additions {
            names.push(FEATURE_ADDITIONS.into());
        }
        if self.extensions.roots {
            names.push(FEATURE_ROOTS.into());
        }
        names
    }

    async fn authorized(&self, header: &Header<'_>) -> production::Result<()> {
        let data = self.data.lock().await;
        if !self.running.load(Ordering::Acquire)
            || data.daemon_owner.is_empty()
            || header.sender().map(|s| s.as_str()) != Some(&data.daemon_owner)
        {
            return Err(ErrorCode::PermissionDenied.into());
        }
        Ok(())
    }

    async fn ready(&self, generation: &str, header: &Header<'_>) -> production::Result<()> {
        self.authorized(header).await?;
        let data = self.data.lock().await;
        if data.generation != generation {
            return Err(ErrorCode::Conflict.into());
        }
        if data.backend == "unavailable" {
            return Err(ErrorCode::Unavailable.into());
        }
        Ok(())
    }

    async fn item(&self, id: &str) -> production::Result<Item> {
        self.data
            .lock()
            .await
            .items
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or(ErrorCode::NotFound.into())
    }
}

#[zbus::interface(name = "org.arinn.LibraryProvider1")]
impl LibraryService {
    async fn get_library(
        &self,
        token: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(LibraryPage,)> {
        self.authorized(&header).await?;
        let owner = header.sender().ok_or(ErrorCode::PermissionDenied)?.as_str();
        Ok((self
            .data
            .lock()
            .await
            .snapshots
            .as_mut()
            .ok_or(ErrorCode::Unavailable)?
            .page(owner, token, Instant::now())?,))
    }

    async fn get_item(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(Item,)> {
        validation::id(id)?;
        self.authorized(&header).await?;
        if self.data.lock().await.generation != generation {
            return Err(ErrorCode::Conflict.into());
        }
        Ok((self.item(id).await?,))
    }

    async fn backend_state(
        &self,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<String> {
        self.authorized(&header).await?;
        Ok(self.data.lock().await.backend.clone())
    }

    async fn features(
        &self,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<Vec<String>> {
        self.authorized(&header).await?;
        Ok(self.feature_names())
    }

    async fn launch(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(PlaybackSource,)> {
        validation::id(id)?;
        self.ready(generation, &header).await?;
        let item = self.item(id).await?;
        if !item.available {
            return Err(ErrorCode::Unavailable.into());
        }
        let source = self.handler.launch(id.into()).await?;
        if source.local_id != item.id || source.validate().is_err() {
            return Err(ErrorCode::Internal.into());
        }
        Ok((source,))
    }

    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}

#[derive(Clone)]
struct AdditionsIface(LibraryService);
#[zbus::interface(name = "org.arinn.LibraryAdditions1")]
impl AdditionsIface {
    async fn get_additions(
        &self,
        generation: &str,
        revision: u64,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(Additions,)> {
        self.0.authorized(&header).await?;
        let data = self.0.data.lock().await;
        if data.snapshots.is_none() {
            return Err(ErrorCode::Unavailable.into());
        }
        if data.generation != generation || data.revision != revision {
            return Err(ErrorCode::Conflict.into());
        }
        Ok((Additions {
            generation: data.generation.clone(),
            revision: data.revision,
            entries: data.additions.clone(),
        },))
    }
}

#[derive(Clone)]
struct ResumeIface(LibraryService);
#[zbus::interface(name = "org.arinn.LibraryResume1")]
impl ResumeIface {
    async fn get_resumes(
        &self,
        token: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(ResumePage,)> {
        self.0.authorized(&header).await?;
        let owner = header.sender().ok_or(ErrorCode::PermissionDenied)?.as_str();
        Ok((self
            .0
            .data
            .lock()
            .await
            .resume_snapshots
            .as_mut()
            .ok_or(ErrorCode::Unavailable)?
            .page(owner, token, Instant::now())?,))
    }

    async fn set_progress(
        &self,
        generation: &str,
        content_id: &str,
        position_us: i64,
        duration_us: i64,
        ended: bool,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        validation::id(content_id)?;
        media::position_us(position_us, duration_us)?;
        self.0.ready(generation, &header).await?;
        self.0.item(content_id).await?;
        self.0
            .extensions
            .resume
            .as_ref()
            .ok_or(ErrorCode::Unsupported)?
            .set_progress(content_id.into(), position_us, duration_us, ended)
            .await
            .map_err(Into::into)
    }

    async fn restart_from_beginning(
        &self,
        generation: &str,
        content_id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        validation::id(content_id)?;
        self.0.ready(generation, &header).await?;
        self.0.item(content_id).await?;
        self.0
            .extensions
            .resume
            .as_ref()
            .ok_or(ErrorCode::Unsupported)?
            .restart(content_id.into())
            .await
            .map_err(Into::into)
    }

    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}

#[derive(Clone)]
struct ImportIface(LibraryService);
#[zbus::interface(name = "org.arinn.LibraryImport1")]
impl ImportIface {
    async fn import_completed(
        &self,
        generation: &str,
        candidates: Vec<ImportCandidate>,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<Vec<String>> {
        library::validate_imports(&candidates)?;
        self.0.ready(generation, &header).await?;
        let ids = self
            .0
            .extensions
            .import
            .as_ref()
            .ok_or(ErrorCode::Unsupported)?
            .import(candidates)
            .await?;
        let mut seen = std::collections::HashSet::new();
        for id in &ids {
            validation::id(id).map_err(|_| ErrorCode::Internal)?;
            if !seen.insert(id.as_str()) {
                return Err(ErrorCode::Internal.into());
            }
        }
        Ok(ids)
    }
}

#[derive(Clone)]
struct RootsIface(LibraryService);
#[zbus::interface(name = "org.arinn.LibraryRoots1")]
impl RootsIface {
    // Readable while the backend is unavailable: a missing drive is exactly
    // what this reports.
    async fn get_roots(
        &self,
        generation: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(Roots,)> {
        self.0.authorized(&header).await?;
        let data = self.0.data.lock().await;
        if data.generation != generation {
            return Err(ErrorCode::Conflict.into());
        }
        Ok((Roots {
            generation: data.generation.clone(),
            revision: data.roots_revision,
            roots: data.roots.clone(),
        },))
    }

    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}

#[derive(Clone)]
struct AssetsIface(LibraryService);
#[zbus::interface(name = "org.arinn.AssetProvider1")]
impl AssetsIface {
    async fn get_asset(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(arinn_protocol::assets::Asset,)> {
        self.0.ready(generation, &header).await?;
        let handler = self
            .0
            .extensions
            .assets
            .as_ref()
            .ok_or(ErrorCode::Unsupported)?;
        super::assets::serve(handler, id).await
    }
}

impl Endpoint for LibraryService {
    const CAPABILITY: &'static str = "library";
    fn registrations(&self) -> &tokio::sync::watch::Sender<Option<RegistrationContext>> {
        &self.registrations
    }
    fn running(&self) -> &Arc<AtomicBool> {
        &self.running
    }
    fn updates(&self) -> &tokio::sync::watch::Sender<(String, u64)> {
        &self.updates
    }
    async fn clear(&self) {
        let mut data = self.data.lock().await;
        data.snapshots = None;
        data.resume_snapshots = None;
        data.generation.clear();
        data.daemon_owner.clear();
        data.resume_revision = 0;
        data.roots_revision = 0;
    }
    async fn daemon_owner(&self) -> String {
        self.data.lock().await.daemon_owner.clone()
    }
    async fn set_daemon_owner(&self, owner: String) {
        self.data.lock().await.daemon_owner = owner;
    }
    async fn registered(&self) -> bool {
        self.data.lock().await.snapshots.is_some()
    }
    async fn install(&self, generation: &str) -> Result<(), ErrorCode> {
        let mut data = self.data.lock().await;
        let mut snapshots = LibrarySnapshots::new(generation)?;
        data.revision = snapshots.publish(data.collections.clone(), data.items.clone())?;
        data.generation = generation.into();
        data.snapshots = Some(snapshots);
        if self.extensions.roots {
            data.roots_revision = 1;
            self.roots_updates.send_replace((generation.into(), 1));
        }
        if self.extensions.resume.is_some() {
            let mut resumes = ResumeSnapshots::new(generation)?;
            data.resume_revision = resumes.publish(data.resumes.clone())?;
            data.resume_snapshots = Some(resumes);
            let revision = data.resume_revision;
            drop(data);
            self.resume_updates
                .send_replace((generation.into(), revision));
        }
        Ok(())
    }
    async fn emit(
        &self,
        connection: &zbus::Connection,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()> {
        let emitter = SignalEmitter::new(connection, PROVIDER_PATH)?;
        LibraryService::state_changed(&emitter, generation, revision).await
    }
    async fn export(
        &self,
        connection: &zbus::Connection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !connection
            .object_server()
            .at(PROVIDER_PATH, self.clone())
            .await?
        {
            return Err("provider endpoint already exported".into());
        }
        if self.extensions.additions {
            connection
                .object_server()
                .at(PROVIDER_PATH, AdditionsIface(self.clone()))
                .await?;
        }
        if self.extensions.assets.is_some() {
            connection
                .object_server()
                .at(PROVIDER_PATH, AssetsIface(self.clone()))
                .await?;
        }
        if self.extensions.resume.is_some() {
            connection
                .object_server()
                .at(PROVIDER_PATH, ResumeIface(self.clone()))
                .await?;
            let connection = connection.clone();
            let mut updates = self.resume_updates.subscribe();
            tokio::spawn(async move {
                loop {
                    if updates.changed().await.is_err() {
                        return;
                    }
                    let (generation, revision) = updates.borrow_and_update().clone();
                    if revision == 0 {
                        continue;
                    }
                    let Ok(emitter) = SignalEmitter::new(&connection, PROVIDER_PATH) else {
                        return;
                    };
                    if ResumeIface::state_changed(&emitter, &generation, revision)
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            });
        }
        if self.extensions.import.is_some() {
            connection
                .object_server()
                .at(PROVIDER_PATH, ImportIface(self.clone()))
                .await?;
        }
        if self.extensions.roots {
            connection
                .object_server()
                .at(PROVIDER_PATH, RootsIface(self.clone()))
                .await?;
            let connection = connection.clone();
            let mut updates = self.roots_updates.subscribe();
            tokio::spawn(async move {
                loop {
                    if updates.changed().await.is_err() {
                        return;
                    }
                    let (generation, revision) = updates.borrow_and_update().clone();
                    if revision == 0 {
                        continue;
                    }
                    let Ok(emitter) = SignalEmitter::new(&connection, PROVIDER_PATH) else {
                        return;
                    };
                    if RootsIface::state_changed(&emitter, &generation, revision)
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            });
        }
        Ok(())
    }
}

pub async fn run_library(
    service: LibraryService,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = zbus::connection::Builder::session()?.build().await?;
    run_library_on(connection, service).await
}

pub async fn run_library_on(
    connection: zbus::Connection,
    service: LibraryService,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_endpoint(connection, service).await
}

#[cfg(test)]
mod unchanged_revision_tests {
    use super::*;
    use crate::runtime::unchanged_revision_tests::{Idle, registered, sent};

    #[tokio::test]
    async fn library_replace_with_same_state_sends_nothing() {
        let extensions = LibraryExtensions::default().resume(Idle);
        let service =
            LibraryService::with_extensions(Idle, vec![], vec![], "available", extensions).unwrap();
        let (generation, mut updates) = registered(&service).await;
        let mut resumes = service.resume_updates.subscribe();
        resumes.borrow_and_update();
        service.replace(vec![], vec![], "available").await.unwrap();
        assert_eq!(sent(&mut updates), None);
        service.replace(vec![], vec![], "degraded").await.unwrap();
        assert_eq!(sent(&mut updates), Some((generation, 2)));
        service.replace(vec![], vec![], "degraded").await.unwrap();
        assert_eq!(sent(&mut updates), None);
        service.replace_resumes(vec![]).await.unwrap();
        assert_eq!(sent(&mut resumes), None);
    }
}
