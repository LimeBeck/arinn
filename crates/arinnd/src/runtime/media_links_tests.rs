// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon join of ADR-0096 against fake providers on a private bus.
use super::*;
use crate::{runtime::Provider, storage::Paths};
use arinn_protocol::{
    browser::{BrowserWindow, Tab},
    media::Session,
    media_association::{MediaAssociationApiProxy, MediaTabLink},
    production::{EntityRef, ProviderInfo},
};
use futures_util::StreamExt;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{
        Mutex as StdMutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use uuid::Uuid;
use zbus::{Connection, message::Header};

const BROWSER_PID: u32 = 4100;

/// Answers only the daemon; the map is the provider's process table.
struct FakeProcesses {
    daemon: String,
    generation: String,
    table: Arc<StdMutex<HashMap<String, u32>>>,
    /// Set: the next call first publishes a new snapshot revision of this
    /// provider, as a provider that changes state under a lookup would.
    bump: Option<(Core, &'static str, Arc<AtomicBool>)>,
    /// Set: the next call first registers this provider, as one that starts
    /// during a lookup would.
    arrive: Option<(Core, Arc<StdMutex<Option<Provider>>>)>,
}
impl FakeProcesses {
    async fn answer(
        &self,
        header: &Header<'_>,
        generation: &str,
        id: &str,
    ) -> production::Result<u32> {
        if let Some((core, provider, armed)) = &self.bump
            && armed.swap(false, Ordering::SeqCst)
        {
            core.state
                .lock()
                .await
                .providers
                .get_mut(*provider)
                .unwrap()
                .info
                .revision += 1;
        }
        if let Some((core, pending)) = &self.arrive {
            let newcomer = pending.lock().unwrap().take();
            if let Some(newcomer) = newcomer {
                let id = newcomer.info.id.clone();
                core.state.lock().await.providers.insert(id, newcomer);
            }
        }
        if header.sender().map(|s| s.as_str()) != Some(self.daemon.as_str()) {
            return Err(ErrorCode::PermissionDenied.into());
        }
        if generation != self.generation {
            return Err(ErrorCode::Conflict.into());
        }
        Ok(*self
            .table
            .lock()
            .unwrap()
            .get(id)
            .ok_or(ErrorCode::NotFound)?)
    }
}
struct FakeMediaProcess(FakeProcesses);
#[zbus::interface(name = "org.arinn.MediaProcess1")]
impl FakeMediaProcess {
    async fn get_process_id(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<u32> {
        self.0.answer(&header, generation, id).await
    }
}
struct FakeBrowserProcess(FakeProcesses);
#[zbus::interface(name = "org.arinn.BrowserProcess1")]
impl FakeBrowserProcess {
    async fn get_process_id(
        &self,
        generation: &str,
        window_id: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<u32> {
        self.0.answer(&header, generation, window_id).await
    }
}

fn provider(id: &str, capability: &str, owner: &str, generation: &str) -> Provider {
    Provider {
        info: ProviderInfo {
            id: id.into(),
            plugin_id: id.into(),
            source_path: String::new(),
            name: id.into(),
            version: "0.0.1".into(),
            path: format!("/org/arinn/Daemon1/providers/p_{id}")
                .try_into()
                .unwrap(),
            capabilities: vec![capability.into()],
            runtime_state: "ready".into(),
            backend_state: "available".into(),
            generation: generation.into(),
            revision: 1,
            last_error: String::new(),
            error_at: 0,
            recovered_at: 0,
            restarts_since_daemon_start: 0,
            cumulative_restart_count: 0,
        },
        manifest: None,
        unit: format!("{id}.service"),
        token_file: std::env::temp_dir().join(format!("{id}.token")),
        token: String::new(),
        owner: owner.into(),
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
    }
}
fn session(id: &str) -> Session {
    Session {
        id: id.into(),
        title: "Same title".into(),
        subtitle: String::new(),
        artwork: String::new(),
        state: "playing".into(),
        duration_us: -1,
        position_us: 0,
        anchor_us: 1,
        actions: Vec::new(),
        has_content: false,
        content: EntityRef {
            provider_id: String::new(),
            capability: String::new(),
            local_id: String::new(),
        },
    }
}
fn tab(id: &str, window: &str, audible: bool) -> Tab {
    Tab {
        id: id.into(),
        window_id: window.into(),
        title: "Same title".into(),
        url: "https://video.example/".into(),
        active: false,
        audible,
    }
}

struct Fixture {
    bus: Child,
    folder: std::path::PathBuf,
    core: Core,
    client: Connection,
    media: Connection,
    browser: Connection,
    media_generation: String,
    browser_generation: String,
    /// The media provider's process table, shared with its fake export.
    media_pids: Arc<StdMutex<HashMap<String, u32>>>,
    /// Arms a snapshot change during the media provider's next lookup.
    media_bump: Arc<AtomicBool>,
    /// A provider that registers during the media provider's next lookup.
    arrival: Arc<StdMutex<Option<Provider>>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.bus.kill();
        let _ = self.bus.wait();
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}
async fn connect(address: &str) -> Connection {
    zbus::connection::Builder::address(address.trim())
        .unwrap()
        .build()
        .await
        .unwrap()
}
impl Fixture {
    async fn new(media_pids: &[(&str, u32)], window_pids: &[(&str, u32)]) -> Self {
        let mut bus = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(bus.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let server = connect(&address).await;
        let media = connect(&address).await;
        let browser = connect(&address).await;
        let client = connect(&address).await;
        let folder = std::env::temp_dir().join(format!("arinn-links-{}", Uuid::new_v4()));
        std::fs::create_dir(&folder).unwrap();
        let core = Core::new(
            &server,
            Paths {
                config: folder.join("config"),
                data: folder.join("data"),
                state: folder.join("state"),
            },
            folder.clone(),
        );
        let daemon = server.unique_name().unwrap().to_string();
        let media_generation = Uuid::new_v4().to_string();
        let browser_generation = Uuid::new_v4().to_string();
        let table = |pairs: &[(&str, u32)]| {
            Arc::new(StdMutex::new(
                pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            ))
        };
        let media_table = table(media_pids);
        let media_bump = Arc::new(AtomicBool::new(false));
        let arrival = Arc::new(StdMutex::new(None));
        media
            .object_server()
            .at(
                production::PROVIDER_PATH,
                FakeMediaProcess(FakeProcesses {
                    daemon: daemon.clone(),
                    generation: media_generation.clone(),
                    table: media_table.clone(),
                    bump: Some((core.clone(), "mpris", media_bump.clone())),
                    arrive: Some((core.clone(), arrival.clone())),
                }),
            )
            .await
            .unwrap();
        browser
            .object_server()
            .at(
                production::PROVIDER_PATH,
                FakeBrowserProcess(FakeProcesses {
                    daemon,
                    generation: browser_generation.clone(),
                    table: table(window_pids),
                    bump: None,
                    arrive: None,
                }),
            )
            .await
            .unwrap();
        {
            let mut state = core.state.lock().await;
            let mut m = provider(
                "mpris",
                "media",
                media.unique_name().unwrap().as_str(),
                &media_generation,
            );
            m.sessions = media_pids.iter().map(|(id, _)| session(id)).collect();
            state.providers.insert("mpris".into(), m);
            let mut b = provider(
                "chrome",
                "browser",
                browser.unique_name().unwrap().as_str(),
                &browser_generation,
            );
            b.windows = window_pids
                .iter()
                .map(|(id, _)| BrowserWindow {
                    id: id.to_string(),
                    window_token: String::new(),
                    focused: false,
                })
                .collect();
            state.providers.insert("chrome".into(), b);
        }
        Self {
            bus,
            folder,
            core,
            client,
            media,
            browser,
            media_generation,
            browser_generation,
            media_pids: media_table,
            media_bump,
            arrival,
        }
    }
    async fn tabs(&self, tabs: Vec<Tab>) {
        self.core
            .state
            .lock()
            .await
            .providers
            .get_mut("chrome")
            .unwrap()
            .tabs = tabs;
    }
    fn link(&self, session: &str, tab: &str) -> MediaTabLink {
        MediaTabLink {
            media_provider: "mpris".into(),
            media_generation: self.media_generation.clone(),
            session: session.into(),
            browser_provider: "chrome".into(),
            browser_generation: self.browser_generation.clone(),
            tab: tab.into(),
        }
    }
}

async fn tabs_of(links: &MediaLinks) -> Vec<MediaTabLink> {
    links.links().await.into_iter().map(|h| h.link).collect()
}

#[tokio::test]
async fn links_hold_move_and_reach_the_shell() {
    let f = Fixture::new(
        &[("chromium.instance1", BROWSER_PID), ("player", 77)],
        &[("c1:w1", BROWSER_PID), ("c1:w2", BROWSER_PID)],
    )
    .await;
    let links = MediaLinks::new(&f.core.epoch);
    let mut cache = Cache::default();
    f.tabs(vec![
        tab("c1:t1", "c1:w1", true),
        tab("c1:t2", "c1:w2", false),
    ])
    .await;
    f.core.associate(&links, &mut cache).await;
    assert_eq!(
        tabs_of(&links).await,
        vec![f.link("chromium.instance1", "c1:t1")]
    );

    // Paused: nothing is audible, the link holds without a new revision.
    let revision = links.published.lock().await.revision;
    f.tabs(vec![
        tab("c1:t1", "c1:w1", false),
        tab("c1:t2", "c1:w2", false),
    ])
    .await;
    f.core.associate(&links, &mut cache).await;
    assert_eq!(
        tabs_of(&links).await,
        vec![f.link("chromium.instance1", "c1:t1")]
    );
    assert_eq!(links.published.lock().await.revision, revision);

    // Another tab of the same browser becomes the only audible one.
    f.tabs(vec![
        tab("c1:t1", "c1:w1", false),
        tab("c1:t2", "c1:w2", true),
    ])
    .await;
    f.core.associate(&links, &mut cache).await;
    assert_eq!(
        tabs_of(&links).await,
        vec![f.link("chromium.instance1", "c1:t2")]
    );

    // The linked tab closes: the link goes.
    f.tabs(vec![tab("c1:t1", "c1:w1", false)]).await;
    f.core.associate(&links, &mut cache).await;
    assert!(tabs_of(&links).await.is_empty());

    // The exported endpoint publishes the same rules to the shell. Its loop
    // starts only now, with no tab open, so no pass of it can carry an older
    // state past the subscription. The match is installed before the change,
    // as the contract requires.
    f.tabs(Vec::new()).await;
    f.core.export_media_association().await.unwrap();
    let api = MediaAssociationApiProxy::builder(&f.client)
        .destination(f.core.connection.unique_name().unwrap().as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert!(api.get().await.unwrap().0.links.is_empty());
    let mut changes = api.receive_changed().await.unwrap();
    f.tabs(vec![tab("c1:t1", "c1:w1", true)]).await;
    let (snapshot,) = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let snapshot = api.get().await.unwrap();
            if !snapshot.0.links.is_empty() {
                break snapshot;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    snapshot.validate().unwrap();
    assert_eq!(snapshot.epoch, f.core.epoch);
    assert_eq!(snapshot.links, vec![f.link("chromium.instance1", "c1:t1")]);
    let signal = tokio::time::timeout(Duration::from_secs(5), changes.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(signal.args().unwrap().epoch(), &f.core.epoch);
    // Process IDs are never part of what the shell reads.
    assert!(!format!("{snapshot:?}").contains(&BROWSER_PID.to_string()));
}

#[tokio::test]
async fn ambiguity_restart_and_missing_interfaces_link_nothing() {
    let f = Fixture::new(
        &[("chromium.instance1", BROWSER_PID)],
        &[("c1:w1", BROWSER_PID)],
    )
    .await;
    let links = MediaLinks::new(&f.core.epoch);
    let mut cache = Cache::default();
    // Two audible tabs: never the first.
    f.tabs(vec![
        tab("c1:t1", "c1:w1", true),
        tab("c1:t2", "c1:w1", true),
    ])
    .await;
    f.core.associate(&links, &mut cache).await;
    assert!(tabs_of(&links).await.is_empty());

    f.tabs(vec![tab("c1:t1", "c1:w1", true)]).await;
    f.core.associate(&links, &mut cache).await;
    assert_eq!(tabs_of(&links).await.len(), 1);

    // A browser provider restart (new generation) drops the link even
    // before its process IDs are read again.
    let generation = Uuid::new_v4().to_string();
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("chrome")
        .unwrap()
        .info
        .generation = generation;
    f.core.associate(&links, &mut cache).await;
    assert!(tabs_of(&links).await.is_empty());

    // A browser provider without BrowserProcess1 links nothing.
    f.browser
        .object_server()
        .remove::<FakeBrowserProcess, _>(production::PROVIDER_PATH)
        .await
        .unwrap();
    let mut fresh = Cache::default();
    let restarted = Uuid::new_v4().to_string();
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("chrome")
        .unwrap()
        .info
        .generation = restarted;
    f.core.associate(&links, &mut fresh).await;
    assert!(tabs_of(&links).await.is_empty());
    let _ = &f.media;
}

#[tokio::test]
async fn a_new_snapshot_revision_reads_process_ids_again() {
    let f = Fixture::new(
        &[("chromium.instance1", BROWSER_PID)],
        &[("c1:w1", BROWSER_PID)],
    )
    .await;
    let links = MediaLinks::new(&f.core.epoch);
    let mut cache = Cache::default();
    f.tabs(vec![tab("c1:t1", "c1:w1", true)]).await;
    f.core.associate(&links, &mut cache).await;
    assert_eq!(
        tabs_of(&links).await,
        vec![f.link("chromium.instance1", "c1:t1")]
    );

    // The provider keeps the local ID for another process and publishes a
    // new snapshot revision: the cached process ID must not keep the link.
    f.media_pids
        .lock()
        .unwrap()
        .insert("chromium.instance1".into(), 77);
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("mpris")
        .unwrap()
        .info
        .revision += 1;
    f.core.associate(&links, &mut cache).await;
    assert!(tabs_of(&links).await.is_empty());

    // The same revision is not read again: answers within one snapshot are
    // stable, so a lookup per pass is not repeated.
    f.media_pids
        .lock()
        .unwrap()
        .insert("chromium.instance1".into(), BROWSER_PID);
    f.core.associate(&links, &mut cache).await;
    assert!(tabs_of(&links).await.is_empty());
}

#[tokio::test]
async fn a_snapshot_that_moves_during_lookups_links_nothing_yet() {
    let f = Fixture::new(
        &[("chromium.instance1", BROWSER_PID)],
        &[("c1:w1", BROWSER_PID)],
    )
    .await;
    let links = MediaLinks::new(&f.core.epoch);
    let mut cache = Cache::default();
    f.tabs(vec![tab("c1:t1", "c1:w1", true)]).await;
    // The media provider publishes a new revision while its process ID is
    // read: the answer may belong to another state than the tabs read.
    f.media_bump.store(true, Ordering::SeqCst);
    f.core.associate(&links, &mut cache).await;
    assert!(tabs_of(&links).await.is_empty());

    // The next pass reads one revision throughout and links.
    f.core.associate(&links, &mut cache).await;
    assert_eq!(
        tabs_of(&links).await,
        vec![f.link("chromium.instance1", "c1:t1")]
    );
}

#[tokio::test]
async fn a_provider_that_registers_during_lookups_links_nothing_yet() {
    let f = Fixture::new(
        &[("chromium.instance1", BROWSER_PID)],
        &[("c1:w1", BROWSER_PID)],
    )
    .await;
    let links = MediaLinks::new(&f.core.epoch);
    let mut cache = Cache::default();
    f.tabs(vec![tab("c1:t1", "c1:w1", true)]).await;
    // A second media provider appears while the first one's process is
    // read. Its session could claim the same process; until a pass sees
    // the new set from the start, nothing new is linked.
    let mut newcomer = provider(
        "mpris2",
        "media",
        f.media.unique_name().unwrap().as_str(),
        &Uuid::new_v4().to_string(),
    );
    newcomer.sessions = vec![session("other")];
    *f.arrival.lock().unwrap() = Some(newcomer);
    f.core.associate(&links, &mut cache).await;
    assert!(tabs_of(&links).await.is_empty());
}
