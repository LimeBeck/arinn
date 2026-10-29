// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Contract fixtures only (ADR-0108): no production producer or shell
//! consumer. The live export must match the published XML, and refusals use
//! the typed `org.arinn.Error1` names.
use arinn_protocol::{
    production,
    system_control::{
        self, Availability, Inhibitor, PowerAction, PowerState, SystemControlApiProxy, Volume,
    },
    validation::ErrorCode,
};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use zbus::Connection;

struct Legacy;
#[zbus::interface(name = "org.arinn.ContractFixture1")]
impl Legacy {
    fn ping(&self) {}
}

#[derive(Default)]
struct State {
    refuse: Option<ErrorCode>,
    calls: Vec<String>,
    volume: u32,
    muted: bool,
}
struct Daemon(Arc<Mutex<State>>);
impl Daemon {
    fn act(&self, name: &str) -> production::Result<()> {
        let mut state = self.0.lock().unwrap();
        state.calls.push(name.into());
        match state.refuse {
            Some(code) => Err(code.into()),
            None => Ok(()),
        }
    }
    fn volume(&self) -> Volume {
        let state = self.0.lock().unwrap();
        Volume {
            audio: if state.muted { "muted" } else { "unmuted" }.into(),
            level: state.volume,
        }
    }
}
#[zbus::interface(name = "org.arinn.SystemControl1")]
impl Daemon {
    fn get_power(&self) -> production::Result<(PowerState,)> {
        Ok((PowerState {
            suspend: "inhibited".into(),
            reboot: "available".into(),
            power_off: "denied".into(),
            inhibitors: vec![Inhibitor {
                what: "sleep".into(),
                who: "Fixture player".into(),
                why: "Playing".into(),
            }],
        },))
    }
    fn suspend(&self) -> production::Result<()> {
        self.act("suspend")
    }
    fn reboot(&self) -> production::Result<()> {
        self.act("reboot")
    }
    fn power_off(&self) -> production::Result<()> {
        self.act("power-off")
    }
    fn get_volume(&self) -> production::Result<(Volume,)> {
        Ok((self.volume(),))
    }
    fn set_muted(&self, muted: bool) -> production::Result<(Volume,)> {
        self.0.lock().unwrap().muted = muted;
        Ok((self.volume(),))
    }
    fn toggle_muted(&self) -> production::Result<(Volume,)> {
        let mut state = self.0.lock().unwrap();
        state.muted = !state.muted;
        drop(state);
        Ok((self.volume(),))
    }
    fn step_volume(&self, steps: i32) -> production::Result<(Volume,)> {
        let steps = system_control::steps(steps)?;
        let mut state = self.0.lock().unwrap();
        state.volume = system_control::stepped_level(state.volume, steps);
        drop(state);
        Ok((self.volume(),))
    }
    fn get_session_switch(&self) -> production::Result<String> {
        Ok("available".into())
    }
    fn switch_to_desktop(&self) -> production::Result<()> {
        self.act("switch-to-desktop")
    }
}

struct Bus(Child, String);
impl Bus {
    fn new() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(child, address.trim().into())
    }
    async fn connect(&self) -> Connection {
        zbus::connection::Builder::address(self.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn compare_xml(actual: &str, filename: &str, interface: &str) {
    let script = r#"
import sys, xml.etree.ElementTree as ET
def members(root):
    interface = root.find("interface[@name='" + sys.argv[2] + "']")
    assert interface is not None
    return sorted((n.tag,n.get('name'),[(a.get('name'),a.get('type'),a.get('direction')) for a in n.findall('arg')]) for n in interface if n.tag in ('method','signal'))
assert members(ET.parse(sys.argv[1]).getroot()) == members(ET.fromstring(sys.stdin.read()))
"#;
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/plugin-api")
        .join(filename);
    let mut child = Command::new("python3")
        .args(["-c", script, file.to_str().unwrap(), interface])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(actual.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success(), "{filename}");
}

fn error_name(error: zbus::Error) -> String {
    match error {
        zbus::Error::MethodError(name, _, _) => name.to_string(),
        other => panic!("not a method error: {other:?}"),
    }
}

#[tokio::test]
async fn daemon_root_interface_wire_and_refusals() {
    tokio::time::timeout(Duration::from_secs(15), contract())
        .await
        .unwrap();
}

async fn contract() {
    let bus = Bus::new();
    let daemon = bus.connect().await;
    let shell = bus.connect().await;
    daemon
        .object_server()
        .at(production::PATH, Legacy)
        .await
        .unwrap();
    let daemon_name = daemon.unique_name().unwrap().to_string();
    let public = SystemControlApiProxy::builder(&shell)
        .destination(daemon_name.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    // A daemon without the interface: the shell reports power as unavailable.
    assert_eq!(
        error_name(public.suspend().await.unwrap_err()),
        "org.freedesktop.DBus.Error.UnknownInterface"
    );

    let state = Arc::new(Mutex::new(State {
        volume: 50,
        ..State::default()
    }));
    daemon
        .object_server()
        .at(production::PATH, Daemon(state.clone()))
        .await
        .unwrap();
    let xml = zbus::fdo::IntrospectableProxy::builder(&shell)
        .destination(daemon_name.as_str())
        .unwrap()
        .path(production::PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    compare_xml(&xml, "system-control1.xml", system_control::INTERFACE);

    let (power,) = public.get_power().await.unwrap();
    power.validate().unwrap();
    assert_eq!(
        PowerAction::ALL.map(|a| power.availability(a)),
        [
            Availability::Inhibited,
            Availability::Available,
            Availability::Denied
        ]
    );
    assert_eq!(power.inhibitors_of(PowerAction::Suspend).count(), 1);

    public.reboot().await.unwrap();
    for (code, name) in [
        (ErrorCode::Conflict, "org.arinn.Error1.Conflict"),
        (
            ErrorCode::PermissionDenied,
            "org.arinn.Error1.PermissionDenied",
        ),
        (ErrorCode::Unsupported, "org.arinn.Error1.Unsupported"),
        (ErrorCode::Unavailable, "org.arinn.Error1.Unavailable"),
        (ErrorCode::Timeout, "org.arinn.Error1.Timeout"),
        (ErrorCode::Internal, "org.arinn.Error1.Internal"),
    ] {
        state.lock().unwrap().refuse = Some(code);
        assert_eq!(error_name(public.power_off().await.unwrap_err()), name);
    }

    // Switch to desktop (#438): the reader accepts only the two producer
    // values; refusals are typed like the power actions.
    let switch = public.get_session_switch().await.unwrap();
    assert_eq!(
        system_control::session_switch(&switch),
        Availability::Available
    );
    state.lock().unwrap().refuse = Some(ErrorCode::Unavailable);
    assert_eq!(
        error_name(public.switch_to_desktop().await.unwrap_err()),
        "org.arinn.Error1.Unavailable"
    );
    state.lock().unwrap().refuse = None;
    public.switch_to_desktop().await.unwrap();

    let (volume,) = public.step_volume(2).await.unwrap();
    volume.validate().unwrap();
    assert_eq!((volume.audio.as_str(), volume.level), ("unmuted", 60));
    let (volume,) = public.toggle_muted().await.unwrap();
    assert_eq!(volume.audio, "muted");
    let (volume,) = public.set_muted(false).await.unwrap();
    assert_eq!(volume.audio, "unmuted");
    let (volume,) = public.get_volume().await.unwrap();
    assert_eq!(volume.level, 60);
    assert_eq!(
        error_name(public.step_volume(0).await.unwrap_err()),
        "org.arinn.Error1.InvalidArgument"
    );
    assert_eq!(
        state.lock().unwrap().calls,
        [
            "reboot",
            "power-off",
            "power-off",
            "power-off",
            "power-off",
            "power-off",
            "power-off",
            "switch-to-desktop",
            "switch-to-desktop"
        ]
    );
}
