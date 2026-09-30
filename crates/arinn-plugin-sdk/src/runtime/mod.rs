// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::snapshots::{Snapshots, TabSnapshots};
use arinn_protocol::{
    application::{Application, ApplicationPage, RunningApplication},
    browser::{BrowserWindow, Tab, TabPage},
    production::{self, ProductionDaemonProxy, Registration},
    validation::{self, ErrorCode},
    web,
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

/// What the shared registration loop needs from a capability service.
trait Endpoint: Clone + Send + Sync + zbus::object_server::Interface {
    const CAPABILITY: &'static str;
    fn registrations(&self) -> &tokio::sync::watch::Sender<Option<RegistrationContext>>;
    fn running(&self) -> &Arc<AtomicBool>;
    fn updates(&self) -> &tokio::sync::watch::Sender<(String, u64)>;
    /// Forget the registration: no snapshots, generation or daemon owner.
    fn clear(&self) -> impl Future<Output = ()> + Send;
    fn daemon_owner(&self) -> impl Future<Output = String> + Send;
    fn set_daemon_owner(&self, owner: String) -> impl Future<Output = ()> + Send;
    fn registered(&self) -> impl Future<Output = bool> + Send;
    /// Create the snapshot store for a new generation with the current entries.
    fn install(&self, generation: &str) -> impl Future<Output = Result<(), ErrorCode>> + Send;
    fn emit(
        &self,
        connection: &zbus::Connection,
        generation: &str,
        revision: u64,
    ) -> impl Future<Output = zbus::Result<()>> + Send;
    /// Export the provider object. Media and library also export optional
    /// extension interfaces on the same path (ADR-0054).
    fn export(
        &self,
        connection: &zbus::Connection,
    ) -> impl Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send {
        let connection = connection.clone();
        let service = self.clone();
        async move {
            if !connection
                .object_server()
                .at(production::PROVIDER_PATH, service)
                .await?
            {
                return Err("provider endpoint already exported".into());
            }
            Ok(())
        }
    }
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
    run_endpoint(connection, service).await
}

/// [`run`] for a `browser` provider (ADR-0048).
pub async fn run_browser(
    service: BrowserService,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = zbus::connection::Builder::session()?.build().await?;
    run_browser_on(connection, service).await
}

/// [`run_on`] for a `browser` provider (ADR-0048).
pub async fn run_browser_on(
    connection: zbus::Connection,
    service: BrowserService,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_endpoint(connection, service).await
}

async fn run_endpoint<E: Endpoint>(
    connection: zbus::Connection,
    service: E,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if service.running().swap(true, Ordering::AcqRel) {
        return Err("provider service already running".into());
    }
    let _registration_guard = RegistrationGuard {
        registrations: service.registrations().clone(),
        running: service.running().clone(),
    };
    service.registrations().send_replace(None);
    service.clear().await;
    let provider_id = std::env::var("ARINN_PROVIDER_ID")?;
    let plugin_id = std::env::var("ARINN_PLUGIN_ID")?;
    let version = std::env::var("ARINN_PLUGIN_VERSION")?;
    let token_file = std::env::var("ARINN_TOKEN_FILE")?;
    service.export(&connection).await?;
    let daemon = ProductionDaemonProxy::new(&connection).await?;
    let bus = zbus::fdo::DBusProxy::new(&connection).await?;
    // Only daemon ownership affects registration. Unrelated name churn can
    // otherwise fill this stream while Register awaits its reply and block
    // the shared connection from delivering that reply.
    let mut owners = bus
        .receive_name_owner_changed_with_args(&[(0, production::SERVICE)])
        .await?;
    let mut updates = service.updates().subscribe();
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            result=updates.changed()=> {
                if result.is_err(){return Ok(());}
                let (generation,revision)=updates.borrow_and_update().clone();
                service.emit(&connection,&generation,revision).await?;
                continue;
            },
            _=tick.tick()=>{},
            event=owners.next()=> {
                let Some(event)=event else {return Ok(());};
                if event.args().is_ok_and(|a|a.name.as_str()==production::SERVICE) {
                    let current=bus.get_name_owner(production::SERVICE.try_into()?).await.ok().map(|name|name.to_string());
                    if current.as_deref()!=Some(service.daemon_owner().await.as_str()) {
                        service.clear().await;
                        service.registrations().send_replace(None);
                    }
                }
            }
        }
        if service.registered().await {
            continue;
        }
        let Ok(owner) = bus.get_name_owner(production::SERVICE.try_into()?).await else {
            continue;
        };
        service.set_daemon_owner(owner.to_string()).await;
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
            capabilities: vec![E::CAPABILITY.into()],
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
        service.install(&reply.generation).await?;
        service
            .registrations()
            .send_replace(Some(RegistrationContext {
                connection: connection.clone(),
                daemon_owner: owner.to_string(),
                daemon_epoch: reply.epoch.clone(),
                generation: reply.generation.clone(),
            }));
        service.emit(&connection, &reply.generation, 1).await?;
    }
}

impl Endpoint for ApplicationService {
    const CAPABILITY: &'static str = "application";
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
        let mut snapshots = Snapshots::new(generation)?;
        snapshots.publish(data.applications.clone(), data.running.clone())?;
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
        let emitter = SignalEmitter::new(connection, production::PROVIDER_PATH)?;
        ApplicationService::state_changed(&emitter, generation, revision).await
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TabAction {
    Activate,
    Close,
}
/// Executes browser commands after the SDK has checked the caller, the
/// generation, the backend state and the tab ID or URL.
pub trait BrowserHandler: Send + Sync + 'static {
    fn tab(
        &self,
        action: TabAction,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>>;
    fn open(&self, url: String)
    -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>>;
}
struct BrowserData {
    daemon_owner: String,
    generation: String,
    snapshots: Option<TabSnapshots>,
    windows: Vec<BrowserWindow>,
    tabs: Vec<Tab>,
    backend: String,
    /// Last published revision of the current generation.
    revision: u64,
}
/// `org.arinn.BrowserProvider1` (ADR-0048). Unlike applications, the backend
/// state changes at runtime: a browser connects and disconnects.
#[derive(Clone)]
pub struct BrowserService {
    data: Arc<Mutex<BrowserData>>,
    handler: Arc<dyn BrowserHandler>,
    registrations: tokio::sync::watch::Sender<Option<RegistrationContext>>,
    running: Arc<AtomicBool>,
    updates: tokio::sync::watch::Sender<(String, u64)>,
}
fn backend_state(value: &str) -> Result<(), ErrorCode> {
    if ["available", "unavailable", "degraded"].contains(&value) {
        Ok(())
    } else {
        Err(ErrorCode::InvalidArgument)
    }
}
impl BrowserService {
    pub fn new(
        handler: impl BrowserHandler,
        windows: Vec<BrowserWindow>,
        tabs: Vec<Tab>,
        backend: &str,
    ) -> Result<Self, ErrorCode> {
        arinn_protocol::browser::validate_entries(&windows, &tabs)?;
        backend_state(backend)?;
        Ok(Self {
            data: Arc::new(Mutex::new(BrowserData {
                daemon_owner: String::new(),
                generation: String::new(),
                snapshots: None,
                windows,
                tabs,
                backend: backend.into(),
                revision: 0,
            })),
            handler: Arc::new(handler),
            registrations: tokio::sync::watch::channel(None).0,
            running: Arc::new(AtomicBool::new(false)),
            updates: tokio::sync::watch::channel((String::new(), 0)).0,
        })
    }
    /// See [`ApplicationService::registrations`].
    pub fn registrations(&self) -> tokio::sync::watch::Receiver<Option<RegistrationContext>> {
        self.registrations.subscribe()
    }
    /// Publish a complete replacement of windows, tabs and the backend state.
    /// A changed backend state alone also publishes a new revision, so the
    /// daemon re-reads `BackendState` on `StateChanged`.
    pub async fn replace(
        &self,
        windows: Vec<BrowserWindow>,
        tabs: Vec<Tab>,
        backend: &str,
    ) -> Result<(), ErrorCode> {
        arinn_protocol::browser::validate_entries(&windows, &tabs)?;
        backend_state(backend)?;
        let (generation, revision) = {
            let mut data = self.data.lock().await;
            let backend_changed = data.backend != backend;
            let before = data.revision;
            let revision = if let Some(snapshots) = data.snapshots.as_mut() {
                let mut revision = snapshots.publish(windows.clone(), tabs.clone())?;
                if backend_changed && revision == before {
                    // Same entries, new backend state: still a new revision.
                    revision = snapshots.touch()?;
                }
                revision
            } else {
                0
            };
            if revision > 0 {
                data.revision = revision;
            }
            data.windows = windows;
            data.tabs = tabs;
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
    async fn tab_command(
        &self,
        action: TabAction,
        generation: &str,
        id: &str,
        header: Header<'_>,
    ) -> production::Result<()> {
        validation::id(id)?;
        self.ready(generation, &header).await?;
        if !self.data.lock().await.tabs.iter().any(|t| t.id == id) {
            return Err(ErrorCode::NotFound.into());
        }
        self.handler
            .tab(action, id.into())
            .await
            .map_err(Into::into)
    }
}
#[zbus::interface(name = "org.arinn.BrowserProvider1")]
impl BrowserService {
    async fn get_tabs(
        &self,
        token: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<(TabPage,)> {
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
        Ok(self.data.lock().await.backend.clone())
    }
    async fn open_url(
        &self,
        generation: &str,
        url: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        web::open_url(url)?;
        self.ready(generation, &h).await?;
        self.handler.open(url.into()).await.map_err(Into::into)
    }
    async fn activate_tab(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        self.tab_command(TabAction::Activate, generation, id, h)
            .await
    }
    async fn close_tab(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        self.tab_command(TabAction::Close, generation, id, h).await
    }
    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}
impl Endpoint for BrowserService {
    const CAPABILITY: &'static str = "browser";
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
        let mut snapshots = TabSnapshots::new(generation)?;
        data.revision = snapshots.publish(data.windows.clone(), data.tabs.clone())?;
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
        let emitter = SignalEmitter::new(connection, production::PROVIDER_PATH)?;
        BrowserService::state_changed(&emitter, generation, revision).await
    }
}

mod assets;
mod library;
mod media;
pub use assets::AssetHandler;
pub use library::{
    LibraryExtensions, LibraryHandler, LibraryImportHandler, LibraryResumeHandler, LibraryService,
    run_library, run_library_on,
};
pub use media::{
    MediaAction, MediaExtensions, MediaHandler, MediaOpenHandler, MediaService,
    MediaTimelineHandler, MediaTracksHandler, run_media, run_media_on,
};
