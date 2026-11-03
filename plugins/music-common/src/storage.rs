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
    limit: usize,
}

impl Storage {
    /// Bounded to 32 MiB of state.
    pub fn open(path: &Path) -> Result<Self, Error> {
        Self::open_bounded(path, LIMIT)
    }
    /// Bounded to `limit` bytes of state, for a provider whose documented
    /// limits need more than the default.
    pub fn open_bounded(path: &Path, limit: usize) -> Result<Self, Error> {
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
        remove_stale_temporaries(&dir);
        Ok(Self {
            dir,
            _lock: lock,
            limit,
        })
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
        if file.metadata().map_err(|_| Error::Unavailable)?.len() > self.limit as u64 {
            return Err(Error::TooLarge);
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take((self.limit + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Unavailable)?;
        if bytes.len() > self.limit {
            return Err(Error::TooLarge);
        }
        Ok(Some(bytes))
    }

    pub fn write(&self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() > self.limit {
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

/// A write interrupted by a crash leaves `state-<pid>-<n>.tmp` behind. With
/// the writer lock held no other writer uses one, so remove them. Only names
/// of that form inside the anchored directory are unlinked.
fn remove_stale_temporaries(dir: &File) {
    let Ok(entries) = std::fs::read_dir(format!("/proc/self/fd/{}", dir.as_raw_fd())) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !(name.starts_with("state-") && name.ends_with(".tmp")) {
            continue;
        }
        let Ok(name) = CString::new(name) else {
            continue;
        };
        // SAFETY: a single component relative to the live anchored directory;
        // unlinkat without AT_REMOVEDIR never removes a directory.
        unsafe {
            libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::DirBuilderExt;
    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("arinn-storage-{}", uuid::Uuid::new_v4()));
        DirBuilder::new().mode(0o700).create(&path).unwrap();
        path
    }
    #[test]
    fn a_bounded_store_enforces_its_own_limit() {
        let path = directory();
        let store = Storage::open_bounded(&path.join("state"), 16).unwrap();
        assert_eq!(store.write(&[b'x'; 17]), Err(Error::TooLarge));
        store.write(&[b'x'; 16]).unwrap();
        assert_eq!(store.read().unwrap().unwrap().len(), 16);
        drop(store);
        // A smaller bound refuses a state written under a larger one.
        let store = Storage::open_bounded(&path.join("state"), 8).unwrap();
        assert_eq!(store.read(), Err(Error::TooLarge));
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
    /// Review of #710: a crash between create and rename left a temporary
    /// file of up to the state limit behind, and nothing removed it.
    #[test]
    fn stale_temporaries_are_removed_when_the_writer_lock_is_taken() {
        let path = directory();
        let state = path.join("state");
        let store = Storage::open(&state).unwrap();
        store.write(b"{}").unwrap();
        drop(store);
        use std::os::unix::fs::OpenOptionsExt;
        for name in ["state-1-1.tmp", "state-99-7.tmp", "unrelated.tmp"] {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(state.join(name))
                .unwrap();
        }
        let store = Storage::open(&state).unwrap();
        let mut names: Vec<String> = std::fs::read_dir(&state)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["lock", "state.json", "unrelated.tmp"]);
        assert_eq!(store.read().unwrap().unwrap(), b"{}");
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
}
