// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `org.arinn.DownloadProvider1` (ADR-0025, ADR-0067). Output file URIs are
//! published for the daemon only and never logged.
use super::{Endpoint, RegistrationContext, backend_state, run_endpoint};
use crate::snapshots::JobSnapshots;
use arinn_protocol::{
    download::{self, Job, JobPage},
    production::{self, PROVIDER_PATH},
    validation::{self, ErrorCode},
};
use futures_util::Future;
use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tokio::sync::Mutex;
use zbus::{message::Header, object_server::SignalEmitter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadAction {
    Pause,
    Resume,
    StartNow,
    Retry,
}
impl DownloadAction {
    fn token(self) -> &'static str {
        match self {
            Self::Pause => download::ACTION_PAUSE,
            Self::Resume => download::ACTION_RESUME,
            Self::StartNow => download::ACTION_START_NOW,
            Self::Retry => download::ACTION_RETRY,
        }
    }
}

/// Runs a command the job lists in `actions`. After an ambiguous backend
/// timeout, return `Timeout` and refresh the job; never repeat the mutation.
pub trait DownloadHandler: Send + Sync + 'static {
    fn control(
        &self,
        action: DownloadAction,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>>;
}

struct Data {
    daemon_owner: String,
    generation: String,
    snapshots: Option<JobSnapshots>,
    jobs: Vec<Job>,
    backend: String,
    revision: u64,
}

#[derive(Clone)]
pub struct DownloadService {
    data: Arc<Mutex<Data>>,
    handler: Arc<dyn DownloadHandler>,
    registrations: tokio::sync::watch::Sender<Option<RegistrationContext>>,
    running: Arc<AtomicBool>,
    updates: tokio::sync::watch::Sender<(String, u64)>,
}

impl DownloadService {
    pub fn new(
        handler: impl DownloadHandler,
        jobs: Vec<Job>,
        backend: &str,
    ) -> Result<Self, ErrorCode> {
        download::validate_jobs(&jobs)?;
        backend_state(backend)?;
        Ok(Self {
            data: Arc::new(Mutex::new(Data {
                daemon_owner: String::new(),
                generation: String::new(),
                snapshots: None,
                jobs,
                backend: backend.into(),
                revision: 0,
            })),
            handler: Arc::new(handler),
            registrations: tokio::sync::watch::channel(None).0,
            running: Arc::new(AtomicBool::new(false)),
            updates: tokio::sync::watch::channel((String::new(), 0)).0,
        })
    }

    pub fn registrations(&self) -> tokio::sync::watch::Receiver<Option<RegistrationContext>> {
        self.registrations.subscribe()
    }

    /// Publish normalized jobs. A new revision is published only when the
    /// jobs or the backend state changed.
    pub async fn replace(&self, jobs: Vec<Job>, backend: &str) -> Result<(), ErrorCode> {
        download::validate_jobs(&jobs)?;
        backend_state(backend)?;
        let (generation, revision) = {
            let mut data = self.data.lock().await;
            let backend_changed = data.backend != backend;
            let before = data.revision;
            let revision = if let Some(snapshots) = data.snapshots.as_mut() {
                let mut revision = snapshots.publish(jobs.clone())?;
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
            data.jobs = jobs;
            data.backend = backend.into();
            (data.generation.clone(), revision)
        };
        if revision > 0 {
            self.updates.send_replace((generation, revision));
        }
        Ok(())
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

    async fn job(&self, generation: &str, id: &str) -> production::Result<Job> {
        let data = self.data.lock().await;
        if data.generation != generation {
            return Err(ErrorCode::Conflict.into());
        }
        data.jobs
            .iter()
            .find(|job| job.id == id)
            .cloned()
            .ok_or(ErrorCode::NotFound.into())
    }

    async fn control(
        &self,
        action: DownloadAction,
        generation: &str,
        id: &str,
        header: Header<'_>,
    ) -> production::Result<()> {
        validation::id(id)?;
        self.authorized(&header).await?;
        {
            let data = self.data.lock().await;
            if data.generation != generation {
                return Err(ErrorCode::Conflict.into());
            }
            // Cached rows stay visible, but commands need the backend.
            if data.backend == "unavailable" {
                return Err(ErrorCode::Unavailable.into());
            }
        }
        let job = self.job(generation, id).await?;
        if !job.actions.iter().any(|name| name == action.token()) {
            return Err(ErrorCode::Unsupported.into());
        }
        self.handler
            .control(action, id.into())
            .await
            .map_err(Into::into)
    }
}

#[zbus::interface(name = "org.arinn.DownloadProvider1")]
impl DownloadService {
    async fn get_jobs(
        &self,
        token: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(JobPage,)> {
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

    async fn get_job(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<(Job,)> {
        validation::id(id)?;
        self.authorized(&header).await?;
        Ok((self.job(generation, id).await?,))
    }

    async fn backend_state(
        &self,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<String> {
        self.authorized(&header).await?;
        Ok(self.data.lock().await.backend.clone())
    }

    /// No optional interfaces yet; a future `AddSource` would be listed here.
    async fn features(
        &self,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<Vec<String>> {
        self.authorized(&header).await?;
        Ok(Vec::new())
    }

    async fn pause(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        self.control(DownloadAction::Pause, generation, id, header)
            .await
    }
    async fn resume(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        self.control(DownloadAction::Resume, generation, id, header)
            .await
    }
    async fn start_now(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        self.control(DownloadAction::StartNow, generation, id, header)
            .await
    }
    async fn retry(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        self.control(DownloadAction::Retry, generation, id, header)
            .await
    }

    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}

impl Endpoint for DownloadService {
    const CAPABILITY: &'static str = "download";
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
        data.generation.clear();
        data.daemon_owner.clear();
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
        let mut snapshots = JobSnapshots::new(generation)?;
        data.revision = snapshots.publish(data.jobs.clone())?;
        data.generation = generation.into();
        data.snapshots = Some(snapshots);
        Ok(())
    }
    async fn emit(
        &self,
        connection: &zbus::Connection,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()> {
        let emitter = SignalEmitter::new(connection, PROVIDER_PATH)?;
        DownloadService::state_changed(&emitter, generation, revision).await
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
        Ok(())
    }
}

pub async fn run_download(
    service: DownloadService,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = zbus::connection::Builder::session()?.build().await?;
    run_download_on(connection, service).await
}

pub async fn run_download_on(
    connection: zbus::Connection,
    service: DownloadService,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_endpoint(connection, service).await
}
