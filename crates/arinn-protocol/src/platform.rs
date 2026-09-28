// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-stable platform-window contract; the production backend is a separate task.
use crate::validation::{self, ErrorCode, MAX_BODY_BYTES, MAX_PAGE_ENTRIES};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zbus::zvariant::{LE, OwnedObjectPath, Type, serialized::Context, to_bytes};

pub const INTERFACE: &str = "org.arinn.PlatformWindows1";
pub const MAX_SNAPSHOT_ENTRIES: usize = 100_000;
pub const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;

/// A live window, never an application launch target. Empty desktop_id means
/// there is no proven desktop-entry association; titles are deliberately absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Window {
    pub token: String,
    pub desktop_id: String,
    pub output: String,
    pub active: bool,
    pub fullscreen: bool,
    pub can_activate: bool,
    pub can_close: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct WindowPage {
    pub generation: String,
    pub revision: u64,
    pub windows: Vec<Window>,
    pub next_token: String,
}

impl Window {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.token)?;
        for optional in [&self.desktop_id, &self.output] {
            if !optional.is_empty() {
                validation::id(optional)?;
            }
        }
        Ok(())
    }
}

impl WindowPage {
    pub fn encoded_len(&self) -> Result<usize, ErrorCode> {
        to_bytes(Context::new_dbus(LE, 0), &(self,))
            .map(|bytes| bytes.len())
            .map_err(|_| ErrorCode::InvalidArgument)
    }

    pub fn validate(&self) -> Result<(), ErrorCode> {
        generation(&self.generation)?;
        if self.revision == 0 || self.windows.len() > MAX_PAGE_ENTRIES {
            return Err(ErrorCode::InvalidArgument);
        }
        if !self.next_token.is_empty() {
            validation::id(&self.next_token)?;
            // Nonterminal empty pages cannot make progress.
            if self.windows.is_empty() {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        validate_windows(&self.windows)?;
        if self.encoded_len()? > MAX_BODY_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}

pub fn generation(value: &str) -> Result<(), ErrorCode> {
    validation::id(value)?;
    let uuid = uuid::Uuid::parse_str(value).map_err(|_| ErrorCode::InvalidArgument)?;
    if uuid.is_nil() {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(())
}

pub fn validate_windows(windows: &[Window]) -> Result<(), ErrorCode> {
    if windows.len() > MAX_SNAPSHOT_ENTRIES {
        return Err(ErrorCode::InvalidArgument);
    }
    let mut tokens = HashSet::new();
    let mut active = false;
    for window in windows {
        window.validate()?;
        if !tokens.insert(&window.token) || (active && window.active) {
            return Err(ErrorCode::InvalidArgument);
        }
        active |= window.active;
    }
    Ok(())
}

pub fn validate_command(
    platform_generation: &str,
    token: &str,
    request_id: &str,
) -> Result<(), ErrorCode> {
    generation(platform_generation)?;
    validation::id(token)?;
    validation::id(request_id)?;
    uuid::Uuid::parse_str(request_id).map_err(|_| ErrorCode::InvalidArgument)?;
    Ok(())
}

#[zbus::proxy(
    interface = "org.arinn.PlatformWindows1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait PlatformWindows {
    fn get_windows(&self, token: &str) -> zbus::Result<(WindowPage,)>;
    fn activate(
        &self,
        generation: &str,
        token: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
    fn close(
        &self,
        generation: &str,
        token: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
    #[zbus(signal)]
    fn windows_changed(&self, generation: &str, revision: u64) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> Window {
        Window {
            token: "opaque:/window with spaces/秘密".into(),
            desktop_id: String::new(),
            output: String::new(),
            active: false,
            fullscreen: false,
            can_activate: true,
            can_close: false,
        }
    }

    fn page() -> WindowPage {
        WindowPage {
            generation: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            windows: vec![window()],
            next_token: String::new(),
        }
    }

    #[test]
    fn g15_window_wire_signatures_and_roundtrip() {
        assert_eq!(Window::SIGNATURE.to_string(), "(sssbbbb)");
        let page = page();
        let bytes = to_bytes(Context::new_dbus(LE, 0), &(&page,)).unwrap();
        assert_eq!(WindowPage::SIGNATURE.to_string(), "(sta(sssbbbb)s)");
        let (decoded,): (WindowPage,) = bytes.deserialize().unwrap().0;
        assert_eq!(decoded, page);
        page.validate().unwrap();
    }

    #[test]
    fn g13_window_limits_and_identity_validation() {
        let original = page();
        for invalid in [
            "",
            "not-a-generation",
            "00000000-0000-0000-0000-000000000000",
        ] {
            let mut page = original.clone();
            page.generation = invalid.into();
            assert_eq!(page.validate(), Err(ErrorCode::InvalidArgument));
        }
        let mut page = original.clone();
        page.revision = 0;
        assert!(page.validate().is_err());
        page = original.clone();
        page.windows.push(window());
        assert!(page.validate().is_err());
        page.windows[1].token = "different".into();
        page.windows.iter_mut().for_each(|w| w.active = true);
        assert!(page.validate().is_err());
        for field in 0..3 {
            let mut w = window();
            let value = match field {
                0 => &mut w.token,
                1 => &mut w.desktop_id,
                _ => &mut w.output,
            };
            *value = "я".repeat(129);
            assert!(w.validate().is_err());
        }
        page = original.clone();
        page.windows = (0..=MAX_PAGE_ENTRIES)
            .map(|i| Window {
                token: i.to_string(),
                ..window()
            })
            .collect();
        assert!(page.validate().is_err());
        page.windows.pop();
        assert!(page.validate().is_ok());
        page.windows.clear();
        page.next_token = "next".into();
        assert!(page.validate().is_err());
        page.next_token.clear();
        assert!(page.validate().is_ok());
        assert!(
            validate_command(
                &original.generation,
                "close;opaque",
                &uuid::Uuid::new_v4().to_string()
            )
            .is_ok()
        );
        assert!(
            validate_command(&original.generation, "", &uuid::Uuid::new_v4().to_string()).is_err()
        );
        assert!(validate_command(&original.generation, "window", "not-a-request-id").is_err());
    }
}
