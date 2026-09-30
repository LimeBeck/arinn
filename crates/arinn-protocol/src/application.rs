// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Application snapshot model. The production wire interface is still under review.
use crate::validation::{self, ErrorCode, MAX_BODY_BYTES, MAX_PAGE_ENTRIES};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zbus::zvariant::{LE, Type, serialized::Context, to_bytes};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Application {
    pub id: String,
    pub title: String,
    pub can_launch: bool,
    /// Presentation group on the Applications screen, chosen by the provider:
    /// one of [`GROUPS`]. Consumers treat an unknown value as `other`, so a
    /// newer provider may add groups without breaking older shells (#62).
    pub group: String,
    /// Optional local icon reference (ADR-0059). Empty means absent.
    /// See [`icon_reference`].
    pub icon: String,
}

/// Application groups in display order (spec §5.5).
pub const GROUPS: &[&str] = &["online", "games", "media", "other"];
pub const MAX_GROUP_BYTES: usize = 32;

/// A group is a short lowercase token; unknown tokens are valid on the wire.
pub fn group(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty()
        || value.len() > MAX_GROUP_BYTES
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        Err(ErrorCode::InvalidArgument)
    } else {
        Ok(())
    }
}

/// The known group for display; unknown groups fall back to `other`.
pub fn display_group(value: &str) -> &'static str {
    GROUPS
        .iter()
        .copied()
        .find(|known| *known == value)
        .unwrap_or("other")
}

/// Maximum UTF-8 bytes of one application icon reference (ADR-0059).
pub const MAX_ICON_REFERENCE_BYTES: usize = 1024;

/// How a consumer shows [`Application::icon`].
///
/// [`Neutral`] is the existing placeholder. A [`Theme`] or [`File`] reference
/// that cannot be loaded as a local image is also [`Neutral`]. Consumers do
/// not fetch a remote icon and do not call the provider for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconPresentation<'a> {
    Neutral,
    Theme(&'a str),
    File(&'a str),
}

/// Empty, a theme icon name, or an absolute local file path.
///
/// A theme name is ASCII letters, digits, `.`, `_` and `-`, and contains at
/// least one letter or digit. A path starts with `/`, is not a network path,
/// and has no empty, `.` or `..` segment. URLs, `file:` URIs and relative
/// paths are rejected. The reference is not image bytes and must not be logged.
pub fn icon_reference(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty() {
        return Ok(());
    }
    if value.len() > MAX_ICON_REFERENCE_BYTES || value.chars().any(char::is_control) {
        return Err(ErrorCode::InvalidArgument);
    }
    if value.starts_with('/') {
        file_reference(value)
    } else {
        theme_reference(value)
    }
}

fn theme_reference(value: &str) -> Result<(), ErrorCode> {
    let bytes = value.as_bytes();
    if bytes.iter().any(|b| {
        !matches!(
            b,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'
        )
    }) || !bytes.iter().any(u8::is_ascii_alphanumeric)
    {
        Err(ErrorCode::InvalidArgument)
    } else {
        Ok(())
    }
}

fn file_reference(value: &str) -> Result<(), ErrorCode> {
    if value.starts_with("//") {
        return Err(ErrorCode::InvalidArgument);
    }
    let mut segments = value.split('/');
    if segments.next() != Some("") {
        return Err(ErrorCode::InvalidArgument);
    }
    let mut any = false;
    for segment in segments {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(ErrorCode::InvalidArgument);
        }
        any = true;
    }
    if any {
        Ok(())
    } else {
        Err(ErrorCode::InvalidArgument)
    }
}

/// Deterministic placeholder rule. Invalid syntax is [`IconPresentation::Neutral`],
/// the same as an empty reference, so a consumer never treats it as a URL.
pub fn icon_presentation(value: &str) -> IconPresentation<'_> {
    if value.is_empty() || icon_reference(value).is_err() {
        IconPresentation::Neutral
    } else if value.starts_with('/') {
        IconPresentation::File(value)
    } else {
        IconPresentation::Theme(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunningApplication {
    pub id: String,
    pub application_id: String,
    pub title: String,
    pub window_token: String,
    pub can_activate: bool,
    pub can_terminate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ApplicationPage {
    pub generation: String,
    pub revision: u64,
    pub applications: Vec<Application>,
    pub running: Vec<RunningApplication>,
    pub next_token: String,
}

impl Application {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        validation::text(&self.title)?;
        group(&self.group)?;
        icon_reference(&self.icon)
    }
}
impl RunningApplication {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        validation::id(&self.application_id)?;
        validation::text(&self.title)?;
        if !self.window_token.is_empty() {
            validation::id(&self.window_token)?;
        }
        Ok(())
    }
}

impl ApplicationPage {
    /// Size of the actual D-Bus body (one struct), including alignment and headers
    /// of arrays/strings. Callers must not substitute JSON length or character count.
    pub fn encoded_len(&self) -> Result<usize, ErrorCode> {
        to_bytes(Context::new_dbus(LE, 0), &(self,))
            .map(|bytes| bytes.len())
            .map_err(|_| ErrorCode::InvalidArgument)
    }

    pub fn validate(&self) -> Result<(), ErrorCode> {
        uuid::Uuid::parse_str(&self.generation).map_err(|_| ErrorCode::InvalidArgument)?;
        if !self.next_token.is_empty() {
            validation::id(&self.next_token)?;
        }
        if self.revision == 0 || self.applications.len() + self.running.len() > MAX_PAGE_ENTRIES {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut applications = HashSet::new();
        for application in &self.applications {
            application.validate()?;
            if !applications.insert(&application.id) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        let mut running = HashSet::new();
        for instance in &self.running {
            instance.validate()?;
            if !running.insert(&instance.id) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        if self.encoded_len()? > MAX_BODY_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}

/// Validate a complete snapshot before making any part visible. IDs are unique
/// within each entity kind; a running instance and its application may share an ID.
pub fn validate_entries(
    applications: &[Application],
    running: &[RunningApplication],
) -> Result<(), ErrorCode> {
    let mut ids = HashSet::new();
    for application in applications {
        application.validate()?;
        if !ids.insert(&application.id) {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    let mut instances = HashSet::new();
    for instance in running {
        instance.validate()?;
        if !ids.contains(&instance.application_id) || !instances.insert(&instance.id) {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> ApplicationPage {
        ApplicationPage {
            generation: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            applications: Vec::new(),
            running: Vec::new(),
            next_token: String::new(),
        }
    }

    #[test]
    fn g13_wire_validation_rejects_oversized_bodies_and_duplicate_ids() {
        let mut page = page();
        page.applications = (0..40)
            .map(|id| Application {
                id: id.to_string(),
                title: "x".repeat(8192),
                can_launch: true,
                group: "other".into(),
                icon: String::new(),
            })
            .collect();
        assert!(page.encoded_len().unwrap() > MAX_BODY_BYTES);
        assert_eq!(page.validate(), Err(ErrorCode::InvalidArgument));
        page.applications = vec![
            Application {
                id: "duplicate".into(),
                title: "Test".into(),
                can_launch: true,
                group: "other".into(),
                icon: String::new(),
            };
            2
        ];
        assert_eq!(page.validate(), Err(ErrorCode::InvalidArgument));
    }

    #[test]
    fn groups_are_short_tokens_and_unknown_ones_display_as_other() {
        assert_eq!(Application::SIGNATURE.to_string(), "(ssbss)");
        for known in GROUPS {
            assert_eq!(group(known), Ok(()));
            assert_eq!(display_group(known), *known);
        }
        assert_eq!(group("future-group2"), Ok(()));
        assert_eq!(display_group("future-group2"), "other");
        for bad in [
            "",
            "Games",
            "with space",
            "x/y",
            &"g".repeat(MAX_GROUP_BYTES + 1),
        ] {
            assert_eq!(group(bad), Err(ErrorCode::InvalidArgument), "{bad:?}");
        }
        let mut page = page();
        page.applications = vec![Application {
            id: "a".into(),
            title: "A".into(),
            can_launch: true,
            group: String::new(),
            icon: String::new(),
        }];
        assert_eq!(page.validate(), Err(ErrorCode::InvalidArgument));
    }

    #[test]
    fn icon_references_are_optional_local_and_fall_back_when_unusable() {
        assert_eq!(
            ApplicationPage::SIGNATURE.to_string(),
            "(sta(ssbss)a(ssssbb)s)"
        );
        assert_eq!(icon_reference(""), Ok(()));
        assert_eq!(icon_presentation(""), IconPresentation::Neutral);
        assert_eq!(
            icon_presentation("applications-games"),
            IconPresentation::Theme("applications-games")
        );
        assert_eq!(
            icon_presentation("org.kde.dolphin"),
            IconPresentation::Theme("org.kde.dolphin")
        );
        let path = "/usr/share/icons/hicolor/48x48/apps/fixture.png";
        assert_eq!(icon_presentation(path), IconPresentation::File(path));
        assert_eq!(
            icon_reference("/tmp/my icon.png"),
            Ok(()),
            "spaces are legal in a file name"
        );
        let max_name = format!("icon{}", "a".repeat(MAX_ICON_REFERENCE_BYTES - 4));
        assert_eq!(max_name.len(), MAX_ICON_REFERENCE_BYTES);
        assert_eq!(icon_reference(&max_name), Ok(()));
        let too_long = format!("{max_name}a");

        for bad in [
            "https://example.invalid/icon.png",
            "http://127.0.0.1/icon.png",
            "file:///tmp/icon.png",
            "qrc:/brand/mark",
            "icons/relative.png",
            "icon name",
            ".",
            "..",
            "-",
            "/tmp/../etc/passwd",
            "/tmp//icon.png",
            "//server/share/icon.png",
            "/",
            too_long.as_str(),
            "bad\nname",
            "bad\0name",
        ] {
            assert_eq!(
                icon_reference(bad),
                Err(ErrorCode::InvalidArgument),
                "{bad:?}"
            );
            assert_eq!(icon_presentation(bad), IconPresentation::Neutral, "{bad:?}");
        }

        let mut page = page();
        page.applications = vec![Application {
            id: "a".into(),
            title: "A".into(),
            can_launch: true,
            group: "games".into(),
            icon: String::new(),
        }];
        assert_eq!(page.validate(), Ok(()));
        page.applications[0].icon = "applications-games".into();
        assert_eq!(page.validate(), Ok(()));
        page.applications[0].icon = path.into();
        assert_eq!(page.validate(), Ok(()));
        page.applications[0].icon = "https://example.invalid/icon.png".into();
        assert_eq!(page.validate(), Err(ErrorCode::InvalidArgument));
    }

    #[test]
    fn g13_wire_validation_requires_a_generation_and_nonzero_revision() {
        let mut page = page();
        assert_eq!(page.validate(), Ok(()));
        page.revision = 0;
        assert_eq!(page.validate(), Err(ErrorCode::InvalidArgument));
        page.revision = 1;
        page.generation = "not a UUID".into();
        assert_eq!(page.validate(), Err(ErrorCode::InvalidArgument));
    }
}
