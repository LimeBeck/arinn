// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Internal trusted-adapter transport; never exposed as a plugin capability.
//! Scalar inputs keep KWin's JavaScript D-Bus argument conversion unambiguous.
use crate::{
    platform,
    validation::{self, ErrorCode},
};

pub const INTERFACE: &str = "org.arinn.KWinBridge1";
pub const MAX_INVENTORY_WINDOWS: i32 = 100_000;

pub fn validate_begin(generation: &str, revision: i32, count: i32) -> Result<(), ErrorCode> {
    platform::generation(generation)?;
    if revision <= 0 || !(0..=MAX_INVENTORY_WINDOWS).contains(&count) {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(())
}

/// Internal flags: active=1, fullscreen=2, can_activate=4, can_close=8.
pub fn window(
    token: &str,
    desktop_id: &str,
    output: &str,
    flags: i32,
) -> Result<platform::Window, ErrorCode> {
    if !(0..=15).contains(&flags) {
        return Err(ErrorCode::InvalidArgument);
    }
    let window = platform::Window {
        token: token.into(),
        desktop_id: desktop_id.into(),
        output: output.into(),
        active: flags & 1 != 0,
        fullscreen: flags & 2 != 0,
        can_activate: flags & 4 != 0,
        can_close: flags & 8 != 0,
    };
    window.validate()?;
    Ok(window)
}

/// Empty means idle. Otherwise exactly [request UUID, action, opaque token].
/// This array is a fixed record, not a command line or executable expression.
pub fn validate_control(fields: &[String]) -> Result<(), ErrorCode> {
    if fields.is_empty() {
        return Ok(());
    }
    if fields.len() != 3 {
        return Err(ErrorCode::InvalidArgument);
    }
    platform::generation(&fields[0])?;
    if !["activate", "close"].contains(&fields[1].as_str()) {
        return Err(ErrorCode::Unsupported);
    }
    validation::id(&fields[2])
}

/// Empty acknowledges dispatch; all other values are safe normalized errors.
pub fn validate_completion(request_id: &str, error: &str) -> Result<(), ErrorCode> {
    platform::generation(request_id)?;
    if ![
        "",
        "NotFound",
        "Unavailable",
        "Unsupported",
        "PermissionDenied",
        "Timeout",
        "InvalidArgument",
        "Conflict",
        "Internal",
        "Cancelled",
    ]
    .contains(&error)
    {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(())
}

#[zbus::proxy(
    interface = "org.arinn.KWinBridge1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait KWinBridge {
    fn connect(&self, instance: &str) -> zbus::Result<String>;
    fn begin_inventory(&self, generation: &str, revision: i32, count: i32) -> zbus::Result<()>;
    #[allow(clippy::too_many_arguments)] // Seven scalar inputs plus self.
    fn put_window(
        &self,
        generation: &str,
        revision: i32,
        index: i32,
        token: &str,
        desktop_id: &str,
        output: &str,
        flags: i32,
    ) -> zbus::Result<()>;
    fn commit_inventory(&self, generation: &str, revision: i32) -> zbus::Result<()>;
    fn next_control(&self, generation: &str) -> zbus::Result<Vec<String>>;
    fn complete_control(&self, generation: &str, request_id: &str, error: &str)
    -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_inventory_and_fixed_control_record() {
        let generation = uuid::Uuid::new_v4().to_string();
        assert_eq!(validate_begin(&generation, 1, 0), Ok(()));
        assert_eq!(validate_begin(&generation, 1, 100_000), Ok(()));
        assert_eq!(
            validate_begin(&generation, 0, 1),
            Err(ErrorCode::InvalidArgument)
        );
        assert_eq!(
            validate_begin(&generation, 1, 100_001),
            Err(ErrorCode::InvalidArgument)
        );
        assert_eq!(
            validate_begin(&generation, 1, -1),
            Err(ErrorCode::InvalidArgument)
        );
        assert_eq!(
            validate_begin(&generation, -1, 0),
            Err(ErrorCode::InvalidArgument)
        );
        for flags in 0..16 {
            let w = window("token", "", "", flags).unwrap();
            assert_eq!(w.active, flags & 1 != 0);
            assert_eq!(w.fullscreen, flags & 2 != 0);
            assert_eq!(w.can_activate, flags & 4 != 0);
            assert_eq!(w.can_close, flags & 8 != 0);
        }
        assert!(window("token", "", "", -1).is_err());
        assert!(window("token", "", "", 16).is_err());
        assert_eq!(validate_control(&[]), Ok(()));
        assert_eq!(
            validate_control(std::slice::from_ref(&generation)),
            Err(ErrorCode::InvalidArgument)
        );
        assert_eq!(
            validate_control(&[
                generation.clone(),
                "activate".into(),
                "opaque;not/a/command".into()
            ]),
            Ok(())
        );
        assert_eq!(
            validate_control(&[generation.clone(), "kill".into(), "window".into()]),
            Err(ErrorCode::Unsupported)
        );
        assert_eq!(
            validate_control(&[generation.clone(), "close".into(), "".into()]),
            Err(ErrorCode::InvalidArgument)
        );
        assert_eq!(validate_completion(&generation, ""), Ok(()));
        assert_eq!(validate_completion(&generation, "PermissionDenied"), Ok(()));
        assert_eq!(
            validate_completion(&generation, "arbitrary backend payload"),
            Err(ErrorCode::InvalidArgument)
        );
        assert_eq!(
            validate_completion("bad-uuid", ""),
            Err(ErrorCode::InvalidArgument)
        );
    }
}
