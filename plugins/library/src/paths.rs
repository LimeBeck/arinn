// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider directories named by the SHA-256 of the provider id (ADR-0034).

use arinn_protocol::validation::{self, ErrorCode};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};

pub fn provider_digest(provider_id: &str) -> Result<String, ErrorCode> {
    validation::id(provider_id)?;
    Ok(format!("{:x}", Sha256::digest(provider_id.as_bytes())))
}

pub fn state_database(provider_id: &str) -> Result<PathBuf, ErrorCode> {
    let dir = arinn_root("XDG_STATE_HOME", ".local/state")?
        .join("providers")
        .join(provider_digest(provider_id)?);
    private_dir(&dir)?;
    Ok(dir.join("library.sqlite3"))
}

fn arinn_root(key: &str, fallback: &str) -> Result<PathBuf, ErrorCode> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or(ErrorCode::InvalidArgument)?;
    let base = std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(fallback));
    Ok(base.join("arinn"))
}

pub fn private_dir(path: &Path) -> Result<(), ErrorCode> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(ErrorCode::PermissionDenied),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(|_| ErrorCode::Unavailable),
        Err(_) => Err(ErrorCode::Unavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_the_full_sha256_of_the_provider_id() {
        assert_eq!(
            provider_digest("library").unwrap(),
            "b718f1354f7247312eca086d9a024afe5fa717ddea5adeddd6f12bcf945b2e8c"
        );
        assert_eq!(provider_digest(""), Err(ErrorCode::InvalidArgument));
    }
}
