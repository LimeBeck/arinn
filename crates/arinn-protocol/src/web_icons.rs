// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Optional saved-shortcut site icons (ADR-0075). Structural validation only;
//! providers must additionally enforce DNS, network, decoding and cache policy.
use crate::{
    assets::Asset,
    platform,
    validation::{self, ErrorCode},
    web,
};

pub const INTERFACE: &str = "org.arinn.WebShortcutIcons1";
pub const PROVIDER_INTERFACE: &str = "org.arinn.WebIconProvider1";
pub const MAX_ICON_BYTES: usize = 128 * 1024;
pub const MAX_DIMENSION: u32 = 256;

pub fn request(epoch: &str, revision: u64, id: &str) -> Result<(), ErrorCode> {
    platform::generation(epoch)?;
    validation::id(id)?;
    if revision == 0 {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(())
}

/// A canonical HTTPS origin. This does NOT authorize network access: DNS/IP
/// public-address checks and pinned TLS connections remain mandatory at runtime.
pub fn origin(value: &str) -> Result<(), ErrorCode> {
    web::url(value)?;
    let authority = value
        .strip_prefix("https://")
        .and_then(|v| v.strip_suffix('/'))
        .ok_or(ErrorCode::InvalidArgument)?;
    if authority.is_empty()
        || !authority.is_ascii()
        || authority != authority.to_ascii_lowercase()
        || authority.contains(['/', '?', '#', '@', '%'])
    {
        return Err(ErrorCode::InvalidArgument);
    }
    if authority.starts_with('[') {
        let ip = authority
            .strip_prefix('[')
            .and_then(|v| v.strip_suffix(']'))
            .and_then(|v| v.parse::<std::net::Ipv6Addr>().ok())
            .ok_or(ErrorCode::InvalidArgument)?;
        if authority != format!("[{ip}]") {
            return Err(ErrorCode::InvalidArgument);
        }
    } else {
        // No explicit port or noncanonical DNS label; IP safety is separate.
        if authority.len() > 253
            || authority.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            })
        {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    Ok(())
}

/// Check the envelope and PNG IHDR dimensions without decoding on a bus thread.
/// This is not a substitute for full bounded decoding in producer and consumer.
pub fn image(value: &Asset) -> Result<(), ErrorCode> {
    value.validate()?;
    let data = &value.data;
    if value.mime != "image/png"
        || data.len() > MAX_ICON_BYTES
        || data.len() < 33
        || data[8..12] != [0, 0, 0, 13]
        || &data[12..16] != b"IHDR"
    {
        return Err(ErrorCode::InvalidArgument);
    }
    let width = u32::from_be_bytes(data[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(data[20..24].try_into().unwrap());
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(())
}

#[zbus::proxy(
    interface = "org.arinn.WebShortcutIcons1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait WebShortcutIcons {
    fn get(&self, epoch: &str, revision: u64, id: &str) -> zbus::Result<(Asset,)>;
}

#[zbus::proxy(
    interface = "org.arinn.WebIconProvider1",
    default_path = "/org/arinn/Provider1"
)]
pub trait WebIconProvider {
    fn get_icon(&self, generation: &str, origin: &str) -> zbus::Result<(Asset,)>;
}

#[cfg(test)]
mod tests {
    use super::*;
    const EPOCH: &str = "77777777-7777-4777-8777-777777777777";
    #[test]
    fn requests_require_epoch_revision_and_identity() {
        assert!(request(EPOCH, 1, "shortcut").is_ok());
        for (epoch, revision, id) in [("", 1, "id"), (EPOCH, 0, "id"), (EPOCH, 1, "")] {
            assert_eq!(
                request(epoch, revision, id),
                Err(ErrorCode::InvalidArgument)
            );
        }
        assert!(request(EPOCH, 1, &"x".repeat(257)).is_err());
    }
    #[test]
    fn canonical_origins_exclude_private_configuration_components() {
        for valid in [
            "https://example.test/",
            "https://xn--e1afmkfd.xn--p1ai/",
            "https://[2001:db8::1]/",
        ] {
            assert!(origin(valid).is_ok(), "{valid}");
        }
        for invalid in [
            "http://example.test/",
            "https://user@example.test/",
            "https://example.test/path",
            "https://example.test/?secret",
            "https://example.test/#fragment",
            "https://example.test:443/",
            "https://example.test:80/",
            "https://EXAMPLE.test/",
            "https://example.test./",
            "https://-bad.test/",
            "https://example.test//",
            "https://[2001:0db8::1]/",
        ] {
            assert!(origin(invalid).is_err(), "{invalid}");
        }
        // Syntactically valid is deliberately not network authorization.
        assert!(origin("https://127.0.0.1/").is_ok());
    }
    #[test]
    fn image_envelope_and_dimensions_are_bounded() {
        let mut data = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        data.extend_from_slice(&256_u32.to_be_bytes());
        data.extend_from_slice(&256_u32.to_be_bytes());
        data.resize(33, 0);
        let mut value = Asset {
            mime: "image/png".into(),
            data,
        };
        // Header fixture, not a fully decoded PNG.
        assert!(image(&value).is_ok());
        value.data[16..20].copy_from_slice(&257_u32.to_be_bytes());
        assert!(image(&value).is_err());
        value.data[16..20].copy_from_slice(&0_u32.to_be_bytes());
        assert!(image(&value).is_err());
        value.data[16..20].copy_from_slice(&1_u32.to_be_bytes());
        value.data.resize(MAX_ICON_BYTES + 1, 0);
        assert!(image(&value).is_err());
        value.data.truncate(33);
        value.mime = "image/jpeg".into();
        assert!(image(&value).is_err());
        value.mime = "image/png".into();
        value.data.truncate(24);
        assert!(image(&value).is_err());
    }
}
