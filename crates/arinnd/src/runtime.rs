// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{
    operations::{Operations, now},
    registry::{Manifest, Registry, discovery_roots},
    storage::{Config, Paths, Store, atomic_write, private_dir},
    supervisor::Supervisor,
};
use arinn_plugin_sdk::snapshots::{Snapshots, TabSnapshots};
use arinn_protocol::{
    application::{Application, ApplicationPage, RunningApplication, validate_entries},
    browser::{self, BrowserProviderProxy, BrowserWindow, Tab, TabPage},
    production::{
        self, ApplicationProviderProxy, EntityRef, ProviderInfo, Registration, RegistrationReply,
    },
    validation::{self, ErrorCode},
};
use futures_util::StreamExt;
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use uuid::Uuid;
use zbus::{Connection, message::Header, object_server::SignalEmitter, zvariant::OwnedObjectPath};

/// Opening a URL with the browser closed: launch, connection and open share
/// the launch timeout of spec §9.5 (ADR-0055).
const BROWSER_LAUNCH_DEADLINE: Duration = Duration::from_secs(30);
/// How long opening a URL waits for the browser's next snapshot before it
/// only hides Home (ADR-0052).
const OPEN_FOCUS_WAIT: Duration = Duration::from_secs(2);

struct Provider {
    info: ProviderInfo,
    manifest: Option<Manifest>,
    unit: String,
    token_file: PathBuf,
    token: String,
    owner: String,
    cache: Option<Snapshots>,
    applications: Vec<Application>,
    running: Vec<RunningApplication>,
    /// `browser` providers (ADR-0048): tab snapshot cache and entries.
    tab_cache: Option<TabSnapshots>,
    windows: Vec<BrowserWindow>,
    tabs: Vec<Tab>,
    /// `media` providers (ADR-0054, ADR-0058): session cache, sessions and
    /// the optional interfaces the provider declared.
    session_cache: Option<arinn_plugin_sdk::snapshots::SessionSnapshots>,
    sessions: Vec<arinn_protocol::media::Session>,
    features: Vec<String>,
    /// `library` providers: catalogue and resume caches and entries.
    library_cache: Option<arinn_plugin_sdk::snapshots::LibrarySnapshots>,
    collections: Vec<arinn_protocol::library::Collection>,
    items: Vec<arinn_protocol::library::Item>,
    resume_cache: Option<arinn_plugin_sdk::snapshots::ResumeSnapshots>,
    resumes: Vec<arinn_protocol::library::Resume>,
    /// The provider's settings schema for one generation (ADR-0065).
    settings_schema: Option<(String, Vec<arinn_protocol::settings::Setting>)>,
    started: Instant,
    enabled: bool,
}
impl Provider {
    /// The single capability a provider serves: `application`, `browser`,
    /// `media` or `library`.
    fn capability(&self) -> &str {
        self.info
            .capabilities
            .first()
            .map(String::as_str)
            .unwrap_or("")
    }
}
mod assets;
mod browse_routing;
mod download_routing;
mod library_routing;
mod media_routing;
mod progress;
mod settings_routing;

/// Capability sets the daemon routes; anything else is incompatible.
const ROUTED: &[&str] = &["application", "browser", "media", "library", "download"];
struct State {
    providers: BTreeMap<String, Provider>,
    error: String,
    listings: crate::listings::Listings,
}
#[derive(Clone)]
pub struct Core {
    state: Arc<Mutex<State>>,
    reload_lock: Arc<Mutex<()>>,
    maintenance: Arc<Mutex<()>>,
    // Serializes token rotation so the token file and the registry agree
    // (#66): a vanished owner's `unavailable` can race `launch` in a reload.
    rotation: Arc<Mutex<()>>,
    // One browser launch at a time: concurrent opens wait for it (ADR-0055).
    browser_launch: Arc<Mutex<()>>,
    /// Last forwarded playback progress per media session (ADR-0061).
    progress: Arc<Mutex<progress::Progress>>,
    /// Serializes settings writes to config.toml (ADR-0065).
    settings_lock: Arc<Mutex<()>>,
    download_import_lock: Arc<Mutex<()>>,
    downloads: Arc<Mutex<BTreeMap<String, download_routing::DownloadState>>>,
    /// Packaged search engines for `Browse1` (ADR-0066).
    search_engines: Arc<Vec<crate::browse::Engine>>,
    pub operations: Operations,
    pub connection: Connection,
    pub epoch: String,
    paths: Paths,
    runtime_dir: PathBuf,
    platform: crate::platform::Platform,
    windows: crate::windows::Windows,
}
fn sender(h: &Header<'_>) -> Result<String, ErrorCode> {
    h.sender()
        .map(ToString::to_string)
        .ok_or(ErrorCode::PermissionDenied)
}
fn failed(info: &mut ProviderInfo, error: ErrorCode) {
    info.last_error = error.to_string();
    info.error_at = now();
    info.recovered_at = 0;
}
impl Core {
    pub fn new(connection: &Connection, paths: Paths, runtime_dir: PathBuf) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                providers: BTreeMap::new(),
                error: String::new(),
                listings: crate::listings::Listings::default(),
            })),
            reload_lock: Arc::new(Mutex::new(())),
            maintenance: Arc::new(Mutex::new(())),
            rotation: Arc::new(Mutex::new(())),
            browser_launch: Arc::new(Mutex::new(())),
            progress: Arc::default(),
            settings_lock: Arc::default(),
            download_import_lock: Arc::default(),
            downloads: Arc::default(),
            search_engines: Arc::default(),
            operations: Operations::default(),
            connection: connection.clone(),
            epoch: Uuid::new_v4().to_string(),
            paths,
            runtime_dir,
            platform: crate::platform::Platform::default(),
            windows: crate::windows::Windows::default(),
        }
    }
    /// Remove every daemon interface a provider path can carry, whatever the
    /// provider's capability was.
    async fn remove_endpoints(&self, path: &OwnedObjectPath) {
        let server = self.connection.object_server();
        let _ = server.remove::<ApplicationEndpoint, _>(path.clone()).await;
        let _ = server.remove::<BrowserEndpoint, _>(path.clone()).await;
        let _ = server
            .remove::<download_routing::DownloadsEndpoint, _>(path.clone())
            .await;
        let _ = server
            .remove::<media_routing::MediaEndpoint, _>(path.clone())
            .await;
        let _ = server
            .remove::<library_routing::LibraryEndpoint, _>(path.clone())
            .await;
        let _ = server
            .remove::<assets::AssetsEndpoint, _>(path.clone())
            .await;
    }
    pub async fn export_platform(&self) -> Result<(), ErrorCode> {
        self.windows.start(&self.connection).await?;
        self.connection
            .object_server()
            .at(production::PATH, self.windows.clone())
            .await
            .map_err(|_| ErrorCode::Internal)?;
        self.connection
            .object_server()
            .at(production::PATH, WindowEndpoint { core: self.clone() })
            .await
            .map_err(|_| ErrorCode::Internal)?;
        Ok(())
    }
    pub async fn start(&self) {
        let core = self.clone();
        let (ready, wait) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            core.monitor(ready).await;
        });
        if wait.await.is_err() {
            self.state.lock().await.error = ErrorCode::Unavailable.to_string();
            return;
        }
        if let Err(error) = self.reload_inner().await {
            self.state.lock().await.error = error.to_string();
        }
    }
    async fn prepare(&self) -> Result<(BTreeMap<String, Provider>, Vec<String>), ErrorCode> {
        let paths = self.paths.clone();
        let runtime = self.runtime_dir.clone();
        tokio::task::spawn_blocking(move || {
            let config = Config::load(&paths.config)?;
            let mut store = Store::open(&paths.state)?;
            private_dir(&runtime)?;
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .ok_or(ErrorCode::InvalidArgument)?;
            let roots = discovery_roots(
                &home,
                std::env::var_os("XDG_DATA_HOME").as_deref(),
                std::env::var_os("XDG_DATA_DIRS").as_deref(),
            );
            let registry = Registry::discover(&roots);
            let mut instances: BTreeMap<String, (String, bool)> = registry
                .providers
                .keys()
                .map(|id| (id.clone(), (id.clone(), true)))
                .collect();
            for (id, instance) in &config.providers {
                instances.insert(id.clone(), (instance.plugin_id.clone(), instance.enabled));
            }
            let mut providers = BTreeMap::new();
            for (id, (plugin_id, enabled)) in instances {
                let record = registry.providers.get(&plugin_id);
                let manifest = record.and_then(|r| r.manifest.clone());
                let uuid = store.provider_uuid(&id)?;
                let uuid = Uuid::parse_str(&uuid)
                    .map_err(|_| ErrorCode::Internal)?
                    .simple()
                    .to_string();
                let unit = format!("arinn-provider-{uuid}.service");
                let token_file = runtime.join(format!("{uuid}.token"));
                let error = record.and_then(|r| r.error).or_else(|| {
                    if record.is_none() {
                        Some(ErrorCode::NotFound)
                    } else {
                        None
                    }
                });
                let mut info = ProviderInfo {
                    id: id.clone(),
                    plugin_id,
                    source_path: record
                        .and_then(|r| r.sources.first())
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    name: manifest
                        .as_ref()
                        .map(|m| m.name.clone())
                        .unwrap_or_default(),
                    version: manifest
                        .as_ref()
                        .map(|m| m.version.clone())
                        .unwrap_or_default(),
                    path: format!("/org/arinn/Daemon1/providers/p_{uuid}")
                        .try_into()
                        .unwrap(),
                    capabilities: manifest
                        .as_ref()
                        .map(|m| m.capabilities.clone())
                        .unwrap_or_default(),
                    runtime_state: if error.is_some() {
                        "incompatible"
                    } else if enabled {
                        "discovered"
                    } else {
                        "stopped"
                    }
                    .into(),
                    backend_state: "unavailable".into(),
                    generation: String::new(),
                    revision: 0,
                    last_error: String::new(),
                    error_at: 0,
                    recovered_at: 0,
                    restarts_since_daemon_start: 0,
                    cumulative_restart_count: store.restart_count(&id)?,
                };
                if let Some(error) = error {
                    failed(&mut info, error);
                }
                if manifest.as_ref().is_some_and(|m| {
                    m.capabilities.len() != 1 || !ROUTED.contains(&m.capabilities[0].as_str())
                }) {
                    info.runtime_state = "incompatible".into();
                    failed(&mut info, ErrorCode::Unsupported);
                }
                providers.insert(
                    id,
                    Provider {
                        info,
                        manifest,
                        unit,
                        token_file,
                        token: String::new(),
                        owner: String::new(),
                        cache: None,
                        applications: Vec::new(),
                        running: Vec::new(),
                        tab_cache: None,
                        session_cache: None,
                        sessions: Vec::new(),
                        features: Vec::new(),
                        library_cache: None,
                        collections: Vec::new(),
                        items: Vec::new(),
                        resume_cache: None,
                        resumes: Vec::new(),
                        settings_schema: None,
                        windows: Vec::new(),
                        tabs: Vec::new(),
                        started: Instant::now(),
                        enabled,
                    },
                );
            }
            Ok((
                providers,
                registry
                    .diagnostics
                    .iter()
                    .map(|d| d.error.to_string())
                    .collect(),
            ))
        })
        .await
        .map_err(|_| ErrorCode::Internal)?
    }
    async fn reload_inner(&self) -> Result<(), ErrorCode> {
        let _serial = self.reload_lock.lock().await;
        let (mut next, diagnostics) = self.prepare().await?;
        let supervisor = Supervisor::new(&self.connection);
        let existing: Vec<_> = self.state.lock().await.providers.keys().cloned().collect();
        for id in existing.iter().filter(|id| !next.contains_key(*id)) {
            let (unit, path, owner) = {
                let state = self.state.lock().await;
                let old = &state.providers[id];
                (old.unit.clone(), old.info.path.clone(), old.owner.clone())
            };
            supervisor.stop(&unit).await?;
            self.unavailable(id, &owner).await;
            self.state.lock().await.providers.remove(id);
            self.remove_endpoints(&path).await;
        }
        for (id, new) in next.iter_mut() {
            let old = {
                let state = self.state.lock().await;
                state.providers.get(id).map(|p| {
                    (
                        p.enabled,
                        p.manifest.clone(),
                        p.unit.clone(),
                        p.owner.clone(),
                    )
                })
            };
            if let Some((enabled, manifest, unit, owner)) = old {
                if enabled == new.enabled
                    && manifest
                        .as_ref()
                        .zip(new.manifest.as_ref())
                        .is_some_and(|(a, b)| !a.requires_restart(b))
                {
                    let mut state = self.state.lock().await;
                    let keep = state.providers.get_mut(id).unwrap();
                    keep.info.name = new.info.name.clone();
                    keep.info.version = new.info.version.clone();
                    keep.info.source_path = new.info.source_path.clone();
                    keep.manifest = new.manifest.clone();
                    continue;
                }
                supervisor.stop(&unit).await?;
                self.unavailable(id, &owner).await;
            }
            let server = self.connection.object_server();
            let exported = if new.capability() == "download" {
                server
                    .at(
                        new.info.path.clone(),
                        download_routing::DownloadsEndpoint {
                            core: self.clone(),
                            id: id.clone(),
                        },
                    )
                    .await
            } else if new.capability() == "library" {
                server
                    .at(
                        new.info.path.clone(),
                        library_routing::LibraryEndpoint {
                            core: self.clone(),
                            id: id.clone(),
                        },
                    )
                    .await
            } else if new.capability() == "media" {
                server
                    .at(
                        new.info.path.clone(),
                        media_routing::MediaEndpoint {
                            core: self.clone(),
                            id: id.clone(),
                        },
                    )
                    .await
            } else if new.capability() == "browser" {
                server
                    .at(
                        new.info.path.clone(),
                        BrowserEndpoint {
                            core: self.clone(),
                            id: id.clone(),
                        },
                    )
                    .await
            } else {
                server
                    .at(
                        new.info.path.clone(),
                        ApplicationEndpoint {
                            core: self.clone(),
                            id: id.clone(),
                        },
                    )
                    .await
            };
            exported.map_err(|_| ErrorCode::Internal)?;
            // Media and library asset ids resolve through Assets1 (ADR-0062).
            if ["media", "library"].contains(&new.capability()) {
                server
                    .at(
                        new.info.path.clone(),
                        assets::AssetsEndpoint {
                            core: self.clone(),
                            id: id.clone(),
                        },
                    )
                    .await
                    .map_err(|_| ErrorCode::Internal)?;
            }
        }
        for (id, new) in next {
            let replace = {
                let state = self.state.lock().await;
                !state.providers.get(&id).is_some_and(|p| {
                    p.enabled == new.enabled
                        && p.manifest
                            .as_ref()
                            .zip(new.manifest.as_ref())
                            .is_some_and(|(a, b)| !a.requires_restart(b))
                })
            };
            if !replace {
                continue;
            }
            let start = new.enabled && new.info.runtime_state != "incompatible";
            self.state.lock().await.providers.insert(id.clone(), new);
            if start
                && let Err(error) = self.launch(&id).await
                && let Some(p) = self.state.lock().await.providers.get_mut(&id)
            {
                p.info.runtime_state = "failed".into();
                failed(&mut p.info, error);
            }
        }
        self.state.lock().await.error = diagnostics.join(",");
        self.changed().await;
        Ok(())
    }
    async fn rotate_token(&self, id: &str) -> Result<(), ErrorCode> {
        // Held across the file write and the registry update: the last token
        // written is always the one Register accepts.
        let _rotation = self.rotation.lock().await;
        let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let path = self
            .state
            .lock()
            .await
            .providers
            .get(id)
            .ok_or(ErrorCode::NotFound)?
            .token_file
            .clone();
        let value = token.clone();
        tokio::task::spawn_blocking(move || atomic_write(&path, value.as_bytes()))
            .await
            .map_err(|_| ErrorCode::Internal)??;
        let mut state = self.state.lock().await;
        let p = state.providers.get_mut(id).ok_or(ErrorCode::NotFound)?;
        p.token = token;
        p.started = Instant::now();
        Ok(())
    }
    async fn launch(&self, id: &str) -> Result<(), ErrorCode> {
        self.rotate_token(id).await?;
        let (unit, manifest, token_file) = {
            let mut state = self.state.lock().await;
            let p = state.providers.get_mut(id).ok_or(ErrorCode::NotFound)?;
            p.info.runtime_state = "starting".into();
            (
                p.unit.clone(),
                p.manifest.clone().ok_or(ErrorCode::Unsupported)?,
                p.token_file.clone(),
            )
        };
        let paths = self.paths.clone();
        let provider_id = id.to_string();
        let (data, state) = tokio::task::spawn_blocking(move || paths.provider_dirs(&provider_id))
            .await
            .map_err(|_| ErrorCode::Internal)??;
        // Settings are read at each start: a changed config.toml reaches the
        // provider on its next start (ADR-0064).
        let settings = Config::load(&self.paths.config)
            .ok()
            .and_then(|c| c.providers.get(id).map(|i| i.settings.clone()))
            .unwrap_or_default();
        let env = provider_env(id, &manifest, &token_file, &data, &state, &settings)?;
        let created = Supervisor::new(&self.connection)
            .start(&unit, &manifest, env)
            .await?;
        if created {
            let state = self.paths.state.clone();
            let id = id.to_string();
            tokio::task::spawn_blocking(move || Store::open(&state)?.reset_supervisor_count(&id))
                .await
                .map_err(|_| ErrorCode::Internal)??;
        }
        Ok(())
    }
    async fn changed(&self) {
        if let Ok(emitter) = SignalEmitter::new(&self.connection, production::PATH) {
            let _ = Daemon::providers_changed(&emitter).await;
        }
    }
    async fn unavailable(&self, id: &str, owner: &str) {
        {
            let mut state = self.state.lock().await;
            if let Some(p) = state.providers.get_mut(id) {
                if p.owner != owner {
                    return;
                }
                p.owner.clear();
                p.cache = None;
                p.applications.clear();
                p.running.clear();
                p.tab_cache = None;
                p.windows.clear();
                p.tabs.clear();
                p.session_cache = None;
                p.sessions.clear();
                self.downloads.lock().await.remove(id);
                p.features.clear();
                p.library_cache = None;
                p.collections.clear();
                p.items.clear();
                p.resume_cache = None;
                p.resumes.clear();
                p.settings_schema = None;
                p.info.revision = 0;
                p.info.backend_state = "unavailable".into();
                p.info.runtime_state = "restarting".into();
                failed(&mut p.info, ErrorCode::Unavailable);
            } else {
                return;
            }
        }
        self.windows.disconnect(owner).await;
        self.operations
            .invalidate(&self.connection, Some(id), None)
            .await;
        let _ = self.rotate_token(id).await;
        self.changed().await;
    }
    async fn monitor(&self, ready: tokio::sync::oneshot::Sender<()>) {
        let Ok(bus) = zbus::fdo::DBusProxy::new(&self.connection).await else {
            return;
        };
        let Ok(mut owners) = bus.receive_name_owner_changed().await else {
            return;
        };
        let _ = ready.send(());
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                event=owners.next()=> {
                    let Some(event)=event else{return;};let Ok(args)=event.args() else{continue;};
                    if args.new_owner.is_some(){continue;}
                    let name=args.name.as_str();
                    {let mut state=self.state.lock().await;state.listings.disconnect(name);for p in state.providers.values_mut(){if let Some(cache)=p.cache.as_mut(){cache.disconnect(name);}
                    if let Some(cache)=p.tab_cache.as_mut(){cache.disconnect(name);}}}
                    let ids:Vec<_>=self.state.lock().await.providers.iter().filter(|(_,p)|p.owner==name).map(|(id,_)|id.clone()).collect();
                    for id in ids {self.unavailable(&id,name).await;}
                    self.operations.invalidate(&self.connection,None,Some(name)).await;
                }
                _=tick.tick()=> {
                    let core=self.clone();tokio::spawn(async move {
                        if let Ok(_guard)=core.maintenance.try_lock(){core.maintain().await;}
                    });
                }
            }
        }
    }
    async fn maintain(&self) {
        let units: Vec<_> = self
            .state
            .lock()
            .await
            .providers
            .iter()
            .filter(|(_, p)| p.enabled && p.info.runtime_state != "incompatible")
            .map(|(id, p)| (id.clone(), p.unit.clone()))
            .collect();
        for (id, unit) in units {
            if let Ok(observed) = Supervisor::new(&self.connection).restarts(&unit).await {
                let state_dir = self.paths.state.clone();
                let provider_id = id.clone();
                let result = tokio::task::spawn_blocking(move || {
                    Store::open(&state_dir)?.observe_restarts(&provider_id, u64::from(observed))
                })
                .await;
                if let Ok(Ok((total, delta))) = result
                    && let Some(p) = self.state.lock().await.providers.get_mut(&id)
                {
                    p.info.cumulative_restart_count = total;
                    p.info.restarts_since_daemon_start += delta;
                }
            }
        }
        let candidates: Vec<_> = self
            .state
            .lock()
            .await
            .providers
            .iter()
            .filter(|(_, p)| ["starting", "restarting"].contains(&p.info.runtime_state.as_str()))
            .map(|(id, p)| (id.clone(), p.unit.clone(), p.started.elapsed()))
            .collect();
        for (id, unit, elapsed) in candidates {
            let supervisor = Supervisor::new(&self.connection);
            if supervisor.state(&unit).await.is_ok_and(|s| s == "failed") {
                if let Some(p) = self.state.lock().await.providers.get_mut(&id) {
                    p.info.runtime_state = "failed".into();
                    failed(&mut p.info, ErrorCode::Unavailable);
                }
                self.changed().await;
            } else if elapsed >= Duration::from_secs(10) {
                let _ = supervisor.fail_handshake(&unit).await;
                if let Some(p) = self.state.lock().await.providers.get_mut(&id) {
                    failed(&mut p.info, ErrorCode::Timeout);
                    p.started = Instant::now();
                }
            }
        }
    }
    async fn watch_provider(&self, id: String, owner: String, generation: String) {
        let capability = self
            .state
            .lock()
            .await
            .providers
            .get(&id)
            .map(|p| p.capability().to_owned())
            .unwrap_or_default();
        if capability == "download" {
            self.watch_downloads(id, owner, generation).await;
        } else if capability == "library" {
            self.watch_library(id, owner, generation).await;
        } else if capability == "media" {
            self.watch_media(id, owner, generation).await;
        } else if capability == "browser" {
            self.watch_browser(id, owner, generation).await;
        } else {
            self.watch_applications(id, owner, generation).await;
        }
    }
    async fn watch_browser(&self, id: String, owner: String, generation: String) {
        let Ok(proxy) = BrowserProviderProxy::builder(&self.connection)
            .destination(owner.as_str())
            .and_then(|b| b.path(production::PROVIDER_PATH))
            .unwrap()
            .build()
            .await
        else {
            return;
        };
        let Ok(mut changes) = proxy.receive_provider_state_changed().await else {
            return;
        };
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {_=tick.tick()=>{},event=changes.next()=>{if event.is_none(){return;}}}
            if !self
                .state
                .lock()
                .await
                .providers
                .get(&id)
                .is_some_and(|p| p.owner == owner && p.info.generation == generation)
            {
                return;
            }
            let result =
                tokio::time::timeout(Duration::from_secs(3), read_tabs(&proxy, &generation))
                    .await
                    .unwrap_or(Err(ErrorCode::Timeout));
            let mut state = self.state.lock().await;
            let Some(p) = state.providers.get_mut(&id) else {
                return;
            };
            if p.owner != owner || p.info.generation != generation {
                return;
            }
            let mut signal = None;
            match result {
                Ok((revision, windows, tabs, backend)) => {
                    let cache = p
                        .tab_cache
                        .get_or_insert_with(|| TabSnapshots::new(&generation).unwrap());
                    match cache.import(revision, windows.clone(), tabs.clone()) {
                        Ok(()) => {
                            if revision != p.info.revision {
                                signal = Some(revision);
                            }
                            p.windows = windows;
                            p.tabs = tabs;
                            p.info.revision = revision;
                            p.info.backend_state = backend;
                            p.info.runtime_state = "ready".into();
                            if !p.info.last_error.is_empty() && p.info.recovered_at == 0 {
                                p.info.recovered_at = now();
                            }
                        }
                        Err(error) => failed(&mut p.info, error),
                    }
                }
                Err(error) => failed(&mut p.info, error),
            }
            drop(state);
            if let Some(revision) = signal {
                if let Ok(emitter) = SignalEmitter::new(&self.connection, production::PATH) {
                    let _ = Daemon::state_changed(&emitter, &id, "browser", &generation, revision)
                        .await;
                }
                self.changed().await;
            }
        }
    }
    async fn watch_applications(&self, id: String, owner: String, generation: String) {
        let Ok(proxy) = ApplicationProviderProxy::builder(&self.connection)
            .destination(owner.as_str())
            .and_then(|b| b.path(production::PROVIDER_PATH))
            .unwrap()
            .build()
            .await
        else {
            return;
        };
        let Ok(mut changes) = proxy.receive_provider_state_changed().await else {
            return;
        };
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {_=tick.tick()=>{},event=changes.next()=>{if event.is_none(){return;}}}
            if !self
                .state
                .lock()
                .await
                .providers
                .get(&id)
                .is_some_and(|p| p.owner == owner && p.info.generation == generation)
            {
                return;
            }
            let result =
                tokio::time::timeout(Duration::from_secs(3), read_snapshot(&proxy, &generation))
                    .await
                    .unwrap_or(Err(ErrorCode::Timeout));
            let mut state = self.state.lock().await;
            let Some(p) = state.providers.get_mut(&id) else {
                return;
            };
            if p.owner != owner || p.info.generation != generation {
                return;
            }
            let mut signal = None;
            match result {
                Ok((revision, apps, running, backend)) => {
                    let cache = p
                        .cache
                        .get_or_insert_with(|| Snapshots::new(&generation).unwrap());
                    match cache.import(revision, apps.clone(), running.clone()) {
                        Ok(()) => {
                            if revision != p.info.revision {
                                signal = Some(revision);
                            }
                            p.applications = apps;
                            p.running = running;
                            p.info.revision = revision;
                            p.info.backend_state = backend;
                            p.info.runtime_state = "ready".into();
                            if !p.info.last_error.is_empty() && p.info.recovered_at == 0 {
                                p.info.recovered_at = now();
                            }
                        }
                        Err(error) => failed(&mut p.info, error),
                    }
                }
                Err(error) => failed(&mut p.info, error),
            }
            drop(state);
            if let Some(revision) = signal {
                if let Ok(emitter) = SignalEmitter::new(&self.connection, production::PATH) {
                    let _ =
                        Daemon::state_changed(&emitter, &id, "application", &generation, revision)
                            .await;
                }
                self.changed().await;
            }
        }
    }
    /// Bring a browser window forward after a tab or URL was opened in it.
    /// With a validated window token the platform activates exactly that
    /// window, which also hides Home; without one Home is hidden and the
    /// browser's own focus request decides (ADR-0048).
    async fn show_browser_window(
        &self,
        id: &str,
        owner: &str,
        registration: &str,
        window_token: &str,
    ) -> Result<(), ErrorCode> {
        if window_token.is_empty() {
            let _ = self.platform.queue("dismiss").await;
            return Ok(());
        }
        let scope = crate::windows::Scope {
            provider: id.into(),
            owner: owner.into(),
            registration: registration.into(),
        };
        let generation = self.windows.current_generation().await;
        let receiver = self
            .windows
            .enqueue(
                &self.connection,
                scope,
                &generation,
                window_token,
                "activate",
            )
            .await?;
        tokio::time::timeout(Duration::from_secs(5), receiver)
            .await
            .map_err(|_| ErrorCode::Timeout)?
            .map_err(|_| ErrorCode::Unavailable)?
    }

    /// Open a URL in the default browser provider: the first ready `browser`
    /// provider with a connected browser. Used by web shortcuts (ADR-0043).
    /// With no browser connected, the browser application is launched first
    /// (ADR-0038, ADR-0055); the whole open shares one launch deadline.
    pub async fn open_url(&self, url: &str) -> Result<(), ErrorCode> {
        arinn_protocol::web::open_url(url)?;
        let deadline = tokio::time::Instant::now() + BROWSER_LAUNCH_DEADLINE;
        if !self.browser_connected().await {
            self.launch_browser(deadline).await?;
        }
        let (id, owner, generation, before, known) = {
            let state = self.state.lock().await;
            let (id, p) = state
                .providers
                .iter()
                .find(|(_, p)| {
                    p.capability() == "browser"
                        && p.info.runtime_state == "ready"
                        && p.info.backend_state != "unavailable"
                        && !p.owner.is_empty()
                })
                .ok_or(ErrorCode::Unavailable)?;
            (
                id.clone(),
                p.owner.clone(),
                p.info.generation.clone(),
                p.info.revision,
                p.windows
                    .iter()
                    .map(|w| w.id.clone())
                    .collect::<HashSet<_>>(),
            )
        };
        let proxy = BrowserProviderProxy::builder(&self.connection)
            .destination(owner.as_str())
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        tokio::time::timeout(Duration::from_secs(10), proxy.open_url(&generation, url))
            .await
            .map_err(|_| ErrorCode::Timeout)?
            .map_err(normalize_error)?;
        // The URL opens in a new window or focuses the window that already
        // shows its origin (ADR-0050). The window focused before the call is
        // not necessarily the one to show, so decide on the first newer
        // snapshot (ADR-0052).
        let window_token = self
            .focused_token_after(&id, &generation, before, &known)
            .await;
        self.show_browser_window(&id, &owner, &generation, &window_token)
            .await
    }

    /// Whether a ready `browser` provider has a connected browser.
    async fn browser_connected(&self) -> bool {
        self.state.lock().await.providers.values().any(|p| {
            p.capability() == "browser"
                && p.info.runtime_state == "ready"
                && p.info.backend_state != "unavailable"
                && !p.owner.is_empty()
        })
    }

    /// Launch the browser application through an ApplicationProvider and
    /// wait until a browser connects, within `deadline` (ADR-0055). Without a
    /// ready browser provider there is nothing to open the URL in, so nothing
    /// is launched. A mutating launch is never retried.
    async fn launch_browser(&self, deadline: tokio::time::Instant) -> Result<(), ErrorCode> {
        let _launching = self.browser_launch.lock().await;
        // Another open may have launched it while this one waited.
        if self.browser_connected().await {
            return Ok(());
        }
        let candidates = {
            let configured = Config::load(&self.paths.config)
                .map(|c| c.browser_application)
                .unwrap_or_default();
            if configured.is_empty() {
                crate::shortcuts::browser_candidates(&crate::shortcuts::data_roots())
            } else {
                vec![configured]
            }
        };
        let (owner, generation, application) = {
            let state = self.state.lock().await;
            let provider_ready = state.providers.values().any(|p| {
                p.capability() == "browser"
                    && p.info.runtime_state == "ready"
                    && !p.owner.is_empty()
            });
            if !provider_ready {
                return Err(ErrorCode::Unavailable);
            }
            candidates
                .iter()
                .find_map(|candidate| {
                    state.providers.values().find_map(|p| {
                        (p.capability() == "application"
                            && p.info.runtime_state == "ready"
                            && !p.owner.is_empty()
                            && p.applications
                                .iter()
                                .any(|a| &a.id == candidate && a.can_launch))
                        .then(|| {
                            (
                                p.owner.clone(),
                                p.info.generation.clone(),
                                candidate.clone(),
                            )
                        })
                    })
                })
                .ok_or(ErrorCode::Unavailable)?
        };
        let proxy = ApplicationProviderProxy::builder(&self.connection)
            .destination(owner.as_str())
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        tokio::time::timeout_at(deadline, proxy.launch(&generation, &application))
            .await
            .map_err(|_| ErrorCode::Timeout)?
            .map_err(normalize_error)?;
        loop {
            if self.browser_connected().await {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(ErrorCode::Timeout);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The token of the focused browser window in the first snapshot newer
    /// than `revision`. Empty when that snapshot has a window that is not in
    /// `known` (a new window has no link yet; the browser raises it itself),
    /// or when no newer snapshot arrives within the wait.
    async fn focused_token_after(
        &self,
        id: &str,
        generation: &str,
        revision: u64,
        known: &HashSet<String>,
    ) -> String {
        let deadline = tokio::time::Instant::now() + OPEN_FOCUS_WAIT;
        loop {
            {
                let state = self.state.lock().await;
                let Some(p) = state.providers.get(id) else {
                    return String::new();
                };
                if p.info.generation != generation {
                    return String::new();
                }
                if p.info.revision > revision {
                    if p.windows.iter().any(|w| !known.contains(&w.id)) {
                        return String::new();
                    }
                    return p
                        .windows
                        .iter()
                        .find(|w| w.focused)
                        .map(|w| w.window_token.clone())
                        .unwrap_or_default();
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return String::new();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn command(
        &self,
        id: &str,
        action: &str,
        generation: &str,
        local_id: &str,
        request_id: &str,
        caller: &str,
    ) -> Result<OwnedObjectPath, ErrorCode> {
        validation::id(local_id)?;
        let fingerprint = serde_json::to_string(&(id, action, generation, local_id))
            .map_err(|_| ErrorCode::Internal)?;
        if let Some(path) = self
            .operations
            .existing(caller, request_id, &fingerprint)
            .await?
        {
            return Ok(path);
        }
        let state = self.state.lock().await;
        let p = state.providers.get(id).ok_or(ErrorCode::NotFound)?;
        if p.info.generation != generation {
            return Err(ErrorCode::Conflict);
        }
        if p.info.runtime_state != "ready" || p.info.backend_state == "unavailable" {
            return Err(ErrorCode::Unavailable);
        }
        let allowed = match action {
            "launch" => p
                .applications
                .iter()
                .find(|a| a.id == local_id)
                .map(|a| a.can_launch),
            "activate" => p
                .running
                .iter()
                .find(|r| r.id == local_id)
                .map(|r| r.can_activate),
            "terminate" => p
                .running
                .iter()
                .find(|r| r.id == local_id)
                .map(|r| r.can_terminate),
            "activate_tab" | "close_tab" => p.tabs.iter().find(|t| t.id == local_id).map(|_| true),
            _ => return Err(ErrorCode::Unsupported),
        }
        .ok_or(ErrorCode::NotFound)?;
        let capability = if action.ends_with("_tab") {
            "browser"
        } else {
            "application"
        };
        if p.capability() != capability {
            return Err(ErrorCode::Unsupported);
        }
        // The compositor window of a tab, when the provider linked it.
        let window_token = p
            .tabs
            .iter()
            .find(|t| t.id == local_id)
            .and_then(|t| p.windows.iter().find(|w| w.id == t.window_id))
            .map(|w| w.window_token.clone())
            .unwrap_or_default();
        if !allowed {
            return Err(ErrorCode::Unsupported);
        }
        let owner = p.owner.clone();
        let (path, op, fresh) = self
            .operations
            .admit(&self.connection, caller, request_id, &fingerprint, id)
            .await?;
        drop(state);
        if !fresh {
            return Ok(path);
        }
        let core = self.clone();
        let id = id.to_owned();
        let action = action.to_owned();
        let generation = generation.to_owned();
        let local_id = local_id.to_owned();
        let path_copy = path.clone();
        tokio::spawn(async move {
            {
                let mut value = op.value.lock().await;
                if value.state != "queued" {
                    return;
                }
                value.state = "running".into();
                value.started_at = now();
            }
            let work = async {
                if capability == "browser" {
                    let proxy = BrowserProviderProxy::builder(&core.connection)
                        .destination(owner.as_str())
                        .map_err(|_| ErrorCode::Internal)?
                        .build()
                        .await
                        .map_err(|_| ErrorCode::Unavailable)?;
                    let result = if action == "activate_tab" {
                        proxy.activate_tab(&generation, &local_id).await
                    } else {
                        proxy.close_tab(&generation, &local_id).await
                    };
                    return result.map_err(normalize_error);
                }
                let proxy = ApplicationProviderProxy::builder(&core.connection)
                    .destination(owner.as_str())
                    .map_err(|_| ErrorCode::Internal)?
                    .build()
                    .await
                    .map_err(|_| ErrorCode::Unavailable)?;
                let result = match action.as_str() {
                    "launch" => proxy.launch(&generation, &local_id).await,
                    "activate" => proxy.activate(&generation, &local_id).await,
                    _ => proxy.terminate(&generation, &local_id).await,
                };
                result.map_err(normalize_error)
            };
            let result = tokio::time::timeout(
                Duration::from_secs(if action == "launch" { 30 } else { 5 }),
                work,
            )
            .await
            .unwrap_or(Err(ErrorCode::Timeout));
            let valid = core
                .state
                .lock()
                .await
                .providers
                .get(&id)
                .is_some_and(|p| p.owner == owner && p.info.generation == generation);
            let mut result = if valid {
                result
            } else {
                Err(ErrorCode::Unavailable)
            };
            if result.is_ok() && action == "activate_tab" {
                // The browser selected the tab; the compositor window still
                // has to come forward, which hides Home (ADR-0048).
                result = core
                    .show_browser_window(&id, &owner, &generation, &window_token)
                    .await;
            }
            if result.is_ok() && action == "launch" {
                // A launched application replaces Home: ask the platform to
                // hide Home without returning to the previous Activity, so the
                // new window is not left underneath the shell. Best effort:
                // without a platform adapter there is no Home overlay.
                let _ = core.platform.queue("dismiss").await;
                let state = core.paths.state.clone();
                let target = crate::storage::Target {
                    provider_id: id.clone(),
                    capability: "application".into(),
                    local_id: local_id.clone(),
                };
                if let Ok(Err(error)) = tokio::task::spawn_blocking(move || {
                    Store::open(&state)?.record_launch(&target, (now() / 1000) as i64)
                })
                .await
                {
                    core.state.lock().await.error = error.to_string();
                }
            }
            op.finish(
                &core.connection,
                &path_copy,
                result.map(|_| {
                    vec![EntityRef {
                        provider_id: id,
                        capability: capability.into(),
                        local_id,
                    }]
                }),
            )
            .await;
        });
        Ok(path)
    }
}

/// The environment a provider unit starts with (spec §9.9, ADR-0034,
/// ADR-0064). `ARINN_PROVIDER_SETTINGS` is always set, `{}` without settings.
fn provider_env(
    id: &str,
    manifest: &Manifest,
    token_file: &std::path::Path,
    data: &std::path::Path,
    state: &std::path::Path,
    settings: &toml::Table,
) -> Result<Vec<String>, ErrorCode> {
    Ok(vec![
        format!("ARINN_PROVIDER_ID={id}"),
        format!("ARINN_PLUGIN_ID={}", manifest.id),
        format!("ARINN_PLUGIN_VERSION={}", manifest.version),
        format!("ARINN_TOKEN_FILE={}", token_file.display()),
        format!("ARINN_PROVIDER_DATA_DIR={}", data.display()),
        format!("ARINN_PROVIDER_STATE_DIR={}", state.display()),
        format!(
            "ARINN_PROVIDER_SETTINGS={}",
            crate::storage::settings_json(settings)?
        ),
    ])
}

fn normalize_error(error: zbus::Error) -> ErrorCode {
    if let zbus::Error::MethodError(name, _, _) = error {
        match name.as_str() {
            "org.arinn.Error1.NotFound" => ErrorCode::NotFound,
            "org.arinn.Error1.Unsupported" => ErrorCode::Unsupported,
            "org.arinn.Error1.PermissionDenied" => ErrorCode::PermissionDenied,
            "org.arinn.Error1.InvalidArgument" => ErrorCode::InvalidArgument,
            "org.arinn.Error1.Conflict" => ErrorCode::Conflict,
            "org.arinn.Error1.Timeout" => ErrorCode::Timeout,
            "org.arinn.Error1.Cancelled" => ErrorCode::Cancelled,
            _ => ErrorCode::Unavailable,
        }
    } else {
        ErrorCode::Unavailable
    }
}
async fn read_snapshot(
    proxy: &ApplicationProviderProxy<'_>,
    generation: &str,
) -> Result<(u64, Vec<Application>, Vec<RunningApplication>, String), ErrorCode> {
    let mut token = String::new();
    let mut tokens = HashSet::new();
    let mut revision = 0;
    let mut apps = Vec::new();
    let mut running = Vec::new();
    let mut total = 0;
    loop {
        let (page,) = proxy
            .get_applications(&token)
            .await
            .map_err(normalize_error)?;
        page.validate()?;
        total += page.encoded_len()?;
        if total > 16 * 1024 * 1024
            || apps.len() + running.len() + page.applications.len() + page.running.len() > 100_000
        {
            return Err(ErrorCode::InvalidArgument);
        }
        if page.generation != generation || (revision != 0 && page.revision != revision) {
            return Err(ErrorCode::Conflict);
        }
        revision = page.revision;
        apps.extend(page.applications);
        running.extend(page.running);
        token = page.next_token;
        if token.is_empty() {
            break;
        }
        if !tokens.insert(token.clone()) {
            return Err(ErrorCode::Conflict);
        }
    }
    validate_entries(&apps, &running)?;
    let backend = proxy.backend_state().await.map_err(normalize_error)?;
    if !["available", "unavailable", "degraded"].contains(&backend.as_str()) {
        return Err(ErrorCode::Unsupported);
    }
    Ok((revision, apps, running, backend))
}

async fn read_tabs(
    proxy: &BrowserProviderProxy<'_>,
    generation: &str,
) -> Result<(u64, Vec<BrowserWindow>, Vec<Tab>, String), ErrorCode> {
    let mut token = String::new();
    let mut tokens = HashSet::new();
    let mut revision = 0;
    let mut windows = Vec::new();
    let mut tabs = Vec::new();
    let mut total = 0;
    loop {
        let (page,): (TabPage,) = proxy.get_tabs(&token).await.map_err(normalize_error)?;
        page.validate()?;
        total += page.encoded_len()?;
        if total > 16 * 1024 * 1024
            || windows.len() + tabs.len() + page.windows.len() + page.tabs.len() > 100_000
        {
            return Err(ErrorCode::InvalidArgument);
        }
        if page.generation != generation || (revision != 0 && page.revision != revision) {
            return Err(ErrorCode::Conflict);
        }
        revision = page.revision;
        windows.extend(page.windows);
        tabs.extend(page.tabs);
        token = page.next_token;
        if token.is_empty() {
            break;
        }
        if !tokens.insert(token.clone()) {
            return Err(ErrorCode::Conflict);
        }
    }
    browser::validate_entries(&windows, &tabs)?;
    let backend = proxy.backend_state().await.map_err(normalize_error)?;
    if !["available", "unavailable", "degraded"].contains(&backend.as_str()) {
        return Err(ErrorCode::Unsupported);
    }
    Ok((revision, windows, tabs, backend))
}

pub struct Daemon {
    pub core: Core,
}
#[zbus::interface(name = "org.arinn.Daemon1")]
impl Daemon {
    async fn home_items(&self) -> Vec<(String, String, String, String, String)> {
        self.core
            .state
            .lock()
            .await
            .providers
            .values()
            .filter(|p| p.info.runtime_state == "ready" && p.info.backend_state != "unavailable")
            .flat_map(|p| {
                p.running.iter().filter(|r| r.can_activate).map(|r| {
                    (
                        p.info.id.clone(),
                        p.info.path.to_string(),
                        p.info.generation.clone(),
                        r.id.clone(),
                        r.title.clone(),
                    )
                })
            })
            .take(5)
            .collect()
    }

    async fn status(&self) -> (arinn_protocol::Status,) {
        let available = self
            .core
            .state
            .lock()
            .await
            .providers
            .values()
            .any(|p| p.info.runtime_state == "ready");
        (self.core.platform.status(&self.core.epoch, available).await,)
    }
    async fn home(&self) -> production::Result<()> {
        self.core.platform.queue("home").await
    }
    async fn back(&self) -> production::Result<()> {
        self.core.platform.queue("return").await
    }
    async fn take_command(&self, #[zbus(header)] h: Header<'_>) -> production::Result<String> {
        self.core
            .platform
            .take_command(&self.core.connection, h)
            .await
    }
    async fn report_home(
        &self,
        visible: bool,
        target: &str,
        output: &str,
        #[zbus(header)] h: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> production::Result<()> {
        self.core
            .platform
            .report_home(&self.core.connection, h, visible, target, output)
            .await?;
        Self::changed(&emitter).await?;
        Ok(())
    }
    async fn report_window(
        &self,
        id: &str,
        class: &str,
        fullscreen: bool,
        output: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        self.core
            .platform
            .report_window(&self.core.connection, h, id, class, fullscreen, output)
            .await
    }
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    async fn providers(
        &self,
        token: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<(production::ProviderPage,)> {
        let owner = sender(&h)?;
        let mut state = self.core.state.lock().await;
        let State {
            providers,
            listings,
            ..
        } = &mut *state;
        Ok((listings.page(&owner, token, || {
            providers.values().map(|p| p.info.clone()).collect()
        })?,))
    }
    async fn diagnostics(&self) -> String {
        self.core.state.lock().await.error.clone()
    }
    async fn register(
        &self,
        request: Registration,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<(RegistrationReply,)> {
        let _serial = self.core.reload_lock.lock().await;
        validation::id(&request.provider_id)?;
        validation::id(&request.plugin_id)?;
        validation::id(&request.version)?;
        if request.token.len() > 256 || request.capabilities.len() > 6 {
            return Err(ErrorCode::InvalidArgument.into());
        }
        for capability in &request.capabilities {
            validation::capability(capability)?;
        }
        if request.api_major != 1 {
            return Err(ErrorCode::Unsupported.into());
        }
        if request.object_path.as_str() != production::PROVIDER_PATH {
            return Err(ErrorCode::InvalidArgument.into());
        }
        let owner = sender(&h)?;
        let unit = {
            let state = self.core.state.lock().await;
            let p = state
                .providers
                .get(&request.provider_id)
                .ok_or(ErrorCode::NotFound)?;
            if !p.owner.is_empty() {
                return Err(ErrorCode::Conflict.into());
            }
            if !p.enabled
                || p.info.runtime_state == "incompatible"
                || request.token.is_empty()
                || p.token != request.token
            {
                return Err(ErrorCode::PermissionDenied.into());
            }
            let manifest = p.manifest.as_ref().ok_or(ErrorCode::Unsupported)?;
            let mut caps = request.capabilities.clone();
            caps.sort();
            if manifest.id != request.plugin_id || manifest.capabilities != caps {
                return Err(ErrorCode::PermissionDenied.into());
            }
            p.unit.clone()
        };
        Supervisor::new(&self.core.connection)
            .authenticate(&owner, &unit)
            .await?;
        let generation = Uuid::new_v4().to_string();
        {
            let mut state = self.core.state.lock().await;
            let p = state
                .providers
                .get_mut(&request.provider_id)
                .ok_or(ErrorCode::NotFound)?;
            if !p.owner.is_empty() || p.token != request.token || p.unit != unit {
                return Err(ErrorCode::Conflict.into());
            }
            p.token.clear();
            p.owner = owner.clone();
            p.info.generation = generation.clone();
            p.info.revision = 0;
            p.cache = None;
        }
        let bus = zbus::fdo::DBusProxy::new(&self.core.connection)
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        if !bus
            .name_has_owner(
                owner
                    .as_str()
                    .try_into()
                    .map_err(|_| ErrorCode::PermissionDenied)?,
            )
            .await
            .unwrap_or(false)
        {
            self.core.unavailable(&request.provider_id, &owner).await;
            return Err(ErrorCode::Unavailable.into());
        }
        let core = self.core.clone();
        let id = request.provider_id;
        let gen_copy = generation.clone();
        tokio::spawn(async move {
            core.watch_provider(id, owner, gen_copy).await;
        });
        Ok((RegistrationReply {
            epoch: self.core.epoch.clone(),
            generation,
            api_minor: 0,
            max_body_bytes: 256 * 1024,
            max_page_entries: 200,
            max_operations: 32,
        },))
    }
    async fn reload(
        &self,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        let caller = sender(&h)?;
        let (path, op, fresh) = self
            .core
            .operations
            .admit(&self.core.connection, &caller, request_id, "reload", "")
            .await?;
        if fresh {
            let core = self.core.clone();
            let object = path.clone();
            tokio::spawn(async move {
                let result = core.reload_inner().await;
                if let Err(error) = result {
                    core.state.lock().await.error = error.to_string();
                }
                op.finish(&core.connection, &object, result.map(|_| Vec::new()))
                    .await;
            });
        }
        Ok(path)
    }
    async fn restart(
        &self,
        provider_id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        validation::id(provider_id)?;
        let caller = sender(&h)?;
        let fingerprint = format!("restart:{provider_id}");
        if let Some(path) = self
            .core
            .operations
            .existing(&caller, request_id, &fingerprint)
            .await?
        {
            return Ok(path);
        }
        let unit = {
            let state = self.core.state.lock().await;
            let p = state
                .providers
                .get(provider_id)
                .ok_or(ErrorCode::NotFound)?;
            if !p.enabled || p.info.runtime_state == "incompatible" {
                return Err(ErrorCode::Unsupported.into());
            }
            p.unit.clone()
        };
        let (path, op, fresh) = self
            .core
            .operations
            .admit(&self.core.connection, &caller, request_id, &fingerprint, "")
            .await?;
        if fresh {
            let core = self.core.clone();
            let id = provider_id.to_string();
            let object = path.clone();
            tokio::spawn(async move {
                let _serial = core.reload_lock.lock().await;
                let owner = core
                    .state
                    .lock()
                    .await
                    .providers
                    .get(&id)
                    .map(|p| p.owner.clone())
                    .unwrap_or_default();
                core.unavailable(&id, &owner).await;
                let supervisor = Supervisor::new(&core.connection);
                let result = match supervisor.unit(&unit).await {
                    Err(ErrorCode::NotFound) => core.launch(&id).await,
                    Err(error) => Err(error),
                    Ok(_) => supervisor.restart(&unit).await,
                };
                if result.is_ok() {
                    let directory = core.paths.state.clone();
                    let provider_id = id.clone();
                    if let Ok(Ok(count)) = tokio::task::spawn_blocking(move || {
                        Store::open(&directory)?.record_restart(&provider_id)
                    })
                    .await
                        && let Some(p) = core.state.lock().await.providers.get_mut(&id)
                    {
                        p.info.cumulative_restart_count = count;
                        p.info.restarts_since_daemon_start += 1;
                    }
                }
                op.finish(&core.connection, &object, result.map(|_| Vec::new()))
                    .await;
            });
        }
        Ok(path)
    }
    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        provider_id: &str,
        capability: &str,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn providers_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}
struct BrowserEndpoint {
    core: Core,
    id: String,
}
#[zbus::interface(name = "org.arinn.Browser1")]
impl BrowserEndpoint {
    async fn get_tabs(
        &self,
        token: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<(TabPage,)> {
        let owner = sender(&h)?;
        let mut state = self.core.state.lock().await;
        let p = state
            .providers
            .get_mut(&self.id)
            .ok_or(ErrorCode::NotFound)?;
        Ok((p.tab_cache.as_mut().ok_or(ErrorCode::Unavailable)?.page(
            &owner,
            token,
            Instant::now(),
        )?,))
    }
    async fn activate_tab(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .command(
                &self.id,
                "activate_tab",
                generation,
                id,
                request_id,
                &sender(&h)?,
            )
            .await?)
    }
    async fn close_tab(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .command(
                &self.id,
                "close_tab",
                generation,
                id,
                request_id,
                &sender(&h)?,
            )
            .await?)
    }
}
struct ApplicationEndpoint {
    core: Core,
    id: String,
}
#[zbus::interface(name = "org.arinn.Application1")]
impl ApplicationEndpoint {
    async fn get_applications(
        &self,
        token: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<(ApplicationPage,)> {
        let owner = sender(&h)?;
        let mut state = self.core.state.lock().await;
        let p = state
            .providers
            .get_mut(&self.id)
            .ok_or(ErrorCode::NotFound)?;
        Ok((p
            .cache
            .as_mut()
            .ok_or(ErrorCode::Unavailable)?
            .page(&owner, token, Instant::now())?,))
    }
    async fn launch(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .command(&self.id, "launch", generation, id, request_id, &sender(&h)?)
            .await?)
    }
    async fn activate(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .command(
                &self.id,
                "activate",
                generation,
                id,
                request_id,
                &sender(&h)?,
            )
            .await?)
    }
    async fn terminate(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .command(
                &self.id,
                "terminate",
                generation,
                id,
                request_id,
                &sender(&h)?,
            )
            .await?)
    }
}

fn window_scope(state: &State, owner: &str) -> Result<crate::windows::Scope, ErrorCode> {
    state
        .providers
        .iter()
        .find(|(_, p)| {
            p.owner == owner
                && !p.info.generation.is_empty()
                && p.enabled
                && ROUTED.contains(&p.capability())
        })
        .map(|(id, p)| crate::windows::Scope {
            provider: id.clone(),
            owner: owner.into(),
            registration: p.info.generation.clone(),
        })
        .ok_or(ErrorCode::PermissionDenied)
}
// Preserve operation completion if a caller's admission budget expires after
// allocating its deduplication record but before spawning the completion task.
struct WindowAdmission(Option<(Connection, OwnedObjectPath, crate::operations::Operation)>);
impl Drop for WindowAdmission {
    fn drop(&mut self) {
        if let Some((connection, path, operation)) = self.0.take() {
            tokio::spawn(async move {
                operation
                    .finish(&connection, &path, Err(ErrorCode::Timeout))
                    .await;
            });
        }
    }
}
struct WindowEndpoint {
    core: Core,
}
impl WindowEndpoint {
    async fn control(
        &self,
        action: &str,
        generation: &str,
        token: &str,
        request_id: &str,
        caller: &str,
    ) -> production::Result<OwnedObjectPath> {
        let state = self.core.state.lock().await;
        let scope = window_scope(&state, caller)?;
        arinn_protocol::platform::validate_command(generation, token, request_id)?;
        let fingerprint = serde_json::to_string(&("platform", action, generation, token))
            .map_err(|_| ErrorCode::Internal)?;
        if let Some(path) = self
            .core
            .operations
            .existing(caller, request_id, &fingerprint)
            .await?
        {
            return Ok(path);
        }
        self.core
            .windows
            .check(&self.core.connection, generation, token, action)
            .await?;
        let (path, operation, fresh) = self
            .core
            .operations
            .admit(
                &self.core.connection,
                caller,
                request_id,
                &fingerprint,
                &scope.provider,
            )
            .await?;
        if fresh {
            let mut admission = WindowAdmission(Some((
                self.core.connection.clone(),
                path.clone(),
                operation.clone(),
            )));
            let work = self
                .core
                .windows
                .enqueue(
                    &self.core.connection,
                    scope.clone(),
                    generation,
                    token,
                    action,
                )
                .await;
            drop(state);
            {
                let mut value = operation.value.lock().await;
                if value.state == "queued" {
                    value.state = "running".into();
                    value.started_at = now();
                }
            }
            let core = self.core.clone();
            let object = path.clone();
            admission.0.take();
            tokio::spawn(async move {
                let result = match work {
                    Ok(receiver) => tokio::time::timeout(Duration::from_secs(5), receiver)
                        .await
                        .map_err(|_| ErrorCode::Timeout)
                        .and_then(|r| r.map_err(|_| ErrorCode::Unavailable))
                        .and_then(|r| r),
                    Err(error) => Err(error),
                };
                let current = window_scope(&*core.state.lock().await, &scope.owner);
                let result = if current.as_ref() == Ok(&scope) {
                    result
                } else {
                    Err(ErrorCode::Unavailable)
                };
                operation
                    .finish(&core.connection, &object, result.map(|_| Vec::new()))
                    .await;
            });
        }
        Ok(path)
    }
}
#[zbus::interface(name = "org.arinn.PlatformWindows1")]
impl WindowEndpoint {
    async fn get_windows(
        &self,
        token: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<(arinn_protocol::platform::WindowPage,)> {
        tokio::time::timeout(Duration::from_secs(3), async {
            let caller = sender(&h)?;
            let state = self.core.state.lock().await;
            let scope = window_scope(&state, &caller)?;
            self.core
                .windows
                .page(&self.core.connection, &scope, token)
                .await
                .map(|page| (page,))
                .map_err(production::Error::from)
        })
        .await
        .map_err(|_| production::Error::from(ErrorCode::Timeout))?
    }
    async fn activate(
        &self,
        generation: &str,
        token: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        tokio::time::timeout(
            Duration::from_secs(3),
            self.control("activate", generation, token, request_id, &sender(&h)?),
        )
        .await
        .map_err(|_| production::Error::from(ErrorCode::Timeout))?
    }
    async fn close(
        &self,
        generation: &str,
        token: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        tokio::time::timeout(
            Duration::from_secs(3),
            self.control("close", generation, token, request_id, &sender(&h)?),
        )
        .await
        .map_err(|_| production::Error::from(ErrorCode::Timeout))?
    }
    #[zbus(signal)]
    async fn windows_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
#[path = "../../../platform/kde/browser_tests.rs"]
mod browser_tests;
#[cfg(test)]
#[path = "../../../platform/kde/window_tests.rs"]
mod window_tests;

#[cfg(test)]
mod token_tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    /// Regression test for #66: concurrent rotations (a vanished owner's
    /// `unavailable` racing `launch` during a reload) must leave the token
    /// file and the in-memory token equal, or the provider can never register.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_token_rotations_keep_file_and_memory_equal() {
        struct Bus(std::process::Child, PathBuf);
        impl Drop for Bus {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
                let _ = std::fs::remove_dir_all(&self.1);
            }
        }
        let folder = std::env::temp_dir().join(format!("arinn-tokens-{}", Uuid::new_v4()));
        std::fs::create_dir(&folder).unwrap();
        let mut bus = Bus(
            Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
            folder.clone(),
        );
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let connection = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .build()
            .await
            .unwrap();
        let core = Core::new(
            &connection,
            Paths {
                config: folder.join("config"),
                data: folder.join("data"),
                state: folder.join("state"),
            },
            folder.clone(),
        );
        let token_file = folder.join("fixture.token");
        core.state.lock().await.providers.insert(
            "fixture".into(),
            Provider {
                info: ProviderInfo {
                    id: "fixture".into(),
                    plugin_id: "fixture".into(),
                    source_path: String::new(),
                    name: "Fixture".into(),
                    version: "0.0.1".into(),
                    path: "/org/arinn/Daemon1/providers/fixture".try_into().unwrap(),
                    capabilities: vec!["application".into()],
                    runtime_state: "starting".into(),
                    backend_state: "unavailable".into(),
                    generation: String::new(),
                    revision: 0,
                    last_error: String::new(),
                    error_at: 0,
                    recovered_at: 0,
                    restarts_since_daemon_start: 0,
                    cumulative_restart_count: 0,
                },
                manifest: None,
                unit: "fixture.service".into(),
                token_file: token_file.clone(),
                token: String::new(),
                owner: String::new(),
                cache: None,
                applications: Vec::new(),
                running: Vec::new(),
                tab_cache: None,
                session_cache: None,
                sessions: Vec::new(),
                features: Vec::new(),
                library_cache: None,
                collections: Vec::new(),
                items: Vec::new(),
                resume_cache: None,
                resumes: Vec::new(),
                settings_schema: None,
                windows: Vec::new(),
                tabs: Vec::new(),
                started: Instant::now(),
                enabled: true,
            },
        );
        for round in 0..30 {
            let rotations: Vec<_> = (0..16)
                .map(|_| {
                    let core = core.clone();
                    tokio::spawn(async move { core.rotate_token("fixture").await })
                })
                .collect();
            for rotation in rotations {
                rotation.await.unwrap().unwrap();
            }
            let file = std::fs::read_to_string(&token_file).unwrap();
            let memory = core.state.lock().await.providers["fixture"].token.clone();
            assert_eq!(
                file, memory,
                "round {round}: token file and memory diverged"
            );
        }
    }
}

#[cfg(test)]
mod provider_env_tests {
    use super::*;

    fn manifest() -> Manifest {
        Manifest {
            id: "library".into(),
            name: "Library".into(),
            version: "0.1.0".into(),
            api_version: 1,
            executable: "/usr/lib/arinn/plugins/arinn-plugin-library".into(),
            args: Vec::new(),
            capabilities: vec!["library".into()],
        }
    }

    #[test]
    fn settings_reach_the_provider_as_json() {
        let settings: toml::Table = toml::from_str("roots = [\"/srv/films\"]").unwrap();
        let env = provider_env(
            "library",
            &manifest(),
            std::path::Path::new("/run/token"),
            std::path::Path::new("/data"),
            std::path::Path::new("/state"),
            &settings,
        )
        .unwrap();
        assert!(env.contains(&r#"ARINN_PROVIDER_SETTINGS={"roots":["/srv/films"]}"#.to_string()));
        assert!(env.contains(&"ARINN_PROVIDER_ID=library".to_string()));
        // Without settings the variable is still set, to an empty object.
        let env = provider_env(
            "library",
            &manifest(),
            std::path::Path::new("/run/token"),
            std::path::Path::new("/data"),
            std::path::Path::new("/state"),
            &toml::Table::new(),
        )
        .unwrap();
        assert!(env.contains(&"ARINN_PROVIDER_SETTINGS={}".to_string()));
    }
}
