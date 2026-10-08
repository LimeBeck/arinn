// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! ADR-0100 sources against a fake NetworkManager on a private bus and a fake
//! `pactl`. No real network or sound server is touched.
use super::*;
use std::{
    io::{BufRead, BufReader as StdBufReader},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, Command as StdCommand},
    sync::{Arc, Mutex},
};
use zbus::{object_server::SignalEmitter, zvariant::OwnedObjectPath};

#[test]
fn network_manager_values_map_to_the_contract() {
    for (state, network) in [
        (0, Network::Unknown),
        (10, Network::Disconnected),
        (20, Network::Disconnected),
        (30, Network::Disconnected),
        (40, Network::Disconnected),
        (50, Network::Connected),
        (60, Network::Connected),
        (70, Network::Connected),
        (80, Network::Unknown),
    ] {
        assert_eq!(network_from_state(state), network, "{state}");
    }
    assert_eq!(link_from_type("802-11-wireless"), Link::Wifi);
    assert_eq!(link_from_type("802-3-ethernet"), Link::Wired);
    assert_eq!(link_from_type("gsm"), Link::Other);
    assert_eq!(link_from_type(""), Link::Unknown);
    for (strength, level) in [
        (0, 0),
        (1, 1),
        (24, 1),
        (25, 2),
        (49, 2),
        (50, 3),
        (74, 3),
        (75, 4),
        (100, 4),
        (255, 4),
    ] {
        assert_eq!(signal_from_strength(strength), level, "{strength}");
    }
}

#[test]
fn pactl_output_and_events() {
    assert_eq!(audio_from_pactl("Mute: yes\n"), Audio::Muted);
    assert_eq!(audio_from_pactl("Mute: no\n"), Audio::Unmuted);
    // A localized answer means LC_ALL did not apply: unknown, not a guess.
    assert_eq!(audio_from_pactl("Mute: да\n"), Audio::Unknown);
    assert_eq!(audio_from_pactl(""), Audio::Unknown);
    assert!(affects_mute("Event 'change' on sink #56"));
    assert!(affects_mute("Event 'change' on server #-1"));
    assert!(!affects_mute("Event 'change' on sink-input #12"));
    assert!(!affects_mute("Event 'new' on client #9"));
}

const AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/3";
const DEVICE: &str = "/org/freedesktop/NetworkManager/Devices/2";
const ACTIVE_PATH: &str = "/org/freedesktop/NetworkManager/ActiveConnection/1";

#[derive(Clone)]
pub(crate) struct Fake {
    pub(crate) state: u32,
    kind: String,
    primary: String,
    devices: Vec<String>,
    strength: u8,
}
pub(crate) type Shared = Arc<Mutex<Fake>>;
fn path(value: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(value).unwrap()
}

struct Manager(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.0.lock().unwrap().state
    }
    #[zbus(property)]
    fn primary_connection(&self) -> OwnedObjectPath {
        path(&self.0.lock().unwrap().primary)
    }
    #[zbus(property)]
    fn primary_connection_type(&self) -> String {
        self.0.lock().unwrap().kind.clone()
    }
}
struct Active(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        self.0
            .lock()
            .unwrap()
            .devices
            .iter()
            .map(|d| path(d))
            .collect()
    }
}
struct Wireless;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        path(AP)
    }
}
struct Point(Shared);
#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl Point {
    #[zbus(property)]
    fn strength(&self) -> u8 {
        self.0.lock().unwrap().strength
    }
}

pub(crate) struct Bus(Child, pub(crate) String);
impl Bus {
    pub(crate) fn new() -> Self {
        let mut child = StdCommand::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        StdBufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(child, address.trim().into())
    }
    pub(crate) async fn connect(&self) -> Connection {
        zbus::connection::Builder::address(self.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
    fn stop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(crate) fn wifi() -> Fake {
    Fake {
        state: 70,
        kind: "802-11-wireless".into(),
        primary: ACTIVE_PATH.into(),
        devices: vec![DEVICE.into()],
        strength: 63,
    }
}

/// A fake NetworkManager; the returned connection owns its name.
pub(crate) async fn network_manager(bus: &Bus, fake: Fake) -> (Connection, Shared) {
    let shared = Arc::new(Mutex::new(fake));
    let service = bus.connect().await;
    let server = service.object_server();
    server.at(NM_PATH, Manager(shared.clone())).await.unwrap();
    server
        .at(ACTIVE_PATH, Active(shared.clone()))
        .await
        .unwrap();
    server.at(DEVICE, Wireless).await.unwrap();
    server.at(AP, Point(shared.clone())).await.unwrap();
    service.request_name(NM).await.unwrap();
    (service, shared)
}

pub(crate) async fn emit_state_changed(service: &Connection) {
    let iface = service
        .object_server()
        .interface::<_, Manager>(NM_PATH)
        .await
        .unwrap();
    let emitter: &SignalEmitter<'_> = iface.signal_emitter();
    iface.get().await.state_changed(emitter).await.unwrap();
}

#[tokio::test]
async fn reads_network_manager_and_names_nothing() {
    let bus = Bus::new();
    let client = bus.connect().await;
    // No NetworkManager on the bus.
    assert_eq!(read_network(&client).await, NetworkState::UNKNOWN);
    let (_service, fake) = network_manager(&bus, wifi()).await;
    let read = || read_network(&client);
    assert_eq!(
        read().await,
        NetworkState {
            network: Network::Connected,
            link: Link::Wifi,
            signal: 3,
        }
    );
    fake.lock().unwrap().strength = 0;
    assert_eq!(read().await.signal, 0);
    // Two devices on the primary connection: no level is named.
    {
        let mut f = fake.lock().unwrap();
        f.strength = 90;
        f.devices
            .push("/org/freedesktop/NetworkManager/Devices/9".into());
    }
    assert_eq!(read().await.signal, 0);
    *fake.lock().unwrap() = Fake {
        kind: "802-3-ethernet".into(),
        ..wifi()
    };
    assert_eq!(
        read().await,
        NetworkState {
            network: Network::Connected,
            link: Link::Wired,
            signal: 0,
        }
    );
    *fake.lock().unwrap() = Fake {
        state: 20,
        kind: String::new(),
        primary: "/".into(),
        devices: vec![],
        ..wifi()
    };
    assert_eq!(
        read().await,
        NetworkState {
            network: Network::Disconnected,
            ..NetworkState::UNKNOWN
        }
    );
    // Connected without a primary connection type: the link is unknown.
    *fake.lock().unwrap() = Fake {
        kind: String::new(),
        ..wifi()
    };
    assert_eq!(
        read().await,
        NetworkState {
            network: Network::Connected,
            ..NetworkState::UNKNOWN
        }
    );
}

/// A fake `pactl`: `get-sink-mute` prints the `mute` file, but only in the C
/// locale, and `subscribe` follows the `events` file.
pub(crate) fn fake_pactl(folder: &Path) -> PathBuf {
    std::fs::create_dir_all(folder).unwrap();
    let script = folder.join("pactl");
    std::fs::write(
        &script,
        "#!/bin/sh\n\
         dir=$(dirname \"$0\")\n\
         case \"$1\" in\n\
         get-sink-mute) echo read >> \"$dir/reads\"; [ \"$LC_ALL\" = C ] || { echo 'Mute: да'; exit 0; }; cat \"$dir/mute\" ;;\n\
         subscribe) [ -e \"$dir/no-subscribe\" ] && exit 1; exec tail -n +1 -f \"$dir/events\" ;;\n\
         *) exit 2 ;;\n\
         esac\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(folder.join("mute"), "Mute: no\n").unwrap();
    std::fs::write(folder.join("events"), "").unwrap();
    script
}
pub(crate) fn pactl_event(folder: &Path, mute: &str, event: &str) {
    use std::io::Write;
    std::fs::write(folder.join("mute"), format!("Mute: {mute}\n")).unwrap();
    let mut events = std::fs::OpenOptions::new()
        .append(true)
        .open(folder.join("events"))
        .unwrap();
    writeln!(events, "{event}").unwrap();
}

fn sources(system_bus: &str, pactl: PathBuf) -> Sources {
    Sources {
        system_bus: Some(system_bus.into()),
        pactl,
        poll: Duration::from_secs(60),
        retry: Duration::from_millis(200),
    }
}
async fn next(updates: &mut mpsc::Receiver<Update>, wanted: impl Fn(&Update) -> bool) -> Update {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let update = updates.recv().await.unwrap();
            if wanted(&update) {
                return update;
            }
        }
    })
    .await
    .expect("expected update")
}
fn is_audio(update: &Update) -> bool {
    matches!(update, Update::Audio(_))
}
fn is_network(update: &Update) -> bool {
    matches!(update, Update::Network(_))
}

#[tokio::test]
async fn mute_follows_the_sound_server_and_fails_to_unknown() {
    let folder = std::env::temp_dir().join(format!("arinn-pactl-{}", uuid::Uuid::new_v4()));
    let pactl = fake_pactl(&folder);
    let (sender, mut updates) = mpsc::channel(64);
    let task = tokio::spawn(sources("unix:path=/nonexistent/arinn-bus", pactl).run(sender));
    assert_eq!(
        next(&mut updates, is_audio).await,
        Update::Audio(Audio::Unmuted)
    );
    pactl_event(&folder, "yes", "Event 'change' on sink #56");
    assert_eq!(
        next(&mut updates, is_audio).await,
        Update::Audio(Audio::Muted)
    );
    // An event that cannot change the sink is not followed; the next sink
    // event reads the current flag.
    pactl_event(&folder, "no", "Event 'change' on client #9");
    tokio::time::sleep(Duration::from_millis(500)).await;
    while let Ok(update) = updates.try_recv() {
        assert!(!is_audio(&update), "{update:?}");
    }
    pactl_event(&folder, "no", "Event 'change' on server #-1");
    assert_eq!(
        next(&mut updates, is_audio).await,
        Update::Audio(Audio::Unmuted)
    );
    // A burst of sink events is one read.
    let reads = || {
        std::fs::read_to_string(folder.join("reads"))
            .unwrap_or_default()
            .lines()
            .count()
    };
    let before = reads();
    pactl_event(
        &folder,
        "yes",
        &["Event 'change' on sink #56"; 5].join("\n"),
    );
    assert_eq!(
        next(&mut updates, is_audio).await,
        Update::Audio(Audio::Muted)
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(reads() - before, 1);
    task.abort();

    // `subscribe` that ends, and no pactl at all: unknown.
    std::fs::write(folder.join("no-subscribe"), "").unwrap();
    let (sender, mut updates) = mpsc::channel(64);
    let task =
        tokio::spawn(sources("unix:path=/nonexistent/arinn-bus", folder.join("pactl")).run(sender));
    assert_eq!(
        next(&mut updates, |u| *u == Update::Audio(Audio::Unknown)).await,
        Update::Audio(Audio::Unknown)
    );
    task.abort();
    let (sender, mut updates) = mpsc::channel(64);
    let task = tokio::spawn(
        sources("unix:path=/nonexistent/arinn-bus", folder.join("missing")).run(sender),
    );
    assert_eq!(
        next(&mut updates, is_audio).await,
        Update::Audio(Audio::Unknown)
    );
    task.abort();
    let _ = std::fs::remove_dir_all(&folder);
}

#[tokio::test]
async fn network_follows_network_manager_and_its_loss() {
    let mut bus = Bus::new();
    let (service, fake) = network_manager(&bus, wifi()).await;
    let (sender, mut updates) = mpsc::channel(64);
    let task = tokio::spawn(sources(&bus.1, PathBuf::from("/nonexistent/pactl")).run(sender));
    let wifi_state = Update::Network(NetworkState {
        network: Network::Connected,
        link: Link::Wifi,
        signal: 3,
    });
    assert_eq!(next(&mut updates, is_network).await, wifi_state);
    // PropertiesChanged from NetworkManager triggers a read.
    fake.lock().unwrap().state = 20;
    emit_state_changed(&service).await;
    assert_eq!(
        next(&mut updates, is_network).await,
        Update::Network(NetworkState {
            network: Network::Disconnected,
            ..NetworkState::UNKNOWN
        })
    );
    // NetworkManager stops while the bus stays up: its name-owner change
    // gives unknown at once, long before the 60 s re-read.
    drop(service);
    assert_eq!(
        next(&mut updates, is_network).await,
        Update::Network(NetworkState::UNKNOWN)
    );
    // It comes back: read again.
    let (_service, _fake) = network_manager(&bus, wifi()).await;
    assert_eq!(next(&mut updates, is_network).await, wifi_state);
    // The bus goes away: unknown, never the last value.
    bus.stop();
    assert_eq!(
        next(&mut updates, is_network).await,
        Update::Network(NetworkState::UNKNOWN)
    );
    task.abort();
}
