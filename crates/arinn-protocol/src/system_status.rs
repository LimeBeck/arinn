// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read-only system status for Home's status area: mute and network
//! connectivity (ADR-0100). Bindings and validation only; producer and reader
//! rules are in docs/architecture/system-status.md.
use crate::{platform, validation::ErrorCode};
use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// Daemon-root snapshot of platform state.
pub const INTERFACE: &str = "org.arinn.SystemStatus1";
/// Highest coarse signal level; 0 means unknown or not applicable.
pub const MAX_SIGNAL: u8 = 4;
/// The wire value every field may carry.
pub const UNKNOWN: &str = "unknown";

/// Whether the default audio output is muted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audio {
    Unknown,
    Muted,
    Unmuted,
}

/// Whether the system has an active network connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Unknown,
    Connected,
    Disconnected,
}

/// The kind of the connection that carries the default route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Unknown,
    Wifi,
    Wired,
    Other,
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
            fn known(value: &str) -> bool {
                value == UNKNOWN || Self::parse(value) != Self::Unknown
            }
        }
    };
}
wire_enum!(Audio { Muted => "muted", Unmuted => "unmuted" });
wire_enum!(Network { Connected => "connected", Disconnected => "disconnected" });
wire_enum!(Link { Wifi => "wifi", Wired => "wired", Other => "other" });

/// One coherent daemon-owned snapshot. All fields `unknown` with signal 0 is
/// the normal state of a platform without a producer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SystemStatus {
    pub epoch: String,
    pub revision: u64,
    pub audio: String,
    pub network: String,
    pub link: String,
    pub signal: u8,
}
impl SystemStatus {
    /// A snapshot with nothing known, as a daemon publishes it without a
    /// producer.
    pub fn unknown(epoch: impl Into<String>, revision: u64) -> Self {
        Self {
            epoch: epoch.into(),
            revision,
            audio: UNKNOWN.into(),
            network: UNKNOWN.into(),
            link: UNKNOWN.into(),
            signal: 0,
        }
    }
    pub fn audio(&self) -> Audio {
        Audio::parse(&self.audio)
    }
    pub fn network(&self) -> Network {
        Network::parse(&self.network)
    }
    /// The link kind, `Unknown` unless the network is connected.
    pub fn link(&self) -> Link {
        match self.network() {
            Network::Connected => Link::parse(&self.link),
            _ => Link::Unknown,
        }
    }
    /// The coarse signal level, 0 unless connected over Wi-Fi.
    pub fn signal(&self) -> u8 {
        match self.link() {
            Link::Wifi if self.signal <= MAX_SIGNAL => self.signal,
            _ => 0,
        }
    }
    /// What a producer may publish: known values only, a link only while
    /// connected, and a signal only for Wi-Fi.
    pub fn validate(&self) -> Result<(), ErrorCode> {
        platform::generation(&self.epoch)?;
        let consistent = self.revision != 0
            && Audio::known(&self.audio)
            && Network::known(&self.network)
            && Link::known(&self.link)
            && (self.network() == Network::Connected || self.link == UNKNOWN)
            && self.signal <= MAX_SIGNAL
            && (self.signal == 0 || Link::parse(&self.link) == Link::Wifi);
        if consistent {
            Ok(())
        } else {
            Err(ErrorCode::InvalidArgument)
        }
    }
}

#[zbus::proxy(
    interface = "org.arinn.SystemStatus1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait SystemStatusApi {
    fn get(&self) -> zbus::Result<(SystemStatus,)>;
    #[zbus(signal)]
    fn changed(&self, epoch: &str, revision: u64) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{LE, serialized::Context, to_bytes};

    const EPOCH: &str = "30000000-0000-4000-8000-000000000003";
    fn sample() -> SystemStatus {
        SystemStatus {
            epoch: EPOCH.into(),
            revision: 1,
            audio: "muted".into(),
            network: "connected".into(),
            link: "wifi".into(),
            signal: 3,
        }
    }

    #[test]
    fn signature_and_roundtrip() {
        assert_eq!(SystemStatus::SIGNATURE.to_string(), "(stsssy)");
        for value in [sample(), SystemStatus::unknown(EPOCH, 7)] {
            value.validate().unwrap();
            let bytes = to_bytes(Context::new_dbus(LE, 0), &(&value,)).unwrap();
            let (decoded,): (SystemStatus,) = bytes.deserialize().unwrap().0;
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn wire_values() {
        for audio in [Audio::Unknown, Audio::Muted, Audio::Unmuted] {
            assert_eq!(Audio::parse(audio.as_str()), audio);
        }
        for network in [Network::Unknown, Network::Connected, Network::Disconnected] {
            assert_eq!(Network::parse(network.as_str()), network);
        }
        for link in [Link::Unknown, Link::Wifi, Link::Wired, Link::Other] {
            assert_eq!(Link::parse(link.as_str()), link);
        }
        let value = sample();
        assert_eq!(
            (value.audio(), value.network(), value.link(), value.signal()),
            (Audio::Muted, Network::Connected, Link::Wifi, 3)
        );
    }

    #[test]
    fn readers_treat_unknown_values_and_inconsistency_as_unknown() {
        // A newer daemon may add values; an older reader shows nothing.
        let mut value = sample();
        value.audio = "ducked".into();
        value.network = "limited".into();
        assert_eq!(value.audio(), Audio::Unknown);
        assert_eq!(value.network(), Network::Unknown);
        assert_eq!((value.link(), value.signal()), (Link::Unknown, 0));
        // Signal belongs to Wi-Fi only, within range.
        let mut value = sample();
        value.link = "wired".into();
        assert_eq!((value.link(), value.signal()), (Link::Wired, 0));
        let mut value = sample();
        value.signal = MAX_SIGNAL + 1;
        assert_eq!(value.signal(), 0);
    }

    #[test]
    fn malformed_snapshots_are_rejected() {
        let mut invalid = vec![];
        let mut v = sample();
        v.revision = 0;
        invalid.push(v);
        let mut v = sample();
        v.epoch = "not-an-epoch".into();
        invalid.push(v);
        let mut v = sample();
        v.epoch = uuid::Uuid::nil().to_string();
        invalid.push(v);
        for field in 0..3 {
            let mut v = sample();
            let target = match field {
                0 => &mut v.audio,
                1 => &mut v.network,
                _ => &mut v.link,
            };
            *target = "Muted".into();
            invalid.push(v);
        }
        let mut v = sample();
        v.audio.clear();
        invalid.push(v);
        // A link or a signal without a connection.
        for network in ["disconnected", "unknown"] {
            let mut v = sample();
            v.network = network.into();
            invalid.push(v.clone());
            v.link = UNKNOWN.into();
            invalid.push(v);
        }
        // A signal on a wired link, or out of range.
        let mut v = sample();
        v.link = "wired".into();
        invalid.push(v);
        let mut v = sample();
        v.signal = MAX_SIGNAL + 1;
        invalid.push(v);
        for v in invalid {
            assert_eq!(v.validate(), Err(ErrorCode::InvalidArgument), "{v:?}");
        }
        // Valid combinations.
        let mut v = sample();
        v.signal = 0;
        v.validate().unwrap();
        v.link = "wired".into();
        v.validate().unwrap();
        v.link = UNKNOWN.into();
        v.validate().unwrap();
        v.network = "disconnected".into();
        v.audio = "unmuted".into();
        v.validate().unwrap();
    }
}
