// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! systemd user-manager adapter. Process supervision is never reimplemented here.
use crate::registry::Manifest;
use arinn_protocol::validation::ErrorCode;
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedObjectPath, Value},
};

#[derive(Clone)]
pub struct Supervisor {
    connection: Connection,
}

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

/// The running process still executes `executable` when `/proc/<pid>/exe`
/// is not marked deleted and names the same device and inode (spec §9.10, #243).
fn running_executable_matches(pid: u32, executable: &Path) -> bool {
    let proc_exe = PathBuf::from(format!("/proc/{pid}/exe"));
    let Ok(link) = fs::read_link(&proc_exe) else {
        return false;
    };
    // The kernel appends this when the executed inode has been unlinked.
    if link.to_string_lossy().ends_with(" (deleted)") {
        return false;
    }
    let (Ok(running), Ok(installed)) = (fs::metadata(&proc_exe), fs::metadata(executable)) else {
        return false;
    };
    running.dev() == installed.dev() && running.ino() == installed.ino()
}
impl Supervisor {
    pub fn new(connection: &Connection) -> Self {
        Self {
            connection: connection.clone(),
        }
    }
    async fn manager(&self) -> Result<Proxy<'_>, ErrorCode> {
        Proxy::new(
            &self.connection,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)
    }
    pub async fn unit(&self, name: &str) -> Result<OwnedObjectPath, ErrorCode> {
        self.manager()
            .await?
            .call("GetUnit", &(name,))
            .await
            .map_err(|error| match error {
                zbus::Error::MethodError(name, _, _)
                    if name.as_str() == "org.freedesktop.systemd1.NoSuchUnit"
                        || name.as_str() == "org.freedesktop.DBus.Error.UnknownObject" =>
                {
                    ErrorCode::NotFound
                }
                _ => ErrorCode::Unavailable,
            })
    }
    pub async fn state(&self, name: &str) -> Result<String, ErrorCode> {
        let path = self.unit(name).await?;
        Proxy::new(
            &self.connection,
            "org.freedesktop.systemd1",
            path,
            "org.freedesktop.systemd1.Unit",
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)?
        .get_property("ActiveState")
        .await
        .map_err(|_| ErrorCode::Unavailable)
    }
    pub async fn restarts(&self, name: &str) -> Result<u32, ErrorCode> {
        let work = async {
            let path = self.unit(name).await?;
            Proxy::new(
                &self.connection,
                "org.freedesktop.systemd1",
                path,
                "org.freedesktop.systemd1.Service",
            )
            .await
            .map_err(|_| ErrorCode::Unavailable)?
            .get_property("NRestarts")
            .await
            .map_err(|_| ErrorCode::Unavailable)
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
    pub async fn authenticate(&self, owner: &str, unit: &str) -> Result<(), ErrorCode> {
        let work = async {
            let bus = zbus::fdo::DBusProxy::new(&self.connection)
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            let name = owner.try_into().map_err(|_| ErrorCode::PermissionDenied)?;
            let uid = bus
                .get_connection_unix_user(name)
                .await
                .map_err(|_| ErrorCode::PermissionDenied)?;
            // SAFETY: geteuid has no arguments or memory preconditions.
            if uid != unsafe { libc::geteuid() } {
                return Err(ErrorCode::PermissionDenied);
            }
            let pid = bus
                .get_connection_unix_process_id(
                    owner.try_into().map_err(|_| ErrorCode::PermissionDenied)?,
                )
                .await
                .map_err(|_| ErrorCode::PermissionDenied)?;
            let actual: OwnedObjectPath = self
                .manager()
                .await?
                .call("GetUnitByPID", &(pid,))
                .await
                .map_err(|_| ErrorCode::PermissionDenied)?;
            if actual != self.unit(unit).await? {
                return Err(ErrorCode::PermissionDenied);
            }
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
    async fn main_pid(&self, name: &str) -> Result<u32, ErrorCode> {
        let path = self.unit(name).await?;
        Proxy::new(
            &self.connection,
            "org.freedesktop.systemd1",
            path,
            "org.freedesktop.systemd1.Service",
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)?
        .get_property("MainPID")
        .await
        .map_err(|_| ErrorCode::Unavailable)
    }
    /// Whether `executable` is what the unit's main process is running.
    /// A unit that is not active, or whose main PID is not assigned yet, is
    /// current: there is no stale process to replace (#243).
    pub async fn executable_current(
        &self,
        name: &str,
        executable: &Path,
    ) -> Result<bool, ErrorCode> {
        let work = async {
            match self.state(name).await {
                Err(ErrorCode::NotFound) => Ok(true),
                Err(error) => Err(error),
                Ok(state) if matches!(state.as_str(), "active" | "activating" | "reloading") => {
                    self.process_matches(name, executable).await
                }
                Ok(_) => Ok(true),
            }
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
    async fn process_matches(&self, name: &str, executable: &Path) -> Result<bool, ErrorCode> {
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
    ) -> Result<Attachment, ErrorCode> {
        // On daemon restart, attach to the surviving unit when it still runs
        // the installed executable. A replaced binary is discarded and created
        // again (#243). A failed unit is left failed; only a manual restart
        // clears its start-limit state.
        let replace = match tokio::time::timeout(Duration::from_secs(3), self.state(name)).await {
            Ok(Ok(state)) if matches!(state.as_str(), "active" | "activating" | "reloading") => {
                let matches = tokio::time::timeout(
                    Duration::from_secs(3),
                    self.process_matches(name, &manifest.executable),
                )
                .await
                .map_err(|_| ErrorCode::Timeout)??;
                if matches {
                    return Ok(Attachment::Attached);
                }
                true
            }
            Ok(Ok(state)) if state == "failed" => return Err(ErrorCode::Unavailable),
            Ok(Ok(_)) | Ok(Err(ErrorCode::NotFound)) => false,
            Ok(Err(error)) => return Err(error),
            Err(_) => return Err(ErrorCode::Timeout),
        };
        if replace {
            self.discard(name).await?;
        }
        self.spawn_transient(name, manifest, environment).await?;
        Ok(if replace {
            Attachment::Replaced
        } else {
            Attachment::Created
        })
    }
    async fn spawn_transient(
        &self,
        name: &str,
        manifest: &Manifest,
        environment: Vec<String>,
    ) -> Result<(), ErrorCode> {
        let work = async {
            let executable = manifest
                .executable
                .to_str()
                .ok_or(ErrorCode::InvalidArgument)?
                .to_string();
            let mut argv = vec![executable.clone()];
            argv.extend(manifest.args.clone());
            let commands = vec![(executable, argv, false)];
            let properties = vec![
                ("Type", Value::from("exec")),
                ("Restart", Value::from("on-failure")),
                ("RestartUSec", Value::from(2_000_000u64)),
                ("StartLimitIntervalUSec", Value::from(60_000_000u64)),
                ("StartLimitBurst", Value::from(5u32)),
                ("TimeoutStopUSec", Value::from(5_000_000u64)),
                ("ExecStart", Value::new(commands)),
                ("Environment", Value::new(environment)),
            ];
            let auxiliary: Vec<(String, Vec<(String, Value<'_>)>)> = Vec::new();
            let _: OwnedObjectPath = self
                .manager()
                .await?
                .call("StartTransientUnit", &(name, "fail", properties, auxiliary))
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
    pub async fn stop(&self, name: &str) -> Result<(), ErrorCode> {
        match self.unit(name).await {
            Err(ErrorCode::NotFound) => return Ok(()),
            Err(error) => return Err(error),
            Ok(_) => (),
        }
        let work = async {
            let _: OwnedObjectPath = self
                .manager()
                .await?
                .call("StopUnit", &(name, "replace"))
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            loop {
                match self.state(name).await {
                    Err(_) => return Ok(()),
                    Ok(state) if state == "inactive" || state == "failed" => break,
                    _ => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            }
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(6), work)
            .await
            .map_err(|_| ErrorCode::Timeout)??;
        // An inactive transient unit is unloaded by systemd's garbage
        // collection shortly after it stops; until then StartTransientUnit
        // with the same name fails with UnitExists. Wait for that, so a
        // following start can create the unit again (#234).
        if self.state(name).await.as_deref() != Ok("failed") {
            let _ = self.unloaded(name).await;
        }
        Ok(())
    }
    /// Wait up to 3 s until the unit is no longer loaded.
    async fn unloaded(&self, name: &str) -> Result<(), ErrorCode> {
        let work = async {
            loop {
                match self.unit(name).await {
                    Err(ErrorCode::NotFound) => return Ok(()),
                    Err(error) => return Err(error),
                    Ok(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
    /// Stop a unit and wait until systemd has unloaded it, also when it had
    /// failed, so it can be created again with a new environment (#234).
    pub async fn discard(&self, name: &str) -> Result<(), ErrorCode> {
        self.stop(name).await?;
        match self.unit(name).await {
            Err(ErrorCode::NotFound) => return Ok(()),
            Err(error) => return Err(error),
            Ok(_) => (),
        }
        // A failed transient unit stays loaded until its failed state is
        // reset; it is then collected like any inactive unit.
        self.reset_failed(name).await?;
        self.unloaded(name).await
    }
    /// Clear a unit's failed state and start-limit counter. A failed
    /// transient unit becomes inactive and is then garbage-collected.
    pub async fn reset_failed(&self, name: &str) -> Result<(), ErrorCode> {
        let work = async {
            let _: () = self
                .manager()
                .await?
                .call("ResetFailedUnit", &(name,))
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
    /// A hung handshake is a failed process, so systemd's existing restart budget
    /// applies. StopUnit would mark it an intentional stop and bypass that budget.
    pub async fn fail_handshake(&self, name: &str) -> Result<(), ErrorCode> {
        let work = async {
            let _: () = self
                .manager()
                .await?
                .call("KillUnit", &(name, "all", libc::SIGKILL))
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
}

#[cfg(test)]
mod tests {
    use super::running_executable_matches;
    use std::{
        fs,
        os::unix::fs::{MetadataExt, PermissionsExt},
        path::Path,
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    // What the check compares, for a failure message (#257).
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

    // Waits briefly for the condition: other tests in this binary spawn
    // processes at the same time, and the check reads live /proc state.
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
        // A unique directory: a PID can repeat across concurrent containers
        // that share a runner host.
        let dir = std::env::temp_dir().join(format!("arinn-stale-{}", uuid::Uuid::new_v4()));
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
}
