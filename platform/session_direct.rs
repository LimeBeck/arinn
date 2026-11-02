// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Start the shared Arinn stack on the session bus without a systemd user manager.
use arinn_protocol::production;
use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
use zbus::fdo::DBusProxy;
use zbus::Connection;

const PLATFORM: &str = "org.arinn.X11";
const SHELL: &str = "org.arinn.Shell1";
const START_TIMEOUT: Duration = Duration::from_secs(30);
const DAEMON: &str = "ARINN_DAEMON";
const PLATFORM_BIN: &str = "ARINN_PLATFORM_X11";
const SHELL_LAUNCH: &str = "ARINN_SHELL_LAUNCH";

pub const BINARY_NOT_FOUND: &str = "binary not found";
pub const DAEMON_BUS: &str = "arinnd did not start on the session bus";
pub const PLATFORM_BUS: &str = "platform adapter did not start on the session bus";
pub const SHELL_BUS: &str = "shell did not start on the session bus";

/// Fixed direct-start failure; [`message`](Self::message) is the only user-visible text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectStartError {
    BinaryNotFound,
    DaemonBus,
    PlatformBus,
    ShellBus,
}

impl DirectStartError {
    pub fn message(self) -> &'static str {
        match self {
            Self::BinaryNotFound => BINARY_NOT_FOUND,
            Self::DaemonBus => DAEMON_BUS,
            Self::PlatformBus => PLATFORM_BUS,
            Self::ShellBus => SHELL_BUS,
        }
    }
}

/// Session variables the EWMH bridge would import into a user manager.
#[derive(Clone, Debug)]
pub struct SessionEnvironment {
    values: HashMap<String, String>,
    unset: Vec<String>,
}

impl SessionEnvironment {
    /// Build the EWMH session environment from the login process.
    pub fn for_ewmh_login() -> Self {
        let mut values = HashMap::from([
            ("XDG_SESSION_TYPE".into(), "x11".into()),
            ("ARINN_PLATFORM_BACKEND".into(), "ewmh".into()),
            ("ARINN_SESSION_BACKEND".into(), "x11".into()),
            ("QT_QPA_PLATFORM".into(), "xcb".into()),
        ]);
        let mut unset = Vec::new();
        if env::var_os("XAUTHORITY").is_some_and(|value| !value.is_empty()) {
            if let Ok(authority) = env::var("XAUTHORITY") {
                values.insert("XAUTHORITY".into(), authority);
            }
        } else {
            unset.push("XAUTHORITY".into());
        }
        match env::var("ARINN_SESSION_ENTRY").as_deref() {
            Ok("xfce") => {
                values.insert("ARINN_SESSION_ENTRY".into(), "xfce".into());
            }
            _ => unset.push("ARINN_SESSION_ENTRY".into()),
        }
        match (
            login_folder("XDG_STATE_HOME", ".local/state"),
            login_folder("XDG_CONFIG_HOME", ".config"),
        ) {
            (Some(state), Some(config)) => {
                values.insert("ARINN_LOGIN_STATE_HOME".into(), state);
                values.insert("ARINN_LOGIN_CONFIG_HOME".into(), config);
            }
            _ => {
                unset.push("ARINN_LOGIN_STATE_HOME".into());
                unset.push("ARINN_LOGIN_CONFIG_HOME".into());
            }
        }
        if env::var("XDG_SESSION_ID")
            .ok()
            .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()))
            .is_some()
        {
            values.insert(
                "ARINN_LOGIN_SESSION_ID".into(),
                env::var("XDG_SESSION_ID").unwrap(),
            );
        } else {
            unset.push("ARINN_LOGIN_SESSION_ID".into());
        }
        Self { values, unset }
    }

    pub fn apply_to(&self, command: &mut Command) {
        for key in &self.unset {
            command.env_remove(key);
        }
        for (key, value) in &self.values {
            command.env(key, value);
        }
    }
}

fn login_folder(variable: &str, fallback: &str) -> Option<String> {
    let absolute =
        |value: std::ffi::OsString| Some(PathBuf::from(value)).filter(|p| p.is_absolute());
    env::var_os(variable)
        .and_then(absolute)
        .or_else(|| {
            env::var_os("HOME")
                .and_then(absolute)
                .map(|home| home.join(fallback))
        })
        .and_then(|path| path.into_os_string().into_string().ok())
}

fn spawn_stack_binary(path: &Path, session: &SessionEnvironment) -> Result<std::process::Child, DirectStartError> {
    if !path.is_absolute() || !path.exists() {
        return Err(DirectStartError::BinaryNotFound);
    }
    let mut command = Command::new(path);
    session.apply_to(&mut command);
    command
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|_| DirectStartError::BinaryNotFound)
}

fn resolve(env_var: &str, default: &str) -> PathBuf {
    env::var_os(env_var)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| PathBuf::from(default))
}

struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Processes started for one XFCE opt-in session without systemd.
pub struct DirectStack {
    daemon: Child,
    platform: Child,
    shell: Child,
    daemon_bin: PathBuf,
    platform_bin: PathBuf,
}

impl DirectStack {
    pub async fn start(
        connection: &Connection,
        session: &SessionEnvironment,
    ) -> Result<Self, DirectStartError> {
        let daemon_bin = resolve(DAEMON, "/usr/bin/arinnd");
        let platform_bin = resolve(PLATFORM_BIN, "/usr/bin/arinn-platform-x11");
        let shell_launch = resolve(SHELL_LAUNCH, "/usr/lib/arinn/arinn-shell-launch");
        let daemon = Child(spawn_stack_binary(&daemon_bin, session)?);
        wait_for_name(connection, production::SERVICE, START_TIMEOUT)
            .await
            .map_err(|_| DirectStartError::DaemonBus)?;
        let platform = Child(spawn_stack_binary(&platform_bin, session)?);
        wait_for_name(connection, PLATFORM, START_TIMEOUT)
            .await
            .map_err(|_| DirectStartError::PlatformBus)?;
        let shell = Child(spawn_stack_binary(&shell_launch, session)?);
        wait_for_name(connection, SHELL, START_TIMEOUT)
            .await
            .map_err(|_| DirectStartError::ShellBus)?;
        Ok(Self {
            daemon,
            platform,
            shell,
            daemon_bin,
            platform_bin,
        })
    }

    pub fn stop(mut self) {
        let _ = self.shell.0.kill();
        let _ = self.shell.0.wait();
        let _ = self.platform.0.kill();
        let _ = self.platform.0.wait();
        run_poststop(&self.platform_bin, &["--release-power-key"]);
        let _ = self.daemon.0.kill();
        let _ = self.daemon.0.wait();
        run_poststop(&self.daemon_bin, &["--release-power-key"]);
    }
}

fn run_poststop(binary: &Path, args: &[&str]) {
    let _ = Command::new(binary)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub async fn arinn_running(connection: &Connection) -> Result<bool, zbus::Error> {
    let bus = DBusProxy::new(connection).await?;
    Ok(bus
        .name_has_owner(production::SERVICE.try_into()?)
        .await?)
}

async fn wait_for_name(connection: &Connection, name: &str, timeout: Duration) -> Result<(), ()> {
    use futures_util::Stream;
    use std::pin::Pin;
    let bus = DBusProxy::new(connection).await.map_err(|_| ())?;
    let well_known = name.try_into().map_err(|_| ())?;
    let mut changes = bus
        .receive_name_owner_changed_with_args(&[(0, name)])
        .await
        .map_err(|_| ())?;
    if bus.name_has_owner(well_known).await.map_err(|_| ())? {
        return Ok(());
    }
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(());
        }
        let event = tokio::time::timeout(
            remaining,
            futures_util::future::poll_fn(|cx| Pin::new(&mut changes).poll_next(cx)),
        )
        .await;
        match event {
            Err(_) => return Err(()),
            Ok(None) => return Err(()),
            Ok(Some(change)) => {
                if change.args().ok().is_some_and(|args| args.new_owner().is_some()) {
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_environment_mirrors_systemd_imports() {
        let session = SessionEnvironment::for_ewmh_login();
        assert_eq!(session.values["XDG_SESSION_TYPE"], "x11");
        assert_eq!(session.values["ARINN_PLATFORM_BACKEND"], "ewmh");
        assert_eq!(session.values["QT_QPA_PLATFORM"], "xcb");
        assert!(session.unset.iter().any(|key| key == "ARINN_SESSION_ENTRY"));
        let root = std::env::temp_dir().join("arinn-session-env-test");
        std::fs::create_dir_all(&root).unwrap();
        // SAFETY: test-only; restored before returning.
        unsafe {
            env::set_var("HOME", &root);
            env::set_var("XDG_STATE_HOME", "/login/state");
            env::set_var("XDG_CONFIG_HOME", "/login/config");
        }
        let session = SessionEnvironment::for_ewmh_login();
        assert_eq!(session.values["ARINN_LOGIN_STATE_HOME"], "/login/state");
        assert_eq!(session.values["ARINN_LOGIN_CONFIG_HOME"], "/login/config");
        unsafe {
            env::remove_var("HOME");
            env::remove_var("XDG_STATE_HOME");
            env::remove_var("XDG_CONFIG_HOME");
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
