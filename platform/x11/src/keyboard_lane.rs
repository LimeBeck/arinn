// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The X11 keyboard deliverer's lane to the daemon (ADR-0105): a worker
//! thread owns the X connection of [`crate::keyboard::Keyboard`]; two tasks
//! connect it with `org.arinn.KeyboardBridge1`. The lock state is checked
//! here, with the adapter's probe order, before every key request and every
//! delivery; a locked or unknown state ends the session.
use crate::key_sequence::Sequence;
use crate::keyboard::{Keyboard, Seen};
use anyhow::Result;
use arinn_protocol::keyboard::{KeyboardBridgeProxy, Record};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
use tokio::{sync::mpsc as tmpsc, task::JoinSet};
use uuid::Uuid;
use zbus::Connection;

enum Command {
    Open(String),
    Type(String, String),
    Press(String, String),
    Close(String),
    Locked(String),
}

#[derive(Debug)]
enum Event {
    Key,
    Opened(String),
    Input(String, &'static str),
    Ended(String, &'static str),
    Failed,
}

fn worker(
    mut keyboard: Keyboard,
    commands: mpsc::Receiver<Command>,
    events: tmpsc::UnboundedSender<Event>,
) {
    let mut session: Option<String> = None;
    loop {
        let wait = Duration::from_millis(if session.is_some() { 20 } else { 50 });
        let command = match commands.recv_timeout(wait) {
            Ok(command) => Some(command),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        let current = |s: &str, session: &Option<String>| session.as_deref() == Some(s);
        let end = |keyboard: &mut Keyboard, session: &mut Option<String>, reason: &'static str| {
            let _ = keyboard.close();
            if let Some(s) = session.take() {
                let _ = events.send(Event::Ended(s, reason));
            }
        };
        match command {
            Some(Command::Open(s)) => {
                if let Some(old) = session.take() {
                    let _ = events.send(Event::Ended(old, "unavailable"));
                }
                match keyboard.open() {
                    Ok(()) => {
                        session = Some(s.clone());
                        let _ = events.send(Event::Opened(s));
                    }
                    Err(_) => {
                        let _ = events.send(Event::Ended(s, "unavailable"));
                    }
                }
            }
            Some(Command::Type(s, text)) if current(&s, &session) => {
                if keyboard.type_text(&text).is_err() {
                    end(&mut keyboard, &mut session, "focus");
                }
            }
            Some(Command::Press(s, key)) if current(&s, &session) => {
                if keyboard.press(&key).is_err() {
                    end(&mut keyboard, &mut session, "focus");
                }
            }
            Some(Command::Close(s)) if current(&s, &session) => {
                let _ = keyboard.close();
                session = None;
            }
            Some(Command::Locked(s)) if current(&s, &session) => {
                end(&mut keyboard, &mut session, "locked");
            }
            _ => (),
        }
        match keyboard.poll() {
            Ok(seen) => {
                for seen in seen {
                    let event = match (seen, &session) {
                        (Seen::Key, _) => Event::Key,
                        (Seen::Input(name), Some(s)) => Event::Input(s.clone(), name),
                        (Seen::Input(_), None) => continue,
                    };
                    let _ = events.send(event);
                }
            }
            Err(_) => {
                let _ = events.send(Event::Failed);
                return;
            }
        }
        if session.is_some() && !keyboard.target_focused().unwrap_or(false) {
            end(&mut keyboard, &mut session, "focus");
        }
    }
}

/// Starts the keyboard lane on `c` against the daemon at `destination`.
/// Returns false, with nothing started, when the keyboard cannot work here
/// (no XTest) or the daemon has no keyboard bridge (an older daemon): the
/// rest of the adapter keeps working.
pub async fn start<F>(
    c: &Connection,
    destination: &str,
    display: Option<String>,
    keys: Vec<Sequence>,
    unlocked: F,
    tasks: &mut JoinSet<Result<()>>,
) -> Result<bool>
where
    F: Fn(Connection) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>>
        + Send
        + Sync
        + 'static,
{
    let keyboard =
        match tokio::task::spawn_blocking(move || Keyboard::connect(display.as_deref(), keys))
            .await?
        {
            Ok(keyboard) => keyboard,
            Err(_) => {
                eprintln!("arinn-platform-x11: no XTest; on-screen keyboard off");
                return Ok(false);
            }
        };
    let bridge = KeyboardBridgeProxy::builder(c)
        .destination(destination.to_owned())?
        .build()
        .await?;
    let Ok(generation) = bridge.connect(&Uuid::new_v4().to_string()).await else {
        eprintln!("arinn-platform-x11: the daemon has no keyboard bridge; on-screen keyboard off");
        return Ok(false);
    };
    let unlocked = Arc::new(unlocked);
    let (commands, receiver) = mpsc::channel();
    let (sender, mut events) = tmpsc::unbounded_channel();
    std::thread::spawn(move || worker(keyboard, receiver, sender));
    let open: Arc<Mutex<Option<String>>> = Arc::default();

    let poll = bridge.clone();
    let g = generation.clone();
    let send = commands.clone();
    let check = unlocked.clone();
    let lock = c.clone();
    tasks.spawn(async move {
        loop {
            let record = Record::parse(&poll.next(&g).await?)
                .map_err(|_| anyhow::anyhow!("invalid keyboard record"))?;
            let command = match record {
                Record::Idle => continue,
                Record::Close(s) => Command::Close(s),
                Record::Open(s) => {
                    if !check(lock.clone()).await {
                        let _ = poll.closed(&g, &s, "locked").await;
                        continue;
                    }
                    Command::Open(s)
                }
                Record::Type(s, text) => {
                    if check(lock.clone()).await {
                        Command::Type(s, text)
                    } else {
                        Command::Locked(s)
                    }
                }
                Record::Press(s, key) => {
                    if check(lock.clone()).await {
                        Command::Press(s, key)
                    } else {
                        Command::Locked(s)
                    }
                }
            };
            send.send(command)?;
        }
    });

    let report = bridge.clone();
    let g = generation;
    let lock = c.clone();
    tasks.spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                event = events.recv() => {
                    match event.ok_or_else(|| anyhow::anyhow!("keyboard worker ended"))? {
                        Event::Key => {
                            // A locked or unknown session never opens the panel.
                            if unlocked(lock.clone()).await {
                                let _ = report.request().await;
                            }
                        }
                        Event::Opened(s) => {
                            *open.lock().unwrap() = Some(s.clone());
                            let _ = report.opened(&g, &s).await;
                        }
                        Event::Input(s, key) => {
                            let _ = report.input(&g, &s, key).await;
                        }
                        Event::Ended(s, reason) => {
                            {
                                let mut current = open.lock().unwrap();
                                if current.as_deref() == Some(s.as_str()) {
                                    *current = None;
                                }
                            }
                            let _ = report.closed(&g, &s, reason).await;
                        }
                        Event::Failed => anyhow::bail!("keyboard worker failed"),
                    }
                }
                _ = tick.tick() => {
                    let session = open.lock().unwrap().clone();
                    if let Some(s) = session
                        && !unlocked(lock.clone()).await
                    {
                        commands.send(Command::Locked(s))?;
                    }
                }
            }
        }
    });
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::fixture::Desktop;
    use std::sync::{Arc, Mutex};
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command as Process, Stdio},
        sync::atomic::{AtomicBool, Ordering},
    };
    use x11rb::{
        CURRENT_TIME,
        connection::Connection as _,
        protocol::{Event as XEvent, xproto::*},
        rust_connection::RustConnection,
    };

    struct Bus(Child, String);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn bus() -> Bus {
        let mut child = Process::new("dbus-daemon")
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

    const SESSION: &str = "30000000-0000-4000-8000-000000000003";

    /// A daemon that serves queued records and records what the lane says.
    #[derive(Clone, Default)]
    struct FakeDaemon {
        queue: Arc<Mutex<Vec<Vec<String>>>>,
        calls: Arc<Mutex<Vec<String>>>,
    }
    #[zbus::interface(name = "org.arinn.KeyboardBridge1")]
    impl FakeDaemon {
        fn request(&self) {
            self.calls.lock().unwrap().push("request".into());
        }
        fn connect(&self, _instance: &str) -> String {
            "40000000-0000-4000-8000-000000000004".into()
        }
        async fn next(&self, _generation: &str) -> Vec<String> {
            for _ in 0..20 {
                if let Some(record) = {
                    let mut queue = self.queue.lock().unwrap();
                    (!queue.is_empty()).then(|| queue.remove(0))
                } {
                    return record;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Vec::new()
        }
        fn opened(&self, _generation: &str, session: &str) {
            self.calls.lock().unwrap().push(format!("opened:{session}"));
        }
        fn input(&self, _generation: &str, _session: &str, key: &str) {
            self.calls.lock().unwrap().push(format!("input:{key}"));
        }
        fn closed(&self, _generation: &str, _session: &str, reason: &str) {
            self.calls.lock().unwrap().push(format!("closed:{reason}"));
        }
    }

    fn focused_window(desktop: &Desktop) -> Result<(RustConnection, Window)> {
        let (c, screen) = x11rb::connect(Some(&desktop.display))?;
        let root = c.setup().roots[screen].root;
        let xid = c.generate_id()?;
        c.create_window(
            0,
            xid,
            root,
            0,
            0,
            100,
            100,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().event_mask(EventMask::KEY_PRESS),
        )?
        .check()?;
        c.flush()?;
        // The fake window manager redirects maps; it maps the window.
        desktop.c.map_window(xid)?.check()?;
        c.set_input_focus(InputFocus::POINTER_ROOT, xid, CURRENT_TIME)?
            .check()?;
        c.flush()?;
        Ok((c, xid))
    }

    async fn wait_for(calls: &Arc<Mutex<Vec<String>>>, wanted: &str) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !calls.lock().unwrap().iter().any(|c| c == wanted) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{wanted} expected, got {:?}", calls.lock().unwrap()));
    }

    #[tokio::test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    async fn the_lane_requests_opens_types_and_ends_on_lock() -> Result<()> {
        let desktop = Desktop::new()?;
        let (app, xid) = focused_window(&desktop)?;
        desktop.inventory(&[xid], xid)?;
        let bus = bus();
        let daemon = FakeDaemon::default();
        let server = zbus::connection::Builder::address(bus.1.as_str())?
            .serve_at("/org/arinn/Daemon1", daemon.clone())?
            .build()
            .await?;
        let lane = zbus::connection::Builder::address(bus.1.as_str())?
            .build()
            .await?;
        let locked = Arc::new(AtomicBool::new(true));
        let flag = locked.clone();
        let mut tasks = JoinSet::new();
        let home = crate::key_sequence::home("Meta+F12").0;
        let keys = crate::key_sequence::keyboard("Ctrl+Alt+K", &home).0;
        assert!(
            start(
                &lane,
                server.unique_name().unwrap().as_str(),
                Some(desktop.display.clone()),
                keys,
                move |_| {
                    let flag = flag.clone();
                    Box::pin(async move { !flag.load(Ordering::SeqCst) })
                },
                &mut tasks,
            )
            .await?
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
        // Locked: the key requests nothing.
        desktop.press(&[0xffe3, 0xffe9], u32::from(b'k'))?;
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(daemon.calls.lock().unwrap().is_empty());
        locked.store(false, Ordering::SeqCst);
        desktop.press(&[0xffe3, 0xffe9], u32::from(b'k'))?;
        wait_for(&daemon.calls, "request").await;
        // The daemon opens a session; the lane binds the focused window.
        daemon
            .queue
            .lock()
            .unwrap()
            .push(Record::Open(SESSION.into()).fields());
        wait_for(&daemon.calls, &format!("opened:{SESSION}")).await;
        desktop.press(&[], 0xff54)?; // Down
        wait_for(&daemon.calls, "input:down").await;
        daemon
            .queue
            .lock()
            .unwrap()
            .push(Record::Type(SESSION.into(), "hi".into()).fields());
        let received = tokio::time::timeout(Duration::from_secs(5), async {
            let mut text = Vec::new();
            while text.len() < 2 {
                app.get_input_focus()?.reply()?;
                while let Some(event) = app.poll_for_event()? {
                    if let XEvent::KeyPress(e) = event {
                        let map = app.get_keyboard_mapping(e.detail, 1)?.reply()?;
                        // The held modifiers of the keyboard key are not text.
                        if !(0xffe1..=0xffee).contains(&map.keysyms[0]) {
                            text.push(map.keysyms[0]);
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok::<_, anyhow::Error>(text)
        })
        .await??;
        assert_eq!(received, [u32::from(b'h'), u32::from(b'i')]);
        // A lock ends the session; what follows is never typed.
        locked.store(true, Ordering::SeqCst);
        wait_for(&daemon.calls, "closed:locked").await;
        daemon
            .queue
            .lock()
            .unwrap()
            .push(Record::Type(SESSION.into(), "no".into()).fields());
        tokio::time::sleep(Duration::from_millis(500)).await;
        app.get_input_focus()?.reply()?;
        let mut late = Vec::new();
        while let Some(event) = app.poll_for_event()? {
            if let XEvent::KeyPress(e) = event {
                late.push(e.detail);
            }
        }
        assert!(late.is_empty());
        tasks.abort_all();
        Ok(())
    }
}
