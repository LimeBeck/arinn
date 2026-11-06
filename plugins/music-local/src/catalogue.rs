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

#[derive(Clone, Serialize, Deserialize)]
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

#[derive(Clone, PartialEq)]
struct Record {
    root: String,
    relative: PathBuf,
    fingerprint: Fingerprint,
    track: Track,
}

/// The persisted catalogue is compact: a record carries no root path (one
/// per root in `paths`), its track ID is the map key and its album ID is
/// derived again on load. Field names are one letter. A 100,000-track
/// library then fits the library's storage budget (review of #710).
#[derive(Clone, Default, PartialEq)]
pub struct Catalogue {
    roots: BTreeMap<String, Identity>,
    paths: BTreeMap<String, PathBuf>,
    records: BTreeMap<String, Record>,
    revision: u64,
}

#[derive(Serialize, Deserialize)]
struct StoredCatalogue {
    roots: BTreeMap<String, Identity>,
    paths: BTreeMap<String, PathBuf>,
    records: BTreeMap<String, StoredRecord>,
    revision: u64,
}

fn empty(value: &str) -> bool {
    value.is_empty()
}
fn zero32(value: &u32) -> bool {
    *value == 0
}
fn zero64(value: &u64) -> bool {
    *value == 0
}
fn no(value: &bool) -> bool {
    !*value
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRecord {
    r: String,
    p: PathBuf,
    /// length, mtime seconds, mtime nanoseconds, device, inode
    f: (u64, i64, i64, u64, u64),
    #[serde(default, skip_serializing_if = "empty")]
    t: String,
    #[serde(default, skip_serializing_if = "empty")]
    a: String,
    #[serde(default, skip_serializing_if = "empty")]
    b: String,
    #[serde(default, skip_serializing_if = "empty")]
    c: String,
    #[serde(default, skip_serializing_if = "zero32")]
    d: u32,
    #[serde(default, skip_serializing_if = "zero32")]
    n: u32,
    #[serde(default, skip_serializing_if = "zero64")]
    u: u64,
    #[serde(default, skip_serializing_if = "no")]
    v: bool,
    #[serde(default, skip_serializing_if = "no")]
    w: bool,
    /// First discovery, Unix seconds (#806). Older catalogues lack it; the
    /// file's mtime is the best estimate for them.
    #[serde(default, skip_serializing_if = "zero64")]
    s: u64,
}

impl Serialize for Catalogue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let records = self
            .records
            .iter()
            .map(|(id, r)| {
                let f = &r.fingerprint;
                let t = &r.track;
                (
                    id.clone(),
                    StoredRecord {
                        r: r.root.clone(),
                        p: r.relative.clone(),
                        f: (f.length, f.modified, f.nanos, f.device, f.inode),
                        t: t.title.clone(),
                        a: t.artist.clone(),
                        b: t.album.clone(),
                        c: t.album_artist.clone(),
                        d: t.disc,
                        n: t.number,
                        u: t.duration_us,
                        v: t.available,
                        w: t.embedded_artwork,
                        s: t.added_at,
                    },
                )
            })
            .collect();
        StoredCatalogue {
            roots: self.roots.clone(),
            paths: self.paths.clone(),
            records,
            revision: self.revision,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Catalogue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = StoredCatalogue::deserialize(deserializer)?;
        let records = stored
            .records
            .into_iter()
            .map(|(id, r)| {
                let mut track = Track {
                    id: id.clone(),
                    title: r.t,
                    artist: r.a,
                    album: r.b,
                    album_artist: r.c,
                    disc: r.d,
                    number: r.n,
                    duration_us: r.u,
                    available: r.v,
                    embedded_artwork: r.w,
                    // A future mtime (a wrong clock, an extracted archive)
                    // must not make a track the newest for years, nor pass
                    // the protocol's date bound.
                    added_at: if r.s == 0 {
                        u64::try_from(r.f.1).unwrap_or(0).min(now())
                    } else {
                        r.s
                    },
                    ..Track::default()
                };
                track.album_id = album_id(&r.r, &r.p, &track);
                let (length, modified, nanos, device, inode) = r.f;
                (
                    id,
                    Record {
                        root: r.r,
                        relative: r.p,
                        fingerprint: Fingerprint {
                            length,
                            modified,
                            nanos,
                            device,
                            inode,
                        },
                        track,
                    },
                )
            })
            .collect();
        Ok(Self {
            roots: stored.roots,
            paths: stored.paths,
            records,
            revision: stored.revision,
        })
    }
}

/// An album is identified by its root, its folder (disc folders share their
/// parent) and the explicit album-artist and album tags. A track's own
/// artist is not part of it, so a compilation without an album-artist tag
/// stays one album (review of #710).
fn album_id(root: &str, relative: &Path, track: &Track) -> String {
    opaque(&[
        root,
        &album_folder(relative),
        &track.album_artist,
        &track.album,
    ])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootStatus {
    pub id: String,
    pub available: bool,
    pub incomplete: bool,
}

impl Catalogue {
    /// A restored catalogue is metadata only until its configured roots rescan.
    pub(crate) fn invalidate(&mut self) {
        if self.records.values().any(|r| r.track.available) {
            for record in self.records.values_mut() {
                record.track.available = false;
            }
            self.revision = self.revision.saturating_add(1);
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Forgets roots that are no longer configured, with their records,
    /// except records a playlist still refers to (those stay unavailable).
    pub(crate) fn retain_roots(&mut self, keep: &HashSet<&str>, referenced: &HashSet<&str>) {
        self.roots.retain(|id, _| keep.contains(id.as_str()));
        self.paths.retain(|id, _| keep.contains(id.as_str()));
        let before = self.records.len();
        self.records.retain(|id, record| {
            keep.contains(record.root.as_str()) || referenced.contains(id.as_str())
        });
        if self.records.len() != before {
            self.revision = self.revision.saturating_add(1);
        }
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
        let path = self
            .paths
            .get(&record.root)
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        crate::paths::open_beneath(path, &record.relative, *identity)
    }

    pub fn open_cover(&self, id: &str) -> std::io::Result<std::fs::File> {
        let record = self
            .records
            .get(id)
            .filter(|r| r.track.available)
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        let identity = *self
            .roots
            .get(&record.root)
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        let parent = record.relative.parent().unwrap_or_else(|| Path::new(""));
        for name in [
            "cover.jpg",
            "cover.png",
            "cover.webp",
            "folder.jpg",
            "folder.png",
        ] {
            if let Ok(file) = crate::paths::open_beneath(
                self.paths
                    .get(&record.root)
                    .ok_or(std::io::ErrorKind::NotFound)?,
                &parent.join(name),
                identity,
            ) {
                return Ok(file);
            }
        }
        Err(std::io::ErrorKind::NotFound.into())
    }
    /// The caller supplies stable opaque root IDs; these never depend on labels.
    /// Missing/replaced roots and failed subtrees preserve last-known records.
    pub fn scan(&mut self, roots: &[Root], probe: &impl Probe) -> Vec<RootStatus> {
        self.scan_bounded(roots, probe, MAX_FILES)
    }

    fn scan_bounded(
        &mut self,
        roots: &[Root],
        probe: &impl Probe,
        limit: usize,
    ) -> Vec<RootStatus> {
        self.scan_cancelled(roots, probe, limit, &|| false)
            .expect("uncancelled scan")
    }
    pub(crate) fn scan_cancelled(
        &mut self,
        roots: &[Root],
        probe: &impl Probe,
        limit: usize,
        cancelled: &impl Fn() -> bool,
    ) -> Option<Vec<RootStatus>> {
        let before = self.tracks();
        for record in self.records.values_mut() {
            record.track.available = false;
        }
        let mut statuses = Vec::new();
        let mut encountered = 0_usize;
        for root in roots {
            if cancelled() {
                return None;
            }
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
                self.paths.insert(root.id.clone(), root.path.clone());
                let mut stack = vec![(root.path.clone(), 0)];
                let mut visited = HashSet::new();
                while let Some((directory, depth)) = stack.pop() {
                    if cancelled() {
                        return None;
                    }
                    if encountered >= limit {
                        status.incomplete = true;
                        break;
                    }
                    // Too deep: skip this subtree, keep scanning its siblings.
                    if depth > MAX_DEPTH {
                        status.incomplete = true;
                        continue;
                    }
                    let entries = match fs::read_dir(&directory) {
                        Ok(v) => v,
                        Err(_) => {
                            status.incomplete = true;
                            continue;
                        }
                    };
                    for entry in entries {
                        if cancelled() {
                            return None;
                        }
                        if encountered >= limit {
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
                        if visited.len() >= limit {
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
                        if self.records.len() >= limit && !self.records.contains_key(&id) {
                            status.incomplete = true;
                            break;
                        }
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
                            record.track.available = true;
                            continue;
                        }
                        let Ok(mut track) = probe.inspect_file(&path, file) else {
                            status.incomplete = true;
                            continue;
                        };
                        track.id = id.clone();
                        track.album_id = album_id(&root.id, relative, &track);
                        // A changed file keeps the date it was first found;
                        // a new one is found now (#806).
                        track.added_at = self
                            .records
                            .get(&id)
                            .map(|r| r.track.added_at)
                            .filter(|at| *at != 0)
                            .unwrap_or_else(now);
                        self.records.insert(
                            id,
                            Record {
                                root: root.id.clone(),
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
        if cancelled() {
            return None;
        }
        Some(statuses)
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
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
    fn traversal_and_retained_record_budgets_apply_across_roots_and_rescans() {
        let a = Fixture::new();
        let b = Fixture::new();
        for root in [&a.path, &b.path] {
            for name in ["a.mp3", "b.mp3"] {
                fs::write(root.join(name), b"fixture").unwrap();
            }
        }
        let roots = [
            Root {
                id: "a".into(),
                path: a.path.clone(),
            },
            Root {
                id: "b".into(),
                path: b.path.clone(),
            },
        ];
        let mut catalogue = Catalogue::default();
        let statuses = catalogue.scan_bounded(&roots, &FakeProbe(Cell::new(0)), 3);
        assert_eq!(catalogue.tracks().len(), 3);
        assert!(statuses.iter().any(|s| s.incomplete));
        for root in [&a.path, &b.path] {
            fs::remove_file(root.join("a.mp3")).unwrap();
            fs::remove_file(root.join("b.mp3")).unwrap();
            fs::write(root.join("new.mp3"), b"fixture").unwrap();
        }
        catalogue.scan_bounded(&roots, &FakeProbe(Cell::new(0)), 3);
        assert_eq!(catalogue.tracks().len(), 3);
        assert!(catalogue.tracks().iter().all(|t| !t.available));
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
    /// #806: a track keeps the date it was first found. A rescan and a
    /// changed file keep it; a new file is dated at its discovery.
    #[test]
    fn first_discovery_date_survives_rescans_and_file_changes() {
        let fixture = Fixture::new();
        fs::write(fixture.path.join("a.flac"), b"one").unwrap();
        let roots = [Root {
            id: "root".into(),
            path: fixture.path.clone(),
        }];
        let probe = FakeProbe(Cell::new(0));
        let mut catalogue = Catalogue::default();
        let before = now();
        catalogue.scan(&roots, &probe);
        let id = catalogue.tracks()[0].id.clone();
        assert!(catalogue.tracks()[0].added_at >= before);
        // Pretend it was found long ago, then rescan and change the file.
        catalogue.records.get_mut(&id).unwrap().track.added_at = 1_700_000_000;
        catalogue.scan(&roots, &probe);
        assert_eq!(catalogue.tracks()[0].added_at, 1_700_000_000);
        fs::write(fixture.path.join("a.flac"), b"changed, longer").unwrap();
        catalogue.scan(&roots, &probe);
        let changed = catalogue.tracks().into_iter().find(|t| t.id == id).unwrap();
        assert_eq!(changed.added_at, 1_700_000_000);
        fs::write(fixture.path.join("b.flac"), b"two").unwrap();
        catalogue.scan(&roots, &probe);
        let new = catalogue.tracks().into_iter().find(|t| t.id != id).unwrap();
        assert!(new.added_at >= before);
        // The date is persisted and read back.
        let json = serde_json::to_string(&catalogue).unwrap();
        let loaded: Catalogue = serde_json::from_str(&json).unwrap();
        let reloaded = loaded.tracks().into_iter().find(|t| t.id == id).unwrap();
        assert_eq!(reloaded.added_at, 1_700_000_000);
    }
    /// #806: a catalogue stored before the date existed dates each track by
    /// its file's mtime instead of making every track new at once.
    #[test]
    fn a_catalogue_without_dates_uses_the_file_mtime() {
        let (id, record) = synthetic(7);
        let mut catalogue = Catalogue::default();
        catalogue.records.insert(id.clone(), record);
        let mut value = serde_json::to_value(&catalogue).unwrap();
        value["records"][&id].as_object_mut().unwrap().remove("s");
        let loaded: Catalogue = serde_json::from_value(value).unwrap();
        assert_eq!(loaded.tracks()[0].added_at, 1_761_998_400);
        // A future or absurd mtime falls back to now, not to that date.
        for future in [now() as i64 + 86_400 * 365, i64::MAX] {
            let mut value = serde_json::to_value(&catalogue).unwrap();
            let record = value["records"][&id].as_object_mut().unwrap();
            record.remove("s");
            record["f"][1] = future.into();
            let loaded: Catalogue = serde_json::from_value(value).unwrap();
            assert!(loaded.tracks()[0].added_at <= now(), "{future}");
            assert!(loaded.tracks()[0].added_at > 1_761_998_400, "{future}");
        }
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
    fn synthetic(i: usize) -> (String, Record) {
        let relative = PathBuf::from(format!(
            "Artist Name {}/Album Name {:04}/{:02} - Track Title Number {i}.flac",
            i % 500,
            i / 12,
            i % 12 + 1
        ));
        let mut track = Track {
            title: format!("Track Title Number {i}"),
            artist: format!("Artist Name {}", i % 500),
            album: format!("Album Name {:04}", i / 12),
            album_artist: format!("Artist Name {}", i % 500),
            disc: 1,
            number: (i % 12 + 1) as u32,
            duration_us: 241_000_000,
            available: true,
            added_at: 1_762_400_000,
            ..Track::default()
        };
        let root = "2b0f6f0e-5d0c-4a59-9a3b-1c1e8d7f6a52".to_owned();
        let id = opaque(&[&root, relative.to_str().unwrap()]);
        track.id = id.clone();
        track.album_id = album_id(&root, &relative, &track);
        let record = Record {
            root,
            relative,
            fingerprint: Fingerprint {
                length: 31_456_789,
                modified: 1_761_998_400,
                nanos: 123_456_789,
                device: 66_305,
                inode: 9_876_543 + i as u64,
            },
            track,
        };
        (id, record)
    }
    /// Review of #710: about 700 bytes per record filled the 32 MiB state at
    /// roughly 46,000 tracks, below the 100,000-track scan limit. The compact
    /// format must hold the scan limit with typical tags and round-trip.
    #[test]
    fn a_full_compact_catalogue_fits_the_library_budget_and_round_trips() {
        let mut catalogue = Catalogue::default();
        catalogue.roots.insert(
            "2b0f6f0e-5d0c-4a59-9a3b-1c1e8d7f6a52".into(),
            Identity {
                device: 66_305,
                inode: 2,
            },
        );
        catalogue.paths.insert(
            "2b0f6f0e-5d0c-4a59-9a3b-1c1e8d7f6a52".into(),
            PathBuf::from("/srv/music"),
        );
        catalogue.records = (0..MAX_FILES).map(synthetic).collect();
        catalogue.revision = 7;
        let bytes = serde_json::to_vec(&catalogue).unwrap();
        let per_record = bytes.len() / MAX_FILES;
        assert!(per_record < 420, "{per_record} bytes per record");
        assert!(bytes.len() < 64 * 1024 * 1024, "{} bytes", bytes.len());
        let restored: Catalogue = serde_json::from_slice(&bytes).unwrap();
        assert!(restored == catalogue, "the compact form loses nothing");
    }
    /// Review of #710: a compilation without an album-artist tag split into
    /// one album per track artist.
    #[test]
    fn a_compilation_without_an_album_artist_stays_one_album() {
        let relative = |name: &str| PathBuf::from(format!("Soundtrack/{name}.flac"));
        let track = |artist: &str| Track {
            album: "Soundtrack".into(),
            artist: artist.into(),
            ..Track::default()
        };
        assert_eq!(
            album_id("root", &relative("a"), &track("One")),
            album_id("root", &relative("b"), &track("Two"))
        );
        // The explicit tag still separates albums with the same title.
        let mut tagged = track("One");
        tagged.album_artist = "Various".into();
        assert_ne!(
            album_id("root", &relative("a"), &tagged),
            album_id("root", &relative("b"), &track("Two"))
        );
    }
    /// Review of #710: one directory past the depth limit stopped the scan
    /// of every sibling subtree still waiting on the stack.
    #[test]
    fn a_too_deep_directory_does_not_stop_its_siblings() {
        let fixture = Fixture::new();
        let mut deep = fixture.path.join("a");
        for _ in 0..=MAX_DEPTH {
            deep = deep.join("d");
        }
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("hidden.flac"), b"fixture").unwrap();
        fs::create_dir(fixture.path.join("z")).unwrap();
        fs::write(fixture.path.join("z/visible.flac"), b"fixture").unwrap();
        fs::write(fixture.path.join("a/shallow.flac"), b"fixture").unwrap();
        let mut catalogue = Catalogue::default();
        let statuses = catalogue.scan(
            &[Root {
                id: "root".into(),
                path: fixture.path.clone(),
            }],
            &FakeProbe(Cell::new(0)),
        );
        assert!(statuses[0].incomplete);
        let mut titles: Vec<String> = catalogue.tracks().into_iter().map(|t| t.title).collect();
        titles.sort();
        assert_eq!(titles, ["shallow", "visible"]);
    }
}
