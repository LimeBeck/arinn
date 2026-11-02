// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use arinn_protocol::music::{self, Command, Entity, Page, Query, State};
use std::collections::{HashMap, HashSet, VecDeque};

pub struct MusicCatalogue {
    pub revision: u64,
    pub entries: Vec<Entity>,
}
pub trait MusicHandler: Send + Sync + 'static {
    fn state(&self) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>>;
    fn catalogue(
        &self,
        query: Query,
    ) -> Pin<Box<dyn Future<Output = Result<MusicCatalogue, ErrorCode>> + Send + '_>>;
    fn command(
        &self,
        command: Command,
    ) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>>;
    fn reset(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}
#[derive(Clone)]
struct Lease {
    owner: String,
    query: Query,
    revision: u64,
    entries: Arc<Vec<Entity>>,
    offset: usize,
    expires: Instant,
    cached: Option<Page>,
    /// Encoded size of `cached`, counted toward the retained budget.
    cached_bytes: usize,
    bytes: usize,
}
/// A lease lives this long after it is issued; each continuation gets its
/// own full period, so slow paging does not fail 30 s after page one.
const LEASE: Duration = Duration::from_secs(30);
/// The longest a handler command or reset may take. The daemon gives up
/// after 5 s; this only bounds a handler that never returns.
const HANDLER: Duration = Duration::from_secs(10);
/// Retained snapshot and cached page data across all leases.
const RETAINED: usize = 16 * 1024 * 1024;
struct MusicData {
    owner: String,
    generation: String,
    leases: HashMap<String, Lease>,
    commands: VecDeque<(String, Command, Result<String, ErrorCode>)>,
}
#[derive(Clone)]
pub struct MusicService {
    handler: Arc<dyn MusicHandler>,
    data: Arc<Mutex<MusicData>>,
    serial: Arc<Mutex<()>>,
    initial_pages: Arc<tokio::sync::Semaphore>,
    command_slots: Arc<tokio::sync::Semaphore>,
    registrations: tokio::sync::watch::Sender<Option<RegistrationContext>>,
    running: Arc<AtomicBool>,
    updates: tokio::sync::watch::Sender<(String, u64)>,
}
impl MusicService {
    pub fn new(handler: impl MusicHandler) -> Self {
        Self {
            handler: Arc::new(handler),
            data: Arc::new(Mutex::new(MusicData {
                owner: String::new(),
                generation: String::new(),
                leases: HashMap::new(),
                commands: VecDeque::new(),
            })),
            serial: Arc::new(Mutex::new(())),
            initial_pages: Arc::new(tokio::sync::Semaphore::new(2)),
            command_slots: Arc::new(tokio::sync::Semaphore::new(32)),
            registrations: tokio::sync::watch::channel(None).0,
            running: Arc::new(AtomicBool::new(false)),
            updates: tokio::sync::watch::channel((String::new(), 0)).0,
        }
    }
    pub fn registrations(&self) -> tokio::sync::watch::Receiver<Option<RegistrationContext>> {
        self.registrations.subscribe()
    }
    async fn ready(&self, generation: &str, h: &Header<'_>) -> production::Result<()> {
        let d = self.data.lock().await;
        if !self.running.load(Ordering::Acquire)
            || d.owner.is_empty()
            || h.sender().map(|s| s.as_str()) != Some(d.owner.as_str())
        {
            return Err(ErrorCode::PermissionDenied.into());
        }
        if d.generation.is_empty() || d.generation != generation {
            return Err(ErrorCode::Conflict.into());
        }
        Ok(())
    }
    async fn page(&self, generation: &str, client: &str, query: Query) -> Result<Page, ErrorCode> {
        let now = Instant::now();
        let lease = if query.token.is_empty() {
            let _slot = self
                .initial_pages
                .try_acquire()
                .map_err(|_| ErrorCode::Unavailable)?;
            let catalogue = self.handler.catalogue(query.without_token()).await?;
            if catalogue.revision == 0 || catalogue.entries.len() > 100_000 {
                return Err(ErrorCode::InvalidArgument);
            }
            let mut ids = HashSet::new();
            let mut bytes = 0;
            for e in &catalogue.entries {
                e.validate()?;
                if !ids.insert(&e.id) {
                    return Err(ErrorCode::InvalidArgument);
                }
                bytes += serde_json::to_vec(e)
                    .map_err(|_| ErrorCode::Internal)?
                    .len();
                if bytes > 16 * 1024 * 1024 {
                    return Err(ErrorCode::InvalidArgument);
                }
            }
            Lease {
                owner: client.into(),
                query: query.without_token(),
                revision: catalogue.revision,
                entries: Arc::new(catalogue.entries),
                offset: 0,
                expires: now + LEASE,
                cached: None,
                cached_bytes: 0,
                bytes,
            }
        } else {
            let mut d = self.data.lock().await;
            d.leases.retain(|_, v| v.expires > now);
            let lease = d.leases.get(&query.token).ok_or(ErrorCode::Conflict)?;
            if lease.owner != client || lease.query != query.without_token() {
                return Err(ErrorCode::PermissionDenied);
            }
            lease.clone()
        };
        if let Some(page) = lease.cached {
            return Ok(page);
        }
        let mut page = Page {
            generation: generation.into(),
            revision: lease.revision,
            entries: vec![],
            next_token: String::new(),
        };
        let mut offset = lease.offset;
        let mut bytes = 0;
        while offset < lease.entries.len() && page.entries.len() < 100 {
            let entity = &lease.entries[offset];
            let n = serde_json::to_vec(entity)
                .map_err(|_| ErrorCode::Internal)?
                .len();
            if bytes + n > validation::MAX_BODY_BYTES - 4096 {
                break;
            }
            page.entries.push(entity.clone());
            bytes += n;
            offset += 1;
        }
        if offset == lease.offset && offset < lease.entries.len() {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut d = self.data.lock().await;
        if d.generation != generation {
            return Err(ErrorCode::Conflict);
        }
        d.leases.retain(|_, v| v.expires > now);
        if offset < lease.entries.len() {
            // Snapshot vectors are shared by continuation leases, not copied.
            // The lease being served is never evicted: a retry of its token
            // must still return its cached page.
            loop {
                let mut seen = HashSet::new();
                let total: usize = d
                    .leases
                    .values()
                    .filter(|l| seen.insert(Arc::as_ptr(&l.entries)))
                    .map(|l| l.bytes)
                    .sum::<usize>()
                    + d.leases.values().map(|l| l.cached_bytes).sum::<usize>();
                let present = d
                    .leases
                    .values()
                    .any(|l| Arc::ptr_eq(&l.entries, &lease.entries));
                if d.leases.len() < 32 && total + if present { 0 } else { lease.bytes } <= RETAINED
                {
                    break;
                }
                let Some(key) = d
                    .leases
                    .iter()
                    .filter(|(k, _)| **k != query.token)
                    .min_by_key(|(_, v)| v.expires)
                    .map(|(k, _)| k.clone())
                else {
                    return Err(ErrorCode::InvalidArgument);
                };
                d.leases.remove(&key);
            }
            page.next_token = uuid::Uuid::new_v4().to_string();
            d.leases.insert(
                page.next_token.clone(),
                Lease {
                    offset,
                    expires: now + LEASE,
                    cached: None,
                    cached_bytes: 0,
                    ..lease.clone()
                },
            );
        }
        page.validate()?;
        let encoded = music::encode(&page)?.len();
        if let Some(saved) = d.leases.get_mut(&query.token) {
            saved.cached = Some(page.clone());
            saved.cached_bytes = encoded;
        }
        Ok(page)
    }
}
#[zbus::interface(name = "org.arinn.MusicProvider1")]
impl MusicService {
    async fn get_state(
        &self,
        generation: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<String> {
        self.ready(generation, &h).await?;
        let state = self.handler.state().await?;
        state.validate()?;
        self.ready(generation, &h).await?;
        Ok(music::encode(&state)?)
    }
    async fn get_page(
        &self,
        generation: &str,
        client: &str,
        query: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<String> {
        self.ready(generation, &h).await?;
        zbus::names::UniqueName::try_from(client).map_err(|_| ErrorCode::InvalidArgument)?;
        let query: Query = music::decode(query)?;
        query.validate()?;
        let page = self.page(generation, client, query).await?;
        self.ready(generation, &h).await?;
        Ok(music::encode(&page)?)
    }
    async fn command(
        &self,
        generation: &str,
        client: &str,
        command: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<String> {
        self.ready(generation, &h).await?;
        zbus::names::UniqueName::try_from(client).map_err(|_| ErrorCode::InvalidArgument)?;
        let _slot = self
            .command_slots
            .try_acquire()
            .map_err(|_| ErrorCode::Unavailable)?;
        let command: Command = music::decode(command)?;
        command.validate()?;
        let _serial = self.serial.lock().await;
        self.ready(generation, &h).await?;
        if let Some((_, prior, result)) = self
            .data
            .lock()
            .await
            .commands
            .iter()
            .find(|(owner, c, _)| owner == client && c.request_id == command.request_id)
        {
            if *prior != command {
                return Err(ErrorCode::Conflict.into());
            }
            return result.clone().map_err(Into::into);
        }
        // Bounded, so a hung handler cannot hold `serial` forever and block
        // `clear()` and the endpoint loop behind it (review of #710).
        let result = match tokio::time::timeout(HANDLER, async {
            let state = self.handler.command(command.clone()).await?;
            state.validate()?;
            music::encode(&state)
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(ErrorCode::Timeout),
        };
        self.ready(generation, &h).await?;
        // A timeout is an uncertain outcome, not a receipt: a retry with the
        // same request ID runs again instead of repeating "Timeout".
        if result == Err(ErrorCode::Timeout) {
            return Err(ErrorCode::Timeout.into());
        }
        let mut d = self.data.lock().await;
        d.commands
            .push_back((client.into(), command, result.clone()));
        while d.commands.len() > 32 {
            d.commands.pop_front();
        }
        result.map_err(Into::into)
    }
    #[zbus(signal)]
    async fn state_changed(
        emitter: &SignalEmitter<'_>,
        generation: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}
impl Endpoint for MusicService {
    const CAPABILITY: &'static str = "music";
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
        let _serial = self.serial.lock().await;
        let _ = tokio::time::timeout(HANDLER, self.handler.reset()).await;
        let mut d = self.data.lock().await;
        d.generation.clear();
        d.owner.clear();
        d.leases.clear();
        d.commands.clear();
    }
    async fn daemon_owner(&self) -> String {
        self.data.lock().await.owner.clone()
    }
    async fn set_daemon_owner(&self, owner: String) {
        self.data.lock().await.owner = owner;
    }
    async fn registered(&self) -> bool {
        !self.data.lock().await.generation.is_empty()
    }
    async fn install(&self, generation: &str) -> Result<(), ErrorCode> {
        validation::id(generation)?;
        self.data.lock().await.generation = generation.into();
        Ok(())
    }
    async fn emit(&self, c: &zbus::Connection, g: &str, r: u64) -> zbus::Result<()> {
        let e = SignalEmitter::new(c, production::PROVIDER_PATH)?;
        Self::state_changed(&e, g, r).await
    }
}
pub async fn run_music_on(
    connection: zbus::Connection,
    service: MusicService,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_endpoint(connection, service).await
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Catalogue;
    impl MusicHandler for Catalogue {
        fn state(&self) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>> {
            Box::pin(async { Ok(State::default()) })
        }
        fn catalogue(
            &self,
            _query: Query,
        ) -> Pin<Box<dyn Future<Output = Result<MusicCatalogue, ErrorCode>> + Send + '_>> {
            Box::pin(async {
                Ok(MusicCatalogue {
                    revision: 1,
                    entries: (0..350)
                        .map(|i| Entity {
                            id: format!("track-{i}"),
                            kind: "track".into(),
                            title: format!("Track {i}"),
                            available: true,
                            ..Entity::default()
                        })
                        .collect(),
                })
            })
        }
        fn command(
            &self,
            _command: Command,
        ) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>> {
            Box::pin(async { Err(ErrorCode::Unsupported) })
        }
        fn reset(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            Box::pin(async {})
        }
    }
    fn query(token: &str) -> Query {
        Query {
            facet: "tracks".into(),
            parent: String::new(),
            search: String::new(),
            token: token.into(),
        }
    }
    /// Review of #710: continuation leases copied the first page's expiry, so
    /// every cursor of a listing died 30 s after page one, however recently
    /// the previous page was read.
    #[tokio::test]
    async fn each_continuation_lease_gets_a_full_lifetime() {
        let service = MusicService::new(Catalogue);
        service.data.lock().await.generation = "g".into();
        let first = service.page("g", ":1.7", query("")).await.unwrap();
        assert_eq!(first.entries.len(), 100);
        // Page one was read 29 s ago: its lease is about to expire.
        let soon = Instant::now() + Duration::from_secs(1);
        for lease in service.data.lock().await.leases.values_mut() {
            lease.expires = soon;
        }
        let second = service
            .page("g", ":1.7", query(&first.next_token))
            .await
            .unwrap();
        let expires = service.data.lock().await.leases[&second.next_token].expires;
        assert!(expires > Instant::now() + Duration::from_secs(25));
        // A retry of the served token returns its cached page.
        let retried = service
            .page("g", ":1.7", query(&first.next_token))
            .await
            .unwrap();
        assert_eq!(retried.next_token, second.next_token);
    }
    /// Review of #710: when 32 leases were held, eviction could drop the lease
    /// being served, so a retry of that token failed with Conflict.
    #[tokio::test]
    async fn a_full_lease_table_never_evicts_the_lease_being_served() {
        let service = MusicService::new(Catalogue);
        service.data.lock().await.generation = "g".into();
        let first = service.page("g", ":1.7", query("")).await.unwrap();
        // Fill the table with leases that expire before the served one.
        {
            let mut d = service.data.lock().await;
            let template = d.leases[&first.next_token].clone();
            let earlier = Instant::now() + Duration::from_secs(5);
            d.leases.get_mut(&first.next_token).unwrap().expires = earlier;
            for i in 0..31 {
                let mut lease = template.clone();
                lease.expires = earlier + Duration::from_secs(1 + i);
                d.leases.insert(format!("other-{i}"), lease);
            }
        }
        let second = service
            .page("g", ":1.7", query(&first.next_token))
            .await
            .unwrap();
        assert!(!second.next_token.is_empty());
        let retried = service
            .page("g", ":1.7", query(&first.next_token))
            .await
            .unwrap();
        assert_eq!(retried.next_token, second.next_token);
        assert!(service.data.lock().await.leases.len() <= 32);
    }
}
