// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pinned music stations (ADR-0143, ADR-0142 MH-6). The daemon keeps what Home
//! shows for a pinned station while its provider is away: the provider and
//! station identity, the title and the artwork ID. The order is the
//! favorites order: a pin is a `Favorites1` target with capability
//! [`MUSIC_STATION_CAPABILITY`], and both interfaces share one revision and
//! the `Favorites1.Changed` signal.
use crate::{
    favorites::{MAX_FAVORITES, MUSIC_STATION_CAPABILITY},
    production::EntityRef,
    validation::{self, ErrorCode, MAX_BODY_BYTES},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zbus::zvariant::{LE, Type, serialized::Context, to_bytes};

pub const INTERFACE: &str = "org.arinn.MusicPins1";
pub const MAX_TITLE_BYTES: usize = 256;

/// One pinned station. `station_id` is the provider's pinnable station ID
/// (`Entity.pinnable`, #811); `artwork` is the station's opaque artwork ID or
/// empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MusicPin {
    pub provider_id: String,
    pub station_id: String,
    pub title: String,
    pub artwork: String,
}
impl MusicPin {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.provider_id)?;
        validation::id(&self.station_id)?;
        if self.title.trim().is_empty()
            || self.title.len() > MAX_TITLE_BYTES
            || self.title.chars().any(char::is_control)
        {
            return Err(ErrorCode::InvalidArgument);
        }
        // An opaque asset ID, as `music::Entity.artwork`: never a path or URL.
        if !self.artwork.is_empty()
            && (self.artwork.len() > 256
                || !self
                    .artwork
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b)))
        {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
    /// The favorites target this pin occupies.
    pub fn target(&self) -> EntityRef {
        EntityRef {
            provider_id: self.provider_id.clone(),
            capability: MUSIC_STATION_CAPABILITY.into(),
            local_id: self.station_id.clone(),
        }
    }
}

/// The pinned stations in favorites order, at the favorites revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MusicPinList {
    pub revision: u64,
    pub pins: Vec<MusicPin>,
}
impl MusicPinList {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.revision == 0 || self.pins.len() > MAX_FAVORITES {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut seen = HashSet::new();
        for pin in &self.pins {
            pin.validate()?;
            if !seen.insert((&pin.provider_id, &pin.station_id)) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        let bytes =
            to_bytes(Context::new_dbus(LE, 0), &(self,)).map_err(|_| ErrorCode::InvalidArgument)?;
        if bytes.len() > MAX_BODY_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}

/// `org.arinn.MusicPins1` on the daemon service and root path.
#[zbus::proxy(
    interface = "org.arinn.MusicPins1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait MusicPins {
    fn list(&self) -> zbus::Result<(MusicPinList,)>;
    /// Adds or refreshes a pin at the captured favorites revision; a new pin
    /// is appended to the favorites. Returns the new revision.
    fn pin(&self, revision: u64, pin: &MusicPin) -> zbus::Result<u64>;
    /// Removes a pin and its favorites entry. Returns the new revision.
    fn unpin(&self, revision: u64, provider_id: &str, station_id: &str) -> zbus::Result<u64>;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pin() -> MusicPin {
        MusicPin {
            provider_id: "fixture.music".into(),
            station_id: "station:library".into(),
            title: "Fixture station".into(),
            artwork: "cover:fixture".into(),
        }
    }
    #[test]
    fn signatures_and_round_trip() {
        assert_eq!(MusicPin::SIGNATURE.to_string(), "(ssss)");
        assert_eq!(MusicPinList::SIGNATURE.to_string(), "(ta(ssss))");
        let list = MusicPinList {
            revision: 3,
            pins: vec![pin()],
        };
        list.validate().unwrap();
        let bytes = to_bytes(Context::new_dbus(LE, 0), &(&list,)).unwrap();
        let decoded: (MusicPinList,) = bytes.deserialize().unwrap().0;
        assert_eq!(decoded.0, list);
        assert_eq!(pin().target().capability, MUSIC_STATION_CAPABILITY);
        crate::favorites::target(&pin().target()).unwrap();
    }
    #[test]
    fn rejects_bad_pins_and_duplicates() {
        let bad = |change: fn(&mut MusicPin)| {
            let mut value = pin();
            change(&mut value);
            assert_eq!(value.validate(), Err(ErrorCode::InvalidArgument));
        };
        bad(|p| p.provider_id.clear());
        bad(|p| p.station_id.clear());
        bad(|p| p.title = "  ".into());
        bad(|p| p.title = "line\nbreak".into());
        bad(|p| p.title = "x".repeat(MAX_TITLE_BYTES + 1));
        bad(|p| p.artwork = "/home/user/cover.jpg".into());
        bad(|p| p.artwork = "https://example.invalid/a".into());
        let mut empty = pin();
        empty.artwork.clear();
        empty.validate().unwrap();
        assert_eq!(
            MusicPinList {
                revision: 1,
                pins: vec![pin(), pin()],
            }
            .validate(),
            Err(ErrorCode::InvalidArgument)
        );
        assert_eq!(
            MusicPinList {
                revision: 0,
                pins: vec![],
            }
            .validate(),
            Err(ErrorCode::InvalidArgument)
        );
    }
}
