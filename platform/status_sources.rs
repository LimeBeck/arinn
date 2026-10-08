// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sources of `SystemStatus1` (ADR-0100): network state from NetworkManager
//! on the system bus and the default sink's mute flag over the PulseAudio
//! protocol through `pactl`. Both are freedesktop services, not KDE ones, so
//! every platform backend uses them. A missing or failing source reports
//! `unknown`; nothing that identifies a network or device leaves this module.
use arinn_protocol::system_status::{Audio, Link, MAX_SIGNAL, Network};
use futures_util::StreamExt;
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::mpsc,
    time::Instant,
};
use zbus::{Connection, MatchRule, MessageStream, fdo::PropertiesProxy, zvariant::OwnedValue};

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const ACCESS_POINT: &str = "org.freedesktop.NetworkManager.AccessPoint";
const CALL_TIMEOUT: Duration = Duration::from_secs(2);
/// Bursts of change notifications become one read.
const SETTLE: Duration = Duration::from_millis(200);

/// Network fields of one snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkState {
    pub network: Network,
    pub link: Link,
    pub signal: u8,
}
impl NetworkState {
    pub const UNKNOWN: Self = Self {
        network: Network::Unknown,
        link: Link::Unknown,
        signal: 0,
    };
}

/// One source changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    Audio(Audio),
    Network(NetworkState),
}

/// NetworkManager `NMState` to `network`.
pub fn network_from_state(state: u32) -> Network {
    match state {
        50 | 60 | 70 => Network::Connected,
        10 | 20 | 30 | 40 => Network::Disconnected,
        _ => Network::Unknown,
    }
}

/// NetworkManager `PrimaryConnectionType` to `link`.
pub fn link_from_type(kind: &str) -> Link {
    match kind {
        "" => Link::Unknown,
        "802-11-wireless" => Link::Wifi,
        "802-3-ethernet" => Link::Wired,
        _ => Link::Other,
    }
}

/// Access point `Strength` in percent to the coarse level; 0 is unknown.
pub fn signal_from_strength(strength: u8) -> u8 {
    match strength {
        0 => 0,
        1..=24 => 1,
        25..=49 => 2,
        50..=74 => 3,
        _ => MAX_SIGNAL,
    }
}

/// `pactl get-sink-mute` output in the C locale.
pub fn audio_from_pactl(output: &str) -> Audio {
    match output.trim() {
        "Mute: yes" => Audio::Muted,
        "Mute: no" => Audio::Unmuted,
        _ => Audio::Unknown,
    }
}

/// A `pactl subscribe` line that can change the default sink's mute flag.
pub fn affects_mute(line: &str) -> bool {
    line.contains(" on sink #") || line.contains(" on server")
}

/// Where the sources are and how often they are re-read.
#[derive(Debug, Clone)]
pub struct Sources {
    /// System bus address; `None` is the default system bus.
    pub system_bus: Option<String>,
    pub pactl: PathBuf,
    /// Re-read interval, which also covers missed notifications and the
    /// signal level.
    pub poll: Duration,
    /// Wait before reconnecting to a source that went away.
    pub retry: Duration,
}
impl Sources {
    pub fn system() -> Self {
        Self {
            system_bus: None,
            pactl: PathBuf::from("pactl"),
            poll: Duration::from_secs(30),
            retry: Duration::from_secs(10),
        }
    }

    /// Runs both sources until `updates` is closed. Each source first reports
    /// its current state, then every change.
    pub async fn run(self, updates: mpsc::Sender<Update>) {
        tokio::join!(
            self.clone().run_audio(updates.clone()),
            self.run_network(updates),
        );
    }

    async fn run_audio(self, updates: mpsc::Sender<Update>) {
        loop {
            let audio = self.watch_audio(&updates).await;
            if updates.send(Update::Audio(audio)).await.is_err() {
                return;
            }
            tokio::time::sleep(self.retry).await;
        }
    }

    /// Follows `pactl subscribe` until it ends. Returns what to report then.
    async fn watch_audio(&self, updates: &mpsc::Sender<Update>) -> Audio {
        let Ok(mut child) = self
            .pactl_command()
            .arg("subscribe")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        else {
            return Audio::Unknown;
        };
        let Some(stdout) = child.stdout.take() else {
            return Audio::Unknown;
        };
        let mut lines = BufReader::new(stdout).lines();
        let mut due = Some(Instant::now());
        let mut poll = Instant::now() + self.poll;
        loop {
            let wake = due.map_or(poll, |d| d.min(poll));
            tokio::select! {
                line = lines.next_line() => match line {
                    Ok(Some(line)) => {
                        if affects_mute(&line) && due.is_none() {
                            due = Some(Instant::now() + SETTLE);
                        }
                    }
                    _ => return Audio::Unknown,
                },
                _ = tokio::time::sleep_until(wake) => {
                    due = None;
                    poll = Instant::now() + self.poll;
                    let audio = self.read_audio().await;
                    if updates.send(Update::Audio(audio)).await.is_err() {
                        return Audio::Unknown;
                    }
                }
            }
        }
    }

    async fn read_audio(&self) -> Audio {
        let output = self
            .pactl_command()
            .args(["get-sink-mute", "@DEFAULT_SINK@"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output();
        match tokio::time::timeout(CALL_TIMEOUT, output).await {
            Ok(Ok(output)) if output.status.success() => {
                audio_from_pactl(&String::from_utf8_lossy(&output.stdout))
            }
            _ => Audio::Unknown,
        }
    }

    fn pactl_command(&self) -> Command {
        let mut command = Command::new(&self.pactl);
        // pactl localizes "yes"/"no".
        command.env("LC_ALL", "C").env_remove("LANGUAGE");
        command
    }

    async fn run_network(self, updates: mpsc::Sender<Update>) {
        loop {
            if let Some(connection) = self.connect().await {
                self.watch_network(&connection, &updates).await;
            }
            if updates
                .send(Update::Network(NetworkState::UNKNOWN))
                .await
                .is_err()
            {
                return;
            }
            tokio::time::sleep(self.retry).await;
        }
    }

    async fn connect(&self) -> Option<Connection> {
        let builder = match &self.system_bus {
            Some(address) => zbus::connection::Builder::address(address.as_str()).ok()?,
            None => zbus::connection::Builder::system().ok()?,
        };
        tokio::time::timeout(CALL_TIMEOUT, builder.build())
            .await
            .ok()?
            .ok()
    }

    /// Follows NetworkManager until the bus connection ends.
    async fn watch_network(&self, connection: &Connection, updates: &mpsc::Sender<Update>) {
        let Some((mut wakes, _drains)) = network_signals(connection).await else {
            return;
        };
        let mut due = Some(Instant::now());
        let mut poll = Instant::now() + self.poll;
        loop {
            let wake = due.map_or(poll, |d| d.min(poll));
            tokio::select! {
                signal = wakes.recv() => match signal {
                    Some(true) => {
                        if due.is_none() {
                            due = Some(Instant::now() + SETTLE);
                        }
                    }
                    _ => return,
                },
                _ = tokio::time::sleep_until(wake) => {
                    due = None;
                    poll = Instant::now() + self.poll;
                    let state = read_network(connection).await;
                    if updates.send(Update::Network(state)).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
}

/// Aborts its task when dropped.
struct Drain(tokio::task::JoinHandle<()>);
impl Drop for Drain {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// NetworkManager's `PropertiesChanged` and its name-owner changes, drained
/// by their own tasks so a burst never fills a queue and stalls the
/// connection. Each yields `true` (coalesced) for a change and `false` when
/// its stream ends.
async fn network_signals(connection: &Connection) -> Option<(mpsc::Receiver<bool>, [Drain; 2])> {
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(NM)
        .ok()?
        .interface("org.freedesktop.DBus.Properties")
        .ok()?
        .member("PropertiesChanged")
        .ok()?
        .build();
    let changes = MessageStream::for_match_rule(rule, connection, Some(64))
        .await
        .ok()?;
    // A NetworkManager that stops or restarts while the bus stays up sends
    // no PropertiesChanged; its name-owner change triggers the read.
    let owners = zbus::fdo::DBusProxy::new(connection)
        .await
        .ok()?
        .receive_name_owner_changed_with_args(&[(0, NM)])
        .await
        .ok()?;
    let (sender, receiver) = mpsc::channel(1);
    let drain = |stream: futures_util::stream::BoxStream<'static, ()>,
                 sender: mpsc::Sender<bool>| {
        Drain(tokio::spawn(async move {
            let mut stream = stream;
            while stream.next().await.is_some() {
                let _ = sender.try_send(true);
            }
            let _ = sender.send(false).await;
        }))
    };
    let drains = [
        drain(changes.map(|_| ()).boxed(), sender.clone()),
        drain(owners.map(|_| ()).boxed(), sender),
    ];
    Some((receiver, drains))
}

async fn property(
    connection: &Connection,
    path: &str,
    interface: &str,
    name: &str,
) -> Option<OwnedValue> {
    let proxy = PropertiesProxy::builder(connection)
        .destination(NM)
        .ok()?
        .path(path)
        .ok()?
        .build()
        .await
        .ok()?;
    let interface = zbus::names::InterfaceName::try_from(interface).ok()?;
    tokio::time::timeout(CALL_TIMEOUT, proxy.get(interface, name))
        .await
        .ok()?
        .ok()
}

/// The current NetworkManager state; `unknown` when it cannot be read.
pub async fn read_network(connection: &Connection) -> NetworkState {
    let Some(state) = property(connection, NM_PATH, NM, "State").await else {
        return NetworkState::UNKNOWN;
    };
    let Ok(state) = u32::try_from(state) else {
        return NetworkState::UNKNOWN;
    };
    let network = network_from_state(state);
    if network != Network::Connected {
        return NetworkState {
            network,
            ..NetworkState::UNKNOWN
        };
    }
    let link = match property(connection, NM_PATH, NM, "PrimaryConnectionType").await {
        Some(kind) => String::try_from(kind).map_or(Link::Unknown, |k| link_from_type(&k)),
        None => Link::Unknown,
    };
    let signal = if link == Link::Wifi {
        wifi_signal(connection).await
    } else {
        0
    };
    NetworkState {
        network,
        link,
        signal,
    }
}

/// The access point strength of the primary connection's Wi-Fi device.
async fn wifi_signal(connection: &Connection) -> u8 {
    let object = |value: OwnedValue| {
        zbus::zvariant::OwnedObjectPath::try_from(value)
            .ok()
            .filter(|p| p.as_str() != "/")
    };
    let Some(primary) = property(connection, NM_PATH, NM, "PrimaryConnection")
        .await
        .and_then(object)
    else {
        return 0;
    };
    let devices = property(connection, primary.as_str(), ACTIVE, "Devices")
        .await
        .and_then(|v| Vec::<zbus::zvariant::OwnedObjectPath>::try_from(v).ok())
        .unwrap_or_default();
    // Only one device may carry the primary connection; anything else is
    // not a level this snapshot can name.
    let [device] = devices.as_slice() else {
        return 0;
    };
    let Some(point) = property(connection, device.as_str(), WIRELESS, "ActiveAccessPoint")
        .await
        .and_then(object)
    else {
        return 0;
    };
    property(connection, point.as_str(), ACCESS_POINT, "Strength")
        .await
        .and_then(|v| u8::try_from(v).ok())
        .map_or(0, signal_from_strength)
}

#[cfg(test)]
#[path = "status_sources_tests.rs"]
pub(crate) mod tests;
