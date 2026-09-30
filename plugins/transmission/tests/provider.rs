// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Private-bus provider against a fake Transmission RPC. No real daemon.

use arinn_plugin_transmission::{Config, Intervals, run, schema};
use arinn_protocol::{
    download::{self, DownloadProviderProxy, Job},
    production::{self, Registration, RegistrationReply},
    settings::{self, ProviderSettingsProxy},
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Mutex,
};
use zbus::{
    Connection,
    message::Header,
    zvariant::{OwnedObjectPath, OwnedValue, Type},
};

const GENERATION: &str = "00000000-0000-4000-8000-000000000002";
const HASH: &str = "0123456789abcdef0123456789abcdef01234567";

fn enter() -> bool {
    if std::env::var_os("ARINN_TRANSMISSION_CHILD").is_some() {
        return true;
    }
    let token = std::env::temp_dir().join(format!("arinn-transmission-{}", std::process::id()));
    std::fs::write(&token, "fixture-token").unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "provider_against_fake_rpc", "--nocapture"])
        .env("ARINN_TRANSMISSION_CHILD", "1")
        .env("ARINN_PROVIDER_ID", "transmission")
        .env("ARINN_PLUGIN_ID", "transmission")
        .env("ARINN_PLUGIN_VERSION", "0.0.1")
        .env("ARINN_TOKEN_FILE", &token)
        .output()
        .unwrap();
    std::fs::remove_file(&token).ok();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        stderr
    );
    assert!(stderr.contains("\"rpc_version\":17"), "{stderr}");
    assert!(!stderr.contains("fixture-session"), "{stderr}");
    assert!(!stderr.contains("fixture:secret"), "{stderr}");
    assert!(!stderr.contains("fixture:wrong"), "{stderr}");
    assert!(!stderr.contains("http://"), "{stderr}");
    assert!(!stderr.contains("/srv/"), "{stderr}");
    false
}

#[test]
fn provider_against_fake_rpc() {
    if !enter() {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(scenario());
}

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

struct Daemon;
#[zbus::interface(name = "org.arinn.Daemon1")]
impl Daemon {
    async fn register(
        &self,
        request: Registration,
        #[zbus(header)] _header: Header<'_>,
    ) -> production::Result<(RegistrationReply,)> {
        assert_eq!(request.capabilities, vec!["download".to_string()]);
        Ok((RegistrationReply {
            epoch: "00000000-0000-4000-8000-000000000001".into(),
            generation: GENERATION.into(),
            api_minor: 0,
            max_body_bytes: 262144,
            max_page_entries: 200,
            max_operations: 32,
        },))
    }
}

#[derive(serde::Serialize, Type)]
struct Secret(OwnedObjectPath, Vec<u8>, Vec<u8>, String);

struct Secrets {
    payload: Arc<std::sync::Mutex<Vec<u8>>>,
}
#[zbus::interface(name = "org.freedesktop.Secret.Service")]
impl Secrets {
    fn open_session(&self, _algorithm: &str, _input: OwnedValue) -> (OwnedValue, OwnedObjectPath) {
        (
            OwnedValue::from(zbus::zvariant::Str::from("")),
            OwnedObjectPath::try_from("/org/freedesktop/secrets/session/fixture").unwrap(),
        )
    }

    fn search_items(
        &self,
        attributes: std::collections::HashMap<String, String>,
    ) -> (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) {
        assert_eq!(
            attributes.get("arinn").map(String::as_str),
            Some("transmission")
        );
        assert_eq!(
            attributes.get("credential").map(String::as_str),
            Some("home")
        );
        (
            vec![OwnedObjectPath::try_from("/org/freedesktop/secrets/collection/login/0").unwrap()],
            Vec::new(),
        )
    }

    fn get_secrets(
        &self,
        _objects: Vec<OwnedObjectPath>,
        session: OwnedObjectPath,
    ) -> std::collections::HashMap<OwnedObjectPath, Secret> {
        let mut secrets = std::collections::HashMap::new();
        secrets.insert(
            OwnedObjectPath::try_from("/org/freedesktop/secrets/collection/login/0").unwrap(),
            Secret(
                session,
                Vec::new(),
                self.payload.lock().unwrap().clone(),
                "text/plain".into(),
            ),
        );
        secrets
    }
}

struct Fake {
    torrents: Mutex<Vec<Value>>,
    methods: Mutex<Vec<String>>,
    auth_failures: AtomicUsize,
    challenges: AtomicUsize,
    hangs: AtomicUsize,
    hang_stop: AtomicBool,
    refuse: AtomicBool,
}

impl Fake {
    fn torrent() -> Value {
        json!({
            "hashString": HASH,
            "name": "Film",
            "status": 4,
            "error": 0,
            "errorString": "secret-path",
            "peers": 4,
            "ratio": 1.2,
            "trackers": [{"announce": "http://tracker.example/announce"}],
            "sizeWhenDone": 100,
            "leftUntilDone": 40,
            "rateDownload": 10,
            "eta": 8,
            "downloadDir": "/srv/downloads",
            "files": [
                {"name": "Film/movie.mkv", "length": 80, "bytesCompleted": 40},
                {"name": "Film/note.txt", "length": 20, "bytesCompleted": 0}
            ]
        })
    }
}

async fn serve_rpc(fake: Arc<Fake>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            if fake.refuse.load(Ordering::Acquire) {
                continue;
            }
            let fake = Arc::clone(&fake);
            tokio::spawn(async move {
                let Some(text) = read_http(&mut socket).await else {
                    return;
                };
                if fake.hang_stop.load(Ordering::Acquire) && text.contains("torrent-stop") {
                    fake.hangs.fetch_add(1, Ordering::AcqRel);
                    std::future::pending::<()>().await;
                }
                if !text.contains("Authorization: Basic Zml4dHVyZTpzZWNyZXQ=") {
                    fake.auth_failures.fetch_add(1, Ordering::AcqRel);
                    let _ = socket
                        .write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                    return;
                }
                if !text
                    .to_ascii_lowercase()
                    .contains("x-transmission-session-id: fixture-session")
                {
                    fake.challenges.fetch_add(1, Ordering::AcqRel);
                    let _ = socket
                        .write_all(b"HTTP/1.1 409 Conflict\r\nX-Transmission-Session-Id: fixture-session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                    return;
                }
                let method = text
                    .split("\"method\":\"")
                    .nth(1)
                    .and_then(|rest| rest.split('"').next())
                    .unwrap_or("unknown")
                    .to_string();
                assert_ne!(method, "torrent-add");
                if method == "torrent-get" {
                    assert!(!text.contains("\"peers\""));
                    assert!(!text.contains("trackers"));
                    assert!(!text.contains("ratio"));
                }
                fake.methods.lock().await.push(method.clone());
                let body = if method == "session-get" {
                    br#"{"result":"success","arguments":{"rpc-version":17}}"#.to_vec()
                } else {
                    let torrents = fake.torrents.lock().await.clone();
                    serde_json::to_vec(&json!({
                        "result": "success",
                        "arguments": {"torrents": torrents}
                    }))
                    .unwrap()
                };
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(&body).await;
            });
        }
    });
    port
}

async fn read_http(socket: &mut tokio::net::TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    let header_end = loop {
        let n = socket.read(&mut tmp).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(end) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
        if buf.len() > 64 * 1024 {
            return None;
        }
    };
    let headers = std::str::from_utf8(&buf[..header_end]).ok()?.to_string();
    let length = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    })?;
    while buf.len() < header_end + length {
        let n = socket.read(&mut tmp).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    String::from_utf8(buf).ok()
}

fn error_name(result: zbus::Result<impl std::fmt::Debug>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => panic!("expected an Arinn error, got {other:?}"),
    }
}

async fn wait_until(mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if pred() {
            return;
        }
        if Instant::now() > deadline {
            panic!("timed out");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_job(proxy: &DownloadProviderProxy<'_>, state: &str) -> Job {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok((page,)) = proxy.get_jobs("").await
            && let Some(job) = page.jobs.into_iter().find(|job| job.state == state)
        {
            return job;
        }
        if Instant::now() > deadline {
            panic!("timed out waiting for {state}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
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
    let payload = Arc::new(std::sync::Mutex::new(b"fixture:wrong".to_vec()));
    daemon
        .object_server()
        .at(
            "/org/freedesktop/secrets",
            Secrets {
                payload: Arc::clone(&payload),
            },
        )
        .await
        .unwrap();
    daemon
        .request_name("org.freedesktop.secrets")
        .await
        .unwrap();

    let fake = Arc::new(Fake {
        torrents: Mutex::new(vec![Fake::torrent()]),
        methods: Mutex::new(Vec::new()),
        auth_failures: AtomicUsize::new(0),
        challenges: AtomicUsize::new(0),
        hangs: AtomicUsize::new(0),
        hang_stop: AtomicBool::new(false),
        refuse: AtomicBool::new(false),
    });
    let port = serve_rpc(Arc::clone(&fake)).await;
    let endpoint = format!("http://127.0.0.1:{port}/transmission/rpc");
    let values = settings::effective_values(
        &schema(),
        json!({"endpoint": endpoint, "credential_ref": "home"})
            .as_object()
            .unwrap(),
    );
    let config = Config::from_values(&values).unwrap();
    let provider = bus.connection().await;
    let provider_name = provider.unique_name().unwrap().clone();
    let _running = tokio::spawn(run(provider, config, Intervals::fast()));

    let proxy = DownloadProviderProxy::builder(&daemon)
        .destination(provider_name.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    wait_until(|| fake.auth_failures.load(Ordering::Acquire) >= 1).await;
    *payload.lock().unwrap() = b"fixture:secret".to_vec();

    let job = wait_job(&proxy, download::STATE_DOWNLOADING).await;
    assert_eq!(job.actions, vec!["pause".to_string()]);
    assert!(job.outputs.is_empty());

    fake.hang_stop.store(true, Ordering::Release);
    assert_eq!(
        error_name(proxy.pause(GENERATION, HASH).await),
        "org.arinn.Error1.Timeout"
    );
    assert_eq!(fake.hangs.load(Ordering::Acquire), 1);
    fake.hang_stop.store(false, Ordering::Release);
    assert!(fake.challenges.load(Ordering::Acquire) >= 1);
    let rendered = format!("{job:?}");
    assert!(!rendered.contains("secret-path"));
    assert!(!rendered.contains("tracker.example"));

    {
        let mut torrents = fake.torrents.lock().await;
        torrents[0]["status"] = json!(3);
    }
    let job = wait_job(&proxy, download::STATE_QUEUED).await;
    assert_eq!(
        job.actions,
        vec!["pause".to_string(), "start-now".to_string()]
    );
    proxy.start_now(GENERATION, HASH).await.unwrap();
    wait_until(|| {
        fake.methods
            .try_lock()
            .is_ok_and(|methods| methods.iter().any(|method| method == "torrent-start-now"))
    })
    .await;

    {
        let mut torrents = fake.torrents.lock().await;
        torrents[0]["status"] = json!(0);
        torrents[0]["error"] = json!(3);
    }
    let job = wait_job(&proxy, download::STATE_ERROR).await;
    assert_eq!(job.actions, vec!["retry".to_string()]);
    assert_eq!(job.error_code, "Internal");
    assert!(!job.error_message.contains("secret-path"));
    proxy.retry(GENERATION, HASH).await.unwrap();
    wait_until(|| {
        fake.methods.try_lock().is_ok_and(|methods| {
            let verify = methods.iter().position(|method| method == "torrent-verify");
            let start = methods.iter().rposition(|method| method == "torrent-start");
            verify.is_some_and(|verify| start.is_some_and(|start| verify < start))
        })
    })
    .await;
    assert!(
        !fake
            .methods
            .lock()
            .await
            .iter()
            .any(|method| method == "torrent-add")
    );

    {
        let mut torrents = fake.torrents.lock().await;
        torrents[0]["status"] = json!(6);
        torrents[0]["error"] = json!(0);
        torrents[0]["leftUntilDone"] = json!(0);
        torrents[0]["files"][0]["bytesCompleted"] = json!(80);
        torrents[0]["files"][1]["bytesCompleted"] = json!(20);
    }
    let job = wait_job(&proxy, download::STATE_COMPLETED).await;
    assert!(job.actions.is_empty(), "{:?}", job.actions);
    assert_eq!(job.eta_seconds, -1);
    assert_eq!(job.outputs.len(), 2);
    assert!(job.outputs.iter().all(|output| output.file_uri.is_empty()));
    let output_ids: Vec<_> = job.outputs.iter().map(|output| output.id.clone()).collect();

    let settings = ProviderSettingsProxy::builder(&daemon)
        .destination(provider_name)
        .unwrap()
        .build()
        .await
        .unwrap();
    settings
        .apply(
            GENERATION,
            &json!({
                "endpoint": format!("http://127.0.0.1:{port}/transmission/rpc"),
                "credential_ref": "home",
                "path_map": "/srv/downloads>/mnt/media"
            })
            .to_string(),
        )
        .await
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mapped = loop {
        let job = proxy.get_job(GENERATION, HASH).await.unwrap().0;
        if job.outputs.iter().any(|output| !output.file_uri.is_empty()) {
            break job;
        }
        if Instant::now() > deadline {
            panic!("timed out waiting for a mapped file");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(
        mapped
            .outputs
            .iter()
            .map(|output| output.id.as_str())
            .collect::<Vec<_>>(),
        output_ids.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(
        mapped.outputs[0].file_uri,
        "file:///mnt/media/Film/movie.mkv"
    );
    assert_eq!(mapped.outputs[1].title, "note.txt");

    fake.refuse.store(true, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if proxy.backend_state().await.ok().as_deref() == Some("unavailable") {
            break;
        }
        if Instant::now() > deadline {
            panic!("backend stayed available");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let cached = proxy.get_job(GENERATION, HASH).await.unwrap().0;
    assert_eq!(cached.state, download::STATE_COMPLETED);
    assert_eq!(
        error_name(proxy.pause(GENERATION, HASH).await),
        "org.arinn.Error1.Unavailable"
    );
}
