// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Private-bus reference contract fixtures; no production exports.
use arinn_protocol::{
    media_window::{self, *},
    production,
    return_association::WindowRef,
    validation::ErrorCode,
};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use zbus::{Connection, message::Header, zvariant::OwnedObjectPath};
const G: &str = "10000000-0000-4000-8000-000000000001";
const P: &str = "20000000-0000-4000-8000-000000000002";
const L: &str = "30000000-0000-4000-8000-000000000003";
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
    assert!(child.wait().unwrap().success());
}

struct Pending {
    id: String,
    action: String,
    token: String,
}
struct State {
    adapter: String,
    generation: String,
    revision: i32,
    tokens: Vec<String>,
    reports: Vec<(String, String)>,
    provider: String,
    presented: Option<(String, String)>,
    pending: Option<Pending>,
    supported: bool,
    launch: String,
}
impl State {
    fn resolve(&self, generation: &str, id: &str) -> production::Result<Vec<WindowRef>> {
        validate_session(generation, id)?;
        if generation != self.provider {
            return Err(ErrorCode::Conflict.into());
        }
        if id != "session" {
            return Err(ErrorCode::NotFound.into());
        }
        if self.reports.len() != self.tokens.len() {
            return Ok(vec![]);
        }
        let matches: Vec<_> = self
            .reports
            .iter()
            .filter(|(t, l)| {
                self.tokens.contains(t) && !self.launch.is_empty() && l == &self.launch
            })
            .collect();
        Ok(if matches.len() == 1 {
            vec![WindowRef {
                generation: self.generation.clone(),
                token: matches[0].0.clone(),
            }]
        } else {
            vec![]
        })
    }
}
struct Legacy;
#[zbus::interface(name = "org.arinn.Legacy1")]
impl Legacy {
    fn ping(&self) {}
}
struct Provider;
#[zbus::interface(name = "org.arinn.MediaWindowProvider1")]
impl Provider {
    fn get_launch_id(&self, generation: &str, id: &str) -> production::Result<String> {
        validate_session(generation, id)?;
        if generation != G {
            return Err(ErrorCode::Conflict.into());
        }
        if id != "session" {
            return Err(ErrorCode::NotFound.into());
        }
        Ok(L.into())
    }
}
struct Public(Arc<Mutex<State>>);
#[zbus::interface(name = "org.arinn.MediaWindow1")]
impl Public {
    fn get_window(&self, generation: &str, id: &str) -> production::Result<Vec<WindowRef>> {
        self.0.lock().unwrap().resolve(generation, id)
    }
    fn present(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> production::Result<OwnedObjectPath> {
        arinn_protocol::platform::generation(request_id)?;
        let mut state = self.0.lock().unwrap();
        let windows = state.resolve(generation, id)?;
        if windows.is_empty() {
            return Err(ErrorCode::Unavailable.into());
        }
        if !state.supported {
            return Err(ErrorCode::Unsupported.into());
        }
        state.pending = Some(Pending {
            id: request_id.into(),
            action: "present".into(),
            token: windows[0].token.clone(),
        });
        Ok("/org/arinn/Operation1/fixture".try_into().unwrap())
    }
    fn release(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> production::Result<OwnedObjectPath> {
        validate_session(generation, id)?;
        arinn_protocol::platform::generation(request_id)?;
        let mut state = self.0.lock().unwrap();
        if generation != state.provider {
            return Err(ErrorCode::Conflict.into());
        }
        if id != "session" {
            return Err(ErrorCode::NotFound.into());
        }
        let token = state
            .presented
            .as_ref()
            .map(|(_, t)| t.clone())
            .filter(|t| state.tokens.contains(t))
            .unwrap_or_default();
        state.pending = Some(Pending {
            id: request_id.into(),
            action: "release".into(),
            token,
        });
        Ok("/org/arinn/Operation1/fixture".try_into().unwrap())
    }
    #[zbus(signal)]
    async fn changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        generation: &str,
    ) -> zbus::Result<()>;
}
struct Bridge(Arc<Mutex<State>>);
#[zbus::interface(name = "org.arinn.KWinMedia1")]
impl Bridge {
    fn report(
        &self,
        generation: &str,
        revision: i32,
        token: &str,
        launch_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        let mut s = self.0.lock().unwrap();
        if h.sender().map(|v| v.as_str()) != Some(s.adapter.as_str()) {
            return Err(ErrorCode::PermissionDenied.into());
        }
        media_window::validate_report(generation, revision, token, launch_id)?;
        if generation != s.generation || revision != s.revision {
            return Err(ErrorCode::Conflict.into());
        }
        if !s.tokens.iter().any(|v| v == token) {
            return Err(ErrorCode::NotFound.into());
        }
        s.reports.retain(|(t, _)| t != token);
        s.reports.push((token.into(), launch_id.into()));
        Ok(())
    }
    fn next_control(
        &self,
        generation: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<Vec<String>> {
        let mut state = self.0.lock().unwrap();
        if h.sender().map(|v| v.as_str()) != Some(state.adapter.as_str()) {
            return Err(ErrorCode::PermissionDenied.into());
        }
        arinn_protocol::platform::generation(generation)?;
        if generation != state.generation {
            return Err(ErrorCode::Conflict.into());
        }
        state.supported = true;
        Ok(state
            .pending
            .as_ref()
            .map(|p| vec![p.id.clone(), p.action.clone(), p.token.clone()])
            .unwrap_or_default())
    }
    fn complete_control(
        &self,
        generation: &str,
        request_id: &str,
        error: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        let mut state = self.0.lock().unwrap();
        if h.sender().map(|v| v.as_str()) != Some(state.adapter.as_str()) {
            return Err(ErrorCode::PermissionDenied.into());
        }
        arinn_protocol::platform::generation(generation)?;
        arinn_protocol::kwin::validate_completion(request_id, error)?;
        if generation != state.generation
            || state.pending.as_ref().is_none_or(|p| p.id != request_id)
        {
            return Err(ErrorCode::Conflict.into());
        }
        let pending = state.pending.take().unwrap();
        if error.is_empty() {
            if pending.action == "present" {
                if !state.tokens.contains(&pending.token) {
                    return Err(ErrorCode::NotFound.into());
                }
                state.presented = Some((state.provider.clone(), pending.token));
            } else {
                state.presented = None;
            }
        }
        Ok(())
    }
}
#[tokio::test]
async fn live_xml_identity_authentication_and_lifetimes() {
    tokio::time::timeout(Duration::from_secs(15), run())
        .await
        .unwrap();
}
async fn run() {
    let bus = Bus::new();
    let server = bus.connect().await;
    let adapter = bus.connect().await;
    let client = bus.connect().await;
    server.object_server().at("/media", Legacy).await.unwrap();
    server.request_name(production::SERVICE).await.unwrap();
    let public = MediaWindowProxy::builder(&client)
        .path("/media")
        .unwrap()
        .build()
        .await
        .unwrap();
    assert!(public.get_window(G, "session").await.is_err());
    let state = Arc::new(Mutex::new(State {
        adapter: adapter.unique_name().unwrap().to_string(),
        generation: P.into(),
        revision: 1,
        tokens: vec!["one".into(), "two".into()],
        reports: vec![],
        provider: G.into(),
        presented: None,
        pending: None,
        supported: false,
        launch: L.into(),
    }));
    server
        .object_server()
        .at("/media", Public(state.clone()))
        .await
        .unwrap();
    server
        .object_server()
        .at(production::PATH, Bridge(state.clone()))
        .await
        .unwrap();
    server
        .object_server()
        .at(production::PROVIDER_PATH, Provider)
        .await
        .unwrap();
    for (path, file, interface) in [
        ("/media", "media-window1.xml", INTERFACE),
        (production::PATH, "kwin-media1.xml", BRIDGE_INTERFACE),
        (
            production::PROVIDER_PATH,
            "media-window-provider1.xml",
            PROVIDER_INTERFACE,
        ),
    ] {
        let xml = zbus::fdo::IntrospectableProxy::builder(&client)
            .destination(production::SERVICE)
            .unwrap()
            .path(path)
            .unwrap()
            .build()
            .await
            .unwrap()
            .introspect()
            .await
            .unwrap();
        compare_xml(&xml, file, interface);
    }
    let provider = MediaWindowProviderProxy::builder(&client)
        .destination(production::SERVICE)
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(provider.get_launch_id(G, "session").await.unwrap(), L);
    assert!(provider.get_launch_id(P, "session").await.is_err());
    assert!(public.get_window(G, "wrong-session").await.is_err());
    assert!(public.get_window(P, "session").await.is_err());
    let bridge = KWinMediaProxy::new(&adapter).await.unwrap();
    assert!(bridge.next_control(G).await.is_err());
    assert!(bridge.next_control(P).await.unwrap().is_empty());
    let untrusted = KWinMediaProxy::new(&client).await.unwrap();
    assert!(untrusted.report(P, 1, "one", L).await.is_err());
    assert!(untrusted.next_control(P).await.is_err());
    assert!(untrusted.complete_control(P, L, "").await.is_err());
    assert!(bridge.complete_control(P, L, "").await.is_err());
    assert!(bridge.report(P, 2, "one", L).await.is_err());
    assert!(bridge.report(G, 1, "one", L).await.is_err());
    assert!(bridge.report(P, 1, "absent", L).await.is_err());
    assert!(public.get_window(G, "session").await.unwrap().is_empty());
    assert!(public.present(G, "session", L).await.is_err());
    bridge.report(P, 1, "one", L).await.unwrap();
    // A partial observation may conceal a duplicate: no resolution yet.
    assert!(public.get_window(G, "session").await.unwrap().is_empty());
    bridge.report(P, 1, "two", "").await.unwrap();
    let w = public.get_window(G, "session").await.unwrap();
    validate_window(&w).unwrap();
    assert_eq!(w[0].token, "one");
    public.present(G, "session", L).await.unwrap();
    let command = bridge.next_control(P).await.unwrap();
    validate_control(&command).unwrap();
    assert_eq!(command[1], "present");
    // Fullscreen/focus field update keeps exact identity. A later ack is valid.
    {
        let mut state = state.lock().unwrap();
        state.revision = 2;
        state.reports.clear();
    }
    bridge.report(P, 2, "one", L).await.unwrap();
    bridge.report(P, 2, "two", "").await.unwrap();
    bridge.complete_control(P, L, "").await.unwrap();
    assert!(state.lock().unwrap().presented.is_some());
    assert!(bridge.complete_control(P, L, "").await.is_err());
    bridge.report(P, 2, "two", L).await.unwrap();
    assert!(public.get_window(G, "session").await.unwrap().is_empty());
    bridge.report(P, 2, "two", "").await.unwrap();
    assert_eq!(
        public.get_window(G, "session").await.unwrap()[0].token,
        "one"
    );
    {
        let mut s = state.lock().unwrap();
        s.revision = 3;
        s.tokens = vec!["remapped".into()];
        s.reports.clear();
    }
    // Stop clears launch identity; removal cannot prevent teardown.
    state.lock().unwrap().launch.clear();
    assert!(state.lock().unwrap().presented.is_some());
    public.release(G, "session", L).await.unwrap();
    let command = bridge.next_control(P).await.unwrap();
    assert_eq!(command[1], "release");
    assert!(command[2].is_empty());
    bridge.complete_control(P, L, "").await.unwrap();
    assert!(state.lock().unwrap().presented.is_none());
    public.release(G, "session", L).await.unwrap();
    assert!(public.get_window(G, "session").await.unwrap().is_empty());
    assert!(bridge.report(P, 1, "one", L).await.is_err());
    state.lock().unwrap().launch = L.into();
    bridge.report(P, 3, "remapped", L).await.unwrap();
    assert_eq!(
        public.get_window(G, "session").await.unwrap()[0].token,
        "remapped"
    );
    {
        let mut s = state.lock().unwrap();
        s.provider = P.into();
        s.reports.clear();
    }
    assert!(public.get_window(G, "session").await.is_err());
    assert!(public.get_window(P, "session").await.unwrap().is_empty());
    {
        let mut s = state.lock().unwrap();
        s.generation = G.into();
        s.adapter = client.unique_name().unwrap().to_string();
        s.reports.clear();
    }
    assert!(bridge.report(P, 3, "remapped", L).await.is_err());
}
