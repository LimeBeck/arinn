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
        // A provider that hangs on these IDs: longer than the call timeout.
        if id.starts_with("slow-") {
            tokio::time::sleep(Duration::from_secs(5)).await;
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
    address: String,
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
            address: address.trim().to_string(),
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
    assert!(!mentions_value(&format!("{snapshot:?}"), BROWSER_PID));
}

/// Whether `pid` appears in `text` as a value of its own. Matching a
/// substring would also find it inside a random UUID (#504).
fn mentions_value(text: &str, pid: u32) -> bool {
    let pid = pid.to_string();
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .any(|token| token == pid)
}

#[test]
fn a_pid_inside_a_uuid_is_not_a_leaked_pid() {
    assert!(mentions_value("Link { pid: 4100, id: \"x\" }", 4100));
    assert!(mentions_value("[4100]", 4100));
    assert!(!mentions_value(
        "Snapshot { epoch: \"0f3a4100-4100-4b2c-9a41-a4100b2c3d4e\" }",
        4100
    ));
    assert!(!mentions_value("pid: 41000", 4100));
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

#[tokio::test]
async fn a_provider_of_unknown_support_links_nothing() {
    let f = Fixture::new(
        &[("chromium.instance1", BROWSER_PID)],
        &[("c1:w1", BROWSER_PID)],
    )
    .await;
    let links = MediaLinks::new(&f.core.epoch);
    let mut cache = Cache::default();
    f.tabs(vec![tab("c1:t1", "c1:w1", true)]).await;
    // A second media provider whose owner cannot be introspected: its
    // sessions could share the process, so nothing is linked.
    let mut unreachable = provider("mpris2", "media", ":1.99999", &Uuid::new_v4().to_string());
    unreachable.sessions = vec![session("other")];
    f.core
        .state
        .lock()
        .await
        .providers
        .insert("mpris2".into(), unreachable);
    f.core.associate(&links, &mut cache).await;
    assert!(tabs_of(&links).await.is_empty());

    // Once it is gone, the link is made.
    f.core.state.lock().await.providers.remove("mpris2");
    f.core.associate(&links, &mut cache).await;
    assert_eq!(
        tabs_of(&links).await,
        vec![f.link("chromium.instance1", "c1:t1")]
    );
}

#[tokio::test]
async fn failing_lookups_do_not_starve_the_others() {
    // 64 sessions the provider does not know, sorted ahead of the one it does.
    let mut pids: Vec<(String, u32)> = (0..MAX_LOOKUPS).map(|n| (format!("a{n:03}"), 0)).collect();
    pids.push(("z-browser".into(), BROWSER_PID));
    let refs: Vec<(&str, u32)> = pids.iter().map(|(id, pid)| (id.as_str(), *pid)).collect();
    let f = Fixture::new(&refs, &[("c1:w1", BROWSER_PID)]).await;
    {
        let mut table = f.media_pids.lock().unwrap();
        table.retain(|id, _| id == "z-browser");
    }
    let links = MediaLinks::new(&f.core.epoch);
    let mut cache = Cache::default();
    let key = (
        "mpris".to_string(),
        f.media_generation.clone(),
        1,
        "z-browser".to_string(),
    );
    f.core.associate(&links, &mut cache).await;
    // The first pass spent its budget; the next one asks the rest first.
    f.core.associate(&links, &mut cache).await;
    assert_eq!(cache.processes.get(&key), Some(&BROWSER_PID));
}

#[tokio::test]
async fn hung_lookups_run_concurrently() {
    let pids: Vec<(String, u32)> = (0..20).map(|n| (format!("slow-{n}"), 1)).collect();
    let refs: Vec<(&str, u32)> = pids.iter().map(|(id, pid)| (id.as_str(), *pid)).collect();
    let f = Fixture::new(&refs, &[("c1:w1", BROWSER_PID)]).await;
    let links = MediaLinks::new(&f.core.epoch);
    let mut cache = Cache::default();
    let started = Instant::now();
    f.core.associate(&links, &mut cache).await;
    // One after another this would be 20 call timeouts (40 s).
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "{:?}",
        started.elapsed()
    );
}

/// Whether the last pass knew everything it asks for: every registration's
/// interface support, and the process of every session and window whose
/// provider exports its interface.
async fn knows_everything(core: &Core, cache: &Cache) -> bool {
    let (media, browsers) = core.sources().await;
    let known = |registration: &Registration, revision: u64, locals: &[String]| {
        let (id, _, generation) = registration;
        match cache.support.get(registration) {
            None => false,
            Some(false) => true,
            Some(true) => locals.iter().all(|local| {
                cache.processes.contains_key(&(
                    id.clone(),
                    generation.clone(),
                    revision,
                    local.clone(),
                ))
            }),
        }
    };
    media
        .iter()
        .all(|m| known(&m.registration, m.revision, &m.sessions))
        && browsers
            .iter()
            .all(|b| known(&b.registration, b.revision, &b.windows))
}

/// Runs passes until one is complete. On a loaded machine a provider call can
/// exceed CALL_TIMEOUT; that pass is incomplete by design and makes no link,
/// and production simply runs the next pass a second later (#404).
async fn complete_pass(core: &Core, links: &MediaLinks, cache: &mut Cache) {
    for _ in 0..10 {
        core.associate(links, cache).await;
        if knows_everything(core, cache).await {
            return;
        }
    }
    panic!("no complete pass in 10 attempts");
}

#[tokio::test]
async fn an_incomplete_pass_keeps_a_link_only_with_its_process() {
    // The middle pass must read the linked session's process again. If that
    // lookup times out under load, the link goes by design, and the attempt
    // shows nothing about the rule under test: start over (#404).
    for _ in 0..5 {
        if keeps_a_link_only_with_its_process().await {
            return;
        }
    }
    panic!("the linked session's process lookup timed out in every attempt");
}

/// `false` when the linked session's own lookup did not answer in time.
async fn keeps_a_link_only_with_its_process() -> bool {
    let f = Fixture::new(
        &[("chromium.instance1", BROWSER_PID), ("player", 77)],
        &[("c1:w1", BROWSER_PID)],
    )
    .await;
    let links = MediaLinks::new(&f.core.epoch);
    let mut cache = Cache::default();
    f.tabs(vec![tab("c1:t1", "c1:w1", true)]).await;
    complete_pass(&f.core, &links, &mut cache).await;
    assert_eq!(tabs_of(&links).await.len(), 1);

    // Another session cannot be read now, so the pass is incomplete; the
    // link holds because its own process is still known and unchanged.
    f.media_pids.lock().unwrap().remove("player");
    let bump = |core: Core| async move {
        let mut state = core.state.lock().await;
        let info = &mut state.providers.get_mut("mpris").unwrap().info;
        info.revision += 1;
        info.revision
    };
    let revision = bump(f.core.clone()).await;
    f.core.associate(&links, &mut cache).await;
    let own = (
        "mpris".to_string(),
        f.media_generation.clone(),
        revision,
        "chromium.instance1".to_string(),
    );
    if cache.processes.get(&own) != Some(&BROWSER_PID) {
        return false;
    }
    assert!(!knows_everything(&f.core, &cache).await);
    assert_eq!(tabs_of(&links).await.len(), 1);

    // Its own process cannot be read either: the evidence is gone, and so
    // is the link. A timed-out lookup leaves it just as unknown.
    f.media_pids.lock().unwrap().remove("chromium.instance1");
    bump(f.core.clone()).await;
    f.core.associate(&links, &mut cache).await;
    assert!(tabs_of(&links).await.is_empty());
    true
}

#[tokio::test]
async fn an_incomplete_pass_drops_a_link_that_known_evidence_breaks() {
    // As above: an attempt whose lookups of both readable sessions did not
    // answer in time proves nothing and starts over (#404).
    for _ in 0..5 {
        if drops_a_link_that_known_evidence_breaks().await {
            return;
        }
    }
    panic!("the readable sessions' process lookups timed out in every attempt");
}

/// `false` when a readable session's lookup did not answer in time.
async fn drops_a_link_that_known_evidence_breaks() -> bool {
    let f = Fixture::new(
        &[("chromium.instance1", BROWSER_PID), ("player", 77)],
        &[("c1:w1", BROWSER_PID)],
    )
    .await;
    let links = MediaLinks::new(&f.core.epoch);
    let mut cache = Cache::default();
    f.tabs(vec![tab("c1:t1", "c1:w1", true)]).await;
    complete_pass(&f.core, &links, &mut cache).await;
    assert_eq!(tabs_of(&links).await.len(), 1);

    // "player" now reports the browser's process too, while a third
    // session cannot be read, so the pass is incomplete. The duplicate is
    // already proof that the session is no longer unique.
    {
        let mut table = f.media_pids.lock().unwrap();
        table.insert("player".into(), BROWSER_PID);
    }
    let revision = {
        let mut state = f.core.state.lock().await;
        let mpris = state.providers.get_mut("mpris").unwrap();
        mpris.sessions.push(session("unreadable"));
        mpris.info.revision += 1;
        mpris.info.revision
    };
    f.core.associate(&links, &mut cache).await;
    // The link must go because of the duplicate, not because the linked
    // session's own lookup timed out.
    let read = |session: &str| {
        cache.processes.get(&(
            "mpris".to_string(),
            f.media_generation.clone(),
            revision,
            session.to_string(),
        )) == Some(&BROWSER_PID)
    };
    if !read("chromium.instance1") || !read("player") {
        return false;
    }
    assert!(tabs_of(&links).await.is_empty());
    true
}

#[tokio::test]
async fn a_provider_without_its_interface_keeps_cards_apart() {
    let f = Fixture::new(
        &[("chromium.instance1", BROWSER_PID)],
        &[("c1:w1", BROWSER_PID)],
    )
    .await;
    let links = MediaLinks::new(&f.core.epoch);
    let mut cache = Cache::default();
    f.tabs(vec![tab("c1:t1", "c1:w1", true)]).await;
    complete_pass(&f.core, &links, &mut cache).await;
    assert_eq!(tabs_of(&links).await.len(), 1);

    // A media provider that does not export MediaProcess1 has a session:
    // it could be the same process, so no link is provable, held or new.
    // (The browser's connection answers introspection, without that one.)
    let mut silent = provider(
        "player2",
        "media",
        f.browser.unique_name().unwrap().as_str(),
        &Uuid::new_v4().to_string(),
    );
    silent.sessions = vec![session("other")];
    f.core
        .state
        .lock()
        .await
        .providers
        .insert("player2".into(), silent);
    // Until its support is known the pass is incomplete and the link is
    // held; a timed-out introspection only delays this (#404).
    complete_pass(&f.core, &links, &mut cache).await;
    assert!(tabs_of(&links).await.is_empty());

    // Without sessions it proves nothing either way and blocks nothing.
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("player2")
        .unwrap()
        .sessions
        .clear();
    f.core.associate(&links, &mut cache).await;
    assert_eq!(tabs_of(&links).await.len(), 1);
}

/// A bus peer that never answers: its connection's runtime is not driven
/// after the connection is built.
fn hung_peer(address: &str) -> String {
    let (sent, received) = std::sync::mpsc::channel();
    let address = address.to_string();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let connection = runtime.block_on(connect(&address));
        sent.send(connection.unique_name().unwrap().to_string())
            .unwrap();
        std::thread::sleep(Duration::from_secs(60));
        drop(connection);
    });
    received.recv().unwrap()
}

#[tokio::test]
async fn unresponsive_registrations_are_introspected_concurrently() {
    let f = Fixture::new(
        &[("chromium.instance1", BROWSER_PID)],
        &[("c1:w1", BROWSER_PID)],
    )
    .await;
    {
        let mut state = f.core.state.lock().await;
        for n in 0..5 {
            let mut hung = provider(
                &format!("hung{n}"),
                "media",
                &hung_peer(&f.address),
                &Uuid::new_v4().to_string(),
            );
            hung.sessions = vec![session("x")];
            state.providers.insert(format!("hung{n}"), hung);
        }
    }
    let links = MediaLinks::new(&f.core.epoch);
    let mut cache = Cache::default();
    let started = Instant::now();
    f.core.associate(&links, &mut cache).await;
    // One after another this would be five call timeouts (10 s).
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "{:?}",
        started.elapsed()
    );
    // Still unknown: nothing is linked, and they are asked again.
    assert!(tabs_of(&links).await.is_empty());
}
