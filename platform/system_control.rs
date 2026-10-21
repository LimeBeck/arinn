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
/// `ListInhibitors` records looked at; the rest are ignored.
const MAX_RECORDS: usize = 256;

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

/// Where the platform services are.
#[derive(Debug, Clone)]
pub struct Control {
    /// System bus address; `None` is the default system bus.
    pub system_bus: Option<String>,
    pub pactl: PathBuf,
}
impl Control {
    pub fn system() -> Self {
        Self {
            system_bus: None,
            pactl: PathBuf::from("pactl"),
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
    /// cannot be read.
    async fn inhibitors(login: &LoginProxy<'_>) -> Option<Vec<Inhibitor>> {
        let records = tokio::time::timeout(READ_TIMEOUT, login.list_inhibitors())
            .await
            .ok()?
            .ok()?;
        let mut inhibitors: Vec<Inhibitor> = records
            .iter()
            .take(MAX_RECORDS)
            .flat_map(|(what, who, why, mode, _, _)| inhibitors_from_record(what, who, why, mode))
            .collect();
        // Keep one of each lock first, so an inhibited action always names
        // a holder even when many inhibitors exist.
        inhibitors.sort_by_key(|i| i.what.clone());
        let mut kept: Vec<Inhibitor> = vec![];
        for lock in [Lock::Sleep, Lock::Shutdown] {
            if let Some(first) = inhibitors.iter().find(|i| i.lock() == lock) {
                kept.push(first.clone());
            }
        }
        for inhibitor in inhibitors {
            if kept.len() >= MAX_INHIBITORS {
                break;
            }
            if !kept.contains(&inhibitor) {
                kept.push(inhibitor);
            }
        }
        Some(kept)
    }

    /// The current availability of every power action. Read on each call:
    /// inhibitors come and go without a signal.
    pub async fn power(&self) -> PowerState {
        let Ok(login) = self.login().await else {
            return PowerState::unknown();
        };
        let inhibitors = Self::inhibitors(&login).await.unwrap_or_default();
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
        // override: it never ignores an inhibitor on the user's behalf.
        if blocked(&Self::inhibitors(&login).await) {
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

    async fn read_level(&self) -> Option<u32> {
        level_from_pactl(
            &self
                .run(&["get-sink-volume", "@DEFAULT_SINK@"])
                .await
                .ok()?,
        )
    }

    /// The default sink's mute state and level; unknown when either cannot
    /// be read.
    pub async fn volume(&self) -> Volume {
        let audio = match self.run(&["get-sink-mute", "@DEFAULT_SINK@"]).await {
            Ok(output) => audio_from_pactl(&output),
            Err(_) => Audio::Unknown,
        };
        let level = self.read_level().await;
        match (audio, level) {
            (Audio::Unknown, _) | (_, None) => Volume::unknown(),
            (audio, Some(level)) => Volume {
                audio: audio.as_str().into(),
                level,
            },
        }
    }

    pub async fn set_muted(&self, muted: bool) -> Result<Volume, ErrorCode> {
        let value = if muted { "1" } else { "0" };
        self.run(&["set-sink-mute", "@DEFAULT_SINK@", value])
            .await?;
        Ok(self.volume().await)
    }

    pub async fn toggle_muted(&self) -> Result<Volume, ErrorCode> {
        self.run(&["set-sink-mute", "@DEFAULT_SINK@", "toggle"])
            .await?;
        Ok(self.volume().await)
    }

    /// Changes the level by `steps` (already validated) relative to every
    /// channel, so the balance stays. The loudest channel lands on
    /// [`stepped_level`].
    pub async fn step_volume(&self, steps: i32) -> Result<Volume, ErrorCode> {
        let current = self.read_level().await.ok_or(ErrorCode::Unavailable)?;
        let target = stepped_level(current, steps);
        if target != current {
            let change = if target > current {
                format!("+{}%", target - current)
            } else {
                format!("-{}%", current - target)
            };
            // `--` keeps a negative change from being read as an option.
            self.run(&["set-sink-volume", "--", "@DEFAULT_SINK@", &change])
                .await?;
        }
        Ok(self.volume().await)
    }
}

#[cfg(test)]
#[path = "system_control_tests.rs"]
pub(crate) mod tests;
