// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Executable wire/staging fixture; not a production platform server.
mod common;
use arinn_protocol::{
    kwin::{self, KWinBridgeProxy},
    platform::{self, Window},
    production,
    validation::ErrorCode,
};
use std::{
    collections::BTreeMap,
    io::Write,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};
use zbus::message::Header;

#[derive(Default)]
struct State {
    generation: String,
    instance: String,
    revision: i32,
    staged: Option<(i32, i32, BTreeMap<i32, Window>)>,
    windows: Vec<Window>,
    pending: Option<String>,
}
#[derive(Clone)]
struct Fixture {
    owner: String,
    state: Arc<Mutex<State>>,
}
impl Fixture {
    fn authorize(&self, h: &Header<'_>) -> production::Result<()> {
        if h.sender().map(|s| s.as_str()) != Some(&self.owner) {
            return Err(ErrorCode::PermissionDenied.into());
        }
        Ok(())
    }
    fn generation(&self, generation: &str) -> production::Result<()> {
        platform::generation(generation)?;
        if self.state.lock().unwrap().generation != generation {
            return Err(ErrorCode::Conflict.into());
        }
        Ok(())
    }
}
#[zbus::interface(name = "org.arinn.KWinBridge1")]
impl Fixture {
    async fn connect(
        &self,
        instance: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<String> {
        self.authorize(&h)?;
        platform::generation(instance)?;
        let mut s = self.state.lock().unwrap();
        if s.instance != instance {
            *s = State {
                instance: instance.into(),
                generation: uuid::Uuid::new_v4().to_string(),
                ..State::default()
            };
        }
        Ok(s.generation.clone())
    }
    async fn begin_inventory(
        &self,
        generation: &str,
        revision: i32,
        count: i32,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        self.authorize(&h)?;
        self.generation(generation)?;
        kwin::validate_begin(generation, revision, count)?;
        let mut s = self.state.lock().unwrap();
        if revision <= s.revision {
            return Err(ErrorCode::Conflict.into());
        }
        s.staged = Some((revision, count, BTreeMap::new()));
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    async fn put_window(
        &self,
        generation: &str,
        revision: i32,
        index: i32,
        token: &str,
        desktop_id: &str,
        output: &str,
        flags: i32,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        self.authorize(&h)?;
        self.generation(generation)?;
        let mut s = self.state.lock().unwrap();
        let window = match kwin::window(token, desktop_id, output, flags) {
            Ok(window) => window,
            Err(error) => {
                s.staged = None;
                return Err(error.into());
            }
        };
        let Some((rev, count, windows)) = s.staged.as_mut() else {
            return Err(ErrorCode::Conflict.into());
        };
        if *rev != revision {
            s.staged = None;
            return Err(ErrorCode::Conflict.into());
        }
        if window.validate().is_err()
            || index < 0
            || index >= *count
            || windows.contains_key(&index)
        {
            s.staged = None;
            return Err(ErrorCode::InvalidArgument.into());
        }
        windows.insert(index, window);
        Ok(())
    }
    async fn commit_inventory(
        &self,
        generation: &str,
        revision: i32,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        self.authorize(&h)?;
        self.generation(generation)?;
        let mut s = self.state.lock().unwrap();
        let (rev, count, windows) = s.staged.take().ok_or(ErrorCode::Conflict)?;
        if rev != revision {
            return Err(ErrorCode::Conflict.into());
        }
        if windows.len() != count as usize {
            return Err(ErrorCode::InvalidArgument.into());
        }
        let windows: Vec<_> = windows.into_values().collect();
        platform::validate_windows(&windows)?;
        s.windows = windows;
        s.revision = revision;
        Ok(())
    }
    async fn next_control(
        &self,
        generation: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<Vec<String>> {
        self.authorize(&h)?;
        self.generation(generation)?;
        let mut s = self.state.lock().unwrap();
        if s.pending.is_some() {
            return Ok(vec![]);
        }
        let Some(window) = s.windows.first() else {
            return Ok(vec![]);
        };
        let id = uuid::Uuid::new_v4().to_string();
        let fields = vec![id.clone(), "activate".into(), window.token.clone()];
        s.pending = Some(id);
        Ok(fields)
    }
    async fn complete_control(
        &self,
        generation: &str,
        request_id: &str,
        error: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        self.authorize(&h)?;
        self.generation(generation)?;
        kwin::validate_completion(request_id, error)?;
        let mut s = self.state.lock().unwrap();
        if s.pending.as_deref() != Some(request_id) {
            return Err(ErrorCode::Conflict.into());
        }
        s.pending = None;
        Ok(())
    }
}
fn assert_error<T: std::fmt::Debug>(value: zbus::Result<T>, code: &str) {
    assert!(
        matches!(value, Err(zbus::Error::MethodError(name, _, _)) if name.as_str()==format!("org.arinn.Error1.{code}"))
    );
}
#[tokio::test]
async fn scalar_wire_xml_staging_authentication_and_acknowledgement() {
    let bus = common::Bus::start();
    let caller = bus.connection().await;
    let server = bus.connection().await;
    let fixture = Fixture {
        owner: caller.unique_name().unwrap().to_string(),
        state: Arc::default(),
    };
    server
        .object_server()
        .at(production::PATH, fixture.clone())
        .await
        .unwrap();
    server.request_name(production::SERVICE).await.unwrap();
    let proxy = KWinBridgeProxy::new(&caller).await.unwrap();
    let instance = uuid::Uuid::new_v4().to_string();
    let generation = proxy.connect(&instance).await.unwrap();
    assert_eq!(generation, proxy.connect(&instance).await.unwrap());
    let other = bus.connection().await;
    assert_error(
        KWinBridgeProxy::new(&other)
            .await
            .unwrap()
            .connect(&instance)
            .await,
        "PermissionDenied",
    );
    proxy.begin_inventory(&generation, 1, 1).await.unwrap();
    proxy
        .put_window(
            &generation,
            1,
            0,
            "opaque/token",
            "demo.desktop",
            "screen",
            13,
        )
        .await
        .unwrap();
    assert!(fixture.state.lock().unwrap().windows.is_empty());
    proxy.commit_inventory(&generation, 1).await.unwrap();
    assert_eq!(fixture.state.lock().unwrap().windows.len(), 1);
    proxy.begin_inventory(&generation, 2, 2).await.unwrap();
    proxy
        .put_window(&generation, 2, 0, "replacement", "", "", 4)
        .await
        .unwrap();
    assert_error(
        proxy.commit_inventory(&generation, 2).await,
        "InvalidArgument",
    );
    assert_eq!(
        fixture.state.lock().unwrap().windows[0].token,
        "opaque/token"
    );
    proxy.begin_inventory(&generation, 3, 1).await.unwrap();
    assert_error(
        proxy
            .put_window(&generation, 3, 2, "bad-index", "", "", 4)
            .await,
        "InvalidArgument",
    );
    assert_error(proxy.commit_inventory(&generation, 3).await, "Conflict");
    let control = proxy.next_control(&generation).await.unwrap();
    kwin::validate_control(&control).unwrap();
    assert_eq!(&control[1..], &["activate", "opaque/token"]);
    assert!(proxy.next_control(&generation).await.unwrap().is_empty());
    proxy
        .complete_control(&generation, &control[0], "PermissionDenied")
        .await
        .unwrap();
    assert_error(
        proxy.complete_control(&generation, &control[0], "").await,
        "Conflict",
    );
    let fresh = proxy
        .connect(&uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    assert_ne!(generation, fresh);
    assert_error(proxy.next_control(&generation).await, "Conflict");
    let xml = zbus::fdo::IntrospectableProxy::builder(&caller)
        .destination(production::SERVICE)
        .unwrap()
        .path(production::PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    let expected = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/kwin-bridge1.xml"
    );
    let script = r#"import sys, xml.etree.ElementTree as E
name = 'org.arinn.KWinBridge1'
def normalized(node):
    children = [normalized(child) for child in node]
    if node.tag in ('node', 'interface'): children.sort()
    return node.tag, tuple(sorted(node.attrib.items())), tuple(children)
actual = E.fromstring(sys.stdin.read()).find(f"interface[@name='{name}']")
expected = E.parse(sys.argv[1]).getroot().find(f"interface[@name='{name}']")
assert actual is not None and expected is not None
assert normalized(actual) == normalized(expected), 'KWin transport XML drift'
"#;
    let mut child = Command::new("python")
        .args(["-c", script, expected])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(xml.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
}
