// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::catalogue::Identity;
use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
    path::{Component, Path},
};

/// Each path component is opened relative to an already-owned directory fd.
/// Renaming a directory or swapping in a symlink cannot redirect later opens.
pub(crate) fn open_beneath(root: &Path, relative: &Path, expected: Identity) -> io::Result<File> {
    let mut directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(root)?;
    let metadata = directory.metadata()?;
    if metadata.dev() != expected.device || metadata.ino() != expected.inode {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }
    let components: Vec<_> = relative.components().collect();
    if components.is_empty() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        };
        let name = CString::new(name.as_bytes())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if index + 1 < components.len() {
                libc::O_DIRECTORY
            } else {
                libc::O_NONBLOCK
            };
        // SAFETY: the directory and NUL-terminated name live through this call;
        // ownership of the returned fd is transferred once to File below.
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fd is a fresh owned descriptor returned by openat.
        directory = unsafe { File::from_raw_fd(fd) };
    }
    if !directory.metadata()?.is_file() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    Ok(directory)
}
