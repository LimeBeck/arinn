// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! AC-10 for the daemon: killing the Chrome provider process does not take
//! `arinnd` down. The browser provider becomes unavailable.
//!
//! Crash/restart of the demo plugin is `tests/integration/phase1.py`. The
//! native host's reaction to a lost bridge is `plugins/chrome/tests/chain.rs`.
//! Neither kills `arinn-plugin-chrome` out from under the daemon.

use arinn_protocol::{
    browser::BrowserApiProxy,
    production::{PATH, ProductionDaemonProxy, ProviderInfo, SERVICE},
};
use arinnd::{
    runtime::{Core, Daemon},
    storage::Paths,
};
use std::{
    fs,
    io::{BufRead, BufReader},
    os::unix::process::ExitStatusExt,
    panic::PanicHookInfo,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use zbus::{
    Connection,
    zvariant::{OwnedObjectPath, OwnedValue},
};

const SIGKILL: i32 = 9;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("arinn-chrome-crash-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(path.join("data/arinn/plugins")).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Bus {
    child: Child,
    address: String,
}
impl Bus {
    fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self {
            child,
            address: address.trim().to_string(),
        }
    }
    async fn connection(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Unit {
    name: String,
    child: Option<Child>,
}
struct Manager {
    address: String,
    executable: String,
    stderr: PathBuf,
    unit: Arc<Mutex<Option<Unit>>>,
    spawn_error: Arc<Mutex<String>>,
}
fn unit_path(name: &str) -> OwnedObjectPath {
    format!(
        "/org/freedesktop/systemd1/unit/u_{}",
        name.bytes().map(|b| format!("{b:02x}")).collect::<String>()
    )
    .try_into()
    .unwrap()
}
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    async fn get_unit(&self, name: &str) -> zbus::fdo::Result<OwnedObjectPath> {
        let unit = self.unit.lock().unwrap();
        if unit.as_ref().is_some_and(|unit| unit.name == name) {
            Ok(unit_path(name))
        } else {
            Err(zbus::fdo::Error::UnknownObject("missing".into()))
        }
    }
    #[zbus(name = "GetUnitByPID")]
    async fn get_unit_by_pid(&self, pid: u32) -> zbus::fdo::Result<OwnedObjectPath> {
        let unit = self.unit.lock().unwrap();
        let Some(unit) = unit.as_ref() else {
            return Err(zbus::fdo::Error::AccessDenied("not in unit".into()));
        };
        if unit.child.as_ref().is_some_and(|child| child.id() == pid) {
            Ok(unit_path(&unit.name))
        } else {
            Err(zbus::fdo::Error::AccessDenied("not in unit".into()))
        }
    }
    async fn start_transient_unit(
        &self,
        name: &str,
        _mode: &str,
        properties: Vec<(String, OwnedValue)>,
        _auxiliary: Vec<(String, Vec<(String, OwnedValue)>)>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let mut properties: std::collections::HashMap<_, _> = properties.into_iter().collect();
        let commands: Vec<(String, Vec<String>, bool)> = properties
            .remove("ExecStart")
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("ExecStart".into()))?
            .try_into()
            .map_err(|_| zbus::fdo::Error::InvalidArgs("ExecStart".into()))?;
        let env: Vec<String> = properties
            .remove("Environment")
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("Environment".into()))?
            .try_into()
            .map_err(|_| zbus::fdo::Error::InvalidArgs("Environment".into()))?;
        let Some((command, args, _)) = commands.into_iter().next() else {
            return Err(zbus::fdo::Error::InvalidArgs("ExecStart".into()));
        };
        if command != self.executable {
            *self.spawn_error.lock().unwrap() = "unexpected executable".into();
            return Err(zbus::fdo::Error::Failed("unexpected executable".into()));
        }
        let mut child = Command::new(&command);
        child
            .args(&args[1..])
            .env("DBUS_SESSION_BUS_ADDRESS", &self.address)
            .stdout(Stdio::null())
            .stderr(Stdio::from(fs::File::create(&self.stderr).unwrap()));
        for entry in &env {
            if let Some((key, value)) = entry.split_once('=') {
                child.env(key, value);
            }
        }
        match child.spawn() {
            Ok(child) => {
                *self.unit.lock().unwrap() = Some(Unit {
                    name: name.to_string(),
                    child: Some(child),
                });
                Ok(unit_path(name))
            }
            Err(error) => {
                *self.spawn_error.lock().unwrap() = error.to_string();
                Err(zbus::fdo::Error::Failed("spawn".into()))
            }
        }
    }
}

struct Reap(Option<Child>);
impl Drop for Reap {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Reap {
    fn id(&self) -> u32 {
        self.0.as_ref().unwrap().id()
    }
    fn exited(&mut self) -> Option<std::process::ExitStatus> {
        let child = self.0.as_mut().unwrap();
        let status = child.try_wait().unwrap()?;
        self.0.take();
        Some(status)
    }
}

fn install_panic_flag() -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let recorded = Arc::clone(&flag);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info: &PanicHookInfo<'_>| {
        recorded.store(true, Ordering::SeqCst);
        previous(info);
    }));
    flag
}

fn discovery_env(temp: &Temp) {
    let home = temp.0.to_str().unwrap();
    let data = temp.0.join("data");
    let system = temp.0.join("system");
    // SAFETY: this binary contains only this test. The variables name an empty
    // temp directory so discovery cannot read the user's plugin manifests, and
    // they are set before the daemon reads them.
    unsafe {
        std::env::set_var("HOME", home);
        std::env::set_var("XDG_DATA_HOME", &data);
        std::env::set_var("XDG_DATA_DIRS", &system);
    }
}

fn write_manifest(temp: &Temp) {
    let executable = env!("CARGO_BIN_EXE_arinn-plugin-chrome");
    let manifest = temp.0.join("data/arinn/plugins/chrome.toml");
    fs::write(
        manifest,
        format!(
            "id = \"chrome\"\nname = \"Chrome Integration\"\nversion = \"0.0.1\"\napi_version = 1\nexecutable = \"{executable}\"\ncapabilities = [\"browser\"]\n"
        ),
    )
    .unwrap();
}

async fn chrome(daemon: &ProductionDaemonProxy<'_>) -> ProviderInfo {
    let (page,) = daemon.provider_page("").await.expect("daemon providers");
    assert_eq!(page.providers.len(), 1, "discovery loaded another provider");
    page.providers.into_iter().next().unwrap()
}

async fn wait_available(
    daemon: &ProductionDaemonProxy<'_>,
    panicked: &AtomicBool,
    want: bool,
    context: &str,
    stderr: &PathBuf,
) -> String {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        assert!(
            !panicked.load(Ordering::SeqCst),
            "daemon panicked while {context}"
        );
        let status = daemon
            .status()
            .await
            .unwrap_or_else(|error| panic!("daemon stopped answering while {context}: {error}"));
        if status.0.provider_available == want {
            return status.0.epoch;
        }
        if Instant::now() >= deadline {
            let note = fs::read_to_string(stderr).unwrap_or_default();
            let diagnostics = daemon.diagnostics().await.unwrap_or_default();
            panic!(
                "timed out waiting for provider_available={want} while {context}; diagnostics={diagnostics}; provider stderr={note}"
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn chrome_provider_crash_does_not_crash_the_daemon() {
    let panicked = install_panic_flag();
    let temp = Temp::new();
    discovery_env(&temp);
    write_manifest(&temp);
    let bus = Bus::start();
    let unit = Arc::new(Mutex::new(None));
    let spawn_error = Arc::new(Mutex::new(String::new()));
    let stderr = temp.0.join("provider.stderr");
    let _systemd = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name("org.freedesktop.systemd1")
        .unwrap()
        .serve_at(
            "/org/freedesktop/systemd1",
            Manager {
                address: bus.address.clone(),
                executable: env!("CARGO_BIN_EXE_arinn-plugin-chrome").to_string(),
                stderr: stderr.clone(),
                unit: Arc::clone(&unit),
                spawn_error: Arc::clone(&spawn_error),
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let server = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name(SERVICE)
        .unwrap()
        .build()
        .await
        .unwrap();
    let core = Core::new(
        &server,
        Paths {
            config: temp.0.join("config"),
            data: temp.0.join("data"),
            state: temp.0.join("state"),
        },
        temp.0.join("run"),
    );
    server
        .object_server()
        .at(PATH, Daemon { core: core.clone() })
        .await
        .unwrap();
    core.export_platform().await.unwrap();
    core.start().await;

    let client = bus.connection().await;
    let daemon = ProductionDaemonProxy::new(&client).await.unwrap();
    let epoch = wait_available(
        &daemon,
        &panicked,
        true,
        "the provider is starting",
        &stderr,
    )
    .await;
    let ready = chrome(&daemon).await;
    assert_eq!(ready.id, "chrome");
    assert_eq!(ready.runtime_state, "ready");
    // No browser is connected. That is backend unavailability, not a crash.
    assert_eq!(ready.backend_state, "unavailable");
    assert!(!ready.generation.is_empty());
    let path = ready.path.clone();

    let spawn_note = spawn_error.lock().unwrap().clone();
    let mut provider = Reap(
        unit.lock()
            .unwrap()
            .as_mut()
            .unwrap_or_else(|| panic!("chrome provider was not started: {spawn_note}"))
            .child
            .take(),
    );
    let pid = provider.id();
    assert_ne!(pid, std::process::id());
    provider.0.as_mut().unwrap().kill().unwrap();
    let status = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(status) = provider.exited() {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("provider process did not exit");
    assert_eq!(status.signal(), Some(SIGKILL));

    let still = wait_available(
        &daemon,
        &panicked,
        false,
        "the provider is crashing",
        &stderr,
    )
    .await;
    assert_eq!(still, epoch, "daemon epoch changed");
    let crashed = chrome(&daemon).await;
    assert_eq!(crashed.runtime_state, "restarting");
    assert_eq!(crashed.backend_state, "unavailable");
    assert_eq!(crashed.last_error, "Unavailable");
    let browser = BrowserApiProxy::builder(&client)
        .path(path)
        .unwrap()
        .build()
        .await
        .unwrap();
    assert!(
        matches!(
            browser.get_tabs("").await,
            Err(zbus::Error::MethodError(name, _, _))
                if name.as_str() == "org.arinn.Error1.Unavailable"
        ),
        "browser section stayed available after the provider crash"
    );
    assert!(
        !panicked.load(Ordering::SeqCst),
        "daemon panicked after the provider crash"
    );
}
