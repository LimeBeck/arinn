// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `file:///` URIs for library launch and import. The value is not logged.

use arinn_protocol::{media, validation::ErrorCode};
use std::path::{Component, Path, PathBuf};

pub fn to_file_uri(path: &Path) -> Result<String, ErrorCode> {
    if !path.is_absolute() {
        return Err(ErrorCode::InvalidArgument);
    }
    let mut uri = String::from("file://");
    for component in path.components() {
        match component {
            Component::RootDir => uri.push('/'),
            Component::Normal(part) => {
                if !uri.ends_with('/') {
                    uri.push('/');
                }
                encode(&mut uri, part.to_str().ok_or(ErrorCode::InvalidArgument)?)?;
            }
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(ErrorCode::InvalidArgument);
            }
        }
    }
    media::file_uri(&uri)?;
    Ok(uri)
}

pub fn path_from_uri(uri: &str) -> Result<PathBuf, ErrorCode> {
    media::file_uri(uri)?;
    let decoded = percent_decode(&uri["file://".len()..])?;
    let path = PathBuf::from(decoded);
    if !path.is_absolute() {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(path)
}

fn encode(out: &mut String, segment: &str) -> Result<(), ErrorCode> {
    if segment.is_empty() || segment == "." || segment == ".." {
        return Err(ErrorCode::InvalidArgument);
    }
    for byte in segment.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(*byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    Ok(())
}

fn percent_decode(value: &str) -> Result<String, ErrorCode> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(ErrorCode::InvalidArgument);
            }
            let byte = (hex(bytes[index + 1])? << 4) | hex(bytes[index + 2])?;
            if byte == 0 {
                return Err(ErrorCode::InvalidArgument);
            }
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ErrorCode::InvalidArgument)
}

fn hex(byte: u8) -> Result<u8, ErrorCode> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ErrorCode::InvalidArgument),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris_round_trip_spaces() {
        let path = Path::new("/media/films/The Matrix (1999).mkv");
        let uri = to_file_uri(path).unwrap();
        assert!(uri.starts_with("file:///"));
        assert!(!uri.contains(' '));
        assert_eq!(path_from_uri(&uri).unwrap(), path);
    }
}
