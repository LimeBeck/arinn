// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Private-bus `serve()`: feature flags, and a restarted provider that
//! restores the session without sending `loadfile`.

use arinn_plugin_mpv::serve;
use arinn_protocol::{
    media::MediaProviderProxy,
    production::{self, Registration, RegistrationReply},
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader},
    net::UnixListener,
};
use zbus::{Connection, message::Header};

const SESSION_ID: &str = "sess-restored";
const LAUNCH_ID: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const LOCAL_ID: &str = "item-1";

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

fn enter(test: &str) -> bool {
    if std::env::var_os("ARINN_MPV_CHILD").is_some() {
        return true;
    }
    let token = std::env::temp_dir().join(format!("arinn-mpv-{}-{}", test, std::process::id()));
    std::fs::write(&token, "fixture-token").unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("ARINN_MPV_CHILD", "1")
        .env("ARINN_PROVIDER_ID", "mpv")
        .env("ARINN_PLUGIN_ID", "mpv")
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
fn serve_exports_features_and_a_restart_restores_the_session() {
    if !enter("serve_exports_features_and_a_restart_restores_the_session") {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(scenario());
}

async fn scenario() {
    let root = runtime_root();
    std::fs::create_dir_all(&root).unwrap();
    let _cleanup = RemoveOnDrop(root.clone());
    // SAFETY: this child process runs only this test. The directory is a
    // private runtime root, not `/tmp`, and it is set before `serve` reads it.
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", &root) };
    let dir = root.join("arinn/mpv");
    std::fs::create_dir_all(&dir).unwrap();
    let state = dir.join("session");
    let stored = json!({
        "id": SESSION_ID,
        "provider_id": "library",
        "capability": "library",
        "local_id": LOCAL_ID,
        "has_content": true,
    });
    std::fs::write(&state, serde_json::to_vec(&stored).unwrap()).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o600)).unwrap();
    let before = std::fs::read_to_string(&state).unwrap();

    let socket = dir.join("ipc");
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).unwrap();
    let commands = Arc::new(Mutex::new(Vec::new()));
    let peer = tokio::spawn(peer(listener, Arc::clone(&commands)));

    let bus = Bus::start();
    let daemon = bus.connection().await;
    daemon
        .object_server()
        .at(production::PATH, Daemon)
        .await
        .unwrap();
    daemon.request_name(production::SERVICE).await.unwrap();

    let first = spawn_provider(&bus).await;
    let proxy = provider_proxy(&daemon, &first.destination).await;
    let (page,) = wait_session(&proxy).await;
    let session = &page.sessions[0];
    assert_eq!(session.id, SESSION_ID);
    assert!(session.has_content);
    assert_eq!(session.content.provider_id, "library");
    assert_eq!(session.content.capability, "library");
    assert_eq!(session.content.local_id, LOCAL_ID);
    assert_eq!(session.state, "playing");
    assert_eq!(session.position_us, 5_000_000);
    assert_eq!(session.title, "Clip");
    assert_eq!(
        proxy.features().await.unwrap(),
        vec![
            "MediaOpen1",
            "MediaTracks1",
            "MediaTimeline1",
            // Seek thumbnails are fetched by asset id (#278).
            "AssetProvider1",
            "MediaWindow1"
        ]
    );
    let xml = introspect(&daemon, &first.destination).await;
    assert!(xml.contains("org.arinn.MediaProvider1"));
    assert!(xml.contains("org.arinn.MediaOpen1"));
    assert!(xml.contains("org.arinn.MediaTracks1"));
    assert!(xml.contains("org.arinn.MediaTimeline1"));
    assert!(xml.contains("org.arinn.MediaWindowProvider1"));
    assert!(xml.contains("org.arinn.AssetProvider1"));
    assert_mpris_controls_the_session(&daemon, &commands).await;
    assert_launch(&daemon, &first.destination).await;
    assert!(!names(&commands).iter().any(|name| name == "loadfile"));
    assert_eq!(std::fs::read_to_string(&state).unwrap(), before);
    assert!(!before.contains("Clip"), "{before}");

    first.runner.abort();
    let second = spawn_provider(&bus).await;
    let proxy = provider_proxy(&daemon, &second.destination).await;
    let (restored,) = wait_session(&proxy).await;
    let session = &restored.sessions[0];
    assert_eq!(session.id, SESSION_ID);
    assert_eq!(session.content.local_id, LOCAL_ID);
    assert_eq!(session.position_us, 5_000_000);
    assert_launch(&daemon, &second.destination).await;
    assert!(
        names(&commands)
            .iter()
            .filter(|name| *name == "observe_property")
            .count()
            >= 18
    );
    assert!(!names(&commands).iter().any(|name| name == "loadfile"));
    assert_eq!(std::fs::read_to_string(&state).unwrap(), before);

    second.runner.abort();
    peer.abort();
    drop(bus);
}

struct Provider {
    runner: tokio::task::JoinHandle<()>,
    destination: zbus::names::OwnedUniqueName,
}

async fn spawn_provider(bus: &Bus) -> Provider {
    let provider = bus.connection().await;
    let destination = provider.unique_name().unwrap().clone();
    let runner = tokio::spawn(async move {
        let _ = serve(provider).await;
    });
    Provider {
        runner,
        destination,
    }
}

async fn provider_proxy<'a>(
    daemon: &'a Connection,
    destination: &'a zbus::names::OwnedUniqueName,
) -> MediaProviderProxy<'a> {
    MediaProviderProxy::builder(daemon)
        .destination(destination)
        .unwrap()
        .build()
        .await
        .unwrap()
}

async fn introspect(daemon: &Connection, destination: &zbus::names::OwnedUniqueName) -> String {
    zbus::fdo::IntrospectableProxy::builder(daemon)
        .destination(destination)
        .unwrap()
        .path(production::PROVIDER_PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap()
}

async fn wait_session(proxy: &MediaProviderProxy<'_>) -> (arinn_protocol::media::SessionPage,) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(Ok(page)) =
            tokio::time::timeout(Duration::from_secs(2), proxy.get_sessions("")).await
            && page
                .0
                .sessions
                .first()
                .is_some_and(|session| session.id == SESSION_ID && session.position_us == 5_000_000)
        {
            return page;
        }
        if Instant::now() > deadline {
            panic!("restored session did not appear");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A private runtime root under Cargo's target tmp directory (`/tmp` is
/// refused by the provider). `CARGO_TARGET_TMPDIR` exists only at compile
/// time. The name is short so the `ipc` socket path fits in `sun_path`.
fn runtime_root() -> PathBuf {
    let id = uuid::Uuid::new_v4().simple().to_string();
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("mpv-{}", &id[..8]))
}

/// Removes the runtime root even when an assertion fails.
struct RemoveOnDrop(PathBuf);
impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_runtime_root_is_ignored_and_fits_a_socket_path() {
    let root = runtime_root();
    assert!(
        !root.starts_with(env!("CARGO_MANIFEST_DIR")),
        "{root:?} is inside the plugin directory, which Git and REUSE do not ignore"
    );
    // `sun_path` holds 108 bytes including the terminating NUL.
    let socket = root.join("arinn/mpv/ipc");
    assert!(socket.as_os_str().len() < 108, "{socket:?} is too long");
}

fn names(commands: &Arc<Mutex<Vec<Value>>>) -> Vec<String> {
    commands
        .lock()
        .unwrap()
        .iter()
        .filter_map(|message| {
            message
                .get("command")
                .and_then(|value| value.as_array())
                .and_then(|command| command.first())
                .and_then(|name| name.as_str())
                .map(str::to_string)
        })
        .collect()
}

async fn peer(listener: UnixListener, commands: Arc<Mutex<Vec<Value>>>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let commands = Arc::clone(&commands);
        tokio::spawn(async move {
            let _ = speak(stream, commands).await;
        });
    }
}

async fn speak(
    stream: tokio::net::UnixStream,
    commands: Arc<Mutex<Vec<Value>>>,
) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut lines = AsyncBufReader::new(read).lines();
    while let Some(line) = lines.next_line().await? {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        commands.lock().unwrap().push(message.clone());
        let id = message.get("request_id").cloned().unwrap_or(json!(0));
        let command = message
            .get("command")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let name = command
            .first()
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let reply_data = if name == "get_property"
            && command.get(1).and_then(Value::as_str) == Some("options/wayland-app-id")
        {
            json!(format!("org.arinn.Playback.{LAUNCH_ID}"))
        } else {
            Value::Null
        };
        send(
            &mut write,
            &json!({"request_id": id, "error": "success", "data": reply_data}),
        )
        .await?;
        if name != "observe_property" {
            continue;
        }
        let property = command
            .get(2)
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let observe_id = command.get(1).cloned().unwrap_or(json!(0));
        send(
            &mut write,
            &json!({"event": "property-change", "id": observe_id, "name": property, "data": data(property)}),
        )
        .await?;
        if property == "media-title" {
            send(
                &mut write,
                &json!({"event": "property-change", "id": 2, "name": "time-pos", "data": 5}),
            )
            .await?;
        }
    }
    Ok(())
}

fn data(property: &str) -> Value {
    match property {
        "pause" | "eof-reached" | "idle-active" => json!(false),
        "time-pos" => json!(5),
        "duration" => json!(120),
        "track-list" | "chapter-list" => json!([]),
        "demuxer-cache-state" => json!({"seekable-ranges": []}),
        "media-title" => json!("Clip"),
        _ => Value::Null,
    }
}

async fn send(write: &mut tokio::net::unix::OwnedWriteHalf, value: &Value) -> std::io::Result<()> {
    let mut line = serde_json::to_string(value).unwrap();
    line.push('\n');
    write.write_all(line.as_bytes()).await?;
    write.flush().await
}

async fn assert_launch(daemon: &Connection, destination: &zbus::names::OwnedUniqueName) {
    let proxy = arinn_protocol::media_window::MediaWindowProviderProxy::builder(daemon)
        .destination(destination)
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        proxy
            .get_launch_id("00000000-0000-4000-8000-000000000002", SESSION_ID)
            .await
            .unwrap(),
        LAUNCH_ID
    );
    assert!(
        proxy
            .get_launch_id("00000000-0000-4000-8000-000000000003", SESSION_ID)
            .await
            .is_err()
    );
}

/// ADR-0081 (#300): while a session exists the provider is an MPRIS player,
/// so Plasma's media keys reach it; PlayPause pauses the playing session.
async fn assert_mpris_controls_the_session(daemon: &Connection, commands: &Arc<Mutex<Vec<Value>>>) {
    let dbus = zbus::fdo::DBusProxy::new(daemon).await.unwrap();
    let mut name = None;
    for _ in 0..100 {
        name = dbus
            .list_names()
            .await
            .unwrap()
            .into_iter()
            .map(|n| n.to_string())
            .find(|n| n.starts_with("org.mpris.MediaPlayer2.arinn_mpv.instance"));
        if name.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let name = name.expect("the provider owns its MPRIS name while a session exists");
    let properties = zbus::fdo::PropertiesProxy::builder(daemon)
        .destination(name.clone())
        .unwrap()
        .path("/org/mpris/MediaPlayer2")
        .unwrap()
        .build()
        .await
        .unwrap();
    let interface = zbus::names::InterfaceName::try_from("org.mpris.MediaPlayer2.Player").unwrap();
    let status: String = properties
        .get(interface.clone(), "PlaybackStatus")
        .await
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(status, "Playing");
    let identity: String = properties
        .get(
            zbus::names::InterfaceName::try_from("org.mpris.MediaPlayer2").unwrap(),
            "DesktopEntry",
        )
        .await
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(identity, "org.arinn.Shell");
    let player = zbus::Proxy::new(
        daemon,
        name.clone(),
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
    )
    .await
    .unwrap();
    // A seek is announced with Seeked (review of #326).
    use futures_util::StreamExt;
    let mut seeked = player.receive_signal("Seeked").await.unwrap();
    let _: () = player.call("Seek", &(1_000_000i64,)).await.unwrap();
    let signal = tokio::time::timeout(Duration::from_secs(5), seeked.next())
        .await
        .expect("Seeked after Seek")
        .unwrap();
    let (position,): (i64,) = signal.body().deserialize().unwrap();
    assert!(position > 0, "{position}");
    assert!(names(commands).iter().any(|name| name == "seek"));
    let _: () = player.call("PlayPause", &()).await.unwrap();
    let paused = commands
        .lock()
        .unwrap()
        .iter()
        .any(|message| message.get("command") == Some(&json!(["set_property", "pause", true])));
    assert!(paused, "PlayPause did not pause the playing session");
}
