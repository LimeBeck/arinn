// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The remote's Menu keys in the EWMH adapter (#507, ADR-0106), end to end:
//! the adapter binary runs against an isolated Xvfb with synthetic EWMH
//! metadata and a private session bus. It never uses the real `DISPLAY`, the
//! real session bus or the system bus.
//!
//! - While `org.arinn.Daemon1` has no owner the adapter grabs no global key,
//!   so the Menu keys never swallow the context-menu key while the daemon is
//!   down; it grabs them again once the daemon is back.
//! - The Menu grabs follow the shell's focus at once, not at the next 0.5 s
//!   tick.
use anyhow::{Result, ensure};
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection as _, protocol::xproto::*, rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

/// A child process that is killed when the test ends.
struct Killed(Child);
impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// An isolated X server with an EWMH window manager's root properties.
struct Desktop {
    _server: Killed,
    display: String,
    c: RustConnection,
    root: Window,
}
impl Desktop {
    fn new() -> Result<Self> {
        let executable = std::env::var("ARINN_XVFB").unwrap_or_else(|_| "Xvfb".into());
        let mut server = Command::new(executable)
            .args([
                "-displayfd",
                "1",
                "-screen",
                "0",
                "1280x720x24",
                "-nolisten",
                "tcp",
                "-noreset",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut number = String::new();
        BufReader::new(server.stdout.take().unwrap()).read_line(&mut number)?;
        let server = Killed(server);
        ensure!(
            !number.trim().is_empty() && number.trim().bytes().all(|c| c.is_ascii_digit()),
            "isolated X server did not start"
        );
        let display = format!(":{}", number.trim());
        let (c, screen) = x11rb::connect(Some(&display))?;
        let root = c.setup().roots[screen].root;
        let desktop = Self {
            _server: server,
            display,
            c,
            root,
        };
        let wm = desktop.window()?;
        let check = desktop.atom("_NET_SUPPORTING_WM_CHECK")?;
        for window in [root, wm] {
            desktop.words(window, check, AtomEnum::WINDOW.into(), &[wm])?;
        }
        let supported: Vec<Atom> = [
            "_NET_CLIENT_LIST",
            "_NET_ACTIVE_WINDOW",
            "_NET_CLOSE_WINDOW",
            "_NET_WM_STATE",
            "_NET_WM_STATE_FULLSCREEN",
            "_NET_WM_STATE_ABOVE",
            "_NET_WM_STATE_BELOW",
            "_NET_WM_DESKTOP",
        ]
        .into_iter()
        .map(|name| desktop.atom(name))
        .collect::<Result<_>>()?;
        desktop.words(
            root,
            desktop.atom("_NET_SUPPORTED")?,
            AtomEnum::ATOM.into(),
            &supported,
        )?;
        desktop.clients(&[], 0)?;
        Ok(desktop)
    }
    fn atom(&self, name: &str) -> Result<Atom> {
        Ok(self.c.intern_atom(false, name.as_bytes())?.reply()?.atom)
    }
    fn words(&self, window: Window, property: Atom, ty: Atom, words: &[u32]) -> Result<()> {
        self.c
            .change_property32(PropMode::REPLACE, window, property, ty, words)?
            .check()?;
        Ok(())
    }
    fn window(&self) -> Result<Window> {
        let xid = self.c.generate_id()?;
        self.c
            .create_window(
                0,
                xid,
                self.root,
                0,
                0,
                100,
                100,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )?
            .check()?;
        Ok(xid)
    }
    /// A client window with a desktop identity (`_GTK_APPLICATION_ID`).
    fn client(&self, identity: &str) -> Result<Window> {
        let xid = self.window()?;
        self.c
            .change_property8(
                PropMode::REPLACE,
                xid,
                self.atom("_GTK_APPLICATION_ID")?,
                self.atom("UTF8_STRING")?,
                identity.as_bytes(),
            )?
            .check()?;
        Ok(xid)
    }
    /// `_NET_CLIENT_LIST` and `_NET_ACTIVE_WINDOW`, as a window manager
    /// writes them.
    fn clients(&self, windows: &[Window], active: Window) -> Result<()> {
        self.words(
            self.root,
            self.atom("_NET_CLIENT_LIST")?,
            AtomEnum::WINDOW.into(),
            windows,
        )?;
        self.activate(active)
    }
    fn activate(&self, active: Window) -> Result<()> {
        self.words(
            self.root,
            self.atom("_NET_ACTIVE_WINDOW")?,
            AtomEnum::WINDOW.into(),
            &[active],
        )
    }
    /// The first keycode that carries `keysym`.
    fn keycode(&self, keysym: u32) -> Result<Keycode> {
        let setup = self.c.setup();
        let mapping = self
            .c
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)?
            .reply()?;
        mapping
            .keysyms
            .chunks(usize::from(mapping.keysyms_per_keycode))
            .position(|syms| syms.contains(&keysym))
            .map(|i| setup.min_keycode + i as u8)
            .ok_or_else(|| anyhow::anyhow!("keysym {keysym:#x} is not mapped"))
    }
}

/// An application window that has the X input focus and records its key
/// presses, on its own connection.
struct App {
    c: RustConnection,
    xid: Window,
}
impl App {
    fn new(desktop: &Desktop, identity: &str) -> Result<Self> {
        let (c, _) = x11rb::connect(Some(&desktop.display))?;
        let xid = c.generate_id()?;
        c.create_window(
            0,
            xid,
            desktop.root,
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
        c.change_property8(
            PropMode::REPLACE,
            xid,
            desktop.atom("_GTK_APPLICATION_ID")?,
            desktop.atom("UTF8_STRING")?,
            identity.as_bytes(),
        )?
        .check()?;
        c.map_window(xid)?.check()?;
        c.set_input_focus(InputFocus::POINTER_ROOT, xid, x11rb::CURRENT_TIME)?
            .check()?;
        Ok(Self { c, xid })
    }
    /// Whether a press of `key` reaches this window, so nobody grabs it. The
    /// press is sent with XTest and synchronised before the window's events
    /// are read, so the answer never depends on timing.
    fn receives(&self, desktop: &Desktop, key: Keycode) -> Result<bool> {
        use x11rb::protocol::xtest::ConnectionExt as _;
        // KEY_PRESS = 2, KEY_RELEASE = 3.
        for kind in [2, 3] {
            desktop
                .c
                .xtest_fake_input(kind, key, x11rb::CURRENT_TIME, desktop.root, 0, 0, 0)?;
        }
        desktop.c.sync()?;
        self.c.get_input_focus()?.reply()?;
        let mut received = false;
        while let Some(event) = self.c.poll_for_event()? {
            if let x11rb::protocol::Event::KeyPress(e) = event {
                received |= e.detail == key;
            }
        }
        Ok(received)
    }
}

/// A private session bus and its address.
fn bus() -> Result<(Killed, String)> {
    let mut child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut address = String::new();
    BufReader::new(child.stdout.take().unwrap()).read_line(&mut address)?;
    Ok((Killed(child), address.trim().into()))
}

/// The adapter binary on a desktop and a bus, and its log.
struct Adapter {
    _child: Killed,
    lines: Arc<Mutex<Vec<String>>>,
    home: PathBuf,
}
impl Drop for Adapter {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}
impl Adapter {
    fn start(desktop: &Desktop, address: &str) -> Result<Self> {
        let home = std::env::temp_dir().join(format!(
            "arinn-x11-menu-{}-{}",
            std::process::id(),
            desktop.display.trim_start_matches(':')
        ));
        std::fs::create_dir_all(&home)?;
        let mut child = Command::new(env!("CARGO_BIN_EXE_arinn-platform-x11"))
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("DISPLAY", &desktop.display)
            .env("DBUS_SESSION_BUS_ADDRESS", address)
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                "unix:path=/nonexistent/arinn-test",
            )
            .env("XDG_SESSION_TYPE", "x11")
            .env("ARINN_PLATFORM_BACKEND", "ewmh")
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_RUNTIME_DIR", &home)
            .stderr(Stdio::piped())
            .spawn()?;
        let stderr = child.stderr.take().unwrap();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                sink.lock().unwrap().push(line);
            }
        });
        Ok(Self {
            _child: Killed(child),
            lines,
            home,
        })
    }
    fn count(&self, text: &str) -> usize {
        self.lines
            .lock()
            .unwrap()
            .iter()
            .filter(|line| line.contains(text))
            .count()
    }
    /// Waits up to `limit` for a log line containing `text`.
    async fn logged(&self, text: &str, limit: Duration) -> bool {
        let start = Instant::now();
        while self.count(text) == 0 {
            if start.elapsed() >= limit {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        true
    }
    fn log(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }
}

/// The adapter's attempt to grab a Menu key that the test's probe holds.
const ATTEMPT: &str = "a quick-menu key is held by another client";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Xvfb and dbus-daemon; spawns an isolated X server and session bus, never uses the real ones"]
async fn no_global_key_is_grabbed_while_the_daemon_is_gone() -> Result<()> {
    let desktop = Desktop::new()?;
    let (_bus, address) = bus()?;
    // The probe holds Menu, so each attempt of the adapter to grab it is
    // logged, however short the attempt.
    let menu = desktop.keycode(0xff67)?;
    desktop
        .c
        .grab_key(
            false,
            desktop.root,
            ModMask::ANY,
            menu,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )?
        .check()?;
    let adapter = Adapter::start(&desktop, &address)?;
    // The adapter starts without a daemon: it waits and grabs nothing. The
    // old adapter tried a session, with its grabs, at once and every 3 s.
    assert!(
        adapter
            .logged("selected backend ewmh", Duration::from_secs(10))
            .await
    );
    assert!(
        !adapter.logged(ATTEMPT, Duration::from_millis(1500)).await,
        "a grab without the daemon: {:?}",
        adapter.log()
    );
    // The daemon appears: the adapter connects and grabs at once. This
    // stand-in serves nothing, so the session then ends and is retried; only
    // the grab attempt matters here.
    let daemon = zbus::connection::Builder::address(address.as_str())?
        .name("org.arinn.Daemon1")?
        .build()
        .await?;
    assert!(
        adapter.logged(ATTEMPT, Duration::from_secs(5)).await,
        "no grab with the daemon: {:?}",
        adapter.log()
    );
    // The daemon goes again. A retry already past its wait may still run
    // once (the 3 s delay); after that nothing more is grabbed for longer
    // than that delay.
    drop(daemon);
    tokio::time::sleep(Duration::from_millis(3500)).await;
    let before = adapter.count(ATTEMPT);
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert_eq!(adapter.count(ATTEMPT), before, "{:?}", adapter.log());
    Ok(())
}

/// A daemon that accepts the adapter's connection and reports, and keeps
/// its long polls idle, so the adapter's session runs.
struct Daemon;
#[zbus::interface(name = "org.arinn.KWinBridge1")]
impl Daemon {
    fn connect(&self, _instance: &str) -> String {
        "50000000-0000-4000-8000-000000000005".into()
    }
    fn begin_inventory(&self, _generation: &str, _revision: i32, _count: i32) {}
    #[allow(clippy::too_many_arguments)]
    fn put_window(
        &self,
        _generation: &str,
        _revision: i32,
        _index: i32,
        _token: &str,
        _desktop_id: &str,
        _output: &str,
        _flags: i32,
    ) {
    }
    fn commit_inventory(&self, _generation: &str, _revision: i32) {}
    async fn next_control(&self, _generation: &str) -> Vec<String> {
        tokio::time::sleep(Duration::from_secs(1)).await;
        Vec::new()
    }
    fn complete_control(&self, _generation: &str, _request_id: &str, _error: &str) {}
}
struct Home;
#[zbus::interface(name = "org.arinn.Daemon1")]
impl Home {
    async fn take_command(&self) -> String {
        tokio::time::sleep(Duration::from_secs(1)).await;
        "idle".into()
    }
    fn report_window(&self, _token: &str, _desktop: &str, _fullscreen: bool, _output: &str) {}
    fn report_home(&self, _visible: bool, _target: &str, _output: &str) {}
    fn report_home_request(&self, _target: &str, _output: &str) {}
    fn report_quick_menu_request(&self, _target: &str, _output: &str) {}
    fn report_power_menu_request(&self, _target: &str, _output: &str) {}
}
struct Media;
#[zbus::interface(name = "org.arinn.KWinMedia1")]
impl Media {
    fn report(&self, _generation: &str, _revision: i32, _token: &str, _launch: &str) {}
    async fn next_control(&self, _generation: &str) -> Vec<String> {
        tokio::time::sleep(Duration::from_secs(1)).await;
        Vec::new()
    }
    fn complete_control(&self, _generation: &str, _request_id: &str, _error: &str) {}
}
struct Returns;
#[zbus::interface(name = "org.arinn.KWinReturn1")]
impl Returns {
    fn report(&self, _generation: &str, _revision: i32, _target: &str, _token: &str) {}
}

/// Waits up to 5 s until `key` is grabbed (`held`) or reaches `app`, and
/// returns how long that took.
async fn until(desktop: &Desktop, app: &App, key: Keycode, held: bool) -> Result<Duration> {
    let start = Instant::now();
    while app.receives(desktop, key)? == held {
        ensure!(
            start.elapsed() < Duration::from_secs(5),
            "Menu still {} after 5 s",
            if held { "free" } else { "grabbed" }
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    Ok(start.elapsed())
}

/// #507 (ADR-0106): when the shell gains focus the Menu keys are let go at
/// once, not at the adapter's next 0.5 s tick, and taken again when another
/// window gains it. Nearly every switch must take less than 0.2 s.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Xvfb and dbus-daemon; spawns an isolated X server and session bus, never uses the real ones"]
async fn the_menu_keys_follow_the_shell_focus_at_once() -> Result<()> {
    let desktop = Desktop::new()?;
    let (_bus, address) = bus()?;
    let path = "/org/arinn/Daemon1";
    let _daemon = zbus::connection::Builder::address(address.as_str())?
        .name("org.arinn.Daemon1")?
        .serve_at(path, Daemon)?
        .serve_at(path, Home)?
        .serve_at(path, Media)?
        .serve_at(path, Returns)?
        .build()
        .await?;
    let shell = desktop.client("org.arinn.Shell")?;
    // The application keeps the X input focus throughout, so a Menu press
    // that nobody grabs reaches it; only `_NET_ACTIVE_WINDOW` changes.
    let app = App::new(&desktop, "org.example.Player")?;
    desktop.clients(&[shell, app.xid], app.xid)?;
    let menu = desktop.keycode(0xff67)?;
    let adapter = Adapter::start(&desktop, &address)?;
    // The session runs: Menu is grabbed while the application is active.
    until(&desktop, &app, menu, true)
        .await
        .map_err(|e| anyhow::anyhow!("{e}: {:?}", adapter.log()))?;
    const ROUNDS: usize = 20;
    let mut releases = Vec::new();
    for _ in 0..ROUNDS {
        desktop.activate(shell)?;
        releases.push(until(&desktop, &app, menu, false).await?);
        desktop.activate(app.xid)?;
        until(&desktop, &app, menu, true).await?;
    }
    let slow = releases
        .iter()
        .filter(|d| **d >= Duration::from_millis(200))
        .count();
    // The tick alone: each switch waits for the next tick, and the rounds
    // fall into step with it, so nearly every release takes about 0.5 s
    // (measured: 19 of 20, three runs). The watch: a few milliseconds; a
    // loaded machine may delay one or two.
    assert!(slow <= 2, "{releases:?} {:?}", adapter.log());
    Ok(())
}
