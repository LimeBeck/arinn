// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Optional trusted Activity background transport. Bindings and structural
//! validation only; runtime ownership, sealed descriptor and lifetime checks
//! belong to the daemon and platform implementations (ADR-0087).
use crate::{
    platform,
    return_association::WindowRef,
    validation::{self, ErrorCode},
};
use serde::{Deserialize, Serialize};
use zbus::zvariant::{OwnedFd, Type};

pub const MAX_WIDTH: u32 = 1920;
pub const MAX_HEIGHT: u32 = 1080;
pub const MAX_FRAME_BYTES: u64 = 8 * 1024 * 1024;
pub const CAPTURE_DEADLINE_MS: u64 = 1500;

/// Every lifetime is explicit; none is inferred from a title or filename.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct BackgroundContext {
    pub epoch: String,
    pub request_id: String,
    pub target: String,
    pub output: String,
    pub window: WindowRef,
}
impl BackgroundContext {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        platform::generation(&self.epoch)?;
        platform::generation(&self.request_id)?;
        validation::text(&self.target)?;
        validation::text(&self.output)?;
        if self.target.is_empty() || self.output.is_empty() {
            return Err(ErrorCode::InvalidArgument);
        }
        self.window.validate()
    }
}

/// Atomic read-only state. Pending may have no context while the exact
/// Return/window association is unresolved; it still has a finite deadline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct BackgroundSnapshot {
    pub epoch: String,
    pub revision: u64,
    pub context: Vec<BackgroundContext>,
    pub state: String,
}
impl BackgroundSnapshot {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        platform::generation(&self.epoch)?;
        if self.revision == 0
            || self.context.len() > 1
            || !matches!(
                self.state.as_str(),
                "idle" | "pending" | "ready" | "unavailable"
            )
            || (self.state == "idle" && !self.context.is_empty())
            || (self.state == "ready" && self.context.len() != 1)
        {
            return Err(ErrorCode::InvalidArgument);
        }
        for context in &self.context {
            context.validate()?;
            if context.epoch != self.epoch {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        Ok(())
    }
}

/// Tightly packed, straight-alpha RGBA8888 pixels in sRGB. The descriptor
/// must be a sealed memfd of exactly pixel_bytes() bytes. This type validates
/// metadata, not descriptor provenance, contents or kernel seals.
#[derive(Debug, Serialize, Deserialize, Type)]
pub struct BackgroundFrame {
    pub context: BackgroundContext,
    pub width: u32,
    pub height: u32,
    pub pixels: OwnedFd,
}
impl BackgroundFrame {
    pub fn validate_metadata(&self) -> Result<(), ErrorCode> {
        self.context.validate()?;
        pixel_bytes(self.width, self.height).map(|_| ())
    }
}
pub fn pixel_bytes(width: u32, height: u32) -> Result<u64, ErrorCode> {
    if width == 0 || height == 0 || width > MAX_WIDTH || height > MAX_HEIGHT {
        return Err(ErrorCode::InvalidArgument);
    }
    let bytes = u64::from(width) * u64::from(height) * 4;
    if bytes > MAX_FRAME_BYTES {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(bytes)
}

#[zbus::proxy(
    interface = "org.arinn.Background1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait Background {
    fn get(&self) -> zbus::Result<(BackgroundSnapshot,)>;
    /// Only the current org.arinn.Shell1 owner may receive pixels.
    fn read_frame(&self, request_id: &str) -> zbus::Result<(BackgroundFrame,)>;
    #[zbus(signal)]
    fn changed(&self, epoch: &str, revision: u64) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.arinn.BackgroundCapture1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait BackgroundCapture {
    /// Only the selected trusted platform owner may publish or cancel.
    fn publish(&self, frame: &BackgroundFrame) -> zbus::Result<()>;
    fn cancel(&self, request_id: &str) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{LE, serialized::Context, to_bytes};
    fn context() -> BackgroundContext {
        BackgroundContext {
            epoch: "11111111-1111-4111-8111-111111111111".into(),
            request_id: "22222222-2222-4222-8222-222222222222".into(),
            target: "fixture-return".into(),
            output: "fixture-output".into(),
            window: WindowRef {
                generation: "33333333-3333-4333-8333-333333333333".into(),
                token: "fixture-window".into(),
            },
        }
    }
    fn snapshot() -> BackgroundSnapshot {
        let context = context();
        BackgroundSnapshot {
            epoch: context.epoch.clone(),
            revision: 1,
            context: vec![context],
            state: "ready".into(),
        }
    }
    #[test]
    fn wire_signatures_and_metadata_roundtrip() {
        assert_eq!(BackgroundContext::SIGNATURE.to_string(), "(ssss(ss))");
        assert_eq!(
            BackgroundSnapshot::SIGNATURE.to_string(),
            "(sta(ssss(ss))s)"
        );
        assert_eq!(BackgroundFrame::SIGNATURE.to_string(), "((ssss(ss))uuh)");
        let value = snapshot();
        value.validate().unwrap();
        let bytes = to_bytes(Context::new_dbus(LE, 0), &(&value,)).unwrap();
        let decoded: (BackgroundSnapshot,) = bytes.deserialize().unwrap().0;
        assert_eq!(decoded.0, value);
        assert!(
            include_str!("../../../docs/plugin-api/background1.xml").contains("(sta(ssss(ss))s)")
        );
        assert!(
            include_str!("../../../docs/plugin-api/background-capture1.xml")
                .contains("((ssss(ss))uuh)")
        );
    }
    #[test]
    fn rejects_invalid_lifetimes_cardinality_and_state() {
        let valid = snapshot();
        let mut bad = valid.clone();
        bad.revision = 0;
        assert!(bad.validate().is_err());
        bad = valid.clone();
        bad.context.push(context());
        assert!(bad.validate().is_err());
        bad = valid.clone();
        bad.context.clear();
        assert!(bad.validate().is_err());
        bad.state = "pending".into();
        bad.validate().unwrap();
        bad.state = "unavailable".into();
        bad.validate().unwrap();
        bad.state = "idle".into();
        bad.validate().unwrap();
        bad = valid.clone();
        bad.state = "idle".into();
        assert!(bad.validate().is_err());
        bad = valid.clone();
        bad.state = "unknown".into();
        assert!(bad.validate().is_err());
        bad = valid.clone();
        bad.context[0].epoch = context().request_id;
        assert!(bad.validate().is_err());
        for field in [
            "epoch",
            "request_id",
            "target",
            "output",
            "generation",
            "token",
        ] {
            let mut value = context();
            match field {
                "epoch" => value.epoch.clear(),
                "request_id" => value.request_id.clear(),
                "target" => value.target.clear(),
                "output" => value.output.clear(),
                "generation" => value.window.generation.clear(),
                _ => value.window.token.clear(),
            }
            assert!(value.validate().is_err(), "{field}");
        }
        let mut value = context();
        value.window.token = "x".repeat(257);
        assert!(value.validate().is_err());
        value = context();
        value.target = "x".repeat(validation::MAX_TEXT_BYTES + 1);
        assert!(value.validate().is_err());
        value = context();
        value.output.push('\0');
        assert!(value.validate().is_err());
    }
    #[test]
    fn bounds_physical_pixels_without_integer_overflow() {
        assert_eq!(pixel_bytes(MAX_WIDTH, MAX_HEIGHT).unwrap(), 8_294_400);
        assert_eq!(pixel_bytes(2, 2).unwrap(), 16);
        for (width, height) in [
            (0, 1),
            (1, 0),
            (MAX_WIDTH + 1, 1),
            (1, MAX_HEIGHT + 1),
            (u32::MAX, u32::MAX),
        ] {
            assert!(pixel_bytes(width, height).is_err());
        }
    }
}
