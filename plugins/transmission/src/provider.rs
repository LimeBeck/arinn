// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Polls Transmission and serves `DownloadProvider1` plus `ProviderSettings1`.

use crate::{
    config::{Config, Intervals, MIN_RPC_VERSION, schema},
    log_event,
    map::{TORRENT_FIELDS, jobs_from_list},
    rpc::Rpc,
    secrets,
};
use arinn_plugin_sdk::runtime::{
    DownloadAction, DownloadHandler, DownloadService, SettingsHandler, run_download_on,
    serve_settings, startup_settings,
};
use arinn_protocol::{download::Job, validation::ErrorCode};
use serde_json::{Map, Value, json};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{Mutex, Notify};
use zbus::Connection;

struct Slot {
    generation: u64,
    client: Option<Rpc>,
    endpoint_key: String,
    credential_ref: String,
}

struct Inner {
    config: Mutex<Config>,
    intervals: Intervals,
    slot: Mutex<Slot>,
    rpc_gate: Mutex<()>,
    jobs: Mutex<Vec<Job>>,
    connection: Mutex<Option<Connection>>,
    refresh: AtomicBool,
    wake: Notify,
}

impl Inner {
    fn new(config: Config, intervals: Intervals) -> Self {
        Self {
            config: Mutex::new(config),
            intervals,
            slot: Mutex::new(Slot {
                generation: 0,
                client: None,
                endpoint_key: String::new(),
                credential_ref: String::new(),
            }),
            rpc_gate: Mutex::new(()),
            jobs: Mutex::new(Vec::new()),
            connection: Mutex::new(None),
            refresh: AtomicBool::new(false),
            wake: Notify::new(),
        }
    }

    async fn with_rpc(&self, method: &str, arguments: Value) -> Result<(Value, i64), ErrorCode> {
        let _gate = self.rpc_gate.lock().await;
        // Clone under the config lock, then release it before taking the slot.
        let config = self.config.lock().await.clone();
        let (generation, existing) = {
            let mut slot = self.slot.lock().await;
            let same = slot.client.is_some()
                && slot.endpoint_key == config.endpoint.key()
                && slot.credential_ref == config.credential_ref;
            if !same {
                slot.client = None;
            }
            (slot.generation, slot.client.take())
        };
        let mut client = if let Some(client) = existing {
            client
        } else {
            match self.open_client(&config).await {
                Ok(client) => client,
                Err(error) => return Err(error),
            }
        };
        if client.rpc_version.is_none()
            && let Err(error) = client.negotiate().await
        {
            self.put_back(generation, &config, client, false).await;
            return Err(error);
        }
        let version = client.rpc_version.unwrap_or(0);
        if version < MIN_RPC_VERSION {
            self.put_back(generation, &config, client, false).await;
            return Err(ErrorCode::Unsupported);
        }
        let result = client.call(method, arguments).await;
        let keep = !matches!(result, Err(ErrorCode::PermissionDenied));
        self.put_back(generation, &config, client, keep).await;
        result.map(|arguments| (arguments, version))
    }

    async fn open_client(&self, config: &Config) -> Result<Rpc, ErrorCode> {
        let auth = if config.credential_ref.is_empty() {
            None
        } else {
            let connection = self
                .connection
                .lock()
                .await
                .clone()
                .ok_or(ErrorCode::Unavailable)?;
            Some(secrets::lookup(&connection, &config.credential_ref).await?)
        };
        let mut rpc = Rpc::new(config.endpoint.clone(), auth, self.intervals.timeout);
        rpc.ensure_tls()?;
        Ok(rpc)
    }

    async fn put_back(&self, generation: u64, config: &Config, client: Rpc, keep: bool) {
        let mut slot = self.slot.lock().await;
        if keep && slot.generation == generation {
            slot.endpoint_key = config.endpoint.key();
            slot.credential_ref = config.credential_ref.clone();
            slot.client = Some(client);
        }
    }

    async fn mutate(&self, method: &str, id: &str) -> Result<(), ErrorCode> {
        let result = self
            .with_rpc(method, json!({"ids": [id]}))
            .await
            .map(|_| ());
        self.kick();
        result
    }

    async fn retry(&self, id: &str) -> Result<(), ErrorCode> {
        let arguments = json!({"ids": [id]});
        if let Err(error) = self.with_rpc("torrent-verify", arguments.clone()).await {
            self.kick();
            return Err(error);
        }
        let result = self.with_rpc("torrent-start", arguments).await.map(|_| ());
        self.kick();
        result
    }

    fn kick(&self) {
        self.refresh.store(true, Ordering::Release);
        self.wake.notify_one();
    }

    async fn refresh_jobs(&self) -> Result<(Vec<Job>, bool, i64), ErrorCode> {
        let (arguments, version) = self
            .with_rpc("torrent-get", json!({"fields": TORRENT_FIELDS}))
            .await?;
        let mappings = self.config.lock().await.mappings.clone();
        let (jobs, active) = match arguments.get("torrents").and_then(Value::as_array) {
            Some(torrents) => jobs_from_list(torrents, &mappings),
            None => (Vec::new(), false),
        };
        Ok((jobs, active, version))
    }

    async fn wait(&self, base: std::time::Duration, step: u32) {
        let notified = self.wake.notified();
        tokio::pin!(notified);
        if self.refresh.swap(false, Ordering::AcqRel) {
            return;
        }
        let delay = self.intervals.wait(base, step);
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = &mut notified => {}
        }
    }
}

#[derive(Clone)]
struct Bridge(Arc<Inner>);

impl DownloadHandler for Bridge {
    fn control(
        &self,
        action: DownloadAction,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        let inner = Arc::clone(&self.0);
        Box::pin(async move {
            match action {
                DownloadAction::Pause => inner.mutate("torrent-stop", &id).await,
                DownloadAction::Resume => inner.mutate("torrent-start", &id).await,
                DownloadAction::StartNow => inner.mutate("torrent-start-now", &id).await,
                DownloadAction::Retry => inner.retry(&id).await,
            }
        })
    }
}

impl SettingsHandler for Bridge {
    fn schema(&self) -> Vec<arinn_protocol::settings::Setting> {
        schema()
    }

    fn apply(
        &self,
        values: Map<String, Value>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        let inner = Arc::clone(&self.0);
        Box::pin(async move {
            let config = Config::from_values(&values)?;
            {
                let mut slot = inner.slot.lock().await;
                slot.generation = slot.generation.wrapping_add(1);
                slot.client = None;
            }
            *inner.config.lock().await = config;
            inner.kick();
            Ok(())
        })
    }
}

async fn poll(inner: Arc<Inner>, service: DownloadService) -> Result<(), ErrorCode> {
    let mut attempt = 0usize;
    let mut logged: Option<ErrorCode> = None;
    let mut logged_version: Option<i64> = None;
    loop {
        match inner.refresh_jobs().await {
            Ok((jobs, active, version)) => {
                *inner.jobs.lock().await = jobs.clone();
                if service.replace(jobs, "available").await.is_err() {
                    log_event("error", "rpc", Some(ErrorCode::Internal), None);
                }
                if logged.take().is_some() || logged_version != Some(version) {
                    log_event("info", "session", None, Some(version));
                    logged_version = Some(version);
                }
                attempt = 0;
                let base = if active {
                    inner.intervals.active
                } else {
                    inner.intervals.idle
                };
                inner.wait(base, 0).await;
            }
            Err(error) => {
                let jobs = inner.jobs.lock().await.clone();
                if service.replace(jobs, "unavailable").await.is_err() {
                    log_event("error", "rpc", Some(ErrorCode::Internal), None);
                }
                if logged != Some(error) {
                    log_event("error", "rpc", Some(error), None);
                    logged = Some(error);
                }
                let base = inner.intervals.backoff[attempt.min(inner.intervals.backoff.len() - 1)];
                let step = u32::try_from(attempt).unwrap_or(u32::MAX);
                attempt = (attempt + 1).min(inner.intervals.backoff.len() - 1);
                inner.wait(base, step).await;
            }
        }
    }
}

/// Read startup settings and serve the provider on `connection`.
pub async fn serve(connection: Connection) -> Result<(), ErrorCode> {
    let config = Config::from_values(&startup_settings(&schema()))?;
    run(connection, config, Intervals::production()).await
}

/// Serve with an explicit configuration. Tests use this with a fake RPC.
pub async fn run(
    connection: Connection,
    config: Config,
    intervals: Intervals,
) -> Result<(), ErrorCode> {
    let inner = Arc::new(Inner::new(config, intervals));
    *inner.connection.lock().await = Some(connection.clone());
    let bridge = Bridge(Arc::clone(&inner));
    let service = DownloadService::new(bridge.clone(), Vec::new(), "unavailable")?;
    serve_settings(&connection, bridge, service.registrations())
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    let polling = poll(inner, service.clone());
    tokio::pin!(polling);
    tokio::select! {
        result = run_download_on(connection, service) => {
            result.map_err(|_| ErrorCode::Unavailable)
        }
        result = &mut polling => result,
        result = tokio::signal::ctrl_c() => {
            result.map_err(|_| ErrorCode::Unavailable)
        }
    }
}
