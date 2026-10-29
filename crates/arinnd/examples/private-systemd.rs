// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Test-only user-manager double. Run only inside dbus-run-session.
use std::{
    collections::{HashMap, VecDeque},
    process::{Child, Command},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use zbus::{
    Connection,
    zvariant::{OwnedObjectPath, OwnedValue},
};
struct Unit {
    application: bool,
    exit_code: i32,
    exit_status: i32,
    exited: u64,
    command: String,
    args: Vec<String>,
    env: Vec<String>,
    child: Option<Child>,
    state: String,
    starts: VecDeque<Instant>,
    retry_at: Option<Instant>,
    restarts: u32,
    /// When an inactive transient unit is garbage-collected (unloaded).
    collect_at: Option<Instant>,
    /// Process group of an application unit. Like `ExitType=cgroup`, the
    /// unit stays active after GIO exits while anything it started runs.
    group: Option<i32>,
}
/// Like systemd's CollectMode=inactive: an inactive transient unit is
/// unloaded shortly after it stops, not at once (#234).
const COLLECT_DELAY: Duration = Duration::from_millis(300);
/// Whether a process of `group` is alive: the stand-in for a non-empty
/// cgroup. Reads /proc, so orphans that nobody reaps yet (zombies) and
/// processes reparented away from this manager are judged correctly.
fn group_alive(group: i32) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    entries.flatten().any(|entry| {
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            return false;
        };
        // pid (comm) state ppid pgrp ...; comm may contain spaces.
        let Some(rest) = stat.rsplit_once(')').map(|(_, rest)| rest) else {
            return false;
        };
        let mut fields = rest.split_whitespace();
        let state = fields.next();
        let pgrp = fields.nth(1).and_then(|value| value.parse::<i32>().ok());
        pgrp == Some(group) && state != Some("Z") && state != Some("X")
    })
}
/// Stop every process of an application unit, as StopUnit does for a cgroup.
fn stop_group(unit: &mut Unit) {
    if let Some(group) = unit.group.take() {
        unsafe { libc::kill(-group, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(1);
        while group_alive(group) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        unsafe { libc::kill(-group, libc::SIGKILL) };
    }
}
type Shared = Arc<Mutex<HashMap<String, Unit>>>;
fn path(name: &str) -> OwnedObjectPath {
    format!(
        "/org/freedesktop/systemd1/unit/u_{}",
        name.bytes().map(|b| format!("{b:02x}")).collect::<String>()
    )
    .try_into()
    .unwrap()
}
fn spawn(unit: &mut Unit) -> std::io::Result<()> {
    // Tests read the environment each start used (#234).
    if let Some(dir) = std::env::var_os("ARINN_TEST_UNIT_ENV_DIR") {
        let name = std::path::Path::new(&unit.command)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let _ = std::fs::write(
            std::path::Path::new(&dir).join(format!("{name}.env")),
            unit.env.join("\n"),
        );
    }
    let mut command = Command::new(&unit.command);
    command.args(&unit.args[1..]);
    for entry in &unit.env {
        if let Some((key, value)) = entry.split_once('=') {
            command.env(key, value);
        }
    }
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if unit.application {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command.spawn()?;
    if unit.application {
        unit.group = Some(child.id() as i32);
    }
    unit.child = Some(child);
    unit.state = "active".into();
    unit.starts.push_back(Instant::now());
    unit.retry_at = None;
    Ok(())
}
struct Manager {
    units: Shared,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    async fn get_unit(&self, name: &str) -> zbus::fdo::Result<OwnedObjectPath> {
        if self.units.lock().await.contains_key(name) {
            Ok(path(name))
        } else {
            Err(zbus::fdo::Error::UnknownObject("missing".into()))
        }
    }
    #[zbus(name = "GetUnitByPID")]
    async fn get_unit_by_pid(&self, pid: u32) -> zbus::fdo::Result<OwnedObjectPath> {
        self.units
            .lock()
            .await
            .iter()
            .find(|(_, u)| u.child.as_ref().is_some_and(|c| c.id() == pid))
            .map(|(name, _)| path(name))
            .ok_or_else(|| zbus::fdo::Error::AccessDenied("not in unit".into()))
    }
    async fn start_transient_unit(
        &self,
        name: &str,
        _mode: &str,
        properties: Vec<(String, OwnedValue)>,
        _auxiliary: Vec<(String, Vec<(String, OwnedValue)>)>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        // systemd refuses a transient unit whose name is still loaded.
        if self.units.lock().await.contains_key(name) {
            return Err(zbus::fdo::Error::Failed(format!("UnitExists: {name}")));
        }
        let mut properties: HashMap<_, _> = properties.into_iter().collect();
        let application = properties.contains_key("ExecStartEx");
        let (command, args, env) = if application {
            assert!(name.starts_with("app-arinn-"));
            let commands: Vec<(String, Vec<String>, Vec<String>)> = properties
                .remove("ExecStartEx")
                .unwrap()
                .try_into()
                .unwrap();
            let (command, args, flags) = commands.into_iter().next().unwrap();
            assert_eq!(command, "/usr/bin/gio");
            assert_eq!(flags, ["no-env-expand"]);
            for (key, value) in [
                ("Type", "exec"),
                ("ExitType", "cgroup"),
                ("Slice", "app.slice"),
                ("StandardOutput", "null"),
                ("StandardError", "null"),
            ] {
                assert_eq!(
                    String::try_from(properties.remove(key).unwrap()).unwrap(),
                    value
                );
            }
            assert!(!properties.contains_key("Environment"));
            (command, args, Vec::new())
        } else {
            let commands: Vec<(String, Vec<String>, bool)> =
                properties.remove("ExecStart").unwrap().try_into().unwrap();
            let env: Vec<String> = properties
                .remove("Environment")
                .unwrap()
                .try_into()
                .unwrap();
            assert_eq!(
                u64::try_from(properties.remove("RestartUSec").unwrap()).unwrap(),
                2_000_000
            );
            assert_eq!(
                u32::try_from(properties.remove("StartLimitBurst").unwrap()).unwrap(),
                5
            );
            assert_eq!(
                String::try_from(properties.remove("Restart").unwrap()).unwrap(),
                "on-failure"
            );
            let (command, args, _) = commands.into_iter().next().unwrap();
            (command, args, env)
        };
        let mut unit = Unit {
            application,
            exit_code: 0,
            exit_status: 0,
            exited: 0,
            command,
            args,
            env,
            child: None,
            state: "inactive".into(),
            starts: VecDeque::new(),
            retry_at: None,
            restarts: 0,
            collect_at: None,
            group: None,
        };
        spawn(&mut unit).map_err(|_| zbus::fdo::Error::Failed("spawn".into()))?;
        if let Some(mut old) = self.units.lock().await.insert(name.into(), unit)
            && let Some(child) = old.child.as_mut()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        connection
            .object_server()
            .at(
                path(name),
                UnitObject {
                    units: self.units.clone(),
                    name: name.into(),
                },
            )
            .await?;
        connection
            .object_server()
            .at(
                path(name),
                ServiceObject {
                    units: self.units.clone(),
                    name: name.into(),
                },
            )
            .await?;
        Ok("/org/freedesktop/systemd1/job/1".try_into().unwrap())
    }
    fn unref_unit(&self, _name: &str) {}
    async fn stop_unit(&self, name: &str, _mode: &str) -> zbus::fdo::Result<OwnedObjectPath> {
        if let Some(unit) = self.units.lock().await.get_mut(name) {
            stop_group(unit);
            if let Some(child) = unit.child.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
            unit.child = None;
            unit.state = "inactive".into();
            unit.retry_at = None;
            unit.collect_at = Some(Instant::now() + COLLECT_DELAY);
        }
        Ok("/org/freedesktop/systemd1/job/1".try_into().unwrap())
    }
    async fn reset_failed_unit(&self, name: &str) -> zbus::fdo::Result<()> {
        if let Some(unit) = self.units.lock().await.get_mut(name) {
            unit.starts.clear();
            if unit.state == "failed" {
                unit.state = "inactive".into();
                unit.collect_at = Some(Instant::now() + COLLECT_DELAY);
            }
        }
        Ok(())
    }
    async fn restart_unit(&self, name: &str, _mode: &str) -> zbus::fdo::Result<OwnedObjectPath> {
        let mut units = self.units.lock().await;
        let unit = units
            .get_mut(name)
            .ok_or_else(|| zbus::fdo::Error::UnknownObject("missing".into()))?;
        if let Some(child) = unit.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        spawn(unit).map_err(|_| zbus::fdo::Error::Failed("spawn".into()))?;
        Ok("/org/freedesktop/systemd1/job/1".try_into().unwrap())
    }
    async fn kill_unit(&self, name: &str, _who: &str, _signal: i32) -> zbus::fdo::Result<()> {
        if let Some(unit) = self.units.lock().await.get_mut(name)
            && let Some(child) = unit.child.as_mut()
        {
            let _ = child.kill();
        }
        Ok(())
    }
}
struct UnitObject {
    units: Shared,
    name: String,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
impl UnitObject {
    #[zbus(property)]
    async fn active_state(&self) -> String {
        self.units
            .lock()
            .await
            .get(&self.name)
            .map(|u| u.state.clone())
            .unwrap_or("inactive".into())
    }
}
struct ServiceObject {
    units: Shared,
    name: String,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Service")]
impl ServiceObject {
    #[zbus(property)]
    async fn exec_main_exit_timestamp_monotonic(&self) -> u64 {
        self.units
            .lock()
            .await
            .get(&self.name)
            .map(|u| u.exited)
            .unwrap_or(0)
    }
    #[zbus(property)]
    async fn exec_main_code(&self) -> i32 {
        self.units
            .lock()
            .await
            .get(&self.name)
            .map(|u| u.exit_code)
            .unwrap_or(0)
    }
    #[zbus(property)]
    async fn exec_main_status(&self) -> i32 {
        self.units
            .lock()
            .await
            .get(&self.name)
            .map(|u| u.exit_status)
            .unwrap_or(0)
    }
    /// The real child pid, so the daemon can compare `/proc/<MainPID>/exe`
    /// with the installed executable (#243).
    #[zbus(property, name = "MainPID")]
    async fn main_pid(&self) -> u32 {
        self.units
            .lock()
            .await
            .get(&self.name)
            .and_then(|unit| unit.child.as_ref().map(std::process::Child::id))
            .unwrap_or(0)
    }
    #[zbus(property, name = "NRestarts")]
    async fn restarts(&self) -> u32 {
        self.units
            .lock()
            .await
            .get(&self.name)
            .map(|u| u.restarts)
            .unwrap_or(0)
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("ARINN_PRIVATE_TEST_BUS").as_deref() == Ok("1"),
        "private-test marker required"
    );
    let units: Shared = Arc::default();
    let _connection = zbus::connection::Builder::session()?
        .name("org.freedesktop.systemd1")?
        .serve_at(
            "/org/freedesktop/systemd1",
            Manager {
                units: units.clone(),
            },
        )?
        .build()
        .await?;
    let shared = units.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            shared
                .lock()
                .await
                .retain(|_, unit| unit.collect_at.is_none_or(|time| Instant::now() < time));
            for unit in shared.lock().await.values_mut() {
                if let Some(child) = unit.child.as_mut()
                    && let Some(status) = child.try_wait().ok().flatten()
                {
                    unit.child = None;
                    unit.exit_code = if status.code().is_some() {
                        libc::CLD_EXITED
                    } else {
                        libc::CLD_KILLED
                    };
                    unit.exit_status = status.code().unwrap_or(1);
                    unit.exited = 1;
                    if unit.application && !status.success() {
                        // A failed main process fails the unit and stops the
                        // rest of its cgroup.
                        stop_group(unit);
                    }
                    unit.state = if unit.application {
                        if status.success() { "active" } else { "failed" }
                    } else {
                        "activating"
                    }
                    .into();
                    unit.retry_at = if unit.application {
                        None
                    } else {
                        Some(Instant::now() + Duration::from_secs(2))
                    };
                }
                if unit.application
                    && unit.child.is_none()
                    && unit.collect_at.is_none()
                    && unit.group.is_none_or(|group| !group_alive(group))
                {
                    unit.group = None;
                    if unit.state == "active" {
                        unit.state = "inactive".into();
                    }
                    unit.collect_at = Some(Instant::now() + COLLECT_DELAY);
                }
                if unit.retry_at.is_some_and(|time| Instant::now() >= time) {
                    while unit
                        .starts
                        .front()
                        .is_some_and(|time| time.elapsed() > Duration::from_secs(60))
                    {
                        unit.starts.pop_front();
                    }
                    if unit.starts.len() >= 5 {
                        unit.state = "failed".into();
                        unit.retry_at = None;
                    } else {
                        unit.restarts += 1;
                        let _ = spawn(unit);
                    }
                }
            }
        }
    });
    tokio::signal::ctrl_c().await?;
    for unit in units.lock().await.values_mut() {
        if let Some(child) = unit.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    Ok(())
}
