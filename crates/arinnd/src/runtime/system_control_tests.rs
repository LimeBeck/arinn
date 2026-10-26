// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon export of ADR-0108 with a fake logind and a fake `pactl` on
//! private buses. Nothing here suspends or touches a sound server.
use super::*;
use crate::{
    status_sources::tests::Bus,
    storage::Paths,
    system_control::{
        SessionSwitch,
        tests::{FakeLogin, fake_pactl, fake_session_switch, logind, record},
    },
};
use arinn_protocol::{
    production::ProviderInfo,
    system_control::{Availability, SystemControlApiProxy},
};
use std::{future::Future, panic::Location, path::PathBuf, pin::pin, task::Poll, time::Duration};
use uuid::Uuid;

/// Far above any answer here, the 10 s backstops of `crate::system_control`
/// included, so only a call that never gets a reply runs into it.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// A call or setup step that fails at its own line instead of hanging the
/// test binary when no reply ever comes (#524).
pub(crate) trait Bounded: Future + Sized {
    #[track_caller]
    fn bounded(self) -> impl Future<Output = Self::Output> {
        let at = Location::caller();
        async move {
            tokio::time::timeout(CALL_TIMEOUT, self)
                .await
                .unwrap_or_else(|_| panic!("no reply within {CALL_TIMEOUT:?} at {at}"))
        }
    }
}
impl<F: Future> Bounded for F {}

/// Returns once `server`'s object server answers `client`.
///
/// zbus subscribes a connection's object server to incoming method calls in
/// a task that the first `object_server()` call spawns, and `at` does not
/// wait for it. A call that the connection reads before that task has run is
/// dropped without a reply, so its caller waits forever (#524). The daemon
/// is safe: clients find it by its well-known name, requested after every
/// export. These tests call the unique name right after the export instead,
/// so they first wait until a ping is answered; a dropped ping is sent again.
pub(crate) async fn served(client: &zbus::Connection, server: &zbus::Connection) {
    let peer = zbus::fdo::PeerProxy::builder(client)
        .destination(server.unique_name().unwrap().to_string())
        .unwrap()
        .path(production::PATH)
        .unwrap()
        .build()
        .await
        .unwrap();
    async {
        loop {
            match tokio::time::timeout(Duration::from_millis(100), peer.ping()).await {
                Ok(answer) => break answer.unwrap(),
                Err(_) => continue,
            }
        }
    }
    .bounded()
    .await
}

struct Fixture {
    core: Core,
    client: zbus::Connection,
    folder: PathBuf,
    _session: Bus,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}
async fn fixture(system: &Bus) -> Fixture {
    fixture_with(system, SessionSwitch::none()).await
}
async fn fixture_with(system: &Bus, switch: SessionSwitch) -> Fixture {
    let session = Bus::new();
    let server = session.connect().bounded().await;
    let client = session.connect().bounded().await;
    serve(session, server, client, system, switch).await
}
/// The daemon on `server` with `SystemControl1` exported and served.
async fn serve(
    session: Bus,
    server: zbus::Connection,
    client: zbus::Connection,
    system: &Bus,
    switch: SessionSwitch,
) -> Fixture {
    let folder = std::env::temp_dir().join(format!("arinn-control-{}", Uuid::new_v4()));
    let pactl = fake_pactl(&folder.join("pactl"));
    let core = Core::new(
        &server,
        Paths {
            config: folder.join("config"),
            data: folder.join("data"),
            state: folder.join("state"),
        },
        folder.clone(),
    );
    core.export_system_control(Control {
        system_bus: Some(system.1.clone()),
        pactl,
        session: switch,
    })
    .bounded()
    .await
    .unwrap();
    served(&client, &server).await;
    Fixture {
        core,
        client,
        folder,
        _session: session,
    }
}
async fn proxy<'a>(f: &Fixture, client: &zbus::Connection) -> SystemControlApiProxy<'a> {
    SystemControlApiProxy::builder(client)
        .destination(f.core.connection.unique_name().unwrap().to_string())
        .unwrap()
        .build()
        .bounded()
        .await
        .unwrap()
}
fn error_name(error: zbus::Error) -> String {
    match error {
        zbus::Error::MethodError(name, _, _) => name.to_string(),
        other => panic!("not a method error: {other:?}"),
    }
}

#[tokio::test]
async fn the_export_matches_the_xml_and_maps_logind() {
    let system = Bus::new();
    let (_logind, fake) = logind(
        &system,
        FakeLogin {
            can: ["yes".into(), "yes".into(), "challenge".into()],
            inhibitors: vec![record("shutdown", "block")],
            ..FakeLogin::default()
        },
    )
    .bounded()
    .await;
    let f = fixture(&system).await;
    let control = proxy(&f, &f.client).await;

    let live = zbus::fdo::IntrospectableProxy::builder(&f.client)
        .destination(f.core.connection.unique_name().unwrap().to_string())
        .unwrap()
        .path(production::PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .bounded()
        .await
        .unwrap();
    let file = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/system-control1.xml"
    ))
    .unwrap();
    let methods = super::super::media_routing::tests::methods;
    assert_eq!(
        methods(&live, system_control::INTERFACE),
        methods(&file, system_control::INTERFACE)
    );

    let (state,) = control.get_power().bounded().await.unwrap();
    state.validate().unwrap();
    assert_eq!(
        PowerAction::ALL.map(|a| state.availability(a)),
        [
            Availability::Available,
            Availability::Inhibited,
            Availability::Inhibited
        ]
    );
    // Accepted means logind took the request; interactive is never asked.
    control.suspend().bounded().await.unwrap();
    assert_eq!(
        fake.lock().unwrap().calls,
        vec![("suspend".to_string(), false)]
    );
    // A block inhibitor: a typed refusal, and logind is not asked.
    assert_eq!(
        error_name(control.reboot().bounded().await.unwrap_err()),
        "org.arinn.Error1.Conflict"
    );
    fake.lock().unwrap().inhibitors.clear();
    fake.lock().unwrap().refuse = Some(zbus::fdo::Error::InteractiveAuthorizationRequired);
    assert_eq!(
        error_name(control.power_off().bounded().await.unwrap_err()),
        "org.arinn.Error1.PermissionDenied"
    );
    assert_eq!(fake.lock().unwrap().calls.len(), 2);
}

#[tokio::test]
async fn volume_changes_return_the_new_state() {
    let system = Bus::new();
    let f = fixture(&system).await;
    let control = proxy(&f, &f.client).await;
    let (volume,) = control.get_volume().bounded().await.unwrap();
    assert_eq!((volume.audio.as_str(), volume.level), ("unmuted", 50));
    let (volume,) = control.step_volume(2).bounded().await.unwrap();
    assert_eq!(volume.level, 60);
    let (volume,) = control.toggle_muted().bounded().await.unwrap();
    assert_eq!((volume.audio.as_str(), volume.level), ("muted", 60));
    let (volume,) = control.set_muted(false).bounded().await.unwrap();
    assert_eq!(volume.audio, "unmuted");
    for steps in [0, 21, -21, i32::MAX] {
        assert_eq!(
            error_name(control.step_volume(steps).bounded().await.unwrap_err()),
            "org.arinn.Error1.InvalidArgument",
            "{steps}"
        );
    }
    // Without logind: power is unknown, an action is unavailable.
    let (state,) = control.get_power().bounded().await.unwrap();
    assert_eq!(state, PowerState::unknown());
    assert_eq!(
        error_name(control.suspend().bounded().await.unwrap_err()),
        "org.arinn.Error1.Unavailable"
    );
    // A sound server that goes away.
    std::fs::write(f.folder.join("pactl").join("no-server"), "").unwrap();
    assert_eq!(
        error_name(control.toggle_muted().bounded().await.unwrap_err()),
        "org.arinn.Error1.Unavailable"
    );
    let (volume,) = control.get_volume().bounded().await.unwrap();
    assert_eq!(volume, Volume::unknown());
}

/// #524: the export starts the object server's dispatcher while the
/// server's socket reader is already queued to run, so the reader goes first
/// and reads the first call before the dispatcher has subscribed to calls.
/// Unless the fixture waits until the export is served, that call is dropped
/// and never answered.
#[tokio::test]
async fn a_call_right_after_the_export_is_answered() {
    let system = Bus::new();
    let session = Bus::new();
    let server = session.connect().bounded().await;
    let client = session.connect().bounded().await;
    // A message waits in the server's socket. Yielding once lets the runtime
    // see it, which queues the server's reader, and resumes this test first.
    client
        .emit_signal(server.unique_name(), "/", "org.arinn.Test1", "Poke", &())
        .bounded()
        .await
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tokio::task::yield_now().await;

    let f = serve(session, server, client, &system, SessionSwitch::none()).await;
    let control = proxy(&f, &f.client).await;
    // Sent without yielding; it reaches the server's socket while the reader
    // is still queued ahead of the dispatcher.
    let mut call = pin!(control.get_session_switch());
    let early = std::future::poll_fn(|cx| Poll::Ready(call.as_mut().poll(cx))).await;
    assert!(early.is_pending());
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(call.bounded().await.unwrap(), "unsupported");
}

fn provider(id: &str, owner: &str) -> super::super::Provider {
    super::super::Provider {
        info: ProviderInfo {
            id: id.into(),
            plugin_id: id.into(),
            source_path: String::new(),
            name: id.into(),
            version: "0.0.1".into(),
            path: format!("/org/arinn/Daemon1/providers/p_{id}")
                .try_into()
                .unwrap(),
            capabilities: vec!["media".into()],
            runtime_state: "ready".into(),
            backend_state: "available".into(),
            generation: "20000000-0000-4000-8000-000000000002".into(),
            revision: 1,
            last_error: String::new(),
            error_at: 0,
            recovered_at: 0,
            restarts_since_daemon_start: 0,
            cumulative_restart_count: 0,
        },
        manifest: None,
        unit: format!("{id}.service"),
        token_file: std::env::temp_dir().join(format!("{id}.token")),
        token: String::new(),
        owner: owner.into(),
        cache: None,
        applications: Vec::new(),
        running: Vec::new(),
        tab_cache: None,
        session_cache: None,
        sessions: Vec::new(),
        features: Vec::new(),
        library_cache: None,
        collections: Vec::new(),
        items: Vec::new(),
        resume_cache: None,
        resumes: Vec::new(),
        settings_schema: None,
        windows: Vec::new(),
        tabs: Vec::new(),
        started: std::time::Instant::now(),
        enabled: true,
    }
}

#[tokio::test]
async fn a_provider_connection_is_refused() {
    let system = Bus::new();
    let (_logind, fake) = logind(
        &system,
        FakeLogin {
            can: ["yes".into(), "yes".into(), "yes".into()],
            ..FakeLogin::default()
        },
    )
    .bounded()
    .await;
    let f = fixture(&system).await;
    let plugin = f._session.connect().bounded().await;
    let owner = plugin.unique_name().unwrap().to_string();
    f.core
        .state
        .lock()
        .await
        .providers
        .insert("fixture".into(), provider("fixture", &owner));
    let control = proxy(&f, &plugin).await;
    let denied = "org.arinn.Error1.PermissionDenied";
    assert_eq!(
        error_name(control.get_power().bounded().await.unwrap_err()),
        denied
    );
    assert_eq!(
        error_name(control.suspend().bounded().await.unwrap_err()),
        denied
    );
    assert_eq!(
        error_name(control.reboot().bounded().await.unwrap_err()),
        denied
    );
    assert_eq!(
        error_name(control.power_off().bounded().await.unwrap_err()),
        denied
    );
    assert_eq!(
        error_name(control.get_volume().bounded().await.unwrap_err()),
        denied
    );
    assert_eq!(
        error_name(control.set_muted(true).bounded().await.unwrap_err()),
        denied
    );
    assert_eq!(
        error_name(control.toggle_muted().bounded().await.unwrap_err()),
        denied
    );
    assert_eq!(
        error_name(control.step_volume(1).bounded().await.unwrap_err()),
        denied
    );
    // Refused before its argument is checked.
    assert_eq!(
        error_name(control.step_volume(0).bounded().await.unwrap_err()),
        denied
    );
    assert_eq!(
        error_name(control.get_session_switch().bounded().await.unwrap_err()),
        denied
    );
    assert_eq!(
        error_name(control.switch_to_desktop().bounded().await.unwrap_err()),
        denied
    );
    assert!(fake.lock().unwrap().calls.is_empty());
    let calls = std::fs::read_to_string(f.folder.join("pactl").join("calls")).unwrap_or_default();
    assert!(calls.is_empty(), "{calls}");

    // The shell, which owns no provider, is served.
    let shell = proxy(&f, &f.client).await;
    shell.suspend().bounded().await.unwrap();
    shell.get_volume().bounded().await.unwrap();
}

#[tokio::test]
async fn a_second_power_request_meanwhile_is_refused_not_queued() {
    let system = Bus::new();
    let (_logind, fake) = logind(
        &system,
        FakeLogin {
            can: ["yes".into(), "yes".into(), "yes".into()],
            hold: Duration::from_millis(500),
            ..FakeLogin::default()
        },
    )
    .bounded()
    .await;
    let f = fixture(&system).await;
    let control = proxy(&f, &f.client).await;
    let other = f._session.connect().bounded().await;
    let second = proxy(&f, &other).await;
    let (first, overlapping) = tokio::join!(control.suspend().bounded(), async {
        tokio::time::sleep(Duration::from_millis(150)).await;
        second.power_off().bounded().await
    });
    first.unwrap();
    assert_eq!(
        error_name(overlapping.unwrap_err()),
        "org.arinn.Error1.Conflict"
    );
    // logind only ever saw the first request.
    assert_eq!(
        fake.lock().unwrap().calls,
        vec![("suspend".to_string(), false)]
    );
}

#[tokio::test]
async fn switch_to_desktop_only_inside_the_arinn_session() {
    let system = Bus::new();
    // An ordinary desktop session: offered as unsupported, refused typed.
    let f = fixture(&system).await;
    let control = proxy(&f, &f.client).await;
    assert_eq!(
        control.get_session_switch().bounded().await.unwrap(),
        "unsupported"
    );
    assert_eq!(
        error_name(control.switch_to_desktop().bounded().await.unwrap_err()),
        "org.arinn.Error1.Unsupported"
    );

    // The dedicated session: the helper runs once per request.
    let helpers = std::env::temp_dir().join(format!("arinn-switch-{}", Uuid::new_v4()));
    let helper = fake_session_switch(&helpers);
    let f = fixture_with(
        &system,
        SessionSwitch {
            entry: Some("plasma".into()),
            helper,
        },
    )
    .bounded()
    .await;
    let control = proxy(&f, &f.client).await;
    assert_eq!(
        control.get_session_switch().bounded().await.unwrap(),
        "available"
    );
    control.switch_to_desktop().bounded().await.unwrap();
    let calls = || std::fs::read_to_string(helpers.join("calls")).unwrap();
    assert_eq!(calls(), "desktop entry=plasma\n");
    std::fs::write(helpers.join("exit"), "1").unwrap();
    assert_eq!(
        error_name(control.switch_to_desktop().bounded().await.unwrap_err()),
        "org.arinn.Error1.Unavailable"
    );
    let _ = std::fs::remove_dir_all(&helpers);
}

#[tokio::test]
async fn the_switch_and_a_power_request_exclude_each_other() {
    let system = Bus::new();
    let (_logind, fake) = logind(
        &system,
        FakeLogin {
            can: ["yes".into(), "yes".into(), "yes".into()],
            hold: Duration::from_millis(500),
            ..FakeLogin::default()
        },
    )
    .bounded()
    .await;
    let helpers = std::env::temp_dir().join(format!("arinn-switch-{}", Uuid::new_v4()));
    let helper = fake_session_switch(&helpers);
    let f = fixture_with(
        &system,
        SessionSwitch {
            entry: Some("plasma".into()),
            helper,
        },
    )
    .bounded()
    .await;
    let control = proxy(&f, &f.client).await;
    let other = f._session.connect().bounded().await;
    let second = proxy(&f, &other).await;
    let calls = || std::fs::read_to_string(helpers.join("calls")).unwrap_or_default();

    // A power request is running: the switch is refused, not queued.
    let (first, overlapping) = tokio::join!(control.suspend().bounded(), async {
        tokio::time::sleep(Duration::from_millis(150)).await;
        second.switch_to_desktop().bounded().await
    });
    first.unwrap();
    assert_eq!(
        error_name(overlapping.unwrap_err()),
        "org.arinn.Error1.Conflict"
    );
    assert_eq!(calls(), "");

    // The switch is running: a power request is refused.
    std::fs::write(helpers.join("delay"), "0.5").unwrap();
    let (first, overlapping) = tokio::join!(control.switch_to_desktop().bounded(), async {
        tokio::time::sleep(Duration::from_millis(150)).await;
        second.reboot().bounded().await
    });
    first.unwrap();
    assert_eq!(
        error_name(overlapping.unwrap_err()),
        "org.arinn.Error1.Conflict"
    );
    assert_eq!(calls(), "desktop entry=plasma\n");
    assert_eq!(
        fake.lock().unwrap().calls,
        vec![("suspend".to_string(), false)]
    );
    let _ = std::fs::remove_dir_all(&helpers);
}
