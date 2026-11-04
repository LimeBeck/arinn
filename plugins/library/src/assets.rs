// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Local artwork bytes for current catalogue ids (ADR-0062).
use arinn_protocol::{
    assets::{self, Asset},
    validation::ErrorCode,
};
use std::{
    ffi::CString,
    fs::File,
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::{Component, Path, PathBuf},
};

pub(crate) struct AssetSource {
    pub root: PathBuf,
    pub relative: String,
    pub series: bool,
    pub episode: bool,
    pub poster: bool,
}

impl AssetSource {
    /// Called on a blocking worker after releasing the catalogue lock.
    pub(crate) fn read(self) -> Result<Asset, ErrorCode> {
        let root = self.root.canonicalize().map_err(|_| ErrorCode::NotFound)?;
        let root_handle = open_root(&root)?;
        let pinned = root_handle.metadata().map_err(|_| ErrorCode::NotFound)?;
        let named = root.metadata().map_err(|_| ErrorCode::NotFound)?;
        if !pinned.is_dir() || (pinned.dev(), pinned.ino()) != (named.dev(), named.ino()) {
            return Err(ErrorCode::NotFound);
        }
        let relative = if self.series {
            self.relative
                .strip_prefix("dir:")
                .filter(|s| !s.is_empty())
                .ok_or(ErrorCode::NotFound)?
        } else {
            &self.relative
        };
        let relative = Path::new(relative);
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(ErrorCode::NotFound);
        }
        let path = root.join(relative);
        let (directory, stem) = if self.series {
            (path.as_path(), "*")
        } else {
            (
                path.parent().ok_or(ErrorCode::NotFound)?,
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or(ErrorCode::NotFound)?,
            )
        };
        let (poster, fanart) = crate::parse::artwork(&root, directory, stem, self.episode);
        let candidate = if self.poster { poster } else { fanart }.ok_or(ErrorCode::NotFound)?;
        let candidate = candidate.canonicalize().map_err(|_| ErrorCode::NotFound)?;
        let relative = candidate
            .strip_prefix(&root)
            .map_err(|_| ErrorCode::NotFound)?;
        let named = root.metadata().map_err(|_| ErrorCode::NotFound)?;
        if (pinned.dev(), pinned.ino()) != (named.dev(), named.ino()) {
            return Err(ErrorCode::NotFound);
        }
        let file = open_beneath(&root_handle, relative)?;
        let metadata = file.metadata().map_err(|_| ErrorCode::NotFound)?;
        if !metadata.is_file() || metadata.len() > assets::MAX_ASSET_BYTES as u64 {
            return Err(ErrorCode::Unsupported);
        }
        let mut data = Vec::new();
        file.take(assets::MAX_ASSET_BYTES as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|_| ErrorCode::Unavailable)?;
        let mime = assets::sniff(&data)
            .ok_or(ErrorCode::Unsupported)?
            .to_string();
        let asset = Asset { mime, data };
        asset.validate().map_err(|_| ErrorCode::Unsupported)?;
        Ok(asset)
    }
}

/// Pin resolution below an opened root, including if an ancestor is exchanged
/// for a symlink between the catalogue lookup and opening the image. Nonblocking
/// open and the regular-file check also prevent a substituted FIFO from hanging.
fn open_root(root: &Path) -> Result<File, ErrorCode> {
    let filesystem = File::open("/").map_err(|_| ErrorCode::NotFound)?;
    let relative = root.strip_prefix("/").map_err(|_| ErrorCode::NotFound)?;
    // A canonical path has no intended symlinks. Reject exchanged ancestors
    // while acquiring the root descriptor, not only its final component.
    open_at(
        &filesystem,
        relative,
        libc::O_PATH | libc::O_DIRECTORY,
        0x08 | 0x04 | 0x02,
    )
}

fn open_beneath(directory: &File, relative: &Path) -> Result<File, ErrorCode> {
    open_at(
        directory,
        relative,
        libc::O_RDONLY | libc::O_NONBLOCK,
        0x08 | 0x02,
    )
}

fn open_at(directory: &File, relative: &Path, flags: i32, resolve: u64) -> Result<File, ErrorCode> {
    let relative =
        CString::new(relative.as_os_str().as_bytes()).map_err(|_| ErrorCode::NotFound)?;
    #[repr(C)]
    struct OpenHow {
        flags: u64,
        mode: u64,
        resolve: u64,
    }
    let how = OpenHow {
        flags: (flags | libc::O_CLOEXEC) as u64,
        mode: 0,
        resolve, // Linux RESOLVE_BENEATH=0x08, NO_SYMLINKS=0x04, NO_MAGICLINKS=0x02.
    };
    // SAFETY: directory and relative remain alive for the syscall; how has the
    // Linux open_how ABI and its size is passed explicitly. A successful fd is
    // owned once by the returned File.
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            directory.as_raw_fd(),
            relative.as_ptr(),
            &how,
            std::mem::size_of::<OpenHow>(),
        )
    };
    if fd < 0 {
        return Err(ErrorCode::NotFound);
    }
    // SAFETY: openat2 returned a new owned file descriptor.
    Ok(unsafe { File::from_raw_fd(fd as i32) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MountTables, Settings, Store};
    use std::{fs, os::unix::fs::symlink};

    const JPEG: &[u8] = &[0xff, 0xd8, 0xff, 0xe0, 1, 2, 3];
    struct Fixture {
        dir: PathBuf,
        root: PathBuf,
        store: Store,
        settings: Settings,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("arinn-artwork-{}", uuid::Uuid::new_v4()));
            let root = dir.join("media");
            fs::create_dir_all(root.join("Show/Season 1")).unwrap();
            fs::write(root.join("Film.mkv"), b"video").unwrap();
            fs::write(root.join("Film-poster.jpg"), JPEG).unwrap();
            fs::write(root.join("Show/Season 1/Show.S01E01.mkv"), b"video").unwrap();
            fs::write(root.join("Show/poster.jpg"), JPEG).unwrap();
            fs::write(root.join("Show/fanart.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
            let mut store = Store::open(&dir.join("library.sqlite3")).unwrap();
            store.use_mount_tables(MountTables::default());
            let settings = Settings {
                roots: vec![root.clone()],
                probe: false,
                autoscan: false,
                ..Settings::default()
            };
            store.scan(&settings).unwrap();
            Self {
                dir,
                root,
                store,
                settings,
            }
        }
        fn poster(&self) -> String {
            self.store
                .snapshot()
                .unwrap()
                .1
                .into_iter()
                .find(|i| i.kind == "film")
                .unwrap()
                .poster
        }
        fn read(&self, id: &str) -> Result<Asset, ErrorCode> {
            self.store.asset_source(id, &self.settings.roots)?.read()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn serves_film_and_series_artwork_without_changing_identity_or_progress() {
        let mut f = Fixture::new();
        let before = f.store.snapshot().unwrap().1;
        let film = before.iter().find(|i| i.kind == "film").unwrap();
        f.store.set_progress(&film.id, 10, 100, false).unwrap();
        let progress = f.store.resumes().unwrap();
        assert_eq!(f.read(&film.poster).unwrap().data, JPEG);
        let series = before.iter().find(|i| i.kind == "series").unwrap();
        assert_eq!(f.read(&series.poster).unwrap().mime, assets::MIME_JPEG);
        assert_eq!(f.read(&series.fanart).unwrap().mime, assets::MIME_PNG);
        assert_eq!(f.store.snapshot().unwrap().1, before);
        assert_eq!(f.store.resumes().unwrap(), progress);
    }

    // #756 C22: a folder-wide poster in a season folder is the season's
    // image, not every episode's. Only an image named after the file is the
    // episode's still.
    #[test]
    fn episodes_take_only_artwork_named_after_their_file() {
        let mut f = Fixture::new();
        let season = f.root.join("Show/Season 1");
        fs::write(season.join("poster.jpg"), JPEG).unwrap();
        fs::write(season.join("fanart.jpg"), JPEG).unwrap();
        fs::write(season.join("Show.S01E02.mkv"), b"video").unwrap();
        fs::write(season.join("Show.S01E02-thumb.jpg"), JPEG).unwrap();
        f.store.scan(&f.settings).unwrap();
        let items = f.store.snapshot().unwrap().1;
        let episode = |number: u32| {
            items
                .iter()
                .find(|i| i.kind == "episode" && i.episode == number)
                .unwrap()
                .clone()
        };
        let first = episode(1);
        assert!(first.poster.is_empty(), "the season poster is not a still");
        assert!(first.fanart.is_empty());
        let second = episode(2);
        assert!(!second.poster.is_empty());
        assert_eq!(f.read(&second.poster).unwrap().data, JPEG);
        let series = items.iter().find(|i| i.kind == "series").unwrap();
        assert!(!series.poster.is_empty());
    }

    #[test]
    fn unknown_stale_and_unconfigured_ids_do_not_resolve() {
        let mut f = Fixture::new();
        assert!(matches!(
            f.read(&uuid::Uuid::new_v4().to_string()),
            Err(ErrorCode::NotFound)
        ));
        let id = f.poster();
        assert!(matches!(
            f.store.asset_source(&id, &[]),
            Err(ErrorCode::NotFound)
        ));
        fs::remove_file(f.root.join("Film-poster.jpg")).unwrap();
        assert!(matches!(f.read(&id), Err(ErrorCode::NotFound)));
        f.store.scan(&f.settings).unwrap();
        fs::write(f.root.join("Film-poster.jpg"), JPEG).unwrap();
        f.store.scan(&f.settings).unwrap();
        assert_ne!(f.poster(), id);
        assert!(matches!(f.read(&id), Err(ErrorCode::NotFound)));
    }

    #[test]
    fn an_exchanged_root_ancestor_is_not_followed() {
        let f = Fixture::new();
        let canonical = f.root.canonicalize().unwrap();
        let outside = f.dir.with_extension("outside");
        let saved = f.dir.with_extension("saved");
        fs::create_dir_all(outside.join("media")).unwrap();
        fs::rename(&f.dir, &saved).unwrap();
        symlink(&outside, &f.dir).unwrap();
        let rejected = open_root(&canonical).is_err();
        fs::remove_file(&f.dir).unwrap();
        fs::rename(&saved, &f.dir).unwrap();
        fs::remove_dir_all(&outside).unwrap();
        assert!(rejected);
    }

    #[test]
    fn replacing_the_root_cannot_redirect_an_already_pinned_read() {
        let f = Fixture::new();
        let root_handle = open_root(&f.root).unwrap();
        let outside = f.dir.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("Film-poster.jpg"), b"outside-data").unwrap();
        fs::rename(&f.root, f.dir.join("original-media")).unwrap();
        symlink(&outside, &f.root).unwrap();
        let mut data = Vec::new();
        open_beneath(&root_handle, Path::new("Film-poster.jpg"))
            .unwrap()
            .read_to_end(&mut data)
            .unwrap();
        assert_eq!(data, JPEG);
    }

    #[test]
    fn invalid_oversized_and_escaping_replacements_are_rejected() {
        let f = Fixture::new();
        let id = f.poster();
        let poster = f.root.join("Film-poster.jpg");
        fs::write(&poster, b"<svg/> not a raster image").unwrap();
        assert!(matches!(f.read(&id), Err(ErrorCode::Unsupported)));
        let file = File::create(&poster).unwrap();
        file.set_len(assets::MAX_ASSET_BYTES as u64 + 1).unwrap();
        assert!(matches!(f.read(&id), Err(ErrorCode::Unsupported)));
        let outside = f.dir.join("outside.jpg");
        fs::write(&outside, JPEG).unwrap();
        fs::remove_file(&poster).unwrap();
        symlink(&outside, &poster).unwrap();
        assert!(matches!(f.read(&id), Err(ErrorCode::NotFound)));
        assert!(open_beneath(&File::open(&f.root).unwrap(), Path::new("Film-poster.jpg")).is_err());
        assert!(open_beneath(&File::open(&f.root).unwrap(), Path::new("../outside.jpg")).is_err());
        fs::remove_file(&poster).unwrap();
        fs::write(f.root.join("safe.jpg"), JPEG).unwrap();
        symlink(f.root.join("safe.jpg"), &poster).unwrap();
        assert_eq!(f.read(&id).unwrap().data, JPEG);
    }
}
