// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{E, raster};
use arinn_protocol::assets::Asset;
use std::{
    ffi::CString,
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::MetadataExt,
    },
    path::{Component, Path},
    time::{SystemTime, UNIX_EPOCH},
};
const TTL: u64 = 7 * 86400;
const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_FILE: u64 = 128 * 1024 + 8;

/// All operations are relative to a held directory descriptor; replacing an
/// ancestor cannot redirect writes. No symlink is followed, even on startup.
pub(super) struct Cache {
    directory: File,
}
fn name(value: &str) -> Result<CString, E> {
    CString::new(value).map_err(|_| E::Unavailable)
}
fn open_at(parent: &File, value: &str, flags: i32) -> Result<File, E> {
    let value = name(value)?;
    // SAFETY: NUL-terminated name and live descriptor; ownership transfers once.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            value.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(E::Unavailable);
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn key_name(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
impl Cache {
    pub fn new(root: &Path) -> Result<Self, E> {
        if !root.is_absolute() {
            return Err(E::Unavailable);
        }
        let mut directory = File::open("/").map_err(|_| E::Unavailable)?;
        for component in root.components() {
            let Component::Normal(value) = component else {
                if component == Component::RootDir {
                    continue;
                }
                return Err(E::Unavailable);
            };
            let value = value.to_str().ok_or(E::Unavailable)?;
            let c = name(value)?;
            // Creation is private; existing ancestors are opened without links.
            unsafe {
                libc::mkdirat(directory.as_raw_fd(), c.as_ptr(), 0o700);
            }
            directory = open_at(&directory, value, libc::O_RDONLY | libc::O_DIRECTORY)?;
        }
        let metadata = directory.metadata().map_err(|_| E::Unavailable)?;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(E::Unavailable);
        }
        let cache = Self { directory };
        cache.prune(None)?;
        Ok(cache)
    }
    fn remove(&self, key: &str) -> Result<(), E> {
        let key = name(key)?;
        let result = unsafe { libc::unlinkat(self.directory.as_raw_fd(), key.as_ptr(), 0) };
        if result == 0 || std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound {
            Ok(())
        } else {
            Err(E::Unavailable)
        }
    }
    fn read(&self, key: &str) -> Result<(u64, Vec<u8>), E> {
        let mut file = open_at(&self.directory, key, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let meta = file.metadata().map_err(|_| E::Unavailable)?;
        if !meta.is_file()
            || meta.nlink() != 1
            || meta.uid() != unsafe { libc::geteuid() }
            || meta.mode() & 0o077 != 0
            || meta.len() > MAX_FILE
            || meta.len() <= 8
        {
            return Err(E::NotFound);
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| E::Unavailable)?;
        if bytes.len() as u64 > MAX_FILE || bytes.len() <= 8 {
            return Err(E::NotFound);
        }
        let stamp = u64::from_be_bytes(bytes[..8].try_into().unwrap());
        if stamp > now() || now().saturating_sub(stamp) >= TTL {
            return Err(E::NotFound);
        }
        Ok((stamp, bytes.split_off(8)))
    }
    fn prune(&self, reserve: Option<(usize, u64)>) -> Result<(), E> {
        // Bounded iteration even if the directory was populated outside Arinn.
        let entries = std::fs::read_dir(format!("/proc/self/fd/{}", self.directory.as_raw_fd()))
            .map_err(|_| E::Unavailable)?;
        let mut live = Vec::new();
        let mut names = Vec::new();
        for entry in entries.take(257) {
            names.push(entry.map_err(|_| E::Unavailable)?.file_name());
        }
        if names.len() > 256 {
            return Err(E::Unavailable);
        }
        for file in names {
            let Some(key) = file.to_str() else {
                return Err(E::Unavailable);
            };
            if !key_name(key) {
                if key.starts_with(".pending-") {
                    self.remove(key)?;
                    continue;
                }
                return Err(E::Unavailable);
            }
            match self.read(key) {
                Ok((stamp, bytes)) => live.push((
                    stamp,
                    key.to_owned(),
                    bytes.len() as u64 + 8 + key.len() as u64,
                )),
                Err(_) => self.remove(key)?,
            }
        }
        live.sort();
        let (extra_count, extra_bytes) = reserve.unwrap_or_default();
        let mut count = live.len() + extra_count;
        let mut size = live.iter().map(|e| e.2).sum::<u64>() + extra_bytes;
        for (_, key, bytes) in live {
            if count <= 64 && size <= MAX_BYTES {
                break;
            }
            self.remove(&key)?;
            count -= 1;
            size -= bytes;
        }
        Ok(())
    }
    pub fn get(&self, key: &str) -> Option<Asset> {
        if !key_name(key) || self.prune(None).is_err() {
            return None;
        }
        let result = self.read(key).and_then(|(_, bytes)| {
            let asset = Asset {
                mime: "image/png".into(),
                data: bytes,
            };
            arinn_protocol::web_icons::image(&asset).map_err(|_| E::NotFound)?;
            raster::normalize(&asset.data)
        });
        if result.is_err() {
            let _ = self.remove(key);
        }
        result.ok()
    }
    pub fn put(
        &self,
        key: &str,
        asset: &Asset,
        deadline: std::time::Instant,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<(), E> {
        if !key_name(key) {
            return Err(E::InvalidArgument);
        }
        arinn_protocol::web_icons::image(asset)?;
        self.remove(key)?;
        self.prune(Some((1, asset.data.len() as u64 + 8 + key.len() as u64)))?;
        // Only one discovery writes each key; O_EXCL refuses any stale link.
        let pending = format!(".pending-{key}");
        let result = (|| {
            let mut file = open_at(
                &self.directory,
                &pending,
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            )?;
            file.write_all(&now().to_be_bytes())
                .and_then(|_| file.write_all(&asset.data))
                .and_then(|_| file.sync_all())
                .map_err(|_| E::Unavailable)?;
            if std::time::Instant::now() >= deadline
                || cancelled.load(std::sync::atomic::Ordering::Acquire)
            {
                return Err(E::Unavailable);
            }
            let from = name(&pending)?;
            let to = name(key)?;
            let result = unsafe {
                libc::renameat(
                    self.directory.as_raw_fd(),
                    from.as_ptr(),
                    self.directory.as_raw_fd(),
                    to.as_ptr(),
                )
            };
            if result != 0 {
                return Err(E::Unavailable);
            }
            Ok(())
        })();
        let _ = self.remove(&pending);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    struct Tree(std::path::PathBuf);
    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn bounds_survive_restart_and_bad_files_never_escape() {
        let tree =
            Tree(std::env::temp_dir().join(format!("arinn-icon-cache-{}", uuid::Uuid::new_v4())));
        let cache = Cache::new(&tree.0).unwrap();
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(8, 8)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        let asset = raster::normalize(&out.into_inner()).unwrap();
        for n in 0..70 {
            cache
                .put(
                    &format!("{n:064x}"),
                    &asset,
                    std::time::Instant::now() + std::time::Duration::from_secs(5),
                    &std::sync::atomic::AtomicBool::new(false),
                )
                .unwrap();
        }
        assert_eq!(std::fs::read_dir(&tree.0).unwrap().count(), 64);
        drop(cache);
        let cache = Cache::new(&tree.0).unwrap();
        assert_eq!(std::fs::read_dir(&tree.0).unwrap().count(), 64);
        let key = format!("{:064x}", 100);
        cache
            .put(
                &key,
                &asset,
                std::time::Instant::now() + std::time::Duration::from_secs(5),
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
        assert!(cache.get(&key).is_some());
        let path = tree.0.join(&key);
        std::fs::write(&path, [0u8; 9]).unwrap();
        assert!(cache.get(&key).is_none());
        cache
            .put(
                &key,
                &asset,
                std::time::Instant::now() + std::time::Duration::from_secs(5),
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(cache.get(&key).is_none());
        let outside = tree.0.join("outside");
        // A symlink cache entry is removed without opening its target.
        std::fs::write(&outside, b"untouched").unwrap();
        symlink(&outside, &path).unwrap();
        assert!(cache.read(&key).is_err());
        assert_eq!(std::fs::read(&outside).unwrap(), b"untouched");
        std::fs::remove_file(&outside).unwrap();
        std::fs::remove_file(&path).unwrap();
        cache
            .put(
                &key,
                &asset,
                std::time::Instant::now() + std::time::Duration::from_secs(5),
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
        let mut expired = now().saturating_sub(TTL + 1).to_be_bytes().to_vec();
        expired.extend_from_slice(&asset.data);
        std::fs::write(&path, expired).unwrap();
        drop(cache);
        let cache = Cache::new(&tree.0).unwrap();
        assert!(cache.get(&key).is_none());
        let link = tree.0.join("linked");
        symlink("/tmp", &link).unwrap();
        assert!(Cache::new(&link.join("forbidden")).is_err());
        std::fs::remove_file(&link).unwrap();
        let undeletable = tree.0.join(format!("{:064x}", 101));
        std::fs::create_dir(&undeletable).unwrap();
        assert!(cache.prune(None).is_err());
    }
}
