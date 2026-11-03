// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_protocol::validation::{self, ErrorCode};
use sha2::{Digest, Sha256};
use std::{
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};
pub fn provider_state(id: &str) -> Result<PathBuf, ErrorCode> {
    validation::id(id)?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or(ErrorCode::InvalidArgument)?;
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/state"));
    let parent = base
        .join("arinn/providers")
        .join(format!("{:x}", Sha256::digest(id.as_bytes())));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&parent)
        .map_err(|_| ErrorCode::Unavailable)?;
    Ok(parent)
}
pub fn runtime() -> Result<PathBuf, ErrorCode> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or(ErrorCode::Unavailable)
}
pub fn group(kind: &str, value: &str) -> String {
    format!("{kind}:{:x}", Sha256::digest(value.as_bytes()))
}
pub fn valid_root(path: &Path) -> bool {
    path.is_absolute()
        && path != Path::new("/")
        && !path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}
