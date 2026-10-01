// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Optional Return/window association (ADR-0073). Bindings and validation only;
//! no production daemon or platform implementation is provided by this module.
use crate::{
    platform,
    validation::{self, ErrorCode},
};
use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

pub const INTERFACE: &str = "org.arinn.ReturnAssociation1";
pub const BRIDGE_INTERFACE: &str = "org.arinn.KWinReturn1";

/// Both fields are opaque. Never parse a token or substitute a daemon/provider
/// epoch for the authoritative platform inventory generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct WindowRef {
    pub generation: String,
    pub token: String,
}
impl WindowRef {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        platform::generation(&self.generation)?;
        validation::id(&self.token)
    }
}

/// One coherent daemon-owned snapshot. `window` is a typed optional value:
/// zero entries means unresolved/unavailable, one means an explicit link.
/// Absence never invalidates the existing opaque Return target itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ReturnAssociation {
    pub epoch: String,
    pub revision: u64,
    pub target: String,
    pub window: Vec<WindowRef>,
}
impl ReturnAssociation {
    /// Structural validation only. The producer must also authenticate the
    /// adapter and validate current Home/inventory lifetimes before publication.
    pub fn validate(&self) -> Result<(), ErrorCode> {
        platform::generation(&self.epoch)?;
        validation::text(&self.target)?;
        if self.revision == 0
            || self.window.len() > 1
            || (self.target.is_empty() && !self.window.is_empty())
        {
            return Err(ErrorCode::InvalidArgument);
        }
        for window in &self.window {
            window.validate()?;
        }
        Ok(())
    }
}

/// Scalar bridge arguments fit KWin callDBus without nested variants or uint64
/// precision loss. Empty token explicitly clears the optional association.
/// Runtime ownership, exact committed revision and target checks are separate.
pub fn validate_report(
    generation: &str,
    inventory_revision: i32,
    target: &str,
    token: &str,
) -> Result<(), ErrorCode> {
    platform::generation(generation)?;
    if inventory_revision <= 0 {
        return Err(ErrorCode::InvalidArgument);
    }
    validation::text(target)?;
    if !token.is_empty() {
        if target.is_empty() {
            return Err(ErrorCode::InvalidArgument);
        }
        validation::id(token)?;
    }
    Ok(())
}

#[zbus::proxy(
    interface = "org.arinn.ReturnAssociation1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait ReturnAssociationApi {
    fn get(&self) -> zbus::Result<(ReturnAssociation,)>;
    #[zbus(signal)]
    fn changed(&self, epoch: &str, revision: u64) -> zbus::Result<()>;
}

/// Optional trusted adapter transport, never a provider capability. Separate
/// from KWinBridge1 so existing implementations/XML remain fully compatible.
#[zbus::proxy(
    interface = "org.arinn.KWinReturn1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait KWinReturn {
    fn report(
        &self,
        generation: &str,
        inventory_revision: i32,
        target: &str,
        token: &str,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{LE, serialized::Context, to_bytes};
    fn sample() -> ReturnAssociation {
        ReturnAssociation {
            epoch: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            target: uuid::Uuid::new_v4().to_string(),
            window: vec![WindowRef {
                generation: uuid::Uuid::new_v4().to_string(),
                token: "opaque token:never parse".into(),
            }],
        }
    }
    #[test]
    fn signatures_and_optional_roundtrip_preserve_legacy_status() {
        assert_eq!(crate::Status::SIGNATURE.to_string(), "(sbstbsssbbss)");
        assert_eq!(WindowRef::SIGNATURE.to_string(), "(ss)");
        assert_eq!(ReturnAssociation::SIGNATURE.to_string(), "(stsa(ss))");
        let mut value = sample();
        for present in [true, false] {
            if !present {
                value.window.clear();
            }
            value.validate().unwrap();
            let bytes = to_bytes(Context::new_dbus(LE, 0), &(&value,)).unwrap();
            let (decoded,): (ReturnAssociation,) = bytes.deserialize().unwrap().0;
            assert_eq!(decoded, value);
        }
    }
    #[test]
    fn malformed_optional_values_and_report_bounds_are_rejected() {
        let good = sample();
        let mut invalid = vec![];
        let mut v = good.clone();
        v.revision = 0;
        invalid.push(v);
        let mut v = good.clone();
        v.epoch.clear();
        invalid.push(v);
        let mut v = good.clone();
        v.target.clear();
        invalid.push(v);
        let mut v = good.clone();
        v.target = "x".repeat(8193);
        invalid.push(v);
        let mut v = good.clone();
        v.window.push(v.window[0].clone());
        invalid.push(v);
        let mut v = good.clone();
        v.window[0].generation = uuid::Uuid::nil().to_string();
        invalid.push(v);
        let mut v = good.clone();
        v.window[0].token.clear();
        invalid.push(v);
        let mut v = good.clone();
        v.window[0].token = "я".repeat(129);
        invalid.push(v);
        for v in invalid {
            assert_eq!(v.validate(), Err(ErrorCode::InvalidArgument));
        }
        let g = &good.window[0].generation;
        assert!(validate_report(g, 1, &good.target, "").is_ok());
        assert!(validate_report(g, 1, "", "").is_ok());
        for (revision, target, token) in [
            (0, "t", "w"),
            (-1, "t", "w"),
            (1, "", "w"),
            (1, "bad\0target", "w"),
        ] {
            assert!(validate_report(g, revision, target, token).is_err());
        }
        assert!(validate_report("not-an-epoch", 1, "t", "w").is_err());
        assert!(validate_report(g, 1, "t", &"x".repeat(257)).is_err());
    }
}
