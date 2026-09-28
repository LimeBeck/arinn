// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Limits shared by the daemon and provider implementations.
use serde::{Deserialize, Serialize};

pub const MAX_BODY_BYTES: usize = 256 * 1024;
pub const MAX_ID_BYTES: usize = 256;
pub const MAX_TEXT_BYTES: usize = 8 * 1024;
pub const MAX_URL_BYTES: usize = 16 * 1024;
pub const MAX_PAGE_ENTRIES: usize = 200;
pub const MAX_OPERATIONS: usize = 32;
pub const CAPABILITIES: &[&str] = &[
    "application",
    "browser",
    "media",
    "library",
    "action",
    "download",
];

/// Safe, payload-free error codes. Messages from a provider are not UI errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    NotFound,
    Unavailable,
    Unsupported,
    PermissionDenied,
    Timeout,
    InvalidArgument,
    Conflict,
    Internal,
    Cancelled,
}
impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ErrorCode {}

pub fn id(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty() || value.len() > MAX_ID_BYTES || value.contains('\0') {
        Err(ErrorCode::InvalidArgument)
    } else {
        Ok(())
    }
}

pub fn text(value: &str) -> Result<(), ErrorCode> {
    if value.len() > MAX_TEXT_BYTES || value.contains('\0') {
        Err(ErrorCode::InvalidArgument)
    } else {
        Ok(())
    }
}

pub fn capability(value: &str) -> Result<(), ErrorCode> {
    id(value)?;
    if CAPABILITIES.contains(&value) {
        Ok(())
    } else {
        Err(ErrorCode::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_opaque_and_limits_count_utf8_bytes() {
        assert_eq!(id("opaque:/with spaces/秘密"), Ok(()));
        assert_eq!(id(&"я".repeat(128)), Ok(()));
        assert_eq!(id(&"я".repeat(129)), Err(ErrorCode::InvalidArgument));
        assert_eq!(id(""), Err(ErrorCode::InvalidArgument));
        assert_eq!(id("a\0b"), Err(ErrorCode::InvalidArgument));
        assert_eq!(text(&"x".repeat(MAX_TEXT_BYTES)), Ok(()));
        assert_eq!(
            text(&"x".repeat(MAX_TEXT_BYTES + 1)),
            Err(ErrorCode::InvalidArgument)
        );
    }

    #[test]
    fn capabilities_are_explicit() {
        for known in CAPABILITIES {
            assert_eq!(capability(known), Ok(()));
        }
        assert_eq!(capability("unknown"), Err(ErrorCode::Unsupported));
    }
}
