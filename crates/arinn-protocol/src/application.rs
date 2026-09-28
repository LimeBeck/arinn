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
        validation::text(&self.title)
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
            })
            .collect();
        assert!(page.encoded_len().unwrap() > MAX_BODY_BYTES);
        assert_eq!(page.validate(), Err(ErrorCode::InvalidArgument));
        page.applications = vec![
            Application {
                id: "duplicate".into(),
                title: "Test".into(),
                can_launch: true,
            };
            2
        ];
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
