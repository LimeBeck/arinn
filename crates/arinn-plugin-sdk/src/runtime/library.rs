// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `org.arinn.LibraryProvider1` and optional resume/import (ADR-0052).
//! Launch returns a file URI to the daemon only. The SDK does not log it.
use super::{Endpoint, RegistrationContext, backend_state, run_endpoint};
use crate::snapshots::{LibrarySnapshots, ResumeSnapshots};
use arinn_protocol::{
    library::{
        self, Collection, FEATURE_IMPORT, FEATURE_RESUME, ImportCandidate, Item, LibraryPage,
        Resume, ResumePage,
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
    resume: Option<Arc<dyn LibraryResumeHandler>>,
    import: Option<Arc<dyn LibraryImportHandler>>,
}
impl LibraryExtensions {
    pub fn resume(mut self, handler: impl LibraryResumeHandler) -> Self {
        self.resume = Some(Arc::new(handler));
        self
    }
    pub fn import(mut self, handler: impl LibraryImportHandler) -> Self {
        self.import = Some(Arc::new(handler));
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
            })),
            handler: Arc::new(handler),
            extensions,
            registrations: tokio::sync::watch::channel(None).0,
            running: Arc::new(AtomicBool::new(false)),
            updates: tokio::sync::watch::channel((String::new(), 0)).0,
            resume_updates: tokio::sync::watch::channel((String::new(), 0)).0,
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
        library::validate_entries(&collections, &items)?;
        backend_state(backend)?;
        let (generation, revision) = {
            let mut data = self.data.lock().await;
            let backend_changed = data.backend != backend;
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
            data.collections = collections;
            data.items = items;
            data.backend = backend.into();
            (data.generation.clone(), revision)
        };
        if revision > 0 {
            self.updates.send_replace((generation, revision));
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
        let (generation, revision) = {
            let mut data = self.data.lock().await;
            let revision = if let Some(snapshots) = data.resume_snapshots.as_mut() {
                snapshots.publish(resumes.clone())?
            } else {
                0
            };
            if revision > 0 {
                data.resume_revision = revision;
            }
            data.resumes = resumes;
            (data.generation.clone(), revision)
        };
        if revision > 0 {
            self.resume_updates.send_replace((generation, revision));
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
