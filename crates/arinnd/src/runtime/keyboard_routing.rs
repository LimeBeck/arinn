// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The global on-screen keyboard (ADR-0105, #434): routes the shell's
//! `org.arinn.ExternalKeyboard1` and the platform's
//! `org.arinn.KeyboardBridge1`. One session at a time; a bounded queue of
//! records for the deliverer; no storage. Typed text and key names never
//! reach a log record, only the operation, the outcome and a fixed reason.
use super::Core;
use crate::platform::{ChildCheck, Platform, authorize_deliverer, is_direct_child};
use arinn_protocol::{
    keyboard::{self, Record},
    production,
    validation::ErrorCode,
};
use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;
use zbus::{message::Header, object_server::SignalEmitter};

/// A deliverer must confirm `Opened` within this time.
const OPEN_TIMEOUT: Duration = Duration::from_secs(3);
/// A deliverer that does not poll `Next` for this long is gone.
const POLL_TIMEOUT: Duration = Duration::from_secs(5);
/// The longest `Next` waits for a record.
const NEXT_WAIT: Duration = Duration::from_secs(1);
const SWEEP: Duration = Duration::from_millis(250);

struct Deliverer {
    owner: String,
    instance: String,
    generation: String,
    seen: Instant,
    /// Since when a `Next` call waits. A poll that never returned (its
    /// caller vanished mid-call) stops counting after twice `NEXT_WAIT`.
    polling: Option<Instant>,
}
impl Deliverer {
    fn polling(&self) -> bool {
        self.polling.is_some_and(|t| t.elapsed() < NEXT_WAIT * 2)
    }
}

struct Session {
    id: String,
    opened: bool,
    since: Instant,
}

struct State {
    deliverer: Option<Deliverer>,
    session: Option<Session>,
    queue: VecDeque<Record>,
    child: ChildCheck,
    open_timeout: Duration,
    poll_timeout: Duration,
}

/// The routing state shared by both endpoints.
#[derive(Clone)]
pub struct Keyboard {
    state: Arc<Mutex<State>>,
    wake: Arc<Notify>,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                deliverer: None,
                session: None,
                queue: VecDeque::new(),
                child: is_direct_child,
                open_timeout: OPEN_TIMEOUT,
                poll_timeout: POLL_TIMEOUT,
            })),
            wake: Arc::new(Notify::new()),
        }
    }
}

fn session_of(record: &Record) -> Option<&str> {
    match record {
        Record::Idle => None,
        Record::Open(s) | Record::Close(s) | Record::Type(s, _) | Record::Press(s, _) => Some(s),
    }
}

impl State {
    /// Ends the current session. Its waiting records are dropped; the
    /// deliverer is told with `[close, S]` when `tell`. Returns the session
    /// the shell must hear `Closed` for: only an opened one.
    fn end(&mut self, tell: bool) -> Option<String> {
        let session = self.session.take()?;
        self.queue
            .retain(|r| session_of(r) != Some(session.id.as_str()));
        if tell && self.deliverer.is_some() {
            self.queue.push_back(Record::Close(session.id.clone()));
        }
        session.opened.then_some(session.id)
    }
    fn current(&self, session: &str) -> Option<&Session> {
        self.session.as_ref().filter(|s| s.id == session)
    }
    fn deliverer(&self, owner: &str, generation: &str) -> Result<(), ErrorCode> {
        match &self.deliverer {
            Some(d) if d.owner == owner && d.generation == generation => Ok(()),
            _ => Err(ErrorCode::Conflict),
        }
    }
}

/// Records of this module; tests read them back to prove that no typed
/// text or key name is ever logged.
#[cfg(test)]
pub(super) static LOGGED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// One log record: a fixed outcome and an optional fixed reason, nothing
/// from the caller.
fn log(outcome: &'static str, reason: Option<&'static str>) {
    let line = serde_json::json!({"level":"info","component":"arinnd","provider_id":null,"request_id":null,"operation":"external-keyboard","outcome":outcome,"error":reason,"duration_ms":0}).to_string();
    #[cfg(test)]
    LOGGED.lock().unwrap().push(line.clone());
    eprintln!("{line}");
}

impl Keyboard {
    /// Replaces the KDE deliverer's process check; tests cannot make the
    /// deliverer a child of a fake KWin.
    #[cfg(test)]
    pub(crate) async fn trust_children(&self, child: ChildCheck) {
        self.state.lock().await.child = child;
    }
    #[cfg(test)]
    pub(crate) async fn set_timeouts(&self, open: Duration, poll: Duration) {
        let mut state = self.state.lock().await;
        state.open_timeout = open;
        state.poll_timeout = poll;
    }
}

impl Core {
    async fn keyboard_closed(&self, session: Option<String>) {
        let Some(session) = session else {
            return;
        };
        if let Ok(emitter) = SignalEmitter::new(&self.connection, production::PATH) {
            let _ = ShellKeyboard::closed(&emitter, &session).await;
        }
    }

    /// Callers that must never type: registered providers, the platform
    /// adapter and the deliverer itself.
    async fn keyboard_caller(&self, h: &Header<'_>) -> Result<(), ErrorCode> {
        let sender = super::sender(h)?;
        if self
            .state
            .lock()
            .await
            .providers
            .values()
            .any(|p| p.owner == sender)
        {
            return Err(ErrorCode::PermissionDenied);
        }
        if self
            .keyboard
            .state
            .lock()
            .await
            .deliverer
            .as_ref()
            .is_some_and(|d| d.owner == sender)
        {
            return Err(ErrorCode::PermissionDenied);
        }
        if Platform::authorize(&self.connection, h).await.is_ok() {
            return Err(ErrorCode::PermissionDenied);
        }
        Ok(())
    }

    async fn deliverer_sender(&self, h: &Header<'_>) -> production::Result<String> {
        let child = self.keyboard.state.lock().await.child;
        authorize_deliverer(&self.connection, h, child).await
    }

    /// Export both keyboard interfaces and start the session watchdog.
    pub async fn export_keyboard(&self) -> Result<(), ErrorCode> {
        let server = self.connection.object_server();
        server
            .at(production::PATH, ShellKeyboard { core: self.clone() })
            .await
            .map_err(|_| ErrorCode::Internal)?;
        server
            .at(production::PATH, KeyboardBridge { core: self.clone() })
            .await
            .map_err(|_| ErrorCode::Internal)?;
        let core = self.clone();
        tokio::spawn(async move {
            let dbus = zbus::fdo::DBusProxy::new(&core.connection).await.ok();
            loop {
                tokio::time::sleep(SWEEP).await;
                core.sweep(dbus.as_ref()).await;
            }
        });
        Ok(())
    }

    /// Ends what timed out: an unconfirmed session, a silent or vanished
    /// deliverer.
    async fn sweep(&self, dbus: Option<&zbus::fdo::DBusProxy<'_>>) {
        let owner = {
            let state = self.keyboard.state.lock().await;
            state.deliverer.as_ref().map(|d| d.owner.clone())
        };
        let vanished = match (owner, dbus) {
            (Some(owner), Some(dbus)) => match owner.as_str().try_into() {
                Ok(name) => !dbus.name_has_owner(name).await.unwrap_or(true),
                Err(_) => true,
            },
            _ => false,
        };
        let mut state = self.keyboard.state.lock().await;
        let mut closed = None;
        let silent = state
            .deliverer
            .as_ref()
            .is_some_and(|d| !d.polling() && d.seen.elapsed() >= state.poll_timeout);
        if vanished || silent {
            state.deliverer = None;
            state.queue.clear();
            closed = state.end(false);
            log("closed", Some("Unavailable"));
        } else if state
            .session
            .as_ref()
            .is_some_and(|s| !s.opened && s.since.elapsed() >= state.open_timeout)
        {
            closed = state.end(true);
            log("closed", Some("Timeout"));
        }
        drop(state);
        self.keyboard_closed(closed).await;
    }
}

/// `org.arinn.ExternalKeyboard1`: the shell's side.
pub(super) struct ShellKeyboard {
    core: Core,
}

#[zbus::interface(name = "org.arinn.ExternalKeyboard1")]
impl ShellKeyboard {
    #[zbus(name = "Type")]
    async fn type_text(
        &self,
        session: &str,
        text: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        keyboard::session(session)?;
        keyboard::text(text)?;
        self.deliver(Record::Type(session.into(), text.into()), &h)
            .await
    }

    async fn press(
        &self,
        session: &str,
        key: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        keyboard::session(session)?;
        keyboard::press_key(key)?;
        self.deliver(Record::Press(session.into(), key.into()), &h)
            .await
    }

    async fn close(&self, session: &str, #[zbus(header)] h: Header<'_>) -> production::Result<()> {
        keyboard::session(session)?;
        self.core.keyboard_caller(&h).await?;
        let closed = {
            let mut state = self.core.keyboard.state.lock().await;
            if state.current(session).is_none() {
                return Err(ErrorCode::Conflict.into());
            }
            state.end(true)
        };
        self.core.keyboard.wake.notify_one();
        log("closed", Some("Cancelled"));
        self.core.keyboard_closed(closed).await;
        Ok(())
    }

    #[zbus(signal)]
    pub(super) async fn opened(emitter: &SignalEmitter<'_>, session: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    pub(super) async fn input(
        emitter: &SignalEmitter<'_>,
        session: &str,
        key: &str,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    pub(super) async fn closed(emitter: &SignalEmitter<'_>, session: &str) -> zbus::Result<()>;
}

impl ShellKeyboard {
    async fn deliver(&self, record: Record, h: &Header<'_>) -> production::Result<()> {
        self.core.keyboard_caller(h).await?;
        let mut state = self.core.keyboard.state.lock().await;
        let session = session_of(&record).unwrap_or_default();
        if !state.current(session).is_some_and(|s| s.opened) {
            return Err(ErrorCode::Conflict.into());
        }
        if state.queue.len() >= keyboard::MAX_QUEUED {
            return Err(ErrorCode::Unavailable.into());
        }
        state.queue.push_back(record);
        drop(state);
        self.core.keyboard.wake.notify_one();
        Ok(())
    }
}

/// `org.arinn.KeyboardBridge1`: the platform's side.
pub(super) struct KeyboardBridge {
    core: Core,
}

#[zbus::interface(name = "org.arinn.KeyboardBridge1")]
impl KeyboardBridge {
    async fn request(&self, #[zbus(header)] h: Header<'_>) -> production::Result<()> {
        Platform::authorize(&self.core.connection, &h).await?;
        let mut state = self.core.keyboard.state.lock().await;
        if state.session.is_some() {
            // The key toggles: a second press hides the panel.
            let closed = state.end(true);
            drop(state);
            self.core.keyboard.wake.notify_one();
            log("closed", Some("Cancelled"));
            self.core.keyboard_closed(closed).await;
            return Ok(());
        }
        if state.deliverer.is_none() {
            log("rejected", Some("Unavailable"));
            return Err(ErrorCode::Unavailable.into());
        }
        let id = Uuid::new_v4().to_string();
        state.queue.push_back(Record::Open(id.clone()));
        state.session = Some(Session {
            id,
            opened: false,
            since: Instant::now(),
        });
        drop(state);
        self.core.keyboard.wake.notify_one();
        log("requested", None);
        Ok(())
    }

    async fn connect(
        &self,
        instance: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<String> {
        keyboard::session(instance)?;
        let sender = self.core.deliverer_sender(&h).await?;
        let mut state = self.core.keyboard.state.lock().await;
        if let Some(d) = &state.deliverer
            && d.owner == sender
            && d.instance == instance
        {
            return Ok(d.generation.clone());
        }
        // A new deliverer never knew the old session.
        let closed = state.end(false);
        state.queue.clear();
        let generation = Uuid::new_v4().to_string();
        state.deliverer = Some(Deliverer {
            owner: sender,
            instance: instance.into(),
            generation: generation.clone(),
            seen: Instant::now(),
            polling: None,
        });
        drop(state);
        log("connected", None);
        self.core.keyboard_closed(closed).await;
        Ok(generation)
    }

    async fn next(
        &self,
        generation: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<Vec<String>> {
        keyboard::session(generation)?;
        let sender = self.core.deliverer_sender(&h).await?;
        {
            let mut state = self.core.keyboard.state.lock().await;
            state.deliverer(&sender, generation)?;
            let d = state.deliverer.as_mut().expect("checked");
            if d.polling() {
                return Err(ErrorCode::Conflict.into());
            }
            d.polling = Some(Instant::now());
            d.seen = Instant::now();
        }
        let deadline = tokio::time::Instant::now() + NEXT_WAIT;
        let record = loop {
            let notified = self.core.keyboard.wake.notified();
            {
                let mut state = self.core.keyboard.state.lock().await;
                if state.deliverer(&sender, generation).is_err() {
                    break None;
                }
                if let Some(record) = state.queue.pop_front() {
                    break Some(record);
                }
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                break Some(Record::Idle);
            }
        };
        let mut state = self.core.keyboard.state.lock().await;
        let Some(record) = record else {
            return Err(ErrorCode::Conflict.into());
        };
        if let Some(d) = state.deliverer.as_mut() {
            d.polling = None;
            d.seen = Instant::now();
        }
        Ok(record.fields())
    }

    async fn opened(
        &self,
        generation: &str,
        session: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        keyboard::session(generation)?;
        keyboard::session(session)?;
        let sender = self.core.deliverer_sender(&h).await?;
        let mut state = self.core.keyboard.state.lock().await;
        state.deliverer(&sender, generation)?;
        let Some(current) = state.session.as_mut().filter(|s| s.id == session) else {
            return Err(ErrorCode::Conflict.into());
        };
        if current.opened {
            return Ok(());
        }
        current.opened = true;
        drop(state);
        log("opened", None);
        let emitter = SignalEmitter::new(&self.core.connection, production::PATH)
            .map_err(|_| ErrorCode::Internal)?;
        ShellKeyboard::opened(&emitter, session)
            .await
            .map_err(|_| ErrorCode::Internal)?;
        Ok(())
    }

    async fn input(
        &self,
        generation: &str,
        session: &str,
        key: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        keyboard::session(generation)?;
        keyboard::session(session)?;
        keyboard::input_key(key)?;
        let sender = self.core.deliverer_sender(&h).await?;
        {
            let state = self.core.keyboard.state.lock().await;
            state.deliverer(&sender, generation)?;
            if !state.current(session).is_some_and(|s| s.opened) {
                return Err(ErrorCode::Conflict.into());
            }
        }
        let emitter = SignalEmitter::new(&self.core.connection, production::PATH)
            .map_err(|_| ErrorCode::Internal)?;
        ShellKeyboard::input(&emitter, session, key)
            .await
            .map_err(|_| ErrorCode::Internal)?;
        Ok(())
    }

    async fn closed(
        &self,
        generation: &str,
        session: &str,
        reason: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        keyboard::session(generation)?;
        keyboard::session(session)?;
        keyboard::close_reason(reason)?;
        let sender = self.core.deliverer_sender(&h).await?;
        let closed = {
            let mut state = self.core.keyboard.state.lock().await;
            state.deliverer(&sender, generation)?;
            if state.current(session).is_none() {
                return Err(ErrorCode::Conflict.into());
            }
            state.end(false)
        };
        log(
            "closed",
            Some(match reason {
                "locked" => "PermissionDenied",
                "unavailable" => "Unavailable",
                _ => "NotFound",
            }),
        );
        self.core.keyboard_closed(closed).await;
        Ok(())
    }
}

#[cfg(test)]
#[path = "keyboard_tests.rs"]
mod tests;
