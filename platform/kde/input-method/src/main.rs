// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `arinn-input-method`: the KDE Plasma 6/Wayland deliverer of the global
//! on-screen keyboard (ADR-0105).
//!
//! KWin starts it as the configured input method (`kwinrc [Wayland]
//! InputMethod`) with `WAYLAND_SOCKET`. It binds `zwp_input_method_v1`, owns
//! `org.arinn.InputMethod` and serves `org.arinn.KeyboardBridge1` records:
//! on open it asks KWin to activate a context for the focused window,
//! grabs the keyboard through it, reports the panel keys and forwards every
//! other key; typed text is committed through the context. Without a
//! session it shows nothing and passes every key on. Nothing typed is
//! logged.
mod machine;

use anyhow::{Context as _, Result};
use arinn_protocol::keyboard::{INPUT_METHOD_SERVICE, KeyboardBridgeProxy, Record};
use machine::{In, Machine, Out, panel_key};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, WEnum, event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_keyboard, wl_registry},
};
use wayland_protocols::wp::input_method::zv1::client::{
    zwp_input_method_context_v1::{self, ZwpInputMethodContextV1},
    zwp_input_method_v1::{self, ZwpInputMethodV1},
};

const LOCK_ENDPOINTS: [(&str, &str, &str); 3] = [
    (
        "org.freedesktop.ScreenSaver",
        "/org/freedesktop/ScreenSaver",
        "org.freedesktop.ScreenSaver",
    ),
    (
        "org.freedesktop.ScreenSaver",
        "/ScreenSaver",
        "org.freedesktop.ScreenSaver",
    ),
    (
        "org.xfce.ScreenSaver",
        "/org/xfce/ScreenSaver",
        "org.xfce.ScreenSaver",
    ),
];

/// An explicit unlocked reply from the first lock service that answers. A
/// missing, slow or malformed service is never evidence of an unlocked
/// session (ADR-0041).
async fn unlocked(c: &zbus::Connection) -> bool {
    tokio::time::timeout(Duration::from_secs(1), async {
        let bus = zbus::fdo::DBusProxy::new(c).await.ok()?;
        for (service, path, interface) in LOCK_ENDPOINTS {
            let Ok(owner) = bus.get_name_owner(service.try_into().ok()?).await else {
                continue;
            };
            let p = zbus::Proxy::new(c, owner.as_str(), path, interface)
                .await
                .ok()?;
            match p.call::<_, _, bool>("GetActive", &()).await {
                Ok(active) => return Some(!active),
                Err(zbus::Error::MethodError(name, _, _))
                    if [
                        "org.freedesktop.DBus.Error.UnknownObject",
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        "org.freedesktop.DBus.Error.UnknownInterface",
                    ]
                    .contains(&name.as_str()) =>
                {
                    continue;
                }
                Err(_) => return None,
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
        == Some(true)
}

/// The Wayland objects the session loop acts on.
#[derive(Default)]
struct Shared {
    context: Option<ZwpInputMethodContextV1>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    /// The latest `commit_state` serial of the context.
    serial: u32,
    /// Key repeat of the grab: (rate per second, delay in ms).
    repeat: (i32, i32),
}

/// Dispatch state of the Wayland thread: it reports events to the session
/// loop and forwards modifier changes straight back to the context.
struct Wayland {
    shared: Arc<Mutex<Shared>>,
    events: mpsc::UnboundedSender<In>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Wayland {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodV1, ()> for Wayland {
    fn event(
        state: &mut Self,
        _: &ZwpInputMethodV1,
        event: zwp_input_method_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_input_method_v1::Event::Activate { id } => {
                let mut shared = state.shared.lock().unwrap();
                shared.context = Some(id);
                shared.keyboard = None;
                shared.serial = 0;
                drop(shared);
                let _ = state.events.send(In::Activated);
            }
            zwp_input_method_v1::Event::Deactivate { context } => {
                let mut shared = state.shared.lock().unwrap();
                let current = shared.context.as_ref() == Some(&context);
                if current {
                    shared.context = None;
                    shared.keyboard = None;
                }
                drop(shared);
                context.destroy();
                if current {
                    let _ = state.events.send(In::Deactivated);
                }
            }
            _ => (),
        }
    }
    event_created_child!(Wayland, ZwpInputMethodV1, [
        zwp_input_method_v1::EVT_ACTIVATE_OPCODE => (ZwpInputMethodContextV1, ()),
    ]);
}

impl Dispatch<ZwpInputMethodContextV1, ()> for Wayland {
    fn event(
        state: &mut Self,
        context: &ZwpInputMethodContextV1,
        event: zwp_input_method_context_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_input_method_context_v1::Event::CommitState { serial } = event {
            let mut shared = state.shared.lock().unwrap();
            if shared.context.as_ref() == Some(context) {
                shared.serial = serial;
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Wayland {
    fn event(
        state: &mut Self,
        keyboard: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut shared = state.shared.lock().unwrap();
        // Events of a grab that is no longer current are dropped.
        if shared.keyboard.as_ref() != Some(keyboard) {
            return;
        }
        match event {
            wl_keyboard::Event::Key {
                serial,
                time,
                key,
                state: key_state,
            } => {
                let pressed = matches!(key_state, WEnum::Value(wl_keyboard::KeyState::Pressed));
                drop(shared);
                let _ = state.events.send(In::Key {
                    code: key,
                    pressed,
                    serial,
                    time,
                });
            }
            wl_keyboard::Event::Modifiers {
                serial,
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            } => {
                if let Some(context) = &shared.context {
                    context.modifiers(serial, mods_depressed, mods_latched, mods_locked, group);
                }
            }
            wl_keyboard::Event::RepeatInfo { rate, delay } => {
                shared.repeat = (rate, delay);
            }
            // The keymap's file descriptor closes when the event is dropped.
            _ => (),
        }
    }
}

/// What the session loop needs of the outside world. Production uses KWin
/// and the session bus; tests a fake compositor and a private bus.
pub struct Endpoints {
    pub bus: zbus::Connection,
    /// The daemon's bus name.
    pub daemon: String,
    /// The bus name that serves `org.kde.kwin.VirtualKeyboard`.
    pub kwin: String,
    /// How long KWin may take to activate a context after the request.
    pub activation: Duration,
}

type Started = (
    mpsc::UnboundedReceiver<In>,
    Arc<Mutex<Shared>>,
    QueueHandle<Wayland>,
);

/// Starts the Wayland dispatch thread on `wayland` and returns its event
/// stream, the shared objects and the queue handle requests are made on.
fn start_wayland(wayland: &Connection) -> Result<Started> {
    let (globals, mut queue): (_, EventQueue<Wayland>) =
        registry_queue_init(wayland).context("Wayland registry")?;
    let handle = queue.handle();
    let _input_method: ZwpInputMethodV1 = globals
        .bind(&handle, 1..=1, ())
        .context("zwp_input_method_v1 (is this KWin's input method?)")?;
    let shared = Arc::new(Mutex::new(Shared::default()));
    let (events, receiver) = mpsc::unbounded_channel();
    let mut state = Wayland {
        shared: shared.clone(),
        events,
    };
    wayland.flush()?;
    std::thread::spawn(move || while queue.blocking_dispatch(&mut state).is_ok() {});
    Ok((receiver, shared, handle))
}

enum Wake {
    Wayland(In),
    Daemon(Record),
    Timer(In),
    Tick,
}

/// One daemon connection of the session loop.
struct Lane<'a> {
    wayland: &'a Connection,
    shared: &'a Arc<Mutex<Shared>>,
    handle: &'a QueueHandle<Wayland>,
    endpoints: &'a Endpoints,
    bridge: KeyboardBridgeProxy<'a>,
    generation: String,
    timers: mpsc::UnboundedSender<In>,
    open: Option<String>,
    repeating: Option<(u32, tokio::task::JoinHandle<()>)>,
}

impl Lane<'_> {
    fn stop_repeat(&mut self) {
        if let Some((_, task)) = self.repeating.take() {
            task.abort();
        }
    }

    /// Arrow keys repeat while held, at the grab's rate.
    fn repeat(&mut self, event: &In) {
        let In::Key { code, pressed, .. } = event else {
            return;
        };
        if self
            .repeating
            .as_ref()
            .is_some_and(|(held, _)| held != code || !pressed)
        {
            self.stop_repeat();
        }
        let (rate, delay) = self.shared.lock().unwrap().repeat;
        if *pressed
            && self.repeating.is_none()
            && rate > 0
            && self.open.is_some()
            && matches!(panel_key(*code), Some("up" | "down" | "left" | "right"))
        {
            let sender = self.timers.clone();
            let repeat = event.clone();
            let task = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(delay.max(0) as u64)).await;
                loop {
                    if sender.send(repeat.clone()).is_err() {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(1000 / rate as u64)).await;
                }
            });
            self.repeating = Some((*code, task));
        }
    }

    /// Turns a daemon record into a machine event, checking the lock state
    /// before anything reaches the application.
    async fn record(&self, record: Record) -> Option<In> {
        let unlocked = || unlocked(&self.endpoints.bus);
        match record {
            Record::Idle => None,
            Record::Open(s) => {
                if unlocked().await {
                    Some(In::Open(s))
                } else {
                    let _ = self.bridge.closed(&self.generation, &s, "locked").await;
                    None
                }
            }
            Record::Type(s, text) => Some(if unlocked().await {
                In::Type(s, text)
            } else {
                In::Locked(s)
            }),
            Record::Press(s, key) => Some(if unlocked().await {
                In::Press(s, key)
            } else {
                In::Locked(s)
            }),
            Record::Close(s) => Some(In::Close(s)),
        }
    }

    fn flush(&self) {
        let _ = self.wayland.flush();
    }

    async fn act(&mut self, out: Out) {
        let kwin = self.endpoints.kwin.as_str();
        match out {
            Out::ForceActivate => {
                let _ = self
                    .endpoints
                    .bus
                    .call_method(
                        Some(kwin),
                        "/VirtualKeyboard",
                        Some("org.kde.kwin.VirtualKeyboard"),
                        "forceActivate",
                        &(),
                    )
                    .await;
            }
            Out::Wait(s) => {
                let sender = self.timers.clone();
                let wait = self.endpoints.activation;
                tokio::spawn(async move {
                    tokio::time::sleep(wait).await;
                    let _ = sender.send(In::OpenTimeout(s));
                });
            }
            Out::Grab => {
                let mut shared = self.shared.lock().unwrap();
                if let Some(context) = shared.context.clone() {
                    shared.keyboard = Some(context.grab_keyboard(self.handle, ()));
                }
                drop(shared);
                self.flush();
            }
            Out::Release => {
                self.stop_repeat();
                self.shared.lock().unwrap().keyboard = None;
                let _ = self
                    .endpoints
                    .bus
                    .call_method(
                        Some(kwin),
                        "/VirtualKeyboard",
                        Some("org.freedesktop.DBus.Properties"),
                        "Set",
                        &(
                            "org.kde.kwin.VirtualKeyboard",
                            "active",
                            zbus::zvariant::Value::from(false),
                        ),
                    )
                    .await;
            }
            Out::Opened(s) => {
                self.open = Some(s.clone());
                let _ = self.bridge.opened(&self.generation, &s).await;
            }
            Out::Input(s, key) => {
                let _ = self.bridge.input(&self.generation, &s, key).await;
            }
            Out::Closed(s, reason) => {
                if self.open.as_deref() == Some(s.as_str()) {
                    self.open = None;
                }
                self.stop_repeat();
                let _ = self.bridge.closed(&self.generation, &s, reason).await;
            }
            Out::Commit(text) => {
                let shared = self.shared.lock().unwrap();
                if let Some(context) = &shared.context {
                    context.commit_string(shared.serial, text);
                }
                drop(shared);
                self.flush();
            }
            Out::Keysym(sym) => {
                let shared = self.shared.lock().unwrap();
                if let Some(context) = &shared.context {
                    context.keysym(shared.serial, 0, sym, 1, 0);
                    context.keysym(shared.serial, 0, sym, 0, 0);
                }
                drop(shared);
                self.flush();
            }
            Out::Forward {
                code,
                pressed,
                serial,
                time,
            } => {
                let shared = self.shared.lock().unwrap();
                if let Some(context) = &shared.context {
                    context.key(serial, time, code, u32::from(pressed));
                }
                drop(shared);
                self.flush();
            }
        }
    }
}

/// Runs one daemon connection until it fails.
async fn session(
    wayland: &Connection,
    shared: &Arc<Mutex<Shared>>,
    handle: &QueueHandle<Wayland>,
    events: &mut mpsc::UnboundedReceiver<In>,
    endpoints: &Endpoints,
) -> Result<()> {
    let bridge = KeyboardBridgeProxy::builder(&endpoints.bus)
        .destination(endpoints.daemon.clone())?
        .build()
        .await?;
    let generation = bridge.connect(&uuid::Uuid::new_v4().to_string()).await?;
    let (records, mut daemon) = mpsc::channel(8);
    let poll = bridge.clone();
    let g = generation.clone();
    let poller = tokio::spawn(async move {
        loop {
            let Ok(fields) = poll.next(&g).await else {
                return;
            };
            let Ok(record) = Record::parse(&fields) else {
                return;
            };
            if record != Record::Idle && records.send(record).await.is_err() {
                return;
            }
        }
    });
    let (timers, mut timer) = mpsc::unbounded_channel::<In>();
    let mut lane = Lane {
        wayland,
        shared,
        handle,
        endpoints,
        bridge,
        generation,
        timers,
        open: None,
        repeating: None,
    };
    let mut machine = Machine::default();
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let result = loop {
        let wake = tokio::select! {
            event = events.recv() => match event {
                Some(event) => Wake::Wayland(event),
                None => break Err(anyhow::anyhow!("Wayland connection ended")),
            },
            record = daemon.recv() => match record {
                Some(record) => Wake::Daemon(record),
                None => break Err(anyhow::anyhow!("daemon lane ended")),
            },
            Some(event) = timer.recv() => Wake::Timer(event),
            _ = tick.tick() => Wake::Tick,
        };
        let input = match wake {
            Wake::Wayland(event) => {
                lane.repeat(&event);
                Some(event)
            }
            Wake::Timer(event) => Some(event),
            Wake::Daemon(record) => lane.record(record).await,
            // A lock or an unknown lock state ends an open session.
            Wake::Tick => match lane.open.clone() {
                Some(s) if !unlocked(&endpoints.bus).await => Some(In::Locked(s)),
                _ => None,
            },
        };
        let Some(input) = input else {
            continue;
        };
        for out in machine.handle(input) {
            lane.act(out).await;
        }
        if !machine.is_open() {
            lane.open = None;
        }
    };
    poller.abort();
    lane.stop_repeat();
    result
}

/// Serves the daemon until the Wayland connection ends, connecting again to
/// a restarted daemon.
pub async fn run(wayland: Connection, endpoints: Endpoints) -> Result<()> {
    let (mut events, shared, handle) = start_wayland(&wayland)?;
    loop {
        let result = session(&wayland, &shared, &handle, &mut events, &endpoints).await;
        if events.is_closed() {
            return result;
        }
        if result.is_err() {
            eprintln!("arinn-input-method: daemon unavailable; retrying");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let wayland = Connection::connect_to_env().context("WAYLAND_SOCKET from KWin")?;
    let bus = zbus::connection::Builder::session()?
        .name(INPUT_METHOD_SERVICE)?
        .build()
        .await
        .context("org.arinn.InputMethod is already owned")?;
    eprintln!("arinn-input-method: started");
    run(
        wayland,
        Endpoints {
            bus,
            daemon: arinn_protocol::production::SERVICE.into(),
            kwin: "org.kde.KWin".into(),
            activation: Duration::from_secs(1),
        },
    )
    .await
}

#[cfg(test)]
mod tests;
