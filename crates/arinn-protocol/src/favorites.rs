// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Daemon-owned ordered favorites (ADR-0084). No titles, URLs or transient IDs.
use crate::{
    production::EntityRef,
    validation::{self, ErrorCode, MAX_BODY_BYTES},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zbus::zvariant::{LE, Type, serialized::Context, to_bytes};

pub const INTERFACE: &str = "org.arinn.Favorites1";
pub const MAX_FAVORITES: usize = 1024;
/// Reserved daemon-owned namespace, never a provider capability.
pub const SHORTCUT_PROVIDER: &str = "arinn";
pub const SHORTCUT_CAPABILITY: &str = "web-shortcut";

/// Stable application launch target or an Arinn-owned shortcut ID.
pub fn target(value: &EntityRef) -> Result<(), ErrorCode> {
    validation::id(&value.local_id)?;
    match value.capability.as_str() {
        "application" => {
            validation::id(&value.provider_id)?;
        }
        SHORTCUT_CAPABILITY if value.provider_id == SHORTCUT_PROVIDER => {}
        _ => return Err(ErrorCode::InvalidArgument),
    }
    Ok(())
}

/// Complete ordered set; an empty list is an explicit empty selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct FavoriteList {
    pub revision: u64,
    pub entries: Vec<EntityRef>,
}
impl FavoriteList {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.revision == 0 || self.entries.len() > MAX_FAVORITES {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut seen = HashSet::new();
        for entry in &self.entries {
            target(entry)?;
            if !seen.insert((&entry.provider_id, &entry.capability, &entry.local_id)) {
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

#[zbus::proxy(
    interface = "org.arinn.Favorites1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait Favorites {
    fn list(&self) -> zbus::Result<(FavoriteList,)>;
    /// Atomically replace the complete ordered set at the captured revision.
    fn set(&self, revision: u64, entries: &[EntityRef]) -> zbus::Result<u64>;
    #[zbus(signal)]
    fn changed(&self, revision: u64) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> EntityRef {
        EntityRef {
            provider_id: "fixture.desktop".into(),
            capability: "application".into(),
            local_id: "org.example.Player.desktop".into(),
        }
    }
    #[test]
    fn signatures_and_order() {
        assert_eq!(FavoriteList::SIGNATURE.to_string(), "(ta(sss))");
        let shortcut = EntityRef {
            provider_id: SHORTCUT_PROVIDER.into(),
            capability: SHORTCUT_CAPABILITY.into(),
            local_id: "saved-site".into(),
        };
        let list = FavoriteList {
            revision: 1,
            entries: vec![app(), shortcut],
        };
        list.validate().unwrap();
        let bytes = to_bytes(Context::new_dbus(LE, 0), &(&list,)).unwrap();
        let decoded: (FavoriteList,) = bytes.deserialize().unwrap().0;
        assert_eq!(decoded.0, list);
        FavoriteList {
            revision: 1,
            entries: vec![],
        }
        .validate()
        .unwrap();
    }
    #[test]
    fn rejects_duplicates_transient_targets_and_bounds() {
        let invalid = |entries| {
            assert_eq!(
                FavoriteList {
                    revision: 1,
                    entries
                }
                .validate(),
                Err(ErrorCode::InvalidArgument)
            )
        };
        invalid(vec![app(), app()]);
        for capability in [
            "browser",
            "media",
            "library",
            "unknown",
            SHORTCUT_CAPABILITY,
        ] {
            let mut entry = app();
            entry.capability = capability.into();
            invalid(vec![entry]);
        }
        let mut entry = app();
        entry.local_id.clear();
        invalid(vec![entry]);
        let mut entry = app();
        entry.provider_id.clear();
        invalid(vec![entry]);
        invalid(vec![app(); MAX_FAVORITES + 1]);
        assert_eq!(
            FavoriteList {
                revision: 0,
                entries: vec![]
            }
            .validate(),
            Err(ErrorCode::InvalidArgument)
        );
        let entries = (0..MAX_FAVORITES)
            .map(|n| {
                let mut entry = app();
                entry.local_id = format!("{n:04}{}", "x".repeat(252));
                entry
            })
            .collect();
        invalid(entries);
    }
}
