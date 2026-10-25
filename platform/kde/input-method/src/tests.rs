// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The input method against a fake compositor (`zwp_input_method_v1` on a
//! socket pair, as KWin hands it over) and fake KWin, lock service and
//! daemon on a private bus.
use super::*;
use std::{
    io::{BufRead, BufReader},
    os::unix::net::UnixStream,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
};
use wayland_protocols::wp::input_method::zv1::server::{
    zwp_input_method_context_v1::{
        Request as ContextRequest, ZwpInputMethodContextV1 as ServerContext,
    },
    zwp_input_method_v1::ZwpInputMethodV1 as ServerInputMethod,
};
use wayland_server::{
    Client, DataInit, Dispatch as ServerDispatch, Display, DisplayHandle, GlobalDispatch, New,
    Resource,
    backend::{ClientData, ClientId, DisconnectReason},
    protocol::wl_keyboard::{KeyState, WlKeyboard as ServerKeyboard},
};

const S: &str = "10000000-0000-4000-8000-000000000001";
const T: &str = "20000000-0000-4000-8000-000000000002";
const U: &str = "30000000-0000-4000-8000-000000000003";

type Log = Arc<Mutex<Vec<String>>>;

/// What the compositor saw, and the commands it takes from the test.
enum Command_ {
    Activate,
    Deactivate,
    Key(u32, bool),
}

#[derive(Default)]
struct Compositor {
    log: Log,
    input_method: Option<ServerInputMethod>,
    context: Option<ServerContext>,
    keyboard: Option<ServerKeyboard>,
    serial: u32,
}

struct Data;
impl ClientData for Data {
    fn initialized(&self, _: ClientId) {}
    fn disconnected(&self, _: ClientId, _: DisconnectReason) {}
}

impl GlobalDispatch<ServerInputMethod, ()> for Compositor {
    fn bind(
        state: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        resource: New<ServerInputMethod>,
        _: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        state.input_method = Some(data_init.init(resource, ()));
    }
}
impl ServerDispatch<ServerInputMethod, ()> for Compositor {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &ServerInputMethod,
        _: <ServerInputMethod as Resource>::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
    }
}
impl ServerDispatch<ServerContext, ()> for Compositor {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &ServerContext,
        request: ContextRequest,
        _: &(),
        _: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        let entry = match request {
            ContextRequest::CommitString { serial, text } => format!("commit:{serial}:{text}"),
            ContextRequest::Keysym { sym, state, .. } => format!("keysym:{sym}:{state}"),
            ContextRequest::Key { key, state, .. } => format!("key:{key}:{state}"),
            ContextRequest::Modifiers { mods_depressed, .. } => {
                format!("modifiers:{mods_depressed}")
            }
            ContextRequest::GrabKeyboard { keyboard } => {
                let keyboard = data_init.init(keyboard, ());
                keyboard.repeat_info(0, 0);
                state.keyboard = Some(keyboard);
                "grab".into()
            }
            _ => return,
        };
        state.log.lock().unwrap().push(entry);
    }
}
impl ServerDispatch<ServerKeyboard, ()> for Compositor {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &ServerKeyboard,
        _: <ServerKeyboard as Resource>::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
    }
}

/// Runs the fake compositor on its own thread; returns the client socket
/// and the command channel.
fn compositor(log: Log) -> (UnixStream, std::sync::mpsc::Sender<Command_>) {
    let (client, server) = UnixStream::pair().unwrap();
    let (commands, receiver) = std::sync::mpsc::channel::<Command_>();
    std::thread::spawn(move || {
        let mut display: Display<Compositor> = Display::new().unwrap();
        let mut handle = display.handle();
        handle.create_global::<Compositor, ServerInputMethod, ()>(1, ());
        let client = handle.insert_client(server, Arc::new(Data)).unwrap();
        let mut state = Compositor {
            log,
            ..Compositor::default()
        };
        loop {
            if display.dispatch_clients(&mut state).is_err() {
                return;
            }
            while let Ok(command) = receiver.try_recv() {
                match command {
                    Command_::Activate => {
                        let Some(im) = &state.input_method else {
                            continue;
                        };
                        let context = client
                            .create_resource::<ServerContext, (), Compositor>(&handle, 1, ())
                            .unwrap();
                        im.activate(&context);
                        state.serial += 1;
                        context.commit_state(state.serial);
                        state.context = Some(context);
                        state.keyboard = None;
                    }
                    Command_::Deactivate => {
                        if let (Some(im), Some(context)) =
                            (&state.input_method, state.context.take())
                        {
                            im.deactivate(&context);
                            state.keyboard = None;
                        }
                    }
                    Command_::Key(code, pressed) => {
                        if let Some(keyboard) = &state.keyboard {
                            state.serial += 1;
                            keyboard.key(
                                state.serial,
                                0,
                                code,
                                if pressed {
                                    KeyState::Pressed
                                } else {
                                    KeyState::Released
                                },
                            );
                        }
                    }
                }
            }
            if display.flush_clients().is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    (client, commands)
}

struct Bus(Child, String);
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn bus() -> Bus {
    let mut child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut address = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    Bus(child, address.trim().into())
}

#[derive(Clone, Default)]
struct FakeDaemon {
    queue: Arc<Mutex<Vec<Vec<String>>>>,
    calls: Log,
    /// `Next` fails, as for a daemon that restarted or replaced the lane.
    fail: Arc<AtomicBool>,
}
#[zbus::interface(name = "org.arinn.KeyboardBridge1")]
impl FakeDaemon {
    fn connect(&self, _instance: &str) -> String {
        self.calls.lock().unwrap().push("connect".into());
        "40000000-0000-4000-8000-000000000004".into()
    }
    async fn next(&self, _generation: &str) -> zbus::fdo::Result<Vec<String>> {
        for _ in 0..20 {
            if self.fail.load(Ordering::SeqCst) {
                return Err(zbus::fdo::Error::Failed("lane ended".into()));
            }
            if let Some(record) = {
                let mut queue = self.queue.lock().unwrap();
                (!queue.is_empty()).then(|| queue.remove(0))
            } {
                return Ok(record);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        Ok(Vec::new())
    }
    fn opened(&self, _generation: &str, session: &str) {
        self.calls.lock().unwrap().push(format!("opened:{session}"));
    }
    fn input(&self, _generation: &str, _session: &str, key: &str) {
        self.calls.lock().unwrap().push(format!("input:{key}"));
    }
    fn closed(&self, _generation: &str, session: &str, reason: &str) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("closed:{session}:{reason}"));
    }
}

/// KWin's `org.kde.kwin.VirtualKeyboard`: forceActivate activates a
/// context; `active = false` deactivates it.
struct FakeKWin {
    compositor: Mutex<std::sync::mpsc::Sender<Command_>>,
    log: Log,
}
#[zbus::interface(name = "org.kde.kwin.VirtualKeyboard")]
impl FakeKWin {
    #[zbus(name = "forceActivate")]
    fn force_activate(&self) {
        self.log.lock().unwrap().push("forceActivate".into());
        let _ = self.compositor.lock().unwrap().send(Command_::Activate);
    }
    #[zbus(property, name = "active")]
    fn active(&self) -> bool {
        false
    }
    #[zbus(property, name = "active")]
    fn set_active(&self, active: bool) {
        self.log.lock().unwrap().push(format!("active:{active}"));
        if !active {
            let _ = self.compositor.lock().unwrap().send(Command_::Deactivate);
        }
    }
}

struct ScreenSaver(Arc<AtomicBool>);
#[zbus::interface(name = "org.freedesktop.ScreenSaver")]
impl ScreenSaver {
    fn get_active(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

async fn wait_for(log: &Log, wanted: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !log.lock().unwrap().iter().any(|c| c == wanted) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{wanted} expected, got {:?}", log.lock().unwrap()));
}

fn push(daemon: &FakeDaemon, record: Record) {
    daemon.queue.lock().unwrap().push(record.fields());
}

#[tokio::test]
async fn sessions_bind_the_focused_context_route_keys_and_commit_text() {
    tokio::time::timeout(Duration::from_secs(60), scenario())
        .await
        .unwrap();
}

/// The input method running against the fakes.
struct Env {
    _bus: Bus,
    surface: Log,
    commands: std::sync::mpsc::Sender<Command_>,
    daemon: FakeDaemon,
    kwin_log: Log,
    locked: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<Result<()>>,
    _connections: Vec<zbus::Connection>,
}

impl Env {
    async fn new(lock_tick: Duration) -> Self {
        let bus = bus();
        let surface = Log::default();
        let (socket, commands) = compositor(surface.clone());
        let daemon = FakeDaemon::default();
        let daemon_connection = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .serve_at("/org/arinn/Daemon1", daemon.clone())
            .unwrap()
            .build()
            .await
            .unwrap();
        let kwin_log = Log::default();
        let kwin = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .serve_at(
                "/VirtualKeyboard",
                FakeKWin {
                    compositor: Mutex::new(commands.clone()),
                    log: kwin_log.clone(),
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let locked = Arc::new(AtomicBool::new(false));
        let _screensaver = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .name("org.freedesktop.ScreenSaver")
            .unwrap()
            .serve_at("/ScreenSaver", ScreenSaver(locked.clone()))
            .unwrap()
            .build()
            .await
            .unwrap();
        let helper = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let wayland = Connection::from_socket(socket).unwrap();
        let endpoints = Endpoints {
            bus: helper,
            daemon: daemon_connection.unique_name().unwrap().to_string(),
            kwin: kwin.unique_name().unwrap().to_string(),
            activation: Duration::from_millis(500),
            lock_tick,
        };
        let task = tokio::spawn(run(wayland, endpoints));
        wait_for(&daemon.calls, "connect").await;
        Env {
            _bus: bus,
            surface,
            commands,
            daemon,
            kwin_log,
            locked,
            task,
            _connections: vec![daemon_connection, kwin, _screensaver],
        }
    }

    /// Opens session `s` on a forced context and waits for the grab.
    async fn open(&self, s: &str) {
        push(&self.daemon, Record::Open(s.into()));
        wait_for(&self.daemon.calls, &format!("opened:{s}")).await;
    }
}

async fn scenario() {
    let Env {
        _bus: bus,
        surface,
        commands,
        daemon,
        kwin_log,
        locked,
        task,
        _connections,
    } = Env::new(Duration::from_millis(500)).await;
    let _ = &bus;

    // A text-input context of some application without a session shows
    // nothing and grabs nothing (no auto-show).
    commands.send(Command_::Activate).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!surface.lock().unwrap().contains(&"grab".to_string()));
    commands.send(Command_::Deactivate).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The key: the helper asks KWin for a context of the focused window and
    // grabs the keyboard through it.
    push(&daemon, Record::Open(S.into()));
    wait_for(&kwin_log, "forceActivate").await;
    wait_for(&surface, "grab").await;
    wait_for(&daemon.calls, &format!("opened:{S}")).await;
    // Panel keys go to the shell; other keys back to the application.
    commands.send(Command_::Key(103, true)).unwrap();
    commands.send(Command_::Key(103, false)).unwrap();
    commands.send(Command_::Key(30, true)).unwrap();
    commands.send(Command_::Key(14, true)).unwrap();
    wait_for(&daemon.calls, "input:up").await;
    wait_for(&surface, "key:30:1").await;
    wait_for(&surface, "key:14:1").await;
    assert!(
        !surface
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.starts_with("key:103"))
    );
    // Text is committed with the context's serial; editing keys as keysyms.
    push(&daemon, Record::Type(S.into(), "Привет".into()));
    push(&daemon, Record::Press(S.into(), "backspace".into()));
    wait_for(&surface, "commit:2:Привет").await;
    wait_for(&surface, "keysym:65288:1").await;
    wait_for(&surface, "keysym:65288:0").await;

    // A lock ends the session: nothing more is committed, keys are handed
    // back.
    locked.store(true, Ordering::SeqCst);
    push(&daemon, Record::Type(S.into(), "secret".into()));
    wait_for(&daemon.calls, &format!("closed:{S}:locked")).await;
    wait_for(&kwin_log, "active:false").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!surface.lock().unwrap().iter().any(|e| e.contains("secret")));

    // Locked: a new session is refused before anything is bound.
    push(&daemon, Record::Open(T.into()));
    wait_for(&daemon.calls, &format!("closed:{T}:locked")).await;
    locked.store(false, Ordering::SeqCst);

    // The target loses focus: KWin deactivates the context.
    push(&daemon, Record::Open(T.into()));
    wait_for(&daemon.calls, &format!("opened:{T}")).await;
    commands.send(Command_::Deactivate).unwrap();
    wait_for(&daemon.calls, &format!("closed:{T}:focus")).await;
    push(&daemon, Record::Type(T.into(), "late".into()));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!surface.lock().unwrap().iter().any(|e| e.contains("late")));

    // The shell closes a session: the grab ends, nothing is reported.
    push(&daemon, Record::Open(U.into()));
    wait_for(&daemon.calls, &format!("opened:{U}")).await;
    let before = daemon.calls.lock().unwrap().len();
    push(&daemon, Record::Close(U.into()));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let log = kwin_log.lock().unwrap().clone();
    assert_eq!(
        log.iter().filter(|e| *e == "active:false").count(),
        2,
        "{log:?}"
    );
    assert_eq!(daemon.calls.lock().unwrap().len(), before);
    task.abort();
}

/// Review of #484 (Codex P1): when the daemon lane ends with a session open,
/// the grab is released at once (`active = false`), no key the old grab
/// queued reaches the application late, and the next connection opens
/// sessions as before.
#[tokio::test]
async fn a_lost_daemon_lane_releases_the_grab_and_drops_queued_keys() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let env = Env::new(Duration::from_secs(3600)).await;
        env.open(S).await;
        wait_for(&env.surface, "grab").await;
        // The lane ends; keys keep coming from the remote meanwhile.
        env.daemon.fail.store(true, Ordering::SeqCst);
        // Panel keys only: before the release they still go to the shell,
        // and none may reach the application afterwards.
        for code in [103, 28] {
            env.commands.send(Command_::Key(code, true)).unwrap();
        }
        wait_for(&env.kwin_log, "active:false").await;
        for code in [103, 28, 30] {
            env.commands.send(Command_::Key(code, true)).unwrap();
        }
        // The daemon comes back; the helper connects again.
        env.daemon.fail.store(false, Ordering::SeqCst);
        tokio::time::timeout(Duration::from_secs(10), async {
            while env
                .daemon
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|c| *c == "connect")
                .count()
                < 2
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let late: Vec<String> = env
            .surface
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.starts_with("key:"))
            .cloned()
            .collect();
        assert!(
            late.is_empty(),
            "keys reached the application late: {late:?}"
        );
        // A new session binds a fresh context and types.
        env.open(U).await;
        push(&env.daemon, Record::Type(U.into(), "ok".into()));
        tokio::time::timeout(Duration::from_secs(10), async {
            while !env
                .surface
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.starts_with("commit:") && e.ends_with(":ok"))
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        env.task.abort();
    })
    .await
    .unwrap();
}

/// Mutation I4 of the #484 review: the 500 ms lock tick alone ends an open
/// session, with no record from the daemon.
#[tokio::test]
async fn the_lock_tick_ends_an_open_session() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let env = Env::new(Duration::from_millis(100)).await;
        env.open(S).await;
        env.locked.store(true, Ordering::SeqCst);
        wait_for(&env.daemon.calls, &format!("closed:{S}:locked")).await;
        wait_for(&env.kwin_log, "active:false").await;
        env.task.abort();
    })
    .await
    .unwrap();
}

/// ADR-0105 after the review of #512: KWin deactivates the context on a
/// lock as on a focus change; while locked the end is `locked`.
#[tokio::test]
async fn a_deactivate_while_locked_is_reported_as_locked() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let env = Env::new(Duration::from_secs(3600)).await;
        env.open(S).await;
        env.locked.store(true, Ordering::SeqCst);
        env.commands.send(Command_::Deactivate).unwrap();
        wait_for(&env.daemon.calls, &format!("closed:{S}:locked")).await;
        assert!(
            !env.daemon
                .calls
                .lock()
                .unwrap()
                .contains(&format!("closed:{S}:focus"))
        );
        env.task.abort();
    })
    .await
    .unwrap();
}
