// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::snapshots::Snapshots;
use arinn_protocol::{
    application::{Application, ApplicationPage, RunningApplication},
    production::{self, ProductionDaemonProxy, Registration},
    validation::{self, ErrorCode},
};
use futures_util::StreamExt;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use zbus::{message::Header, object_server::SignalEmitter};

/// Valid only while the registration watch channel contains this context.
/// Consumers must discard platform clients and snapshots on every change.
#[derive(Clone, Debug)]
pub struct RegistrationContext {
    pub connection: zbus::Connection,
    pub daemon_owner: String,
    pub daemon_epoch: String,
    pub generation: String,
}

struct RegistrationGuard {
    registrations: tokio::sync::watch::Sender<Option<RegistrationContext>>,
    running: Arc<AtomicBool>,
}
impl Drop for RegistrationGuard {
    fn drop(&mut self) {
        self.registrations.send_replace(None);
        self.running.store(false, Ordering::Release);
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Launch,
    Activate,
    Terminate,
}
pub trait ApplicationHandler: Send + Sync + 'static {
    fn execute(
        &self,
        action: Action,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>>;
}
struct Data {
    daemon_owner: String,
    generation: String,
    snapshots: Option<Snapshots>,
    applications: Vec<Application>,
    running: Vec<RunningApplication>,
}
#[derive(Clone)]
pub struct ApplicationService {
    data: Arc<Mutex<Data>>,
    handler: Arc<dyn ApplicationHandler>,
    backend: String,
    registrations: tokio::sync::watch::Sender<Option<RegistrationContext>>,
    running: Arc<AtomicBool>,
    updates: tokio::sync::watch::Sender<(String, u64)>,
}
impl ApplicationService {
    pub fn new(
        handler: impl ApplicationHandler,
        applications: Vec<Application>,
        running: Vec<RunningApplication>,
        backend: &str,
    ) -> Result<Self, ErrorCode> {
        arinn_protocol::application::validate_entries(&applications, &running)?;
        if !["available", "unavailable", "degraded"].contains(&backend) {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(Self {
            data: Arc::new(Mutex::new(Data {
                daemon_owner: String::new(),
                generation: String::new(),
                snapshots: None,
                applications,
                running,
            })),
            handler: Arc::new(handler),
            backend: backend.into(),
            registrations: tokio::sync::watch::channel(None).0,
            running: Arc::new(AtomicBool::new(false)),
            updates: tokio::sync::watch::channel((String::new(), 0)).0,
        })
    }
    /// Subscribe before starting run(). None means unregistered/unavailable;
    /// Some carries the same connection/unique owner used to register the provider.
    pub fn registrations(&self) -> tokio::sync::watch::Receiver<Option<RegistrationContext>> {
        self.registrations.subscribe()
    }
    pub async fn replace(
        &self,
        applications: Vec<Application>,
        running: Vec<RunningApplication>,
    ) -> Result<(), ErrorCode> {
        arinn_protocol::application::validate_entries(&applications, &running)?;
        let (generation, revision) = {
            let mut data = self.data.lock().await;
            let revision = if let Some(snapshots) = data.snapshots.as_mut() {
                snapshots.publish(applications.clone(), running.clone())?
            } else {
                0
            };
            data.applications = applications;
            data.running = running;
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
    async fn command(
        &self,
        action: Action,
        generation: &str,
        id: &str,
        header: Header<'_>,
    ) -> production::Result<()> {
        self.authorized(&header).await?;
        validation::id(id)?;
        let data = self.data.lock().await;
        if data.generation != generation {
            return Err(ErrorCode::Conflict.into());
        }
        if self.backend == "unavailable" {
            return Err(ErrorCode::Unavailable.into());
        }
        let allowed = match action {
            Action::Launch => data
                .applications
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.can_launch),
            Action::Activate => data
                .running
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.can_activate),
            Action::Terminate => data
                .running
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.can_terminate),
        }
        .ok_or(ErrorCode::NotFound)?;
        if !allowed {
            return Err(ErrorCode::Unsupported.into());
        }
        drop(data);
        self.handler
            .execute(action, id.into())
            .await
            .map_err(Into::into)
    }
}
#[zbus::interface(name = "org.arinn.Provider1")]
impl ApplicationService {
    async fn get_applications(
        &self,
        token: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<(ApplicationPage,)> {
        self.authorized(&h).await?;
        let owner = h.sender().ok_or(ErrorCode::PermissionDenied)?.as_str();
        Ok((self
            .data
            .lock()
            .await
            .snapshots
            .as_mut()
            .ok_or(ErrorCode::Unavailable)?
            .page(owner, token, Instant::now())?,))
    }
    async fn backend_state(&self, #[zbus(header)] h: Header<'_>) -> production::Result<String> {
        self.authorized(&h).await?;
        Ok(self.backend.clone())
    }
    async fn launch(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        self.command(Action::Launch, generation, id, h).await
    }
    async fn activate(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        self.command(Action::Activate, generation, id, h).await
    }
    async fn terminate(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        self.command(Action::Terminate, generation, id, h).await
    }
    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}

/// Connect using the launch context created by arinnd. No mutating application
/// command is retried. Re-registration always replaces the complete generation.
pub async fn run(
    service: ApplicationService,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = zbus::connection::Builder::session()?.build().await?;
    run_on(connection, service).await
}

/// Use an explicit bus connection (also useful for private-bus test fixtures).
/// A service may be run only once at a time; callers own the task lifetime.
pub async fn run_on(
    connection: zbus::Connection,
    service: ApplicationService,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if service.running.swap(true, Ordering::AcqRel) {
        return Err("provider service already running".into());
    }
    let _registration_guard = RegistrationGuard {
        registrations: service.registrations.clone(),
        running: service.running.clone(),
    };
    service.registrations.send_replace(None);
    {
        let mut data = service.data.lock().await;
        data.snapshots = None;
        data.generation.clear();
        data.daemon_owner.clear();
    }
    let provider_id = std::env::var("ARINN_PROVIDER_ID")?;
    let plugin_id = std::env::var("ARINN_PLUGIN_ID")?;
    let version = std::env::var("ARINN_PLUGIN_VERSION")?;
    let token_file = std::env::var("ARINN_TOKEN_FILE")?;
    if !connection
        .object_server()
        .at(production::PROVIDER_PATH, service.clone())
        .await?
    {
        return Err("provider endpoint already exported".into());
    }
    let daemon = ProductionDaemonProxy::new(&connection).await?;
    let bus = zbus::fdo::DBusProxy::new(&connection).await?;
    let mut owners = bus.receive_name_owner_changed().await?;
    let mut updates = service.updates.subscribe();
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            result=updates.changed()=> {
                if result.is_err(){return Ok(());}
                let (generation,revision)=updates.borrow_and_update().clone();
                let emitter=SignalEmitter::new(&connection,production::PROVIDER_PATH)?;
                ApplicationService::state_changed(&emitter,&generation,revision).await?;
                continue;
            },
            _=tick.tick()=>{},
            event=owners.next()=> {
                let Some(event)=event else {return Ok(());};
                if event.args().is_ok_and(|a|a.name.as_str()==production::SERVICE) {
                    let current=bus.get_name_owner(production::SERVICE.try_into()?).await.ok().map(|name|name.to_string());
                    let mut data=service.data.lock().await;
                    if current.as_deref()!=Some(data.daemon_owner.as_str()) {
                        data.snapshots=None; data.generation.clear(); data.daemon_owner.clear();
                        service.registrations.send_replace(None);
                    }
                }
            }
        }
        if service.data.lock().await.snapshots.is_some() {
            continue;
        }
        let Ok(owner) = bus.get_name_owner(production::SERVICE.try_into()?).await else {
            continue;
        };
        service.data.lock().await.daemon_owner = owner.to_string();
        let path = token_file.clone();
        let token = tokio::task::spawn_blocking(move || std::fs::read_to_string(path)).await?;
        let Ok(token) = token else {
            continue;
        };
        if token.len() > 256 {
            continue;
        }
        let request = Registration {
            plugin_id: plugin_id.clone(),
            provider_id: provider_id.clone(),
            api_major: 1,
            api_minor: 0,
            version: version.clone(),
            capabilities: vec!["application".into()],
            object_path: production::PROVIDER_PATH.try_into()?,
            token,
        };
        let result = tokio::time::timeout(Duration::from_secs(3), daemon.register(request)).await;
        let Ok(Ok((reply,))) = result else {
            continue;
        };
        // Drop a reply from a daemon that lost its bus name during registration.
        if bus
            .get_name_owner(production::SERVICE.try_into()?)
            .await
            .ok()
            .as_ref()
            != Some(&owner)
        {
            continue;
        }
        let mut data = service.data.lock().await;
        let mut snapshots = Snapshots::new(&reply.generation)?;
        snapshots.publish(data.applications.clone(), data.running.clone())?;
        data.generation = reply.generation.clone();
        data.snapshots = Some(snapshots);
        service
            .registrations
            .send_replace(Some(RegistrationContext {
                connection: connection.clone(),
                daemon_owner: owner.to_string(),
                daemon_epoch: reply.epoch.clone(),
                generation: reply.generation.clone(),
            }));
        drop(data);
        let emitter = SignalEmitter::new(&connection, production::PROVIDER_PATH)?;
        ApplicationService::state_changed(&emitter, &reply.generation, 1).await?;
    }
}
