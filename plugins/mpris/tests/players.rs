// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Private-bus MPRIS player: sessions appear and disappear, anchors move on
//! state changes, commands map, and a missing capability is `Unsupported`.
use arinn_plugin_mpris::serve;
use arinn_protocol::{
    media::MediaProviderProxy,
    production::{self, Registration, RegistrationReply},
};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use zbus::{
    Connection,
    message::Header,
    zvariant::{OwnedObjectPath, OwnedValue},
};

struct Bus(Child, String);
impl Bus {
    fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("private dbus-daemon");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(child, address.trim().into())
    }
    async fn connection(&self) -> Connection {
        zbus::connection::Builder::address(self.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Clone, Default)]
struct Daemon;
#[zbus::interface(name = "org.arinn.Daemon1")]
impl Daemon {
    async fn register(
        &self,
        request: Registration,
        #[zbus(header)] _header: Header<'_>,
    ) -> production::Result<(RegistrationReply,)> {
        assert_eq!(request.capabilities, vec!["media".to_string()]);
        Ok((RegistrationReply {
            epoch: "00000000-0000-4000-8000-000000000001".into(),
            generation: "00000000-0000-4000-8000-000000000002".into(),
            api_minor: 0,
            max_body_bytes: 262144,
            max_page_entries: 200,
            max_operations: 32,
        },))
    }
}

struct Inner {
    status: Mutex<String>,
    position: Mutex<i64>,
    metadata: Mutex<HashMap<String, OwnedValue>>,
    can_play: AtomicBool,
    can_pause: AtomicBool,
    can_seek: AtomicBool,
    can_next: AtomicBool,
    can_previous: AtomicBool,
    can_control: AtomicBool,
    calls: Mutex<Vec<String>>,
}
#[derive(Clone)]
struct Player(Arc<Inner>);
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn play(&self) {
        self.0.calls.lock().unwrap().push("Play".into());
    }
    fn pause(&self) {
        self.0.calls.lock().unwrap().push("Pause".into());
    }
    fn play_pause(&self) {
        self.0.calls.lock().unwrap().push("PlayPause".into());
    }
    fn stop(&self) {
        self.0.calls.lock().unwrap().push("Stop".into());
    }
    fn next(&self) {
        self.0.calls.lock().unwrap().push("Next".into());
    }
    fn previous(&self) {
        self.0.calls.lock().unwrap().push("Previous".into());
    }
    fn seek(&self, offset: i64) {
        self.0.calls.lock().unwrap().push(format!("Seek:{offset}"));
    }
    fn set_position(&self, _track_id: OwnedObjectPath, position: i64) {
        self.0
            .calls
            .lock()
            .unwrap()
            .push(format!("SetPosition:{position}"));
        *self.0.position.lock().unwrap() = position;
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.0.status.lock().unwrap().clone()
    }
    #[zbus(property)]
    fn position(&self) -> i64 {
        *self.0.position.lock().unwrap()
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        self.0.metadata.lock().unwrap().clone()
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.0.can_play.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.0.can_pause.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.0.can_seek.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.0.can_next.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.0.can_previous.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        self.0.can_control.load(Ordering::SeqCst)
    }
}

/// `PlaybackStatus` never returns, so `GetAll` on this player does not finish.
struct HungPlayer;
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl HungPlayer {
    #[zbus(property)]
    async fn playback_status(&self) -> String {
        std::future::pending().await
    }
}

fn owned(value: impl Into<zbus::zvariant::Value<'static>>) -> OwnedValue {
    OwnedValue::try_from(value.into()).unwrap()
}

fn player(title: &str, with_track: bool) -> Player {
    let mut metadata = HashMap::new();
    metadata.insert("xesam:title".into(), owned(title.to_string()));
    metadata.insert("xesam:artist".into(), owned(vec!["Artist".to_string()]));
    metadata.insert("xesam:album".into(), owned("Album".to_string()));
    metadata.insert("mpris:length".into(), owned(10_000_000_i64));
    metadata.insert("mpris:artUrl".into(), owned("not-forwarded".to_string()));
    if with_track {
        metadata.insert(
            "mpris:trackid".into(),
            owned(OwnedObjectPath::try_from("/org/mpris/MediaPlayer2/Track/1").unwrap()),
        );
    }
    Player(Arc::new(Inner {
        status: Mutex::new("Playing".into()),
        position: Mutex::new(1_000),
        metadata: Mutex::new(metadata),
        can_play: AtomicBool::new(true),
        can_pause: AtomicBool::new(true),
        can_seek: AtomicBool::new(true),
        can_next: AtomicBool::new(true),
        can_previous: AtomicBool::new(true),
        can_control: AtomicBool::new(true),
        calls: Mutex::new(Vec::new()),
    }))
}

async fn emit(connection: &Connection, player: &Player, which: &str) {
    let iface = connection
        .object_server()
        .interface::<_, Player>("/org/mpris/MediaPlayer2")
        .await
        .unwrap();
    let emitter = iface.signal_emitter();
    match which {
        "status" => Player::playback_status_changed(player, emitter)
            .await
            .unwrap(),
        "position" => Player::position_changed(player, emitter).await.unwrap(),
        "seek" => Player::can_seek_changed(player, emitter).await.unwrap(),
        _ => panic!("unknown property"),
    }
}

fn error_name(result: zbus::Result<impl std::fmt::Debug>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => panic!("expected an Arinn error, got {other:?}"),
    }
}

async fn scenario() {
    let bus = Bus::start();
    let daemon = bus.connection().await;
    daemon
        .object_server()
        .at(production::PATH, Daemon)
        .await
        .unwrap();
    daemon.request_name(production::SERVICE).await.unwrap();

    let player_bus = bus.connection().await;
    let fixture = player("Fixture", true);
    let fixture_iface = fixture.clone();
    let calls = fixture.0.clone();
    player_bus
        .object_server()
        .at("/org/mpris/MediaPlayer2", fixture)
        .await
        .unwrap();
    player_bus
        .request_name("org.mpris.MediaPlayer2.fixture")
        .await
        .unwrap();

    let provider = bus.connection().await;
    let destination = provider.unique_name().unwrap().clone();
    let runner = tokio::spawn(async move {
        // Dropping the bus at the end of the test closes this task.
        let _ = serve(provider).await;
    });
    let proxy = MediaProviderProxy::builder(&daemon)
        .destination(&destination)
        .unwrap()
        .build()
        .await
        .unwrap();

    let (page,) = wait_page(&proxy, |page| page.sessions.len() == 1).await;
    let session = &page.sessions[0];
    assert_eq!(session.id, "fixture");
    assert_eq!(session.title, "Fixture");
    assert_eq!(session.subtitle, "Artist");
    assert!(session.artwork.is_empty());
    assert_eq!(session.state, "playing");
    assert_eq!(session.duration_us, 10_000_000);
    assert_eq!(session.position_us, 1_000);
    assert!(session.anchor_us > 0);
    assert_eq!(
        session.actions,
        vec![
            "play",
            "pause",
            "play-pause",
            "seek",
            "next",
            "previous",
            "stop",
        ]
    );
    assert!(!session.has_content);
    let anchor = session.anchor_us;
    let generation = page.generation.clone();

    assert!(proxy.features().await.unwrap().is_empty());
    let introspect = zbus::fdo::IntrospectableProxy::builder(&daemon)
        .destination(&destination)
        .unwrap()
        .path(production::PROVIDER_PATH)
        .unwrap()
        .build()
        .await
        .unwrap();
    let xml = introspect.introspect().await.unwrap();
    assert!(xml.contains("org.arinn.MediaProvider1"));
    assert!(!xml.contains("org.arinn.MediaOpen1"));
    assert!(!xml.contains("org.arinn.MediaTracks1"));
    assert!(!xml.contains("org.arinn.MediaTimeline1"));
    // Introspection can outlast the one-second playing refresh. The anchor
    // is unchanged, or it has moved by that one republish (ADR-0054).
    let (again,) = proxy.get_sessions("").await.unwrap();
    assert_eq!(again.sessions[0].id, "fixture");
    assert_eq!(again.sessions[0].state, "playing");
    assert_eq!(again.sessions[0].position_us, 1_000);
    assert_same_or_one_republish(anchor, again.sessions[0].anchor_us);

    tokio::time::sleep(Duration::from_millis(2)).await;
    *calls.status.lock().unwrap() = "Paused".into();
    emit(&player_bus, &fixture_iface, "status").await;
    let (paused,) = wait_page(&proxy, |page| {
        page.sessions
            .first()
            .is_some_and(|session| session.state == "paused")
    })
    .await;
    assert!(paused.sessions[0].anchor_us > anchor);
    let paused_anchor = paused.sessions[0].anchor_us;

    tokio::time::sleep(Duration::from_millis(2)).await;
    *calls.status.lock().unwrap() = "Playing".into();
    emit(&player_bus, &fixture_iface, "status").await;
    let (playing,) = wait_page(&proxy, |page| {
        page.sessions
            .first()
            .is_some_and(|session| session.state == "playing" && session.anchor_us != paused_anchor)
    })
    .await;
    let playing_anchor = playing.sessions[0].anchor_us;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (held,) = proxy.get_sessions("").await.unwrap();
    assert_eq!(held.sessions[0].state, "playing");
    assert_same_or_one_republish(playing_anchor, held.sessions[0].anchor_us);

    let generation = generation.as_str();
    proxy.play(generation, "fixture").await.unwrap();
    proxy.pause(generation, "fixture").await.unwrap();
    proxy.play_pause(generation, "fixture").await.unwrap();
    proxy.seek(generation, "fixture", 2_000_000).await.unwrap();
    proxy.next(generation, "fixture").await.unwrap();
    proxy.previous(generation, "fixture").await.unwrap();
    proxy.stop(generation, "fixture").await.unwrap();
    assert_eq!(
        *calls.calls.lock().unwrap(),
        vec![
            "Play".to_string(),
            "Pause".to_string(),
            "PlayPause".to_string(),
            "SetPosition:2000000".to_string(),
            "Next".to_string(),
            "Previous".to_string(),
            "Stop".to_string(),
        ]
    );
    let (sought,) = wait_page(&proxy, |page| {
        page.sessions.first().is_some_and(|session| {
            session.position_us == 2_000_000 && session.anchor_us != playing_anchor
        })
    })
    .await;
    let _ = sought;

    calls.can_seek.store(false, Ordering::SeqCst);
    emit(&player_bus, &fixture_iface, "seek").await;
    let (unseekable,) = wait_page(&proxy, |page| {
        page.sessions
            .first()
            .is_some_and(|session| !session.actions.iter().any(|action| action == "seek"))
    })
    .await;
    assert_eq!(
        error_name(
            proxy
                .seek(&unseekable.generation, "fixture", 3_000_000)
                .await
        ),
        "org.arinn.Error1.Unsupported"
    );
    assert!(
        !calls
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| call.starts_with("SetPosition") && call.ends_with("3000000"))
    );

    let other_bus = bus.connection().await;
    let other = player("Other", false);
    let other_calls = other.0.clone();
    *other_calls.position.lock().unwrap() = 1_000;
    other_calls.metadata.lock().unwrap().insert(
        "mpris:trackid".into(),
        owned(OwnedObjectPath::try_from("/org/mpris/MediaPlayer2/TrackList/NoTrack").unwrap()),
    );
    other_bus
        .object_server()
        .at("/org/mpris/MediaPlayer2", other)
        .await
        .unwrap();
    other_bus
        .request_name("org.mpris.MediaPlayer2.other")
        .await
        .unwrap();
    let (both,) = wait_page(&proxy, |page| page.sessions.len() == 2).await;
    assert_eq!(
        both.sessions
            .iter()
            .map(|session| session.id.as_str())
            .collect::<Vec<_>>(),
        vec!["fixture", "other"]
    );
    assert_eq!(both.sessions[1].title, "Other");
    assert!(both.sessions[1].artwork.is_empty());
    assert_eq!(both.sessions[1].position_us, 1_000);
    // The published position stays at the anchor. The offset must use the
    // live Position (500_000), and the no-track id must not become SetPosition.
    *other_calls.position.lock().unwrap() = 500_000;
    let before = other_calls.calls.lock().unwrap().len();
    proxy
        .seek(both.generation.as_str(), "other", 4_000_000)
        .await
        .unwrap();
    wait_call(&other_calls, before, "Seek:3500000").await;
    *other_calls.position.lock().unwrap() = 1_000;

    player_bus
        .release_name("org.mpris.MediaPlayer2.fixture")
        .await
        .unwrap();
    let (left,) = wait_page(&proxy, |page| page.sessions.len() == 1).await;
    assert_eq!(left.sessions[0].id, "other");

    let before = other_calls.calls.lock().unwrap().len();
    proxy.seek(&left.generation, "other", 4_000).await.unwrap();
    wait_call(&other_calls, before, "Seek:3000").await;

    other_bus
        .release_name("org.mpris.MediaPlayer2.other")
        .await
        .unwrap();
    let (gone,) = wait_page(&proxy, |page| page.sessions.is_empty()).await;
    assert!(gone.sessions.is_empty());
    runner.abort();
}

async fn hung_scenario() {
    let bus = Bus::start();
    let daemon = bus.connection().await;
    daemon
        .object_server()
        .at(production::PATH, Daemon)
        .await
        .unwrap();
    daemon.request_name(production::SERVICE).await.unwrap();

    let provider = bus.connection().await;
    let destination = provider.unique_name().unwrap().clone();
    let runner = tokio::spawn(async move {
        // Dropping the bus at the end of the test closes this task.
        let _ = serve(provider).await;
    });
    let proxy = MediaProviderProxy::builder(&daemon)
        .destination(&destination)
        .unwrap()
        .build()
        .await
        .unwrap();

    let hung_bus = bus.connection().await;
    hung_bus
        .object_server()
        .at("/org/mpris/MediaPlayer2", HungPlayer)
        .await
        .unwrap();
    hung_bus
        .request_name("org.mpris.MediaPlayer2.hung")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let healthy_bus = bus.connection().await;
    let healthy = player("Healthy", true);
    let healthy_iface = healthy.clone();
    let healthy_state = healthy.0.clone();
    healthy_bus
        .object_server()
        .at("/org/mpris/MediaPlayer2", healthy)
        .await
        .unwrap();
    healthy_bus
        .request_name("org.mpris.MediaPlayer2.healthy")
        .await
        .unwrap();

    let (page,) = wait_page_for(&proxy, Duration::from_secs(8), |page| {
        page.sessions.iter().any(|session| session.id == "healthy")
    })
    .await;
    assert!(page.sessions.iter().all(|session| session.id != "hung"));
    let answered = tokio::time::timeout(Duration::from_secs(2), proxy.get_sessions("")).await;
    assert!(
        answered.is_ok(),
        "GetSessions did not return while a player was stalled"
    );

    *healthy_state.status.lock().unwrap() = "Paused".into();
    emit(&healthy_bus, &healthy_iface, "status").await;
    let (paused,) = wait_page_for(&proxy, Duration::from_secs(8), |page| {
        page.sessions
            .iter()
            .any(|session| session.id == "healthy" && session.state == "paused")
    })
    .await;
    assert!(paused.sessions.iter().all(|session| session.id != "hung"));
    runner.abort();
}

async fn late_scenario() {
    let bus = Bus::start();
    let daemon = bus.connection().await;
    daemon
        .object_server()
        .at(production::PATH, Daemon)
        .await
        .unwrap();
    daemon.request_name(production::SERVICE).await.unwrap();

    let provider = bus.connection().await;
    let destination = provider.unique_name().unwrap().clone();
    let runner = tokio::spawn(async move {
        // Dropping the bus at the end of the test closes this task.
        let _ = serve(provider).await;
    });
    let proxy = MediaProviderProxy::builder(&daemon)
        .destination(&destination)
        .unwrap()
        .build()
        .await
        .unwrap();

    let player_bus = bus.connection().await;
    player_bus
        .request_name("org.mpris.MediaPlayer2.late")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let fixture = player("Late", true);
    player_bus
        .object_server()
        .at("/org/mpris/MediaPlayer2", fixture)
        .await
        .unwrap();

    let (page,) = wait_page(&proxy, |page| {
        page.sessions.iter().any(|session| session.id == "late")
    })
    .await;
    assert_eq!(page.sessions[0].title, "Late");
    runner.abort();
}

/// `after` matches `before`, or is one playing republish later (about 1s).
fn assert_same_or_one_republish(before: u64, after: u64) {
    if after == before {
        return;
    }
    let delta = after.saturating_sub(before);
    assert!(
        (900_000..1_500_000).contains(&delta),
        "anchor moved by {delta} us; expected the same anchor or one ~1s republish"
    );
}

async fn wait_page(
    proxy: &MediaProviderProxy<'_>,
    ready: impl Fn(&arinn_protocol::media::SessionPage) -> bool,
) -> (arinn_protocol::media::SessionPage,) {
    wait_page_for(proxy, Duration::from_secs(5), ready).await
}

async fn wait_page_for(
    proxy: &MediaProviderProxy<'_>,
    limit: Duration,
    ready: impl Fn(&arinn_protocol::media::SessionPage) -> bool,
) -> (arinn_protocol::media::SessionPage,) {
    let deadline = Instant::now() + limit;
    loop {
        if let Ok(Ok(page)) =
            tokio::time::timeout(Duration::from_secs(2), proxy.get_sessions("")).await
            && ready(&page.0)
        {
            return page;
        }
        if Instant::now() > deadline {
            panic!("session condition timed out");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_call(inner: &Arc<Inner>, before: usize, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let recorded = inner.calls.lock().unwrap().clone();
        if recorded.len() > before {
            assert_eq!(recorded.last().map(String::as_str), Some(expected));
            return;
        }
        if Instant::now() > deadline {
            panic!("call {expected} was not recorded, got {recorded:?}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn enter(test: &str) -> bool {
    if std::env::var_os("ARINN_MPRIS_CHILD").is_some() {
        return true;
    }
    let token = std::env::temp_dir().join(format!("arinn-mpris-{}-{}", test, std::process::id()));
    std::fs::write(&token, "fixture-token").unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("ARINN_MPRIS_CHILD", "1")
        .env("ARINN_PROVIDER_ID", "mpris")
        .env("ARINN_PLUGIN_ID", "mpris")
        .env("ARINN_PLUGIN_VERSION", "0.0.1")
        .env("ARINN_TOKEN_FILE", &token)
        .output()
        .unwrap();
    std::fs::remove_file(token).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

#[test]
fn mpris_players_appear_and_take_commands() {
    if !enter("mpris_players_appear_and_take_commands") {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(scenario());
}

#[test]
fn mpris_hung_player_does_not_block_other_sessions() {
    if !enter("mpris_hung_player_does_not_block_other_sessions") {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(hung_scenario());
}

#[test]
fn mpris_player_appears_after_a_failed_first_read() {
    if !enter("mpris_player_appears_after_a_failed_first_read") {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(late_scenario());
}
