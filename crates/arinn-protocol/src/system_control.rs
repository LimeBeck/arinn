// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! System actions for the shell: power (suspend, reboot, power off) and the
//! default output's volume and mute (ADR-0108). Bindings and validation only;
//! producer and reader rules are in docs/architecture/system-control.md.
use crate::{system_status::Audio, validation::ErrorCode};
use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// Daemon-root interface at `/org/arinn/Daemon1`.
pub const INTERFACE: &str = "org.arinn.SystemControl1";
/// The wire value every enum may carry.
pub const UNKNOWN: &str = "unknown";
/// Highest level a producer reports, in percent. Sound servers allow
/// amplification above 100 %; Arinn never steps above 100 % itself.
pub const MAX_LEVEL: u32 = 150;
/// The level `StepVolume` stops at when stepping up.
pub const FULL_LEVEL: u32 = 100;
/// Percentage points per volume step.
pub const STEP_PERCENT: u32 = 5;
/// Largest number of steps in one `StepVolume` call, in either direction.
pub const MAX_STEPS: i32 = 20;
/// Upper bound of inhibitors in one power state.
pub const MAX_INHIBITORS: usize = 16;
/// Upper bound of an inhibitor's `who` in bytes.
pub const MAX_WHO_BYTES: usize = 128;
/// Upper bound of an inhibitor's `why` in bytes.
pub const MAX_WHY_BYTES: usize = 256;

/// One power action of the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Suspend,
    Reboot,
    PowerOff,
}
impl PowerAction {
    pub const ALL: [Self; 3] = [Self::Suspend, Self::Reboot, Self::PowerOff];
    /// The inhibitor lock that blocks this action.
    pub const fn lock(self) -> Lock {
        match self {
            Self::Suspend => Lock::Sleep,
            Self::Reboot | Self::PowerOff => Lock::Shutdown,
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Suspend => "suspend",
            Self::Reboot => "reboot",
            Self::PowerOff => "power-off",
        }
    }
}

/// What an inhibitor holds, as far as Arinn's power actions are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lock {
    Unknown,
    Sleep,
    Shutdown,
}

/// Whether a power action can be requested now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// The platform could not say; the shell may still try.
    Unknown,
    /// The platform allows it without asking.
    Available,
    /// A block inhibitor holds it; `inhibitors` names the holder.
    Inhibited,
    /// The platform would need authorization or refuses it for this user.
    Denied,
    /// The system cannot do it at all.
    Unsupported,
}

macro_rules! wire_enum {
    ($name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self {
                    Self::Unknown => UNKNOWN,
                    $(Self::$variant => $wire,)+
                }
            }
            /// Reader side: a value this build does not know is `Unknown`.
            pub fn parse(value: &str) -> Self {
                match value {
                    $($wire => Self::$variant,)+
                    _ => Self::Unknown,
                }
            }
            /// Producer side: only the values this contract defines.
            #[allow(dead_code)]
            fn known(value: &str) -> bool {
                value == UNKNOWN || Self::parse(value) != Self::Unknown
            }
        }
    };
}
wire_enum!(Lock { Sleep => "sleep", Shutdown => "shutdown" });
wire_enum!(Availability {
    Available => "available",
    Inhibited => "inhibited",
    Denied => "denied",
    Unsupported => "unsupported",
});

/// A block inhibitor that holds a power action. `who` and `why` are the
/// holder's own words, bounded and single-line. They are shown to the user
/// and never logged (spec §13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Inhibitor {
    pub what: String,
    pub who: String,
    pub why: String,
}
impl Inhibitor {
    pub fn lock(&self) -> Lock {
        Lock::parse(&self.what)
    }
}

/// Power availability, read when the shell asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PowerState {
    pub suspend: String,
    pub reboot: String,
    pub power_off: String,
    pub inhibitors: Vec<Inhibitor>,
}
impl PowerState {
    /// Nothing known, as on a platform without logind.
    pub fn unknown() -> Self {
        Self {
            suspend: UNKNOWN.into(),
            reboot: UNKNOWN.into(),
            power_off: UNKNOWN.into(),
            inhibitors: vec![],
        }
    }
    pub fn availability(&self, action: PowerAction) -> Availability {
        Availability::parse(match action {
            PowerAction::Suspend => &self.suspend,
            PowerAction::Reboot => &self.reboot,
            PowerAction::PowerOff => &self.power_off,
        })
    }
    /// Reader side: the inhibitors that hold `action`.
    pub fn inhibitors_of(&self, action: PowerAction) -> impl Iterator<Item = &Inhibitor> {
        self.inhibitors
            .iter()
            .filter(move |i| i.lock() == action.lock())
            .take(MAX_INHIBITORS)
    }
    /// What a producer may publish: known values, bounded single-line
    /// inhibitors of a known lock, and `inhibited` only with an inhibitor
    /// that holds the action.
    pub fn validate(&self) -> Result<(), ErrorCode> {
        let bounded = |text: &str, max: usize| text.len() <= max && !text.chars().any(breaks_line);
        let inhibitors_ok = self.inhibitors.len() <= MAX_INHIBITORS
            && self.inhibitors.iter().all(|i| {
                i.lock() != Lock::Unknown
                    && bounded(&i.who, MAX_WHO_BYTES)
                    && bounded(&i.why, MAX_WHY_BYTES)
            });
        let actions_ok = PowerAction::ALL.into_iter().all(|action| {
            let value = match action {
                PowerAction::Suspend => &self.suspend,
                PowerAction::Reboot => &self.reboot,
                PowerAction::PowerOff => &self.power_off,
            };
            Availability::known(value)
                && (self.availability(action) != Availability::Inhibited
                    || self.inhibitors_of(action).next().is_some())
        });
        if inhibitors_ok && actions_ok {
            Ok(())
        } else {
            Err(ErrorCode::InvalidArgument)
        }
    }
}

/// A character that is not allowed in single-line text: a control character
/// or a Unicode line or paragraph separator (U+2028, U+2029), which renderers
/// break lines at although they are not control characters.
fn breaks_line(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

/// Producer helper: an inhibitor text as the contract allows it. Control
/// characters and line or paragraph separators become spaces, the ends are trimmed and the text is cut at a
/// character boundary to `max` bytes.
pub fn bounded_text(text: &str, max: usize) -> String {
    let clean: String = text
        .chars()
        .map(|c| if breaks_line(c) { ' ' } else { c })
        .collect();
    let clean = clean.trim();
    let mut end = clean.len().min(max);
    while !clean.is_char_boundary(end) {
        end -= 1;
    }
    clean[..end].trim_end().to_owned()
}

/// The default output's mute state and level after a read or a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Volume {
    pub audio: String,
    pub level: u32,
}
impl Volume {
    pub fn unknown() -> Self {
        Self {
            audio: UNKNOWN.into(),
            level: 0,
        }
    }
    pub fn audio(&self) -> Audio {
        Audio::parse(&self.audio)
    }
    /// Reader side: the level in percent, 0 when the state is unknown and at
    /// most [`MAX_LEVEL`].
    pub fn level(&self) -> u32 {
        match self.audio() {
            Audio::Unknown => 0,
            _ => self.level.min(MAX_LEVEL),
        }
    }
    /// What a producer may publish.
    pub fn validate(&self) -> Result<(), ErrorCode> {
        let audio_ok = self.audio == UNKNOWN || self.audio() != Audio::Unknown;
        let level_ok =
            self.level <= MAX_LEVEL && (self.audio() != Audio::Unknown || self.level == 0);
        if audio_ok && level_ok {
            Ok(())
        } else {
            Err(ErrorCode::InvalidArgument)
        }
    }
}

/// `StepVolume` argument check.
pub fn steps(value: i32) -> Result<i32, ErrorCode> {
    if value != 0 && value.checked_abs().is_some_and(|v| v <= MAX_STEPS) {
        Ok(value)
    } else {
        Err(ErrorCode::InvalidArgument)
    }
}

/// The level after `steps` from `current`: 5 points per step, never below 0
/// and never above 100 % when stepping up. A level already above 100 % is
/// kept when stepping up.
pub fn stepped_level(current: u32, steps: i32) -> u32 {
    let delta = steps.unsigned_abs().saturating_mul(STEP_PERCENT);
    if steps > 0 {
        if current >= FULL_LEVEL {
            current
        } else {
            current.saturating_add(delta).min(FULL_LEVEL)
        }
    } else {
        current.saturating_sub(delta)
    }
}

/// Whether "Switch to desktop" can be requested ([#438]): `available` only
/// inside the dedicated Arinn login session, `unsupported` elsewhere. A
/// reader treats any other value as `unknown`, which it does not offer.
///
/// [#438]: https://github.com/LimeBeck/arinn/issues/438
pub fn session_switch(value: &str) -> Availability {
    match Availability::parse(value) {
        availability @ (Availability::Available | Availability::Unsupported) => availability,
        _ => Availability::Unknown,
    }
}

#[zbus::proxy(
    interface = "org.arinn.SystemControl1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait SystemControlApi {
    fn get_power(&self) -> zbus::Result<(PowerState,)>;
    fn suspend(&self) -> zbus::Result<()>;
    fn reboot(&self) -> zbus::Result<()>;
    fn power_off(&self) -> zbus::Result<()>;
    fn get_volume(&self) -> zbus::Result<(Volume,)>;
    fn set_muted(&self, muted: bool) -> zbus::Result<(Volume,)>;
    fn toggle_muted(&self) -> zbus::Result<(Volume,)>;
    fn step_volume(&self, steps: i32) -> zbus::Result<(Volume,)>;
    fn get_session_switch(&self) -> zbus::Result<String>;
    fn switch_to_desktop(&self) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{LE, serialized::Context, to_bytes};

    fn inhibitor(what: &str) -> Inhibitor {
        Inhibitor {
            what: what.into(),
            who: "Fixture player".into(),
            why: "Playing".into(),
        }
    }
    fn sample() -> PowerState {
        PowerState {
            suspend: "inhibited".into(),
            reboot: "available".into(),
            power_off: "denied".into(),
            inhibitors: vec![inhibitor("sleep")],
        }
    }

    #[test]
    fn signatures_and_roundtrip() {
        assert_eq!(PowerState::SIGNATURE.to_string(), "(sssa(sss))");
        assert_eq!(Volume::SIGNATURE.to_string(), "(su)");
        for value in [sample(), PowerState::unknown()] {
            value.validate().unwrap();
            let bytes = to_bytes(Context::new_dbus(LE, 0), &(&value,)).unwrap();
            let (decoded,): (PowerState,) = bytes.deserialize().unwrap().0;
            assert_eq!(decoded, value);
        }
        let volume = Volume {
            audio: "muted".into(),
            level: 35,
        };
        volume.validate().unwrap();
        let bytes = to_bytes(Context::new_dbus(LE, 0), &(&volume,)).unwrap();
        let (decoded,): (Volume,) = bytes.deserialize().unwrap().0;
        assert_eq!(decoded, volume);
    }

    #[test]
    fn readers_fall_back_to_unknown() {
        let mut state = sample();
        state.reboot = "hibernating".into();
        assert_eq!(
            state.availability(PowerAction::Reboot),
            Availability::Unknown
        );
        assert_eq!(
            state.availability(PowerAction::Suspend),
            Availability::Inhibited
        );
        state.inhibitors.push(inhibitor("idle"));
        state.inhibitors.push(inhibitor("shutdown"));
        assert_eq!(state.inhibitors_of(PowerAction::Suspend).count(), 1);
        assert_eq!(state.inhibitors_of(PowerAction::PowerOff).count(), 1);
        let volume = Volume {
            audio: "ducked".into(),
            level: 40,
        };
        assert_eq!((volume.audio(), volume.level()), (Audio::Unknown, 0));
        let loud = Volume {
            audio: "unmuted".into(),
            level: 400,
        };
        assert_eq!(loud.level(), MAX_LEVEL);
    }

    #[test]
    fn producers_publish_only_the_contract() {
        let mut invalid = vec![];
        let mut v = sample();
        v.suspend = "Available".into();
        invalid.push(v);
        // Inhibited needs an inhibitor that holds the action.
        let mut v = sample();
        v.inhibitors = vec![inhibitor("shutdown")];
        invalid.push(v);
        let mut v = sample();
        v.inhibitors.push(inhibitor("idle"));
        invalid.push(v);
        let mut v = sample();
        v.inhibitors[0].who = "two\nlines".into();
        invalid.push(v);
        let mut v = sample();
        v.inhibitors[0].why = "x".repeat(MAX_WHY_BYTES + 1);
        invalid.push(v);
        for separator in ['\u{2028}', '\u{2029}'] {
            let mut v = sample();
            v.inhibitors[0].why = format!("two{separator}lines");
            invalid.push(v);
        }
        let mut v = sample();
        v.inhibitors = vec![inhibitor("sleep"); MAX_INHIBITORS + 1];
        invalid.push(v);
        for v in invalid {
            assert_eq!(v.validate(), Err(ErrorCode::InvalidArgument), "{v:?}");
        }
        for (audio, level, ok) in [
            ("unknown", 0, true),
            ("unknown", 5, false),
            ("muted", MAX_LEVEL, true),
            ("unmuted", MAX_LEVEL + 1, false),
            ("Muted", 10, false),
        ] {
            let volume = Volume {
                audio: audio.into(),
                level,
            };
            assert_eq!(volume.validate().is_ok(), ok, "{volume:?}");
        }
    }

    #[test]
    fn session_switch_has_two_producer_values() {
        assert_eq!(session_switch("available"), Availability::Available);
        assert_eq!(session_switch("unsupported"), Availability::Unsupported);
        for value in ["inhibited", "denied", "unknown", "Available", ""] {
            assert_eq!(session_switch(value), Availability::Unknown, "{value}");
        }
    }

    #[test]
    fn inhibitor_text_is_bounded() {
        assert_eq!(bounded_text("  a\tb\nc  ", 64), "a b c");
        // Unicode line and paragraph separators break lines too.
        assert_eq!(bounded_text("a\u{2028}b\u{2029}c\u{85}d", 64), "a b c d");
        // Cut at a character boundary, never inside one.
        assert_eq!(bounded_text("ёёё", 3), "ё");
        assert_eq!(bounded_text(&"x".repeat(300), MAX_WHY_BYTES).len(), 256);
    }

    #[test]
    fn steps_stay_within_bounds() {
        assert_eq!(steps(0), Err(ErrorCode::InvalidArgument));
        assert_eq!(steps(MAX_STEPS + 1), Err(ErrorCode::InvalidArgument));
        assert_eq!(steps(-MAX_STEPS - 1), Err(ErrorCode::InvalidArgument));
        assert_eq!(steps(i32::MIN), Err(ErrorCode::InvalidArgument));
        assert_eq!(steps(-3), Ok(-3));
        for (current, step, level) in [
            (50, 1, 55),
            (98, 1, 100),
            (100, 1, 100),
            (120, 1, 120),
            (120, -1, 115),
            (3, -1, 0),
            (0, -20, 0),
            (0, 20, 100),
        ] {
            assert_eq!(stepped_level(current, step), level, "{current} {step}");
        }
    }
}
