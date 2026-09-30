// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-stable asset contract (ADR-0062). Media artwork, thumbnails and library
//! posters and fanart are opaque asset ids (ADR-0054). A provider that can
//! serve them exports the optional `org.arinn.AssetProvider1` and lists
//! [`FEATURE_ASSETS`] in its `Features`. The daemon serves them to the shell
//! as `org.arinn.Assets1` on the provider's daemon path. Assets are image
//! bytes, never file paths or URLs, and are never logged.
use crate::validation::{self, ErrorCode};
use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

pub const PROVIDER_INTERFACE: &str = "org.arinn.AssetProvider1";
pub const INTERFACE: &str = "org.arinn.Assets1";
/// Feature flag a `media` or `library` provider lists when it serves assets.
pub const FEATURE_ASSETS: &str = "AssetProvider1";
/// Largest asset in bytes: a 4K poster or fanart fits comfortably.
pub const MAX_ASSET_BYTES: usize = 8 * 1024 * 1024;
pub const MIME_JPEG: &str = "image/jpeg";
pub const MIME_PNG: &str = "image/png";
pub const MIME_WEBP: &str = "image/webp";
pub const MIME_TYPES: &[&str] = &[MIME_JPEG, MIME_PNG, MIME_WEBP];

/// One image. `mime` must match the bytes' own signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Asset {
    pub mime: String,
    pub data: Vec<u8>,
}

/// The image type the bytes start with, if it is an allowed one.
pub fn sniff(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(MIME_JPEG)
    } else if data.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some(MIME_PNG)
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some(MIME_WEBP)
    } else {
        None
    }
}

impl Asset {
    /// Checks the size limit, the allowed types and that the declared type
    /// is the type of the bytes (no SVG, no HTML, no disguised content).
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.data.is_empty() || self.data.len() > MAX_ASSET_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        if !MIME_TYPES.contains(&self.mime.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
        if sniff(&self.data) != Some(self.mime.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}

/// An asset id as used in sessions, items and thumbnails: non-empty here.
pub fn asset_id(value: &str) -> Result<(), ErrorCode> {
    validation::id(value)
}

pub mod provider {
    use super::Asset;
    #[zbus::proxy(
        interface = "org.arinn.AssetProvider1",
        default_path = "/org/arinn/Provider1"
    )]
    pub trait AssetProvider {
        fn get_asset(&self, generation: &str, id: &str) -> zbus::Result<(Asset,)>;
    }
}
pub use provider::AssetProviderProxy;

/// `org.arinn.Assets1` on the provider's daemon path. `NotFound` for an
/// unknown id, `Unsupported` when the provider does not serve assets.
#[zbus::proxy(interface = "org.arinn.Assets1", default_service = "org.arinn.Daemon1")]
pub trait AssetsApi {
    fn get_asset(&self, generation: &str, id: &str) -> zbus::Result<(Asset,)>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{LE, serialized::Context, to_bytes};

    fn png() -> Vec<u8> {
        let mut data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        data.extend_from_slice(&[0; 16]);
        data
    }

    #[test]
    fn wire_signature_round_trips() {
        assert_eq!(Asset::SIGNATURE.to_string(), "(say)");
        let asset = Asset {
            mime: MIME_PNG.into(),
            data: png(),
        };
        let bytes = to_bytes(Context::new_dbus(LE, 0), &(&asset,)).unwrap();
        let (back,): (Asset,) = bytes.deserialize().unwrap().0;
        assert_eq!(back, asset);
    }

    #[test]
    fn allowed_images_validate() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0];
        let mut webp = b"RIFF\0\0\0\0WEBPVP8 ".to_vec();
        webp.extend_from_slice(&[0; 8]);
        for (mime, data) in [
            (MIME_PNG, png()),
            (MIME_JPEG, jpeg.to_vec()),
            (MIME_WEBP, webp),
        ] {
            let asset = Asset {
                mime: mime.into(),
                data,
            };
            assert_eq!(asset.validate(), Ok(()), "{mime}");
        }
    }

    #[test]
    fn everything_else_is_rejected() {
        let bad = [
            // Declared type does not match the bytes.
            Asset {
                mime: MIME_JPEG.into(),
                data: png(),
            },
            // Not an allowed type, even with a truthful declaration.
            Asset {
                mime: "image/svg+xml".into(),
                data: b"<svg/>".to_vec(),
            },
            Asset {
                mime: "text/html".into(),
                data: b"<html>".to_vec(),
            },
            // Empty and oversized.
            Asset {
                mime: MIME_PNG.into(),
                data: Vec::new(),
            },
            Asset {
                mime: MIME_PNG.into(),
                data: [png(), vec![0; MAX_ASSET_BYTES]].concat(),
            },
            // A truncated WebP header.
            Asset {
                mime: MIME_WEBP.into(),
                data: b"RIFF".to_vec(),
            },
        ];
        for asset in bad {
            assert_eq!(
                asset.validate(),
                Err(ErrorCode::InvalidArgument),
                "{}",
                asset.mime
            );
        }
        assert_eq!(asset_id(""), Err(ErrorCode::InvalidArgument));
        assert_eq!(asset_id("poster-1"), Ok(()));
    }
}
