// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Build identity (ADR-0097): `org.arinn.Daemon1.BuildInfo` returns the
//! daemon's version and build id, so a client can tell whether it runs the
//! same build.
//!
//! A build id is `<version>+g<12 lowercase hex>`, with `-dirty` for a
//! modified checkout, or the bare version when the commit is unknown.
use crate::validation::ErrorCode;

/// Check that `build_id` is well formed and belongs to `version`.
pub fn validate(version: &str, build_id: &str) -> Result<(), ErrorCode> {
    let semver = |v: &str| {
        let parts: Vec<&str> = v.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    };
    if !semver(version) {
        return Err(ErrorCode::InvalidArgument);
    }
    let Some(rest) = build_id.strip_prefix(version) else {
        return Err(ErrorCode::InvalidArgument);
    };
    if rest.is_empty() {
        return Ok(());
    }
    let hash = rest
        .strip_prefix("+g")
        .map(|h| h.strip_suffix("-dirty").unwrap_or(h))
        .ok_or(ErrorCode::InvalidArgument)?;
    if hash.len() == 12 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        Ok(())
    } else {
        Err(ErrorCode::InvalidArgument)
    }
}

/// Whether two build ids name the same build. `-dirty` is ignored: two
/// builds of one commit differ only by local edits, which a packaged build
/// never has. A bare version matches only the same bare version.
pub fn same_build(a: &str, b: &str) -> bool {
    let clean = |id: &str| id.strip_suffix("-dirty").unwrap_or(id).to_string();
    clean(a) == clean(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_ids_are_versioned_and_hashed() {
        for id in ["0.1.0", "0.1.0+g1bd20d647afe", "0.1.0+g1bd20d647afe-dirty"] {
            assert_eq!(validate("0.1.0", id), Ok(()), "{id}");
        }
        for id in [
            "",
            "0.1.1+g1bd20d647afe",
            "0.1.0+1bd20d647afe",
            "0.1.0+g1bd20d647af",
            "0.1.0+g1bd20d647afe0",
            "0.1.0+g1BD20D647AFE",
            "0.1.0+g1bd20d647afe-",
            "0.1.0-dirty",
            "0.1.0+g1bd20d647afe-dirty-dirty",
        ] {
            assert_eq!(
                validate("0.1.0", id),
                Err(ErrorCode::InvalidArgument),
                "{id}"
            );
        }
        assert_eq!(validate("0.1", "0.1"), Err(ErrorCode::InvalidArgument));
    }

    #[test]
    fn same_build_ignores_only_dirty() {
        assert!(same_build(
            "0.1.0+g1bd20d647afe",
            "0.1.0+g1bd20d647afe-dirty"
        ));
        assert!(same_build("0.1.0", "0.1.0"));
        assert!(!same_build("0.1.0+g1bd20d647afe", "0.1.0+g000000000000"));
        assert!(!same_build("0.1.0", "0.1.0+g1bd20d647afe"));
        assert!(!same_build("0.1.0+g1bd20d647afe", "0.2.0+g1bd20d647afe"));
    }
}
