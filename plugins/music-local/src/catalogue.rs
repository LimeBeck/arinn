// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider-owned last-known catalogue. Records never leave the provider with
//! their paths; only Track is suitable for a later validated wire projection.
use crate::probe::Probe;
use arinn_music_common::Track;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

const MAX_FILES: usize = 100_000;
const MAX_DEPTH: usize = 64;
const EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "opus", "m4a", "aac", "wav"];

#[derive(Clone)]
pub struct Root {
    pub id: String,
    pub path: PathBuf,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Identity {
    pub(crate) device: u64,
    pub(crate) inode: u64,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Fingerprint {
    length: u64,
    modified: i64,
    nanos: i64,
    device: u64,
    inode: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct Record {
    root: String,
    root_path: PathBuf,
    relative: PathBuf,
    fingerprint: Fingerprint,
    track: Track,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Catalogue {
    roots: BTreeMap<String, Identity>,
    records: BTreeMap<String, Record>,
    revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootStatus {
    pub id: String,
    pub available: bool,
    pub incomplete: bool,
}

impl Catalogue {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn tracks(&self) -> Vec<Track> {
        self.records.values().map(|r| r.track.clone()).collect()
    }
    pub fn search(&self, query: &str) -> Vec<Track> {
        let query = query.to_lowercase();
        self.records
            .values()
            .filter(|r| {
                [&r.track.title, &r.track.artist, &r.track.album]
                    .iter()
                    .any(|v| v.to_lowercase().contains(&query))
            })
            .map(|r| r.track.clone())
            .collect()
    }
    /// Open through anchored, no-follow directory descriptors again at play time.
    /// Never return a path that could become a symlink between scan and playback.
    pub fn open_track(&self, id: &str) -> std::io::Result<std::fs::File> {
        let record = self
            .records
            .get(id)
            .filter(|r| r.track.available)
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        let identity = self
            .roots
            .get(&record.root)
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        crate::paths::open_beneath(&record.root_path, &record.relative, *identity)
    }

    /// The caller supplies stable opaque root IDs; these never depend on labels.
    /// Missing/replaced roots and failed subtrees preserve last-known records.
    pub fn scan(&mut self, roots: &[Root], probe: &impl Probe) -> Vec<RootStatus> {
        let before = self.tracks();
        for record in self.records.values_mut() {
            record.track.available = false;
        }
        let mut statuses = Vec::new();
        for root in roots {
            let identity = root
                .path
                .is_absolute()
                .then(|| fs::symlink_metadata(&root.path).ok())
                .flatten()
                .filter(|m| m.is_dir() && !m.file_type().is_symlink())
                .map(|m| Identity {
                    device: m.dev(),
                    inode: m.ino(),
                });
            let valid = identity.is_some_and(|identity| {
                self.roots.get(&root.id).is_none_or(|old| *old == identity)
            });
            let mut status = RootStatus {
                id: root.id.clone(),
                available: valid,
                incomplete: false,
            };
            if let Some(identity) = identity.filter(|_| valid) {
                self.roots.insert(root.id.clone(), identity);
                let mut stack = vec![(root.path.clone(), 0)];
                let mut visited = HashSet::new();
                let mut encountered = 0_usize;
                while let Some((directory, depth)) = stack.pop() {
                    if depth > MAX_DEPTH || encountered >= MAX_FILES {
                        status.incomplete = true;
                        break;
                    }
                    let entries = match fs::read_dir(&directory) {
                        Ok(v) => v,
                        Err(_) => {
                            status.incomplete = true;
                            continue;
                        }
                    };
                    for entry in entries {
                        if encountered >= MAX_FILES {
                            status.incomplete = true;
                            break;
                        }
                        encountered += 1;
                        let entry = match entry {
                            Ok(v) => v,
                            Err(_) => {
                                status.incomplete = true;
                                continue;
                            }
                        };
                        let path = entry.path();
                        // Do not follow links, including file links outside the root.
                        let metadata = match fs::symlink_metadata(&path) {
                            Ok(v) => v,
                            Err(_) => {
                                status.incomplete = true;
                                continue;
                            }
                        };
                        if metadata.file_type().is_symlink() {
                            continue;
                        }
                        if metadata.is_dir() {
                            stack.push((path, depth + 1));
                            continue;
                        }
                        if !metadata.is_file() || !audio_candidate(&path) {
                            continue;
                        }
                        if visited.len() >= MAX_FILES {
                            status.incomplete = true;
                            break;
                        }
                        visited.insert(path.clone());
                        let relative = match path.strip_prefix(&root.path) {
                            Ok(p) => p,
                            Err(_) => continue,
                        };
                        let Some(relative_text) = relative.to_str() else {
                            status.incomplete = true;
                            continue;
                        };
                        let id = opaque(&[&root.id, relative_text]);
                        let Ok(file) = crate::paths::open_beneath(&root.path, relative, identity)
                        else {
                            status.incomplete = true;
                            continue;
                        };
                        let Ok(metadata) = file.metadata() else {
                            status.incomplete = true;
                            continue;
                        };
                        let fingerprint = Fingerprint {
                            length: metadata.len(),
                            modified: metadata.mtime(),
                            nanos: metadata.mtime_nsec(),
                            device: metadata.dev(),
                            inode: metadata.ino(),
                        };

                        if let Some(record) = self
                            .records
                            .get_mut(&id)
                            .filter(|r| r.fingerprint == fingerprint)
                        {
                            record.root_path = root.path.clone();
                            record.track.available = true;
                            continue;
                        }
                        let Ok(mut track) = probe.inspect_file(&path, file) else {
                            status.incomplete = true;
                            continue;
                        };
                        track.id = id.clone();
                        let folder = album_folder(relative);
                        track.album_id =
                            opaque(&[&root.id, &folder, &track.album_artist, &track.album]);
                        self.records.insert(
                            id,
                            Record {
                                root: root.id.clone(),
                                root_path: root.path.clone(),
                                relative: relative.to_path_buf(),
                                fingerprint,
                                track,
                            },
                        );
                    }
                }
            }
            statuses.push(status);
        }
        if before != self.tracks() {
            self.revision = self.revision.saturating_add(1).max(1);
        }
        statuses
    }
}

fn opaque(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part.as_bytes());
    }
    format!("{:x}", hash.finalize())
}
fn audio_candidate(path: &Path) -> bool {
    path.extension()
        .and_then(|p| p.to_str())
        .is_some_and(|v| EXTENSIONS.contains(&v.to_ascii_lowercase().as_str()))
}
fn album_folder(relative: &Path) -> String {
    let parent = relative.parent().unwrap_or(Path::new(""));
    let name = parent
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let disc = ["disc", "disk", "cd"].iter().any(|prefix| {
        name.strip_prefix(prefix)
            .is_some_and(|v| !v.trim().is_empty() && v.trim().chars().all(|c| c.is_ascii_digit()))
    });
    if disc {
        parent.parent().unwrap_or(parent)
    } else {
        parent
    }
    .to_string_lossy()
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, os::unix::fs::symlink};
    struct Fixture {
        path: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("arinn-music-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self { path }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
    struct FakeProbe(Cell<usize>);
    impl Probe for FakeProbe {
        fn inspect(&self, path: &Path) -> Result<Track, crate::probe::Error> {
            self.0.set(self.0.get() + 1);
            if path.file_stem().unwrap() == "bad" {
                return Err(crate::probe::Error::Invalid);
            }
            Ok(Track {
                title: path.file_stem().unwrap().to_str().unwrap().into(),
                album: "Same title".into(),
                artist: "Singer".into(),
                album_artist: "Various".into(),
                available: true,
                ..Track::default()
            })
        }
    }
    #[test]
    fn stable_rescan_search_failures_and_missing_root_preserve_references() {
        let fixture = Fixture::new();
        fs::write(fixture.path.join("Song.MP3"), b"synthetic").unwrap();
        fs::write(fixture.path.join("bad.flac"), b"corrupt").unwrap();
        let roots = [Root {
            id: "root-a".into(),
            path: fixture.path.clone(),
        }];
        let probe = FakeProbe(Cell::new(0));
        let mut catalogue = Catalogue::default();
        assert!(catalogue.scan(&roots, &probe)[0].incomplete);
        let id = catalogue.tracks()[0].id.clone();
        let revision = catalogue.revision();
        catalogue.scan(&roots, &probe);
        assert_eq!(catalogue.revision(), revision);
        assert_eq!(probe.0.get(), 3); // good reused, corrupt retried
        assert_eq!(catalogue.search("SONG")[0].id, id);
        let moved = fixture.path.with_extension("offline");
        fs::rename(&fixture.path, &moved).unwrap();
        assert!(!catalogue.scan(&roots, &probe)[0].available);
        assert!(!catalogue.tracks()[0].available);
        assert!(catalogue.open_track(&id).is_err());
        fs::rename(moved, &fixture.path).unwrap();
        catalogue.scan(&roots, &probe);
        assert_eq!(catalogue.tracks()[0].id, id);
        assert!(catalogue.tracks()[0].available);
    }
    #[test]
    fn links_do_not_escape_roots_and_disc_folders_group_without_merging_other_albums() {
        let fixture = Fixture::new();
        let outside = Fixture::new();
        fs::write(outside.path.join("private.mp3"), b"outside").unwrap();
        symlink(&outside.path, fixture.path.join("escape")).unwrap();
        for path in ["Album/CD1/a.flac", "Album/CD2/b.flac", "Other/c.flac"] {
            fs::create_dir_all(fixture.path.join(path).parent().unwrap()).unwrap();
            fs::write(fixture.path.join(path), b"fixture").unwrap();
        }
        let mut catalogue = Catalogue::default();
        catalogue.scan(
            &[Root {
                id: "root".into(),
                path: fixture.path.clone(),
            }],
            &FakeProbe(Cell::new(0)),
        );
        let tracks = catalogue.tracks();
        assert_eq!(tracks.len(), 3);
        let album = |name| {
            tracks
                .iter()
                .find(|t| t.title == name)
                .unwrap()
                .album_id
                .clone()
        };
        assert_eq!(album("a"), album("b"));
        assert_ne!(album("a"), album("c"));
    }
}
