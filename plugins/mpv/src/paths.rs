// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Runtime socket directory and `file:///` decoding. Paths are not logged.

use arinn_protocol::{media, validation::ErrorCode};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub fn validate_runtime(runtime: &Path) -> Result<PathBuf, ErrorCode> {
    if !runtime.is_absolute() || is_public_tmp(runtime) {
        return Err(ErrorCode::Unavailable);
    }
    let dir = runtime.join("arinn/mpv");
    if is_public_tmp(&dir) {
        return Err(ErrorCode::Unavailable);
    }
    Ok(dir.join("ipc"))
}

pub fn socket_path() -> Result<PathBuf, ErrorCode> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or(ErrorCode::Unavailable)?;
    let runtime = PathBuf::from(runtime);
    let socket = validate_runtime(&runtime)?;
    let dir = socket.parent().ok_or(ErrorCode::Unavailable)?;
    if let Some(parent) = dir.parent() {
        ensure_private_dir(parent)?;
    }
    ensure_private_dir(dir)?;
    Ok(socket)
}

pub fn ensure_private_dir(dir: &Path) -> Result<(), ErrorCode> {
    fs::create_dir_all(dir).map_err(|_| ErrorCode::Unavailable)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
        .map_err(|_| ErrorCode::Unavailable)?;
    let mode = fs::metadata(dir)
        .map_err(|_| ErrorCode::Unavailable)?
        .permissions()
        .mode()
        & 0o777;
    if mode == 0o700 {
        Ok(())
    } else {
        Err(ErrorCode::Unavailable)
    }
}

pub fn is_public_tmp(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text == "/tmp" || text.starts_with("/tmp/") || text.contains("/../")
}

pub fn file_path(uri: &str) -> Result<PathBuf, ErrorCode> {
    media::file_uri(uri)?;
    let encoded = uri
        .strip_prefix("file://")
        .ok_or(ErrorCode::InvalidArgument)?;
    let decoded = decode_percent(encoded)?;
    if !decoded.starts_with('/') {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(PathBuf::from(decoded))
}

fn decode_percent(value: &str) -> Result<String, ErrorCode> {
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
    fn public_tmp_is_rejected_and_a_private_dir_is_0700() {
        assert!(validate_runtime(Path::new("/tmp")).is_err());
        assert!(validate_runtime(Path::new("/tmp/arinn")).is_err());
        assert_eq!(
            validate_runtime(Path::new("/run/user/1000")).unwrap(),
            PathBuf::from("/run/user/1000/arinn/mpv/ipc")
        );
        let dir = std::env::temp_dir().join(format!("arinn-mpv-mode-{}", uuid::Uuid::new_v4()));
        ensure_private_dir(&dir).unwrap();
        let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_uris_decode_to_absolute_paths() {
        assert_eq!(
            file_path("file:///library/my%20film.mkv").unwrap(),
            PathBuf::from("/library/my film.mkv")
        );
        assert!(file_path("https://example.org/a.mkv").is_err());
        assert!(file_path("file:///library/../etc/passwd").is_err());
    }
}
