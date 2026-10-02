// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Optional, provider-owned site icons. Never consults or launches a browser.
mod cache;
mod candidates;
mod network;
mod raster;
use arinn_plugin_sdk::runtime::RegistrationContext;
use arinn_protocol::{assets::Asset, production, validation::ErrorCode as E, web_icons};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use url::Url;
struct Cancellation(Arc<AtomicBool>);
impl Drop for Cancellation {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
type Lease = Arc<OwnedSemaphorePermit>;
type Reply = Result<Asset, E>;
type Flight = watch::Sender<Option<Reply>>;
#[derive(Default)]
struct State {
    flights: HashMap<String, Flight>,
    negative: HashMap<String, (Instant, E)>,
}
#[derive(Clone)]
struct Engine {
    state: Arc<Mutex<State>>,
    slots: Arc<Semaphore>,
    cache: Arc<Mutex<Option<cache::Cache>>>,
}
impl Engine {
    fn new() -> Self {
        let root = std::env::var_os("XDG_CACHE_HOME")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache"))
            });
        // Construction runs on a blocking worker before endpoint registration.
        let cache = root.and_then(|p| cache::Cache::new(&p.join("arinn/web-icons")).ok());
        Self {
            state: Arc::default(),
            slots: Arc::new(Semaphore::new(2)),
            cache: Arc::new(Mutex::new(cache)),
        }
    }
    async fn get(&self, origin: &str) -> Reply {
        let key = format!("{:x}", Sha256::digest(origin.as_bytes()));
        let mut receiver = {
            let mut state = self.state.lock().map_err(|_| E::Internal)?;
            state
                .negative
                .retain(|_, (at, _)| at.elapsed() < Duration::from_secs(600));
            if let Some((_, error)) = state.negative.get(&key) {
                return Err(*error);
            }
            if let Some(flight) = state.flights.get(&key) {
                if flight.receiver_count() >= 4 {
                    return Err(E::Unavailable);
                }
                flight.subscribe()
            } else {
                let lease = Arc::new(
                    self.slots
                        .clone()
                        .try_acquire_owned()
                        .map_err(|_| E::Unavailable)?,
                );
                let (sender, receiver) = watch::channel(None);
                state.flights.insert(key.clone(), sender.clone());
                let engine = self.clone();
                let origin = origin.to_owned();
                tokio::spawn(async move {
                    let result = tokio::select! {
                        result = tokio::time::timeout(Duration::from_secs(5), engine.discover(&origin, &key, lease)) => result.unwrap_or(Err(E::Unavailable)),
                        _ = sender.closed() => Err(E::Cancelled),
                    };
                    if let Ok(mut state) = engine.state.lock() {
                        state.flights.remove(&key);
                        if result.as_ref().is_err_and(|e| *e != E::Cancelled) {
                            if state.negative.len() >= 64
                                && let Some(oldest) = state
                                    .negative
                                    .iter()
                                    .min_by_key(|(_, (at, _))| *at)
                                    .map(|(k, _)| k.clone())
                            {
                                state.negative.remove(&oldest);
                            }
                            state
                                .negative
                                .insert(key, (Instant::now(), *result.as_ref().unwrap_err()));
                        }
                    }
                    sender.send_replace(Some(result));
                });
                receiver
            }
        };
        loop {
            if let Some(result) = receiver.borrow_and_update().clone() {
                return result;
            }
            receiver.changed().await.map_err(|_| E::Unavailable)?;
        }
    }
    async fn discover(&self, origin: &str, key: &str, lease: Lease) -> Reply {
        let deadline = Instant::now() + Duration::from_secs(5);
        let cancelled = Cancellation(Arc::new(AtomicBool::new(false)));
        let cache = self.cache.clone();
        let cache_key = key.to_owned();
        let keep = lease.clone();
        if let Some(asset) = tokio::task::spawn_blocking(move || {
            let _keep = keep;
            cache.lock().ok()?.as_ref()?.get(&cache_key)
        })
        .await
        .map_err(|_| E::Internal)?
        {
            return Ok(asset);
        }
        let base = Url::parse(origin).map_err(|_| E::InvalidArgument)?;
        let mut fetch = network::Fetcher::new(lease.clone());
        let mut icons = Vec::new();
        let mut failure = E::NotFound;
        let root = fetch.get(base.clone(), 512 * 1024).await;
        if let Err(error) = &root {
            if *error == E::Unsupported {
                return Err(*error);
            }
            failure = *error;
        }
        if let Ok((final_url, html)) = root {
            let keep = lease.clone();
            let (found, manifest, count) = tokio::task::spawn_blocking(move || {
                let _keep = keep;
                candidates::html(&final_url, &html)
            })
            .await
            .map_err(|_| E::Internal)?;
            icons = found;
            if let Some(manifest) = manifest {
                match fetch.get(manifest, 128 * 1024).await {
                    Ok((final_url, bytes)) => {
                        let keep = lease.clone();
                        icons.extend(
                            tokio::task::spawn_blocking(move || {
                                let _keep = keep;
                                candidates::manifest(&final_url, &bytes, 32 - count)
                            })
                            .await
                            .map_err(|_| E::Internal)?,
                        );
                    }
                    Err(E::Unavailable) => failure = E::Unavailable,
                    Err(_) => {}
                }
            }
        }
        for url in candidates::order(icons, &base) {
            let bytes = match fetch.get(url, 1024 * 1024).await {
                Ok((_, bytes)) => bytes,
                Err(error) => {
                    if error == E::Unavailable {
                        failure = error;
                    }
                    continue;
                }
            };
            let keep = lease.clone();
            let decoded = tokio::task::spawn_blocking(move || {
                let _keep = keep;
                raster::normalize(&bytes)
            })
            .await
            .map_err(|_| E::Internal)?;
            if let Ok(asset) = decoded {
                let cache = self.cache.clone();
                let key = key.to_owned();
                let value = asset.clone();
                let keep = lease.clone();
                let cancel = cancelled.0.clone();
                tokio::task::spawn_blocking(move || {
                    let _keep = keep;
                    if Instant::now() < deadline
                        && let Ok(cache) = cache.lock()
                        && let Some(cache) = cache.as_ref()
                    {
                        let _ = cache.put(&key, &value, deadline, &cancel);
                    }
                })
                .await
                .map_err(|_| E::Internal)?;
                return Ok(asset);
            }
        }
        Err(failure)
    }
}

pub struct WebIcons {
    registrations: watch::Receiver<Option<RegistrationContext>>,
    engine: Engine,
}
impl WebIcons {
    pub async fn new(
        registrations: watch::Receiver<Option<RegistrationContext>>,
    ) -> Result<Self, E> {
        let engine = tokio::task::spawn_blocking(Engine::new)
            .await
            .map_err(|_| E::Internal)?;
        Ok(Self {
            registrations,
            engine,
        })
    }
    fn context(&self, sender: &str, generation: &str) -> Result<(String, String, String), E> {
        let current = self.registrations.borrow();
        let current = current.as_ref().ok_or(E::Conflict)?;
        if current.daemon_owner != sender {
            return Err(E::PermissionDenied);
        }
        arinn_protocol::platform::generation(generation)?;
        if current.generation != generation {
            return Err(E::Conflict);
        }
        Ok((
            current.daemon_owner.clone(),
            current.daemon_epoch.clone(),
            current.generation.clone(),
        ))
    }
}
#[zbus::interface(name = "org.arinn.WebIconProvider1")]
impl WebIcons {
    async fn get_icon(
        &self,
        generation: &str,
        origin: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> production::Result<(Asset,)> {
        let sender = header.sender().ok_or(E::PermissionDenied)?.as_str();
        let before = self.context(sender, generation)?;
        web_icons::origin(origin)?;
        let result = self.engine.get(origin).await;
        if before != self.context(sender, generation)? {
            return Err(E::Conflict.into());
        }
        Ok((result?,))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn engine() -> Engine {
        Engine {
            state: Arc::default(),
            slots: Arc::new(Semaphore::new(2)),
            cache: Arc::new(Mutex::new(None)),
        }
    }
    #[tokio::test]
    async fn discovery_slots_and_coalesced_waiters_are_bounded() {
        let engine = engine();
        let first = engine.slots.clone().acquire_owned().await.unwrap();
        let second = engine.slots.clone().acquire_owned().await.unwrap();
        assert_eq!(
            engine.get("https://example.invalid/").await.unwrap_err(),
            E::Unavailable
        );
        drop((first, second));
        let key = format!("{:x}", Sha256::digest(b"https://example.invalid/"));
        let (sender, _receiver) = watch::channel(None);
        let _a = sender.subscribe();
        let _b = sender.subscribe();
        let _c = sender.subscribe();
        engine.state.lock().unwrap().flights.insert(key, sender);
        assert_eq!(
            engine.get("https://example.invalid/").await.unwrap_err(),
            E::Unavailable
        );
    }
    #[tokio::test]
    async fn negative_cache_is_bounded_without_network() {
        let engine = engine();
        for n in 0..70 {
            assert!(
                engine
                    .get(&format!("https://127.0.0.{}/", n + 1))
                    .await
                    .is_err()
            );
        }
        assert!(engine.state.lock().unwrap().negative.len() <= 64);
        assert!(engine.state.lock().unwrap().flights.is_empty());
        assert_eq!(engine.slots.available_permits(), 2);
        assert_eq!(
            engine.get("https://127.0.0.70/").await.unwrap_err(),
            E::Unsupported
        );
    }
    #[tokio::test]
    async fn endpoint_authenticates_and_rechecks_registration() {
        use std::{
            io::{BufRead, BufReader},
            process::{Command, Stdio},
        };
        struct Bus(std::process::Child);
        impl Drop for Bus {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let _bus = Bus(child);
        let provider = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .build()
            .await
            .unwrap();
        let daemon = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .build()
            .await
            .unwrap();
        let intruder = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .build()
            .await
            .unwrap();
        let generation = "77777777-7777-4777-8777-777777777777";
        let context = RegistrationContext {
            connection: provider.clone(),
            daemon_owner: daemon.unique_name().unwrap().to_string(),
            daemon_epoch: generation.into(),
            generation: generation.into(),
        };
        let (registration, receiver) = watch::channel(Some(context));
        let engine = engine();
        let origin = "https://example.invalid/";
        let key = format!("{:x}", Sha256::digest(origin.as_bytes()));
        let (result, _) = watch::channel(None);
        engine
            .state
            .lock()
            .unwrap()
            .flights
            .insert(key, result.clone());
        provider
            .object_server()
            .at(
                "/org/arinn/Provider1",
                WebIcons {
                    registrations: receiver,
                    engine,
                },
            )
            .await
            .unwrap();
        let make_proxy = |connection: zbus::Connection| {
            let owner = provider.unique_name().unwrap().to_owned();
            async move {
                web_icons::WebIconProviderProxy::builder(&connection)
                    .destination(owner)
                    .unwrap()
                    .build()
                    .await
                    .unwrap()
            }
        };
        let foreign = make_proxy(intruder).await;
        let denied = foreign.get_icon(generation, origin).await.unwrap_err();
        assert!(
            matches!(denied,zbus::Error::MethodError(ref name,_,_) if name.as_str()=="org.arinn.Error1.PermissionDenied")
        );
        let proxy = make_proxy(daemon).await;
        let request = tokio::spawn(async move { proxy.get_icon(generation, origin).await });
        tokio::time::timeout(Duration::from_secs(2), async {
            while result.receiver_count() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        registration.send_replace(None);
        result.send_replace(Some(Err(E::NotFound)));
        let stale = request.await.unwrap().unwrap_err();
        assert!(
            matches!(stale,zbus::Error::MethodError(ref name,_,_) if name.as_str()=="org.arinn.Error1.Conflict")
        );
    }
    #[tokio::test]
    async fn cancelled_blocking_work_retains_its_physical_slot() {
        let slots = Arc::new(Semaphore::new(1));
        let lease = Arc::new(slots.clone().try_acquire_owned().unwrap());
        let (release, wait) = std::sync::mpsc::channel();
        let keep = lease.clone();
        let worker = tokio::task::spawn_blocking(move || {
            let _keep = keep;
            wait.recv().unwrap();
        });
        drop(lease);
        // Cancellation cannot create replacement workers while the old native
        // DNS/parser/decoder call is still running.
        assert!(slots.clone().try_acquire_owned().is_err());
        release.send(()).unwrap();
        worker.await.unwrap();
        assert_eq!(slots.available_permits(), 1);
    }
    #[tokio::test]
    async fn total_deadline_includes_blocking_cache_work() {
        let engine = engine();
        let cache = engine.cache.clone();
        let (locked, ready) = std::sync::mpsc::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _guard = cache.lock().unwrap();
            locked.send(()).unwrap();
            let _ = wait.recv();
        });
        ready.recv().unwrap();
        let start = Instant::now();
        let result = tokio::time::timeout(Duration::from_secs(6), engine.get("https://127.0.0.1/"))
            .await
            .unwrap();
        assert_eq!(result.unwrap_err(), E::Unavailable);
        assert!(start.elapsed() >= Duration::from_secs(5));
        assert_eq!(engine.slots.available_permits(), 1);
        release.send(()).unwrap();
        holder.join().unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while engine.slots.available_permits() != 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
