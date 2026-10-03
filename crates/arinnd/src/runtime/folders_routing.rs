// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.Folders1` (ADR-0071): folder names for the shell's folder
//! picker. Only folders are listed: no files, no hidden entries, no contents.
//! File system calls run on the blocking pool with a time limit. Paths are
//! never logged (spec §13).
use super::*;
use arinn_protocol::{
    folders::{self, MOUNT_PARENTS, PLACE_HOME, PLACE_MOUNT, PLACE_ROOT, Place},
    settings,
};
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

/// A stalled mount must not hold a picker request for long.
const FOLDER_TIMEOUT: Duration = Duration::from_secs(3);

/// Where `Places` looks: the home folder and the mount table. Tests point
/// both at fixtures.
#[derive(Clone)]
pub(super) struct FoldersEndpoint {
    pub(super) home: Option<PathBuf>,
    pub(super) mountinfo: PathBuf,
}

impl FoldersEndpoint {
    pub(super) fn new() -> Self {
        Self {
            home: std::env::var_os("HOME").map(PathBuf::from),
            mountinfo: PathBuf::from("/proc/self/mountinfo"),
        }
    }
}

/// Mount points from `/proc/self/mountinfo` text at any depth under
/// [`MOUNT_PARENTS`], unescaped, UTF-8 only, in picker order without
/// duplicates.
pub(super) fn mounts(mountinfo: &str) -> Vec<String> {
    let mut found: Vec<String> = mountinfo
        .lines()
        .filter_map(|line| line.split(' ').nth(4))
        .filter_map(unescape)
        .filter(|point| {
            settings::path(point)
                && MOUNT_PARENTS
                    .iter()
                    .any(|parent| point.starts_with(&format!("{parent}/")))
        })
        .collect();
    found.sort_by(|a, b| folders::order(a, b));
    found.dedup();
    found
}

/// The kernel escapes space, tab, newline and backslash as `\ooo` octal.
fn unescape(field: &str) -> Option<String> {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 4 <= bytes.len() {
            let value = std::str::from_utf8(&bytes[i + 1..i + 4])
                .ok()
                .and_then(|digits| u8::from_str_radix(digits, 8).ok());
            if let Some(value) = value {
                out.push(value);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

pub(super) fn places(home: Option<&Path>, mountinfo: &str) -> Vec<Place> {
    let mut list = Vec::new();
    if let Some(home) = home
        .and_then(Path::to_str)
        .filter(|h| settings::path(h) && *h != "/")
    {
        list.push(Place {
            kind: PLACE_HOME.into(),
            path: home.into(),
        });
    }
    list.extend(mounts(mountinfo).into_iter().map(|path| Place {
        kind: PLACE_MOUNT.into(),
        path,
    }));
    list.push(Place {
        kind: PLACE_ROOT.into(),
        path: "/".into(),
    });
    list
}

/// The names of the folders directly under `path`. A symbolic link counts
/// when it resolves to a folder.
pub(super) fn subfolders(path: &Path) -> Result<Vec<String>, ErrorCode> {
    let entries = std::fs::read_dir(path).map_err(io_error)?;
    let mut names = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !folders::name(&name) {
            continue;
        }
        let folder = match entry.file_type() {
            Ok(kind) if kind.is_dir() => true,
            Ok(kind) if kind.is_symlink() => {
                std::fs::metadata(entry.path()).is_ok_and(|m| m.is_dir())
            }
            _ => false,
        };
        if folder {
            names.push(name);
        }
    }
    Ok(names)
}

fn io_error(error: std::io::Error) -> ErrorCode {
    match error.kind() {
        ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
        // Missing, not a folder, or a mount that has gone away.
        _ => ErrorCode::NotFound,
    }
}

#[zbus::interface(name = "org.arinn.Folders1")]
impl FoldersEndpoint {
    async fn places(&self) -> production::Result<Vec<Place>> {
        let this = self.clone();
        let list = tokio::time::timeout(
            FOLDER_TIMEOUT,
            tokio::task::spawn_blocking(move || {
                let table = std::fs::read_to_string(&this.mountinfo).unwrap_or_default();
                places(this.home.as_deref(), &table)
            }),
        )
        .await
        .map_err(|_| ErrorCode::Timeout)?
        .map_err(|_| ErrorCode::Internal)?;
        Ok(list)
    }

    async fn list(
        &self,
        path: &str,
        after: &str,
        limit: u32,
    ) -> production::Result<(Vec<String>, bool)> {
        if !settings::path(path) {
            return Err(ErrorCode::InvalidArgument.into());
        }
        // Check the cursor and limit before touching the file system.
        folders::page(Vec::new(), after, limit)?;
        let folder = PathBuf::from(path);
        let names = tokio::time::timeout(
            FOLDER_TIMEOUT,
            tokio::task::spawn_blocking(move || subfolders(&folder)),
        )
        .await
        .map_err(|_| ErrorCode::Timeout)?
        .map_err(|_| ErrorCode::Internal)??;
        Ok(folders::page(names, after, limit)?)
    }
}

impl Core {
    pub async fn export_folders(&self) -> Result<(), ErrorCode> {
        self.connection
            .object_server()
            .at(production::PATH, FoldersEndpoint::new())
            .await
            .map(|_| ())
            .map_err(|_| ErrorCode::Internal)
    }
}

#[cfg(test)]
#[path = "folders_tests.rs"]
mod tests;
