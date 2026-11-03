// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Private-bus music deadline and replacement regression tests.
use super::*;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command as ProcessCommand, Stdio},
};
struct Bus(Child);
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
async fn connect(address: &str) -> Connection {
    zbus::connection::Builder::address(address.trim())
        .unwrap()
        .build()
        .await
        .unwrap()
}
fn provider(
    id: &str,
    capability: &str,
    owner: &str,
    generation: &str,
    path: OwnedObjectPath,
) -> Provider {
    Provider {
        info: ProviderInfo {
            id: id.into(),
            plugin_id: id.into(),
            source_path: String::new(),
            name: "Fixture".into(),
            version: "0.0.1".into(),
            path,
            capabilities: vec![capability.into()],
            runtime_state: "ready".into(),
            backend_state: "available".into(),
            generation: generation.into(),
            revision: 0,
            last_error: String::new(),
            error_at: 0,
            recovered_at: 0,
            restarts_since_daemon_start: 0,
            cumulative_restart_count: 0,
        },
        manifest: None,
        unit: String::new(),
        token_file: PathBuf::new(),
        token: String::new(),
        owner: owner.into(),
        cache: None,
        applications: vec![],
        running: vec![],
        tab_cache: None,
        windows: vec![],
        tabs: vec![],
        session_cache: None,
        sessions: vec![],
        features: vec![],
        library_cache: None,
        collections: vec![],
        items: vec![],
        resume_cache: None,
        resumes: vec![],
        settings_schema: None,
        started: Instant::now(),
        enabled: true,
    }
}
struct Fake {
    serial: Mutex<Option<String>>,
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
    starts: Arc<std::sync::atomic::AtomicUsize>,
}
#[zbus::interface(name = "org.arinn.MusicProvider1")]
impl Fake {
    async fn command(
        &self,
        _generation: &str,
        _client: &str,
        command: &str,
    ) -> production::Result<String> {
        let command: Command = music::decode(command)?;
        if command.action == "scan" {
            return Err(ErrorCode::Timeout.into());
        }
        let mut receipt = self.serial.lock().await;
        if receipt.as_ref() != Some(&command.request_id) {
            self.starts
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.entered.notify_one();
            self.release.notified().await;
            *receipt = Some(command.request_id);
        }
        Ok(music::encode(&MusicState {
            revision: 2,
            session: "late-session".into(),
            playback: "playing".into(),
            current: Some(music::QueueEntry {
                occurrence: "1".into(),
                track: music::Entity {
                    id: "track".into(),
                    kind: "track".into(),
                    title: "Fixture".into(),
                    ..music::Entity::default()
                },
            }),
            ..MusicState::default()
        })?)
    }
}
fn play() -> Command {
    Command {
        request_id: Uuid::new_v4().to_string(),
        action: "play".into(),
        session: String::new(),
        entity: "track".into(),
        occurrence: String::new(),
        position_us: 0,
        replaces_provider: String::new(),
        replaces_session: String::new(),
        text: String::new(),
    }
}
#[tokio::test]
async fn timed_out_start_is_settled_before_another_start_can_run() {
    let mut child = ProcessCommand::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut address = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let _bus = Bus(child);
    let server = connect(&address).await;
    let client = connect(&address).await;
    let remote = connect(&address).await;
    server.request_name(production::SERVICE).await.unwrap();
    let folder = std::env::temp_dir().join(format!("arinn-music-routing-{}", Uuid::new_v4()));
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
    let path = OwnedObjectPath::try_from("/org/arinn/Providers/MusicFixture").unwrap();
    core.state.lock().await.providers.insert(
        "fixture".into(),
        provider(
            "fixture",
            "music",
            remote.unique_name().unwrap().as_str(),
            "generation",
            path.clone(),
        ),
    );
    server
        .object_server()
        .at(
            path.clone(),
            MusicEndpoint {
                core: core.clone(),
                id: "fixture".into(),
            },
        )
        .await
        .unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let starts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    remote
        .object_server()
        .at(
            production::PROVIDER_PATH,
            Fake {
                serial: Mutex::new(None),
                entered: entered.clone(),
                release: release.clone(),
                starts: starts.clone(),
            },
        )
        .await
        .unwrap();
    let mut terminal_timeout = play();
    terminal_timeout.action = "scan".into();
    assert!(matches!(
        core.forward_music(
            "fixture",
            "generation",
            client.unique_name().unwrap().as_str(),
            &terminal_timeout,
            tokio::time::Instant::now() + Duration::from_secs(2)
        )
        .await,
        Err(ErrorCode::Timeout)
    ));
    assert!(core.music.registry.lock().await.pending.is_none());
    let command = play();
    assert!(matches!(
        core.forward_music(
            "fixture",
            "generation",
            client.unique_name().unwrap().as_str(),
            &command,
            tokio::time::Instant::now() + Duration::from_millis(100)
        )
        .await,
        Err(ErrorCode::Timeout)
    ));
    entered.notified().await;
    assert!(core.music.registry.lock().await.pending.is_some());
    let proxy = zbus::Proxy::new(
        &client,
        production::SERVICE,
        path.as_str(),
        "org.arinn.Music1",
    )
    .await
    .unwrap();
    let request = music::encode(&play()).unwrap();
    let (result, ()) = tokio::join!(
        async {
            tokio::time::timeout(
                Duration::from_secs(3),
                proxy.call::<_, _, String>("Command", &("generation", request.as_str())),
            )
            .await
            .unwrap()
        },
        async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 1);
            release.notify_one();
        }
    );
    assert!(
        matches!(result,Err(zbus::Error::MethodError(name,_,_)) if name.as_str()=="org.arinn.Error1.Conflict")
    );
    assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(core.music.registry.lock().await.pending.is_none());
    assert_eq!(
        core.active_music().await.unwrap().state.session,
        "late-session"
    );
    std::fs::remove_dir_all(folder).unwrap();
}
