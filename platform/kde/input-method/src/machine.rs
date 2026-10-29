// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The input method's session logic (ADR-0105), free of Wayland and D-Bus
//! so that every transition is testable. The I/O layer feeds [`In`] events
//! and carries out the returned [`Out`] actions.

/// Something that happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum In {
    /// The daemon opened session S: bind the focused window.
    Open(String),
    /// Text for S; the I/O layer checked the lock state first.
    Type(String, String),
    /// An editing key for S, already checked like `Type`.
    Press(String, String),
    /// The shell hid the panel, or the key was pressed again.
    Close(String),
    /// A delivery for S found the session locked or the lock state unknown.
    Locked(String),
    /// KWin activated an input-method context (for whatever reason).
    Activated,
    /// KWin deactivated the context: focus left the window, it closed, or
    /// the screen locked.
    Deactivated,
    /// KWin did not activate a context in time after `forceActivate`.
    OpenTimeout(String),
    /// The daemon lane ended (restart, crash, `Conflict`): no session
    /// survives it, and there is nobody to report the end to.
    Lost,
    /// A delivery found the context the session grabbed no longer active:
    /// KWin moved on to another window before the machine heard of it.
    TargetLost,
    /// A key from the keyboard grab (evdev code).
    Key {
        code: u32,
        pressed: bool,
        serial: u32,
        time: u32,
    },
}

/// Something to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Out {
    /// `org.kde.kwin.VirtualKeyboard.forceActivate()`.
    ForceActivate,
    /// `grab_keyboard` on the current context.
    Grab,
    /// Hand keys back: `VirtualKeyboard.active = false`.
    Release,
    /// Start the 1 s activation timer for S.
    Wait(String),
    Opened(String),
    Input(String, &'static str),
    Closed(String, &'static str),
    /// `commit_string` on the context.
    Commit(String),
    /// `keysym` press and release on the context.
    Keysym(u32),
    /// Pass a key on to the application (`key` on the context).
    Forward {
        code: u32,
        pressed: bool,
        serial: u32,
        time: u32,
    },
}

/// The panel keys among evdev codes, by the name the shell receives.
pub fn panel_key(code: u32) -> Option<&'static str> {
    Some(match code {
        103 => "up",
        108 => "down",
        105 => "left",
        106 => "right",
        28 | 96 | 352 | 353 => "select", // ENTER, KPENTER, OK, SELECT
        1 | 158 => "back",               // ESC, BACK
        _ => return None,
    })
}

/// The keysym of an editing key.
pub fn press_keysym(key: &str) -> Option<u32> {
    Some(match key {
        "backspace" => 0xff08,
        "return" => 0xff0d,
        "left" => 0xff51,
        "right" => 0xff53,
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    /// `forceActivate` sent; waiting for KWin's context.
    Waiting,
    /// The context is grabbed; the panel is open.
    Open,
}

#[derive(Default)]
pub struct Machine {
    session: Option<(String, Phase)>,
    context: bool,
    grabbed: bool,
}

impl Machine {
    /// Whether a session is open (bound and grabbed).
    pub fn is_open(&self) -> bool {
        matches!(self.session, Some((_, Phase::Open)))
    }

    fn current(&self, session: &str) -> Option<&Phase> {
        self.session
            .as_ref()
            .filter(|(s, _)| s == session)
            .map(|(_, phase)| phase)
    }

    fn end(&mut self, reason: Option<&'static str>) -> Vec<Out> {
        let Some((session, _)) = self.session.take() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(reason) = reason {
            out.push(Out::Closed(session, reason));
        }
        if self.context {
            // KWin drops the context; its late deactivate is not this
            // machine's concern any more.
            out.push(Out::Release);
            self.context = false;
            self.grabbed = false;
        }
        out
    }

    pub fn handle(&mut self, event: In) -> Vec<Out> {
        match event {
            In::Open(session) => {
                let mut out = self.end(Some("unavailable"));
                if self.context {
                    // A context already exists for the focused window (it
                    // uses text-input): bind it without forcing.
                    self.session = Some((session.clone(), Phase::Open));
                    if !self.grabbed {
                        self.grabbed = true;
                        out.push(Out::Grab);
                    }
                    out.push(Out::Opened(session));
                } else {
                    self.session = Some((session.clone(), Phase::Waiting));
                    out.push(Out::ForceActivate);
                    out.push(Out::Wait(session));
                }
                out
            }
            In::Activated => {
                self.context = true;
                self.grabbed = false;
                match &self.session {
                    Some((session, Phase::Waiting)) => {
                        let session = session.clone();
                        self.session = Some((session.clone(), Phase::Open));
                        self.grabbed = true;
                        vec![Out::Grab, Out::Opened(session)]
                    }
                    // A new context while open belongs to another window:
                    // the target lost focus. Without a session it is
                    // ordinary text-input activity: never shown by itself.
                    Some((_, Phase::Open)) => self.end(Some("focus")),
                    None => Vec::new(),
                }
            }
            In::Deactivated => {
                self.context = false;
                self.grabbed = false;
                match &self.session {
                    Some((_, Phase::Open)) => self.end(Some("focus")),
                    _ => Vec::new(),
                }
            }
            In::OpenTimeout(session) => match self.current(&session) {
                Some(Phase::Waiting) => self.end(Some("unavailable")),
                _ => Vec::new(),
            },
            In::Close(session) => {
                if self.current(&session).is_some() {
                    self.end(None)
                } else {
                    Vec::new()
                }
            }
            In::Lost => self.end(None),
            In::TargetLost => match &self.session {
                Some((_, Phase::Open)) => self.end(Some("focus")),
                _ => Vec::new(),
            },
            In::Locked(session) => {
                if self.current(&session).is_some() {
                    self.end(Some("locked"))
                } else {
                    Vec::new()
                }
            }
            In::Type(session, text) => match self.current(&session) {
                Some(Phase::Open) if self.context => vec![Out::Commit(text)],
                _ => Vec::new(),
            },
            In::Press(session, key) => match (self.current(&session), press_keysym(&key)) {
                (Some(Phase::Open), Some(sym)) if self.context => vec![Out::Keysym(sym)],
                _ => Vec::new(),
            },
            In::Key {
                code,
                pressed,
                serial,
                time,
            } => {
                if let Some((session, Phase::Open)) = &self.session
                    && let Some(name) = panel_key(code)
                {
                    // Panel keys never reach the application, presses or
                    // releases.
                    return if pressed {
                        vec![Out::Input(session.clone(), name)]
                    } else {
                        Vec::new()
                    };
                }
                // Every other key, and every key without an open session,
                // goes on to the application.
                vec![Out::Forward {
                    code,
                    pressed,
                    serial,
                    time,
                }]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: &str = "10000000-0000-4000-8000-000000000001";
    const T: &str = "20000000-0000-4000-8000-000000000002";

    fn key(code: u32, pressed: bool) -> In {
        In::Key {
            code,
            pressed,
            serial: 7,
            time: 9,
        }
    }

    #[test]
    fn a_session_forces_a_context_grabs_it_and_types() {
        let mut m = Machine::default();
        assert_eq!(
            m.handle(In::Open(S.into())),
            [Out::ForceActivate, Out::Wait(S.into())]
        );
        // Nothing is typed before the context is bound.
        assert_eq!(m.handle(In::Type(S.into(), "a".into())), []);
        assert_eq!(m.handle(In::Activated), [Out::Grab, Out::Opened(S.into())]);
        assert_eq!(
            m.handle(In::Type(S.into(), "Привет".into())),
            [Out::Commit("Привет".into())]
        );
        assert_eq!(
            m.handle(In::Press(S.into(), "backspace".into())),
            [Out::Keysym(0xff08)]
        );
        assert_eq!(m.handle(In::Press(S.into(), "tab".into())), []);
        // Another session's records do nothing.
        assert_eq!(m.handle(In::Type(T.into(), "x".into())), []);
        assert_eq!(m.handle(In::OpenTimeout(S.into())), []);
    }

    #[test]
    fn panel_keys_go_to_the_shell_and_everything_else_to_the_application() {
        let mut m = Machine::default();
        // Without a session every key passes through.
        assert_eq!(
            m.handle(key(103, true)),
            [Out::Forward {
                code: 103,
                pressed: true,
                serial: 7,
                time: 9
            }]
        );
        m.handle(In::Open(S.into()));
        m.handle(In::Activated);
        assert_eq!(m.handle(key(103, true)), [Out::Input(S.into(), "up")]);
        assert_eq!(m.handle(key(103, false)), []);
        assert_eq!(m.handle(key(28, true)), [Out::Input(S.into(), "select")]);
        assert_eq!(m.handle(key(158, true)), [Out::Input(S.into(), "back")]);
        assert_eq!(m.handle(key(1, true)), [Out::Input(S.into(), "back")]);
        // Backspace (14) and letters reach the application.
        for code in [14, 30] {
            assert_eq!(
                m.handle(key(code, true)),
                [Out::Forward {
                    code,
                    pressed: true,
                    serial: 7,
                    time: 9
                }]
            );
        }
    }

    #[test]
    fn focus_loss_lock_and_close_end_the_session() {
        let mut m = Machine::default();
        m.handle(In::Open(S.into()));
        m.handle(In::Activated);
        // KWin deactivated the context: the target lost focus.
        assert_eq!(m.handle(In::Deactivated), [Out::Closed(S.into(), "focus")]);
        assert_eq!(m.handle(In::Type(S.into(), "x".into())), []);

        m.handle(In::Open(T.into()));
        m.handle(In::Activated);
        assert_eq!(
            m.handle(In::Locked(T.into())),
            [Out::Closed(T.into(), "locked"), Out::Release]
        );
        assert_eq!(m.handle(In::Press(T.into(), "return".into())), []);

        m.handle(In::Open(S.into()));
        assert_eq!(m.handle(In::Activated), [Out::Grab, Out::Opened(S.into())]);
        // The shell closes: nothing to report, keys handed back.
        assert_eq!(m.handle(In::Close(S.into())), [Out::Release]);
        assert_eq!(m.handle(In::Close(S.into())), []);
    }

    #[test]
    fn a_new_context_while_open_is_another_window() {
        let mut m = Machine::default();
        m.handle(In::Open(S.into()));
        m.handle(In::Activated);
        assert_eq!(
            m.handle(In::Activated),
            [Out::Closed(S.into(), "focus"), Out::Release]
        );
        // A context without a session never opens anything (no auto-show).
        let mut idle = Machine::default();
        assert_eq!(idle.handle(In::Activated), []);
        assert_eq!(idle.handle(In::Deactivated), []);
    }

    /// Codex P1 on #484: a delivery that found its context gone ends the
    /// session as `focus`; nothing else happens without an open session.
    #[test]
    fn a_lost_target_ends_the_session() {
        let mut m = Machine::default();
        assert_eq!(m.handle(In::TargetLost), []);
        m.handle(In::Open(S.into()));
        m.handle(In::Activated);
        assert_eq!(
            m.handle(In::TargetLost),
            [Out::Closed(S.into(), "focus"), Out::Release]
        );
        assert_eq!(m.handle(In::Type(S.into(), "x".into())), []);
    }

    /// Review of #484: a lost daemon lane hands the keys back at once.
    #[test]
    fn a_lost_lane_releases_the_grab_and_reports_nothing() {
        let mut m = Machine::default();
        m.handle(In::Open(S.into()));
        m.handle(In::Activated);
        assert_eq!(m.handle(In::Lost), [Out::Release]);
        assert!(!m.is_open());
        // A key of the old grab now goes on, as without a session; the I/O
        // layer drops the ones queued before the release.
        assert_eq!(m.handle(In::Type(S.into(), "x".into())), []);
        assert_eq!(m.handle(In::Lost), []);
        // While still waiting for KWin there is nothing to release.
        m.handle(In::Open(T.into()));
        assert_eq!(m.handle(In::Lost), []);
        assert_eq!(m.handle(In::Activated), []);
    }

    #[test]
    fn an_existing_context_is_bound_without_forcing() {
        let mut m = Machine::default();
        m.handle(In::Activated);
        assert_eq!(
            m.handle(In::Open(S.into())),
            [Out::Grab, Out::Opened(S.into())]
        );
    }

    #[test]
    fn no_context_in_time_is_unavailable() {
        let mut m = Machine::default();
        m.handle(In::Open(S.into()));
        assert_eq!(
            m.handle(In::OpenTimeout(S.into())),
            [Out::Closed(S.into(), "unavailable")]
        );
        assert_eq!(m.handle(In::Activated), []);
        // A new session replaces an open one; the old one is unavailable.
        let mut m = Machine::default();
        m.handle(In::Open(S.into()));
        m.handle(In::Activated);
        assert_eq!(
            m.handle(In::Open(T.into())),
            [
                Out::Closed(S.into(), "unavailable"),
                Out::Release,
                Out::ForceActivate,
                Out::Wait(T.into())
            ]
        );
    }
}
