// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Optional managed playback window association (ADR-0074).
use crate::{
    platform,
    return_association::WindowRef,
    validation::{self, ErrorCode},
};
use zbus::zvariant::OwnedObjectPath;

pub const FEATURE: &str = "MediaWindow1";
pub const INTERFACE: &str = "org.arinn.MediaWindow1";
pub const PROVIDER_INTERFACE: &str = "org.arinn.MediaWindowProvider1";
pub const BRIDGE_INTERFACE: &str = "org.arinn.KWinMedia1";
pub const APP_ID_PREFIX: &str = "org.arinn.Playback.";

/// One random launch UUID belongs to the player process lifetime, not a file.
/// Empty means the provider cannot establish a managed window identity.
pub fn validate_launch(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty() {
        Ok(())
    } else {
        platform::generation(value)?;
        if uuid::Uuid::parse_str(value)
            .map_err(|_| ErrorCode::InvalidArgument)?
            .to_string()
            != value
        {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}
pub fn app_id(launch: &str) -> Result<String, ErrorCode> {
    validate_launch(launch)?;
    if launch.is_empty() {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(format!("{APP_ID_PREFIX}{launch}"))
}
pub fn validate_session(generation: &str, id: &str) -> Result<(), ErrorCode> {
    platform::generation(generation)?;
    validation::id(id)
}
pub fn validate_window(window: &[WindowRef]) -> Result<(), ErrorCode> {
    if window.len() > 1 {
        return Err(ErrorCode::InvalidArgument);
    }
    for reference in window {
        reference.validate()?;
    }
    Ok(())
}
pub fn validate_report(
    generation: &str,
    revision: i32,
    token: &str,
    launch: &str,
) -> Result<(), ErrorCode> {
    platform::generation(generation)?;
    if revision <= 0 {
        return Err(ErrorCode::InvalidArgument);
    }
    validation::id(token)?;
    validate_launch(launch)
}

pub fn validate_control(fields: &[String]) -> Result<(), ErrorCode> {
    if fields.is_empty() {
        return Ok(());
    }
    if fields.len() != 3 {
        return Err(ErrorCode::InvalidArgument);
    }
    platform::generation(&fields[0])?;
    if !["present", "release"].contains(&fields[1].as_str()) {
        return Err(ErrorCode::Unsupported);
    }
    if fields[1] == "release" && fields[2].is_empty() {
        return Ok(());
    }
    validation::id(&fields[2])
}

#[zbus::proxy(
    interface = "org.arinn.MediaWindowProvider1",
    default_path = "/org/arinn/Provider1"
)]
pub trait MediaWindowProvider {
    fn get_launch_id(&self, generation: &str, id: &str) -> zbus::Result<String>;
}
#[zbus::proxy(
    interface = "org.arinn.MediaWindow1",
    default_service = "org.arinn.Daemon1"
)]
pub trait MediaWindow {
    fn get_window(&self, generation: &str, id: &str) -> zbus::Result<Vec<WindowRef>>;
    fn present(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
    fn release(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
    #[zbus(signal)]
    fn changed(&self, generation: &str) -> zbus::Result<()>;
}
#[zbus::proxy(
    interface = "org.arinn.KWinMedia1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait KWinMedia {
    fn report(
        &self,
        generation: &str,
        revision: i32,
        token: &str,
        launch_id: &str,
    ) -> zbus::Result<()>;
    fn next_control(&self, generation: &str) -> zbus::Result<Vec<String>>;
    fn complete_control(&self, generation: &str, request_id: &str, error: &str)
    -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_launch_and_window_bounds() {
        let id = uuid::Uuid::new_v4().to_string();
        assert_eq!(app_id(&id).unwrap(), format!("org.arinn.Playback.{id}"));
        assert!(validate_launch("").is_ok());
        for bad in ["not-a-uuid", "00000000-0000-0000-0000-000000000000", "x\0"] {
            assert!(validate_launch(bad).is_err());
        }
        assert!(app_id("").is_err());
        for noncanonical in [
            id.replace('-', ""),
            format!("urn:uuid:{id}"),
            "ABCDEFAB-0000-4000-8000-000000000001".into(),
        ] {
            assert!(validate_launch(&noncanonical).is_err());
            assert!(app_id(&noncanonical).is_err());
        }
        assert!(validate_control(&[id.clone(), "present".into(), "token".into()]).is_ok());
        assert!(validate_control(&[id.clone(), "release".into(), "".into()]).is_ok());
        assert!(validate_control(&[id.clone(), "present".into(), "".into()]).is_err());
        assert!(validate_control(&[id.clone(), "kill".into(), "token".into()]).is_err());
        assert!(validate_report(&id, 1, "opaque-token", &id).is_ok());
        assert!(validate_report(&id, 1, "opaque-token", "").is_ok());
        assert!(validate_report(&id, 0, "token", &id).is_err());
        assert!(validate_report(&id, 1, "", &id).is_err());
        assert!(validate_report(&id, 1, &"x".repeat(257), &id).is_err());
        assert!(validate_session(&id, "session").is_ok());
        assert!(validate_session(&id, "").is_err());
        let window = WindowRef {
            generation: id,
            token: "opaque".into(),
        };
        assert!(validate_window(&[]).is_ok());
        assert!(validate_window(std::slice::from_ref(&window)).is_ok());
        assert!(validate_window(&[window.clone(), window]).is_err());
    }
}
