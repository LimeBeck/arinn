// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider supervision through systemd when a user manager exists, otherwise in-process.
mod daemon;
mod systemd;

use crate::registry::Manifest;
use arinn_protocol::validation::ErrorCode;
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Arc,
};
use zbus::{Connection, zvariant::OwnedObjectPath};

/// How [`Supervisor::start`] treated a provider unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Attachment {
    /// The surviving unit still runs the manifest executable.
    Attached,
    /// No surviving unit; a new transient unit was created.
    Created,
    /// The surviving unit ran a replaced executable and was created again.
    Replaced,
}

/// Which runtime supervises provider processes for this daemon instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Systemd,
    Daemon,
}

/// Chosen once at daemon startup; a provider never uses both runtimes.
#[derive(Clone)]
pub struct Supervision {
    mode: Mode,
    systemd: systemd::Backend,
    daemon: Arc<daemon::Pool>,
}

impl Supervision {
    pub async fn bootstrap(connection: &Connection) -> Self {
        let systemd = systemd::Backend::new(connection);
        let daemon = Arc::new(daemon::Pool::new());
        let mode = if systemd::Backend::available(connection).await {
            Mode::Systemd
        } else if std::env::var_os("ARINN_PRIVATE_TEST_BUS").as_deref() == Some("1".as_ref()) {
            // phase1.py starts the test systemd double after arinnd; keep the
            // systemd adapter so the provider stays failed until the manager exists.
            Mode::Systemd
        } else {
            daemon.ensure_maintainer();
            Mode::Daemon
        };
        Self {
            mode,
            systemd,
            daemon,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub async fn shutdown(&self) {
        if self.mode == Mode::Daemon {
            self.daemon.shutdown_all().await;
        }
    }

    #[cfg(test)]
    pub(crate) async fn daemon_loaded(&self, unit: &str) -> bool {
        self.daemon.loaded(unit).await
    }
}

#[derive(Clone)]
pub struct Supervisor {
    connection: Connection,
    supervision: Arc<Supervision>,
}

impl Supervisor {
    pub fn new(connection: &Connection, supervision: Arc<Supervision>) -> Self {
        Self {
            connection: connection.clone(),
            supervision,
        }
    }

    pub async fn unit(&self, name: &str) -> Result<OwnedObjectPath, ErrorCode> {
        match self.supervision.mode {
            Mode::Systemd => self.supervision.systemd.unit(name).await,
            Mode::Daemon => {
                if self.supervision.daemon.loaded(name).await {
                    daemon_unit_path(name)
                } else {
                    Err(ErrorCode::NotFound)
                }
            }
        }
    }

    pub async fn state(&self, name: &str) -> Result<String, ErrorCode> {
        match self.supervision.mode {
            Mode::Systemd => self.supervision.systemd.state(name).await,
            Mode::Daemon => self.supervision.daemon.state(name).await,
        }
    }

    pub async fn restarts(&self, name: &str) -> Result<u32, ErrorCode> {
        match self.supervision.mode {
            Mode::Systemd => self.supervision.systemd.restarts(name).await,
            Mode::Daemon => self.supervision.daemon.restarts(name).await,
        }
    }

    pub async fn authenticate(&self, owner: &str, unit: &str) -> Result<(), ErrorCode> {
        match self.supervision.mode {
            Mode::Systemd => self.supervision.systemd.authenticate(owner, unit).await,
            Mode::Daemon => {
                self.supervision
                    .daemon
                    .authenticate(&self.connection, owner, unit)
                    .await
            }
        }
    }

    pub async fn executable_current(
        &self,
        name: &str,
        executable: &Path,
    ) -> Result<bool, ErrorCode> {
        match self.supervision.mode {
            Mode::Systemd => {
                self.supervision
                    .systemd
                    .executable_current(name, executable)
                    .await
            }
            Mode::Daemon => match self.supervision.daemon.state(name).await {
                Err(ErrorCode::NotFound) => Ok(true),
                Err(error) => Err(error),
                Ok(state) if matches!(state.as_str(), "active" | "activating" | "reloading") => {
                    self.supervision
                        .daemon
                        .process_matches(name, executable)
                        .await
                }
                Ok(_) => Ok(true),
            },
        }
    }

    pub async fn start(
        &self,
        name: &str,
        manifest: &Manifest,
        environment: Vec<String>,
    ) -> Result<Attachment, ErrorCode> {
        match self.supervision.mode {
            Mode::Systemd => {
                self.supervision
                    .systemd
                    .start(name, manifest, environment)
                    .await
            }
            Mode::Daemon => self.start_daemon(name, manifest, environment).await,
        }
    }

    async fn start_daemon(
        &self,
        name: &str,
        manifest: &Manifest,
        environment: Vec<String>,
    ) -> Result<Attachment, ErrorCode> {
        let pool = &self.supervision.daemon;
        let replace = match pool.state(name).await {
            Ok(state) if matches!(state.as_str(), "active" | "activating" | "reloading") => {
                let matches = pool.process_matches(name, &manifest.executable).await?;
                if matches {
                    return Ok(Attachment::Attached);
                }
                true
            }
            Ok(state) if state == "failed" => return Err(ErrorCode::Unavailable),
            Ok(_) | Err(ErrorCode::NotFound) => false,
            Err(error) => return Err(error),
        };
        if replace {
            self.discard(name).await?;
        }
        pool.start(name, manifest, environment).await?;
        Ok(if replace {
            Attachment::Replaced
        } else {
            Attachment::Created
        })
    }

    pub async fn stop(&self, name: &str) -> Result<(), ErrorCode> {
        match self.supervision.mode {
            Mode::Systemd => self.supervision.systemd.stop(name).await,
            Mode::Daemon => {
                self.supervision.daemon.stop(name).await?;
                if self.supervision.daemon.state(name).await.as_deref() != Ok("failed") {
                    let _ = self.supervision.daemon.wait_unloaded(name).await;
                }
                Ok(())
            }
        }
    }

    pub async fn discard(&self, name: &str) -> Result<(), ErrorCode> {
        match self.supervision.mode {
            Mode::Systemd => self.supervision.systemd.discard(name).await,
            Mode::Daemon => {
                self.supervision.daemon.stop(name).await?;
                if self.supervision.daemon.loaded(name).await {
                    self.supervision.daemon.reset_failed(name).await?;
                    self.supervision.daemon.unload(name).await?;
                    self.supervision.daemon.wait_unloaded(name).await?;
                }
                Ok(())
            }
        }
    }

    pub async fn reset_failed(&self, name: &str) -> Result<(), ErrorCode> {
        match self.supervision.mode {
            Mode::Systemd => self.supervision.systemd.reset_failed(name).await,
            Mode::Daemon => self.supervision.daemon.reset_failed(name).await,
        }
    }

    pub async fn fail_handshake(&self, name: &str) -> Result<(), ErrorCode> {
        match self.supervision.mode {
            Mode::Systemd => self.supervision.systemd.fail_handshake(name).await,
            Mode::Daemon => self.supervision.daemon.fail_handshake(name).await,
        }
    }
}

/// The running process still executes `executable` when `/proc/<pid>/exe`
/// is not marked deleted and names the same device and inode (spec §9.10, #243).
fn daemon_unit_path(name: &str) -> Result<OwnedObjectPath, ErrorCode> {
    let encoded = name
        .bytes()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("/org/arinn/Daemon1/supervisor/u_{encoded}")
        .try_into()
        .map_err(|_| ErrorCode::Internal)
}

pub(crate) fn running_executable_matches(pid: u32, executable: &Path) -> bool {
    let proc_exe = PathBuf::from(format!("/proc/{pid}/exe"));
    let Ok(link) = fs::read_link(&proc_exe) else {
        return false;
    };
    if link.to_string_lossy().ends_with(" (deleted)") {
        return false;
    }
    let (Ok(running), Ok(installed)) = (fs::metadata(&proc_exe), fs::metadata(executable)) else {
        return false;
    };
    running.dev() == installed.dev() && running.ino() == installed.ino()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{BufRead, BufReader},
        os::unix::fs::{MetadataExt, PermissionsExt},
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    use uuid::Uuid;

    fn describe(pid: u32, executable: &Path) -> String {
        let proc_exe = format!("/proc/{pid}/exe");
        let id = |path: &Path| {
            fs::metadata(path)
                .map(|m| format!("dev {} ino {}", m.dev(), m.ino()))
                .unwrap_or_else(|error| error.to_string())
        };
        format!(
            "link {:?}, running {}, installed {}",
            fs::read_link(&proc_exe),
            id(Path::new(&proc_exe)),
            id(executable)
        )
    }

    fn eventually(mut condition: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if condition() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn replaced_executable_does_not_match_the_running_process() {
        let dir = std::env::temp_dir().join(format!("arinn-stale-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sleep");
        fs::copy("/bin/sleep", &path).unwrap();
        let executable = |path: &std::path::Path| {
            let mut permissions = fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).unwrap();
        };
        executable(&path);
        let child = Command::new(&path)
            .arg("30")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        struct Kill(std::process::Child);
        impl Drop for Kill {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let child = Kill(child);
        let pid = child.0.id();
        assert!(
            eventually(|| running_executable_matches(pid, &path)),
            "the process must match the executable it was started from: {}",
            describe(pid, &path)
        );
        fs::remove_file(&path).unwrap();
        fs::copy("/bin/sleep", &path).unwrap();
        executable(&path);
        assert!(
            !running_executable_matches(pid, &path),
            "a replaced executable must not match the process still mapped to the old inode: {}",
            describe(pid, &path)
        );
        drop(child);
        let _ = fs::remove_dir_all(&dir);
    }

    struct SessionBus(std::process::Child, String, PathBuf);
    impl Drop for SessionBus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
            let _ = fs::remove_dir_all(&self.2);
        }
    }

    async fn session_bus() -> SessionBus {
        let folder = std::env::temp_dir().join(format!("arinn-supervisor-{}", Uuid::new_v4()));
        fs::create_dir(&folder).unwrap();
        let mut bus = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(bus.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        SessionBus(bus, address.trim().to_string(), folder)
    }

    async fn session_connection(address: &str) -> Connection {
        zbus::connection::Builder::address(address)
            .unwrap()
            .build()
            .await
            .unwrap()
    }

    fn manifest(executable: PathBuf, args: Vec<String>) -> Manifest {
        Manifest {
            id: "fixture".into(),
            name: "Fixture".into(),
            version: "0.0.1".into(),
            api_version: 1,
            executable,
            args,
            capabilities: vec!["application".into()],
        }
    }

    #[tokio::test]
    async fn daemon_supervisor_off_without_user_manager_on() {
        let bus = session_bus().await;
        let connection = session_connection(&bus.1).await;
        let supervision = Supervision::bootstrap(&connection).await;
        assert_eq!(supervision.mode(), Mode::Daemon);
    }

    fn private_systemd_bin() -> Option<PathBuf> {
        std::env::var_os("CARGO_BIN_EXE_private-systemd")
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| {
                let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../target/debug/examples/private-systemd");
                path.is_file().then_some(path)
            })
    }

    #[tokio::test]
    async fn daemon_supervisor_off_when_user_manager_exists() {
        let Some(manager_bin) = private_systemd_bin() else {
            return;
        };
        let bus = session_bus().await;
        let mut manager = Command::new(manager_bin)
            .env("ARINN_PRIVATE_TEST_BUS", "1")
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.1)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("private-systemd");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let connection = session_connection(&bus.1).await;
        let supervision = Supervision::bootstrap(&connection).await;
        assert_eq!(supervision.mode(), Mode::Systemd);
        let _ = manager.kill();
        let _ = manager.wait();
    }

    #[tokio::test]
    async fn daemon_start_uses_absolute_executable_and_argv_without_shell() {
        let bus = session_bus().await;
        let connection = session_connection(&bus.1).await;
        let supervision = Arc::new(Supervision::bootstrap(&connection).await);
        assert_eq!(supervision.mode(), Mode::Daemon);
        let dir = bus.2.join("unit-env");
        fs::create_dir(&dir).unwrap();
        unsafe {
            std::env::set_var("ARINN_TEST_UNIT_ENV_DIR", &dir);
        }
        let supervisor = Supervisor::new(&connection, supervision);
        let unit = "arinn-provider-test.service";
        let executable = PathBuf::from("/bin/sleep");
        let args = vec!["120".into()];
        supervisor
            .start(
                unit,
                &manifest(executable.clone(), args.clone()),
                vec!["ARINN_PROVIDER_ID=test".into()],
            )
            .await
            .unwrap();
        let argv = fs::read_to_string(dir.join("sleep.argv")).unwrap();
        assert_eq!(argv, "/bin/sleep\n120");
        assert!(
            fs::read_to_string(dir.join("sleep.env"))
                .unwrap()
                .contains("ARINN_PROVIDER_ID=test")
        );
        supervisor.stop(unit).await.unwrap();
        unsafe {
            std::env::remove_var("ARINN_TEST_UNIT_ENV_DIR");
        }
    }

    #[tokio::test]
    async fn daemon_restart_budget_then_failed_until_manual_restart() {
        let bus = session_bus().await;
        let connection = session_connection(&bus.1).await;
        let supervision = Arc::new(Supervision::bootstrap(&connection).await);
        let supervisor = Supervisor::new(&connection, supervision);
        let unit = "arinn-provider-fail.service";
        supervisor
            .start(
                unit,
                &manifest(PathBuf::from("/bin/false"), Vec::new()),
                Vec::new(),
            )
            .await
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if supervisor.state(unit).await == Ok("failed".into()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        assert_eq!(supervisor.state(unit).await.unwrap(), "failed");
        assert!(
            supervisor
                .start(
                    unit,
                    &manifest(PathBuf::from("/bin/false"), Vec::new()),
                    Vec::new(),
                )
                .await
                .is_err()
        );
        supervisor.reset_failed(unit).await.unwrap();
        supervisor.discard(unit).await.unwrap();
        supervisor
            .start(
                unit,
                &manifest(PathBuf::from("/bin/sleep"), vec!["5".into()]),
                Vec::new(),
            )
            .await
            .unwrap();
        assert_eq!(supervisor.state(unit).await.unwrap(), "active");
        supervisor.stop(unit).await.unwrap();
    }

    #[tokio::test]
    async fn register_accepts_only_daemon_launched_pids() {
        let bus = session_bus().await;
        let connection = session_connection(&bus.1).await;
        let supervision = Arc::new(Supervision::bootstrap(&connection).await);
        let supervisor = Supervisor::new(&connection, supervision.clone());
        let unit = "arinn-provider-auth.service";
        supervisor
            .start(
                unit,
                &manifest(PathBuf::from("/bin/sleep"), vec!["120".into()]),
                Vec::new(),
            )
            .await
            .unwrap();
        let mut foreign = Command::new("/bin/sleep")
            .arg("120")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let foreign_client = session_connection(&bus.1).await;
        let foreign_name = format!("org.arinn.TestForeign{}", Uuid::new_v4().simple());
        foreign_client
            .request_name(foreign_name.as_str())
            .await
            .unwrap();
        assert!(
            supervisor.authenticate(&foreign_name, unit).await.is_err(),
            "a foreign same-user process must not authenticate as the provider unit"
        );
        let _ = foreign.kill();
        let _ = foreign.wait();
        supervisor.stop(unit).await.unwrap();
    }

    #[tokio::test]
    async fn no_dual_supervision_runtime_per_daemon_instance() {
        let bus = session_bus().await;
        let connection = session_connection(&bus.1).await;
        let supervision = Arc::new(Supervision::bootstrap(&connection).await);
        assert_eq!(supervision.mode(), Mode::Daemon);
        let supervisor = Supervisor::new(&connection, supervision.clone());
        let unit = "arinn-provider-one.service";
        supervisor
            .start(
                unit,
                &manifest(PathBuf::from("/bin/sleep"), vec!["30".into()]),
                Vec::new(),
            )
            .await
            .unwrap();
        assert!(supervision.daemon_loaded(unit).await);
        assert!(supervisor.unit(unit).await.is_ok());
        // systemd adapter is not active: GetUnit on the real manager would fail on this bus.
        assert!(!systemd::Backend::available(&connection).await);
        supervisor.stop(unit).await.unwrap();
    }
}
