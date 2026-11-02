// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Anchored, locked, bounded provider-private state. Errors expose no paths.
use std::{
    ffi::CString,
    fs::{DirBuilder, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

const LIMIT: usize = 32 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unavailable,
    UnsafeStorage,
    Busy,
    InvalidState,
    TooLarge,
    Durability,
}

pub struct Storage {
    dir: File,
    _lock: File,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self, Error> {
        match DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(Error::Unavailable),
        }
        let dir = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .map_err(|_| Error::UnsafeStorage)?;
        secure(&dir, true)?;
        let lock = open_at(&dir, "lock", libc::O_RDWR | libc::O_CREAT, 0o600)?;
        secure(&lock, false)?;
        // SAFETY: lock owns this descriptor; its lifetime holds the advisory lock.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(Error::Busy);
        }
        Ok(Self { dir, _lock: lock })
    }

    pub fn read(&self) -> Result<Option<Vec<u8>>, Error> {
        let mut file = match open_at(&self.dir, "state.json", libc::O_RDONLY, 0) {
            Ok(file) => file,
            Err(Error::Unavailable)
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) =>
            {
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        secure(&file, false)?;
        if file.metadata().map_err(|_| Error::Unavailable)?.len() > LIMIT as u64 {
            return Err(Error::TooLarge);
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take((LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Unavailable)?;
        if bytes.len() > LIMIT {
            return Err(Error::TooLarge);
        }
        Ok(Some(bytes))
    }

    pub fn write(&self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() > LIMIT {
            return Err(Error::TooLarge);
        }
        let name = format!(
            "state-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let mut file = open_at(
            &self.dir,
            &name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        let result = (|| {
            file.write_all(bytes).map_err(|_| Error::Unavailable)?;
            file.sync_all().map_err(|_| Error::Unavailable)?;
            let source = CString::new(name.as_str()).map_err(|_| Error::InvalidState)?;
            let target = c"state.json";
            // SAFETY: both names are single components relative to the live anchored directory.
            if unsafe {
                libc::renameat(
                    self.dir.as_raw_fd(),
                    source.as_ptr(),
                    self.dir.as_raw_fd(),
                    target.as_ptr(),
                )
            } != 0
            {
                return Err(Error::Unavailable);
            }
            self.dir.sync_all().map_err(|_| Error::Durability)
        })();
        if result.is_err() {
            let name = CString::new(name).map_err(|_| Error::InvalidState)?;
            // SAFETY: removes only our exclusively created temporary file.
            unsafe {
                libc::unlinkat(self.dir.as_raw_fd(), name.as_ptr(), 0);
            }
        }
        result
    }
}

fn open_at(dir: &File, name: &str, flags: i32, mode: u32) -> Result<File, Error> {
    let name = CString::new(name).map_err(|_| Error::InvalidState)?;
    // SAFETY: directory and name stay live; ownership of a successful fd transfers once.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            mode,
        )
    };
    if fd < 0 {
        return Err(Error::Unavailable);
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn secure(file: &File, directory: bool) -> Result<(), Error> {
    let m = file.metadata().map_err(|_| Error::Unavailable)?;
    // SAFETY: geteuid has no preconditions.
    if m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o077 != 0
        || (if directory {
            !m.is_dir()
        } else {
            !m.is_file() || m.nlink() != 1
        })
    {
        return Err(Error::UnsafeStorage);
    }
    Ok(())
}
