// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Platform side of `SystemControl1` (ADR-0108): power actions through
//! systemd-logind on the system bus and the default sink's volume and mute
//! through the PulseAudio protocol (`pactl`). Both are freedesktop services,
//! so every platform backend uses them. Arinn never asks for authorization
//! interactively and never overrides a block inhibitor.
use arinn_protocol::{
    system_control::{
        Availability, Inhibitor, Lock, MAX_INHIBITORS, MAX_LEVEL, MAX_WHO_BYTES, MAX_WHY_BYTES,
        PowerAction, PowerState, Volume, bounded_text, stepped_level,
    },
    system_status::Audio,
    validation::ErrorCode,
};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::process::Command;
use zbus::Connection;

/// Reads: `Can*`, `ListInhibitors`, `pactl get-*`.
const READ_TIMEOUT: Duration = Duration::from_secs(2);
/// A power request: logind asks polkit before it answers.
const ACTION_TIMEOUT: Duration = Duration::from_secs(10);
/// The dedicated-session switch helper (#438, ADR-0107).
pub const SESSION_SWITCH: &str = "/usr/lib/arinn/arinn-session-switch";
/// The helper writes one file and asks the desktop to log out.
const SWITCH_TIMEOUT: Duration = Duration::from_secs(10);

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Login {
    fn can_suspend(&self) -> zbus::Result<String>;
    fn can_reboot(&self) -> zbus::Result<String>;
    fn can_power_off(&self) -> zbus::Result<String>;
    fn suspend(&self, interactive: bool) -> zbus::Result<()>;
    fn reboot(&self, interactive: bool) -> zbus::Result<()>;
    fn power_off(&self, interactive: bool) -> zbus::Result<()>;
    fn list_inhibitors(&self) -> zbus::Result<Vec<(String, String, String, String, u32, u32)>>;
}

/// logind's `Can*` answer for an action with no block inhibitor.
pub fn availability_from_can(answer: &str) -> Availability {
    match answer {
        "yes" => Availability::Available,
        // Arinn never asks interactively, so "challenge" is a refusal too.
        "no" | "challenge" => Availability::Denied,
        "na" => Availability::Unsupported,
        _ => Availability::Unknown,
    }
}

/// One `ListInhibitors` record `(what, who, why, mode, uid, pid)` as the
/// inhibitors it contributes: only `block` mode, only the `sleep` and
/// `shutdown` locks, bounded texts. User and process IDs are dropped.
pub fn inhibitors_from_record(what: &str, who: &str, why: &str, mode: &str) -> Vec<Inhibitor> {
    if mode != "block" {
        return vec![];
    }
    what.split(':')
        .map(Lock::parse)
        .filter(|lock| *lock != Lock::Unknown)
        .map(|lock| Inhibitor {
            what: lock.as_str().into(),
            who: bounded_text(who, MAX_WHO_BYTES),
            why: bounded_text(why, MAX_WHY_BYTES),
        })
        .collect()
}

/// The error a refused logind request maps to. `blocked` says whether a
/// block inhibitor for the action exists; a permission refusal is then
/// reported as the inhibitor's, because overriding it is what was refused.
pub fn error_from_logind(name: &str, blocked: bool) -> ErrorCode {
    match name {
        "org.freedesktop.login1.BlockedByInhibitorLock"
        | "org.freedesktop.login1.OperationInProgress" => ErrorCode::Conflict,
        "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired"
        | "org.freedesktop.DBus.Error.AccessDenied"
        | "org.freedesktop.DBus.Error.AuthFailed" => {
            if blocked {
                ErrorCode::Conflict
            } else {
                ErrorCode::PermissionDenied
            }
        }
        "org.freedesktop.login1.SleepVerbNotSupported"
        | "org.freedesktop.DBus.Error.NotSupported"
        | "org.freedesktop.DBus.Error.UnknownMethod" => ErrorCode::Unsupported,
        "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.NameHasNoOwner"
        | "org.freedesktop.DBus.Error.UnknownObject"
        | "org.freedesktop.DBus.Error.Disconnected" => ErrorCode::Unavailable,
        "org.freedesktop.DBus.Error.NoReply" | "org.freedesktop.DBus.Error.Timeout" => {
            ErrorCode::Timeout
        }
        _ => ErrorCode::Internal,
    }
}

fn error_name(error: &zbus::Error) -> Option<String> {
    match error {
        zbus::Error::MethodError(name, _, _) => Some(name.to_string()),
        zbus::Error::FDO(error) => Some(zbus::DBusError::name(error.as_ref()).to_string()),
        _ => None,
    }
}

/// `pactl get-sink-volume` output in the C locale: the loudest channel in
/// percent, capped at [`MAX_LEVEL`]. `None` when no level can be read.
pub fn level_from_pactl(output: &str) -> Option<u32> {
    let line = output
        .lines()
        .find(|line| line.trim_start().starts_with("Volume:"))?;
    line.split('/')
        .filter_map(|part| part.trim().strip_suffix('%')?.trim().parse::<u32>().ok())
        .max()
        .map(|level| level.min(MAX_LEVEL))
}

/// `pactl get-sink-mute` output in the C locale.
pub fn audio_from_pactl(output: &str) -> Audio {
    crate::status_sources::audio_from_pactl(output)
}

/// The published inhibitor list: the first inhibitor of each lock, so an
/// inhibited action always names a holder, then the others in the order
/// logind listed them, without duplicates, up to [`MAX_INHIBITORS`].
pub fn bounded_inhibitors(inhibitors: Vec<Inhibitor>) -> Vec<Inhibitor> {
    let mut reserved: Vec<usize> = [Lock::Sleep, Lock::Shutdown]
        .iter()
        .filter_map(|lock| inhibitors.iter().position(|i| i.lock() == *lock))
        .collect();
    reserved.sort_unstable();
    let mut kept: Vec<Inhibitor> = reserved.iter().map(|&i| inhibitors[i].clone()).collect();
    for (index, inhibitor) in inhibitors.into_iter().enumerate() {
        if kept.len() >= MAX_INHIBITORS {
            break;
        }
        if !reserved.contains(&index) && !kept.contains(&inhibitor) {
            kept.push(inhibitor);
        }
    }
    kept
}

/// The desktop of the dedicated Arinn login session, from the
/// `ARINN_SESSION_ENTRY` its session wrapper exports; `None` for any other
/// value, including an ordinary desktop session.
pub fn session_entry(value: Option<&str>) -> Option<String> {
    match value {
        Some(entry @ ("plasma" | "xfce")) => Some(entry.to_owned()),
        _ => None,
    }
}

/// "Switch to desktop" of the dedicated Arinn session.
#[derive(Debug, Clone)]
pub struct SessionSwitch {
    /// See [`session_entry`].
    pub entry: Option<String>,
    pub helper: PathBuf,
}
impl SessionSwitch {
    /// No dedicated session: the action is unsupported.
    pub fn none() -> Self {
        Self {
            entry: None,
            helper: PathBuf::from(SESSION_SWITCH),
        }
    }
}

/// Where the platform services are.
#[derive(Debug, Clone)]
pub struct Control {
    /// System bus address; `None` is the default system bus.
    pub system_bus: Option<String>,
    pub pactl: PathBuf,
    pub session: SessionSwitch,
}
impl Control {
    pub fn system() -> Self {
        Self {
            system_bus: None,
            pactl: PathBuf::from("pactl"),
            session: SessionSwitch {
                entry: session_entry(std::env::var("ARINN_SESSION_ENTRY").ok().as_deref()),
                helper: PathBuf::from(SESSION_SWITCH),
            },
        }
    }

    /// `Available` inside the dedicated Arinn session with the helper
    /// installed, `Unsupported` otherwise.
    pub fn session_switch(&self) -> Availability {
        use std::os::unix::fs::PermissionsExt;
        let installed = std::fs::metadata(&self.session.helper)
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
        if self.session.entry.is_some() && installed {
            Availability::Available
        } else {
            Availability::Unsupported
        }
    }

    /// Ends the dedicated session; the next login starts the plain desktop.
    /// `Ok` means the desktop accepted the logout request. The helper takes
    /// fixed arguments only and restores the user's previous choice when the
    /// desktop refuses (`Unavailable`).
    pub async fn switch_to_desktop(&self) -> Result<(), ErrorCode> {
        // Never run what the read would not offer.
        let (Some(entry), Availability::Available) = (&self.session.entry, self.session_switch())
        else {
            return Err(ErrorCode::Unsupported);
        };
        let status = Command::new(&self.session.helper)
            .arg("desktop")
            .env("ARINN_SESSION_ENTRY", entry)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            // The helper's own message is not a daemon log line (§13).
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status();
        match tokio::time::timeout(SWITCH_TIMEOUT, status).await {
            Err(_) => Err(ErrorCode::Timeout),
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(ErrorCode::Unsupported)
            }
            Ok(Err(_)) => Err(ErrorCode::Internal),
            Ok(Ok(status)) if status.success() => Ok(()),
            // No supported desktop to log out of, or the desktop refused.
            Ok(Ok(status)) if status.code() == Some(1) => Err(ErrorCode::Unavailable),
            Ok(Ok(_)) => Err(ErrorCode::Internal),
        }
    }

    async fn login(&self) -> Result<LoginProxy<'static>, ErrorCode> {
        let builder = match &self.system_bus {
            Some(address) => zbus::connection::Builder::address(address.as_str()),
            None => zbus::connection::Builder::system(),
        }
        .map_err(|_| ErrorCode::Unavailable)?;
        let connection: Connection = tokio::time::timeout(READ_TIMEOUT, builder.build())
            .await
            .map_err(|_| ErrorCode::Unavailable)?
            .map_err(|_| ErrorCode::Unavailable)?;
        LoginProxy::new(&connection)
            .await
            .map_err(|_| ErrorCode::Unavailable)
    }

    /// The block inhibitors of `sleep` and `shutdown`; `None` when they
    /// cannot be read. Every record is inspected; only the published list is
    /// bounded.
    async fn inhibitors(login: &LoginProxy<'_>) -> Option<Vec<Inhibitor>> {
        let records = tokio::time::timeout(READ_TIMEOUT, login.list_inhibitors())
            .await
            .ok()?
            .ok()?;
        let inhibitors: Vec<Inhibitor> = records
            .iter()
            .flat_map(|(what, who, why, mode, _, _)| inhibitors_from_record(what, who, why, mode))
            .collect();
        Some(bounded_inhibitors(inhibitors))
    }

    /// The current availability of every power action. Read on each call:
    /// inhibitors come and go without a signal.
    pub async fn power(&self) -> PowerState {
        let Ok(login) = self.login().await else {
            return PowerState::unknown();
        };
        // Without the inhibitor list nothing can be called available: an
        // action would be refused anyway (fail closed, see `act`).
        let Some(inhibitors) = Self::inhibitors(&login).await else {
            return PowerState::unknown();
        };
        let mut state = PowerState {
            inhibitors,
            ..PowerState::unknown()
        };
        for action in PowerAction::ALL {
            let answer = match action {
                PowerAction::Suspend => {
                    tokio::time::timeout(READ_TIMEOUT, login.can_suspend()).await
                }
                PowerAction::Reboot => tokio::time::timeout(READ_TIMEOUT, login.can_reboot()).await,
                PowerAction::PowerOff => {
                    tokio::time::timeout(READ_TIMEOUT, login.can_power_off()).await
                }
            };
            let mut availability = match answer {
                Ok(Ok(answer)) => availability_from_can(&answer),
                _ => Availability::Unknown,
            };
            let held = state.inhibitors.iter().any(|i| i.lock() == action.lock());
            if held && availability != Availability::Unsupported {
                availability = Availability::Inhibited;
            }
            let value = availability.as_str().to_owned();
            match action {
                PowerAction::Suspend => state.suspend = value,
                PowerAction::Reboot => state.reboot = value,
                PowerAction::PowerOff => state.power_off = value,
            }
        }
        state
    }

    /// Asks logind for `action` without interactive authorization. `Ok`
    /// means logind accepted the request, not that it has happened.
    pub async fn act(&self, action: PowerAction) -> Result<(), ErrorCode> {
        let login = self.login().await?;
        let blocked = |inhibitors: &Option<Vec<Inhibitor>>| {
            inhibitors
                .as_ref()
                .is_some_and(|list| list.iter().any(|i| i.lock() == action.lock()))
        };
        // Arinn respects every block inhibitor, even one the user may
        // override: it never ignores an inhibitor on the user's behalf. A
        // list it cannot read refuses the action (fail closed).
        let inhibitors = Self::inhibitors(&login).await;
        if inhibitors.is_none() {
            return Err(ErrorCode::Unavailable);
        }
        if blocked(&inhibitors) {
            return Err(ErrorCode::Conflict);
        }
        let call = async {
            match action {
                PowerAction::Suspend => login.suspend(false).await,
                PowerAction::Reboot => login.reboot(false).await,
                PowerAction::PowerOff => login.power_off(false).await,
            }
        };
        match tokio::time::timeout(ACTION_TIMEOUT, call).await {
            Err(_) => Err(ErrorCode::Timeout),
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => {
                let Some(name) = error_name(&error) else {
                    return Err(ErrorCode::Unavailable);
                };
                let blocked = blocked(&Self::inhibitors(&login).await);
                Err(error_from_logind(&name, blocked))
            }
        }
    }

    fn pactl(&self) -> Command {
        let mut command = Command::new(&self.pactl);
        // pactl localizes its answers.
        command
            .env("LC_ALL", "C")
            .env_remove("LANGUAGE")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        command
    }

    /// Runs `pactl` with `args`; its standard output on success.
    async fn run(&self, args: &[&str]) -> Result<String, ErrorCode> {
        let output = self.pactl().args(args).output();
        match tokio::time::timeout(READ_TIMEOUT, output).await {
            Err(_) => Err(ErrorCode::Timeout),
            // No pactl: no PulseAudio-protocol client on this system.
            Ok(Err(_)) => Err(ErrorCode::Unavailable),
            // No server or no default sink.
            Ok(Ok(output)) if !output.status.success() => Err(ErrorCode::Unavailable),
            Ok(Ok(output)) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        }
    }

    async fn read_level(&self) -> Result<u32, ErrorCode> {
        let output = self.run(&["get-sink-volume", "@DEFAULT_SINK@"]).await?;
        level_from_pactl(&output).ok_or(ErrorCode::Unavailable)
    }

    /// The default sink's mute state and level, or why they cannot be read.
    async fn read_volume(&self) -> Result<Volume, ErrorCode> {
        let output = self.run(&["get-sink-mute", "@DEFAULT_SINK@"]).await?;
        let audio = audio_from_pactl(&output);
        if audio == Audio::Unknown {
            return Err(ErrorCode::Unavailable);
        }
        let level = self.read_level().await?;
        Ok(Volume {
            audio: audio.as_str().into(),
            level,
        })
    }

    /// The default sink's mute state and level; unknown when either cannot
    /// be read. After a change, a failed read is an error instead.
    pub async fn volume(&self) -> Volume {
        self.read_volume()
            .await
            .unwrap_or_else(|_| Volume::unknown())
    }

    pub async fn set_muted(&self, muted: bool) -> Result<Volume, ErrorCode> {
        let value = if muted { "1" } else { "0" };
        self.run(&["set-sink-mute", "@DEFAULT_SINK@", value])
            .await?;
        self.read_volume().await
    }

    pub async fn toggle_muted(&self) -> Result<Volume, ErrorCode> {
        self.run(&["set-sink-mute", "@DEFAULT_SINK@", "toggle"])
            .await?;
        self.read_volume().await
    }

    /// Changes the level by `steps` (already validated) relative to every
    /// channel, so the balance stays. The loudest channel lands on
    /// [`stepped_level`].
    pub async fn step_volume(&self, steps: i32) -> Result<Volume, ErrorCode> {
        let current = self.read_level().await?;
        let target = stepped_level(current, steps);
        if target != current {
            let change = if target > current {
                format!("+{}%", target - current)
            } else {
                format!("-{}%", current - target)
            };
            // pactl stops parsing options at its command, so "-5%" after
            // the sink is a volume, and "--" would be taken as the sink.
            self.run(&["set-sink-volume", "@DEFAULT_SINK@", &change])
                .await?;
        }
        self.read_volume().await
    }
}

#[cfg(test)]
#[path = "system_control_tests.rs"]
pub(crate) mod tests;
