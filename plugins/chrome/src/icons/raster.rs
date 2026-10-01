// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::E;
use arinn_protocol::{assets::Asset, web_icons};
use image::{ImageDecoder, ImageFormat, ImageReader};
use std::io::Cursor;

/// Reject animation containers before decoding and enforce frame dimensions
/// before pixel allocation. Only re-encoded pixels cross the provider boundary.
pub(super) fn normalize(bytes: &[u8]) -> Result<Asset, E> {
    if bytes.is_empty() || bytes.len() > 1024 * 1024 {
        return Err(E::NotFound);
    }
    let format = image::guess_format(bytes).map_err(|_| E::NotFound)?;
    match format {
        ImageFormat::Png => {
            let mut at: usize = 8;
            let mut ended = false;
            while at.checked_add(12).is_some_and(|end| end <= bytes.len()) {
                let size = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
                let end = at
                    .checked_add(12)
                    .and_then(|n| n.checked_add(size))
                    .filter(|&n| n <= bytes.len())
                    .ok_or(E::NotFound)?;
                let kind = &bytes[at + 4..at + 8];
                if kind == b"acTL" {
                    return Err(E::NotFound);
                }
                if kind == b"IEND" {
                    ended = end == bytes.len() && size == 0;
                    break;
                }
                at = end;
            }
            if !ended {
                return Err(E::NotFound);
            }
        }
        ImageFormat::WebP => {
            if bytes.len() < 12
                || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8 != bytes.len()
            {
                return Err(E::NotFound);
            }
            let mut at = 12usize;
            while at < bytes.len() {
                if at + 8 > bytes.len() {
                    return Err(E::NotFound);
                }
                let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
                let end = at
                    .checked_add(8)
                    .and_then(|n| n.checked_add(size))
                    .filter(|&n| n <= bytes.len())
                    .ok_or(E::NotFound)?;
                let kind = &bytes[at..at + 4];
                if kind == b"ANIM"
                    || kind == b"ANMF"
                    || (kind == b"VP8X" && bytes.get(at + 8).is_some_and(|v| v & 2 != 0))
                {
                    return Err(E::NotFound);
                }
                at = end + size % 2;
            }
        }
        ImageFormat::Ico => {
            if bytes.len() < 6 {
                return Err(E::NotFound);
            }
            let count = u16::from_le_bytes(bytes[4..6].try_into().unwrap()) as usize;
            if count == 0 || count > 32 || 6 + count * 16 > bytes.len() {
                return Err(E::NotFound);
            }
            for frame in bytes[6..6 + count * 16].chunks_exact(16) {
                let len = u32::from_le_bytes(frame[8..12].try_into().unwrap()) as usize;
                let start = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
                if start < 6 + count * 16 || start.checked_add(len).is_none_or(|n| n > bytes.len())
                {
                    return Err(E::NotFound);
                }
                let data = &bytes[start..start + len];
                if data.starts_with(b"\x89PNG\r\n\x1a\n") {
                    normalize(data)?;
                } else {
                    if data.len() < 40 || u32::from_le_bytes(data[..4].try_into().unwrap()) < 40 {
                        return Err(E::NotFound);
                    }
                    let w = i32::from_le_bytes(data[4..8].try_into().unwrap());
                    let h = i32::from_le_bytes(data[8..12].try_into().unwrap());
                    if w <= 0
                        || w > 2048
                        || h <= 0
                        || h > 4096
                        || i64::from(w) * i64::from(h / 2) > 4_000_000
                    {
                        return Err(E::NotFound);
                    }
                }
            }
        }
        ImageFormat::Jpeg => {
            if !bytes.ends_with(&[0xff, 0xd9]) {
                return Err(E::NotFound);
            }
        }
        _ => return Err(E::NotFound),
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(|_| E::NotFound)?;
    let (w, h) = decoder.dimensions();
    if w == 0 || h == 0 || w > 2048 || h > 2048 || u64::from(w) * u64::from(h) > 4_000_000 {
        return Err(E::NotFound);
    }
    let decoded = image::DynamicImage::from_decoder(decoder).map_err(|_| E::NotFound)?;
    for edge in [256, 128] {
        let pixels = decoded.thumbnail(edge, edge).into_rgba8();
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(pixels)
            .write_to(&mut out, ImageFormat::Png)
            .map_err(|_| E::NotFound)?;
        let asset = Asset {
            mime: "image/png".into(),
            data: out.into_inner(),
        };
        if web_icons::image(&asset).is_ok() {
            return Ok(asset);
        }
    }
    Err(E::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_pixels_and_rejects_truncation_and_oversize() {
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(512, 300)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        let bytes = out.into_inner();
        let result = normalize(&bytes).unwrap();
        web_icons::image(&result).unwrap();
        assert!(normalize(&bytes[..bytes.len() - 1]).is_err());
        assert!(normalize(b"<svg width='16' height='16'/>").is_err());
        assert!(normalize(&vec![0; 1024 * 1024 + 1]).is_err());
    }
}
