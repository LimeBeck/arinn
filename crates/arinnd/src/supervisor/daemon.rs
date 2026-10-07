// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! In-process provider supervision when the session has no systemd user manager.
use crate::registry::Manifest;
use arinn_protocol::validation::ErrorCode;
use std::{
    collections::{HashMap, VecDeque},
    ffi::OsStr,
    fs,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use zbus::Connection;

use super::running_executable_matches;

const RESTART_DELAY: Duration = Duration::from_secs(2);
const START_LIMIT: usize = 5;
const START_INTERVAL: Duration = Duration::from_secs(60);
const COLLECT_DELAY: Duration = Duration::from_millis(300);

struct Unit {
    executable: PathBuf,
    args: Vec<String>,
    env: Vec<String>,
    child: Option<Child>,
    state: String,
    starts: VecDeque<Instant>,
    retry_at: Option<Instant>,
    restarts: u32,
    loaded: bool,
    collect_at: Option<Instant>,
}

pub struct Pool {
    units: Mutex<HashMap<String, Unit>>,
    maintainer_started: AtomicBool,
}

impl Pool {
    pub fn new() -> Self {
        Self {
            units: Mutex::new(HashMap::new()),
            maintainer_started: AtomicBool::new(false),
        }
    }

    pub fn ensure_maintainer(self: &Arc<Self>) {
        if self.maintainer_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let pool = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                pool.tick().await;
            }
        });
    }

    /// Stop every supervised provider process group before the daemon exits.
    pub async fn shutdown_all(&self) {
        let mut units = self.units.lock().await;
        for unit in units.values_mut() {
            stop_child_group(unit);
        }
    }

    async fn tick(&self) {
        let mut units = self.units.lock().await;
        units.retain(|_, unit| unit.collect_at.is_none_or(|time| Instant::now() < time));
        for unit in units.values_mut() {
            if let Some(child) = unit.child.as_mut()
                && let Ok(Some(status)) = child.try_wait()
            {
                unit.child = None;
                let failed = !status.success();
                unit.state = if failed { "activating" } else { "inactive" }.into();
                unit.retry_at = if failed {
                    Some(Instant::now() + RESTART_DELAY)
                } else {
                    None
                };
                if !failed {
                    unit.collect_at = Some(Instant::now() + COLLECT_DELAY);
                }
            }
            if unit.retry_at.is_some_and(|time| Instant::now() >= time) {
                unit.retry_at = None;
                record_failed_start(unit);
                if unit.state == "failed" {
                    continue;
                }
                unit.restarts += 1;
                if spawn_unit(unit).is_err() {
                    record_failed_start(unit);
                }
            }
        }
    }

    pub async fn loaded(&self, name: &str) -> bool {
        self.units.lock().await.get(name).is_some_and(|u| u.loaded)
    }

    pub async fn state(&self, name: &str) -> Result<String, ErrorCode> {
        let units = self.units.lock().await;
        if units.get(name).is_some_and(|u| u.loaded) {
            Ok(units
                .get(name)
                .map(|u| u.state.clone())
                .unwrap_or_else(|| "inactive".into()))
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    pub async fn restarts(&self, name: &str) -> Result<u32, ErrorCode> {
        let units = self.units.lock().await;
        units
            .get(name)
            .filter(|u| u.loaded)
            .map(|u| u.restarts)
            .ok_or(ErrorCode::NotFound)
    }

    pub async fn main_pid(&self, name: &str) -> Result<u32, ErrorCode> {
        let units = self.units.lock().await;
        let unit = units
            .get(name)
            .filter(|u| u.loaded)
            .ok_or(ErrorCode::NotFound)?;
        Ok(unit.child.as_ref().map(Child::id).unwrap_or(0))
    }

    pub async fn process_matches(&self, name: &str, executable: &Path) -> Result<bool, ErrorCode> {
        let pid = self.main_pid(name).await?;
        if pid == 0 {
            return Ok(true);
        }
        let executable = executable.to_path_buf();
        tokio::task::spawn_blocking(move || running_executable_matches(pid, &executable))
            .await
            .map_err(|_| ErrorCode::Internal)
    }

    pub async fn start(
        &self,
        name: &str,
        manifest: &Manifest,
        environment: Vec<String>,
    ) -> Result<(), ErrorCode> {
        if !manifest.executable.is_absolute() {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut units = self.units.lock().await;
        if units.contains_key(name) && units[name].loaded {
            return Err(ErrorCode::Unavailable);
        }
        let args = manifest.args.clone();
        let mut unit = Unit {
            executable: manifest.executable.clone(),
            args,
            env: environment,
            child: None,
            state: "inactive".into(),
            starts: VecDeque::new(),
            retry_at: None,
            restarts: 0,
            loaded: true,
            collect_at: None,
        };
        if spawn_unit(&mut unit).is_err() {
            record_failed_start(&mut unit);
        }
        units.insert(name.to_string(), unit);
        Ok(())
    }

    pub async fn stop(&self, name: &str) -> Result<(), ErrorCode> {
        let mut units = self.units.lock().await;
        let Some(unit) = units.get_mut(name) else {
            return Ok(());
        };
        stop_child_group(unit);
        unit.child = None;
        unit.state = "inactive".into();
        unit.retry_at = None;
        unit.collect_at = Some(Instant::now() + COLLECT_DELAY);
        Ok(())
    }

    pub async fn unload(&self, name: &str) -> Result<(), ErrorCode> {
        let mut units = self.units.lock().await;
        if let Some(unit) = units.get_mut(name) {
            unit.loaded = false;
            unit.collect_at = Some(Instant::now() + COLLECT_DELAY);
        }
        Ok(())
    }

    pub async fn wait_unloaded(&self, name: &str) -> Result<(), ErrorCode> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if !self.units.lock().await.contains_key(name) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(ErrorCode::Timeout);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub async fn reset_failed(&self, name: &str) -> Result<(), ErrorCode> {
        let mut units = self.units.lock().await;
        if let Some(unit) = units.get_mut(name) {
            unit.starts.clear();
            if unit.state == "failed" {
                unit.state = "inactive".into();
                unit.collect_at = Some(Instant::now() + COLLECT_DELAY);
            }
        }
        Ok(())
    }

    pub async fn fail_handshake(&self, name: &str) -> Result<(), ErrorCode> {
        let mut units = self.units.lock().await;
        if let Some(unit) = units.get_mut(name) {
            stop_child_group(unit);
            unit.child = None;
            unit.state = "activating".into();
            unit.retry_at = Some(Instant::now() + RESTART_DELAY);
        }
        Ok(())
    }

    pub async fn authenticate(
        &self,
        connection: &Connection,
        owner: &str,
        unit: &str,
    ) -> Result<(), ErrorCode> {
        let work = async {
            let bus = zbus::fdo::DBusProxy::new(connection)
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            let name: zbus::names::BusName<'_> =
                owner.try_into().map_err(|_| ErrorCode::PermissionDenied)?;
            let uid = bus
                .get_connection_unix_user(name.clone())
                .await
                .map_err(|_| ErrorCode::PermissionDenied)?;
            // SAFETY: geteuid has no arguments or memory preconditions.
            if uid != unsafe { libc::geteuid() } {
                return Err(ErrorCode::PermissionDenied);
            }
            let pid = bus
                .get_connection_unix_process_id(name.clone())
                .await
                .map_err(|_| ErrorCode::PermissionDenied)?;
            let main_pid = self.main_pid(unit).await?;
            if main_pid == 0 || pid != main_pid {
                return Err(ErrorCode::PermissionDenied);
            }
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
}

fn record_failed_start(unit: &mut Unit) {
    while unit
        .starts
        .front()
        .is_some_and(|time| time.elapsed() > START_INTERVAL)
    {
        unit.starts.pop_front();
    }
    unit.starts.push_back(Instant::now());
    if unit.starts.len() >= START_LIMIT {
        unit.state = "failed".into();
        unit.retry_at = None;
        unit.child = None;
    } else {
        unit.state = "activating".into();
        unit.retry_at = Some(Instant::now() + RESTART_DELAY);
    }
}

fn stop_child_group(unit: &mut Unit) {
    if let Some(child) = unit.child.as_mut() {
        let pgid = child.id() as libc::pid_t;
        // SAFETY: kill only this provider's process group; desktop apps stay outside it.
        let _ = unsafe { libc::killpg(pgid, libc::SIGKILL) };
        let _ = child.wait();
    }
}

fn spawn_unit(unit: &mut Unit) -> Result<(), ErrorCode> {
    record_test_artifacts(unit);
    let executable = unit.executable.to_str().ok_or(ErrorCode::InvalidArgument)?;
    let mut command = Command::new(executable);
    command.args(&unit.args);
    for entry in &unit.env {
        if let Some((key, value)) = entry.split_once('=') {
            command.env(key, value);
        }
    }
    command.stdout(Stdio::null()).stderr(Stdio::null());
    // Never invoke a shell: the manifest executable is started directly.
    command.process_group(0);
    unsafe {
        command.pre_exec(|| {
            // SAFETY: prctl only configures the child about to exec.
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        });
    }
    unit.child = Some(command.spawn().map_err(|_| ErrorCode::Unavailable)?);
    unit.state = "active".into();
    unit.starts.push_back(Instant::now());
    unit.retry_at = None;
    Ok(())
}

fn record_test_artifacts(unit: &Unit) {
    if let Some(dir) = std::env::var_os("ARINN_TEST_UNIT_ENV_DIR") {
        let name = unit
            .executable
            .file_name()
            .map(OsStr::to_string_lossy)
            .unwrap_or_default();
        let base = Path::new(&dir);
        let _ = fs::write(base.join(format!("{name}.env")), unit.env.join("\n"));
        let _ = fs::write(
            base.join(format!("{name}.argv")),
            std::iter::once(unit.executable.as_os_str())
                .chain(unit.args.iter().map(OsStr::new))
                .map(OsStr::to_string_lossy)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
}
