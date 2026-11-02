// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Persistent local-library lifecycle; no daemon, shell or playback ownership.
use crate::{
    catalogue::{Catalogue, Root, RootStatus},
    probe::Probe,
};
pub use arinn_music_common::storage::Error;
use arinn_music_common::storage::Storage;
use arinn_music_common::{Track, text};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    path::{Component, Path},
};

const MAX_PLAYLISTS: usize = 256;
const MAX_ENTRIES: usize = 10_000;
const VERSION: u32 = 2;
/// The compact catalogue takes roughly 300–400 bytes per track with typical
/// tags, so 100,000 tracks fit well inside this budget. A library that still
/// does not fit stays usable in memory and is reported as not persisted.
const STATE_LIMIT: usize = 96 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
struct State {
    version: u32,
    #[serde(default)]
    revision: u64,
    catalogue: Catalogue,
    roots: Vec<Root>,
    playlists: BTreeMap<String, Playlist>,
    next_playlist: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            version: VERSION,
            revision: 0,
            catalogue: Catalogue::default(),
            roots: Vec::new(),
            playlists: BTreeMap::new(),
            next_playlist: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub title: String,
    /// Ordered track references; repeats are deliberately preserved.
    pub tracks: Vec<String>,
}

pub struct Library {
    storage: Storage,
    state: State,
}

/// Version 1 (this unreleased branch only) stored a larger catalogue. Its
/// configured roots carry over; the catalogue is rebuilt by the next scan
/// with the same track IDs. Playlists could not be created over D-Bus yet.
#[derive(Deserialize)]
struct LegacyState {
    version: u32,
    #[serde(default)]
    revision: u64,
    roots: Vec<Root>,
}

impl Library {
    pub fn catalogue_snapshot(&self) -> Catalogue {
        self.state.catalogue.clone()
    }
    /// Requires a private directory; a second writer fails without changing it.
    /// Corrupt/unsupported snapshots are errors, never silently reset.
    pub fn open(directory: &Path) -> Result<Self, Error> {
        Self::open_bounded(directory, STATE_LIMIT)
    }
    fn open_bounded(directory: &Path, limit: usize) -> Result<Self, Error> {
        let storage = Storage::open_bounded(directory, limit)?;
        let mut state: State = match storage.read()? {
            Some(bytes) => match serde_json::from_slice::<LegacyState>(&bytes) {
                Ok(legacy) if legacy.version == 1 => State {
                    revision: legacy.revision.saturating_add(1),
                    roots: legacy.roots,
                    ..State::default()
                },
                _ => serde_json::from_slice(&bytes).map_err(|_| Error::InvalidState)?,
            },
            None => State::default(),
        };
        validate(&state)?;
        let previous = state.catalogue.revision();
        state.catalogue.invalidate();
        if previous != state.catalogue.revision() {
            state.revision = state.revision.checked_add(1).ok_or(Error::TooLarge)?;
        }
        Ok(Self { storage, state })
    }
    pub fn tracks(&self) -> Vec<Track> {
        self.state.catalogue.tracks()
    }
    pub fn search(&self, query: &str) -> Vec<Track> {
        self.state.catalogue.search(query)
    }
    pub fn roots(&self) -> &[Root] {
        &self.state.roots
    }
    pub fn revision(&self) -> u64 {
        self.state.revision
    }
    pub fn playlists(&self) -> Vec<Playlist> {
        self.state.playlists.values().cloned().collect()
    }
    pub fn open_track(&self, id: &str) -> std::io::Result<File> {
        self.state.catalogue.open_track(id)
    }

    /// Roots that are no longer configured are forgotten with their tracks,
    /// except tracks a playlist still refers to.
    pub fn configure_roots(&mut self, roots: Vec<Root>) -> Result<(), Error> {
        let mut next = self.state.clone();
        next.roots = roots;
        let keep: HashSet<&str> = next.roots.iter().map(|r| r.id.as_str()).collect();
        let referenced: HashSet<&str> = next
            .playlists
            .values()
            .flat_map(|p| p.tracks.iter().map(String::as_str))
            .collect();
        next.catalogue.retain_roots(&keep, &referenced);
        next.catalogue.invalidate();
        if next.roots.iter().map(|r| (&r.id, &r.path)).eq(self
            .state
            .roots
            .iter()
            .map(|r| (&r.id, &r.path)))
            && next.catalogue == self.state.catalogue
        {
            return Ok(());
        }
        self.commit(next)
    }
    pub fn scan(&mut self, probe: &impl Probe) -> Result<Vec<RootStatus>, Error> {
        let mut next = self.state.clone();
        let status = next.catalogue.scan(&next.roots, probe);
        self.commit_scan(next)?;
        Ok(status)
    }
    pub fn scan_cancelled(
        &mut self,
        probe: &impl Probe,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Option<Vec<RootStatus>>, Error> {
        let mut next = self.state.clone();
        let Some(status) = next
            .catalogue
            .scan_cancelled(&next.roots, probe, 100_000, cancelled)
        else {
            return Ok(None);
        };
        if cancelled() {
            return Ok(None);
        }
        self.commit_scan(next)?;
        Ok(Some(status))
    }
    pub fn create_playlist(&mut self, title: &str, tracks: Vec<String>) -> Result<String, Error> {
        if self.state.next_playlist == u64::MAX {
            return Err(Error::TooLarge);
        }
        let mut next = self.state.clone();
        let id = format!("playlist-{}", next.next_playlist);
        next.next_playlist += 1;
        next.playlists.insert(
            id.clone(),
            Playlist {
                id: id.clone(),
                title: text(title),
                tracks,
            },
        );
        self.commit(next)?;
        Ok(id)
    }
    pub fn replace_playlist(
        &mut self,
        id: &str,
        title: &str,
        tracks: Vec<String>,
    ) -> Result<(), Error> {
        if !self.state.playlists.contains_key(id) {
            return Err(Error::InvalidState);
        }
        let mut next = self.state.clone();
        next.playlists.insert(
            id.into(),
            Playlist {
                id: id.into(),
                title: text(title),
                tracks,
            },
        );
        self.commit(next)
    }
    pub fn remove_playlist(&mut self, id: &str) -> Result<(), Error> {
        let mut next = self.state.clone();
        if next.playlists.remove(id).is_none() {
            return Err(Error::InvalidState);
        }
        self.commit(next)
    }
    /// A scan that changed nothing writes nothing and keeps the revision, so
    /// the shell keeps its paging (review of #710). A scan whose snapshot
    /// cannot be written is still adopted in memory: the library stays
    /// usable and the error reports that it was not persisted.
    fn commit_scan(&mut self, next: State) -> Result<(), Error> {
        if next.catalogue == self.state.catalogue {
            return Ok(());
        }
        match self.commit(next.clone()) {
            Err(error) if error != Error::Durability => {
                validate(&next)?;
                let revision = self.state.revision.checked_add(1).ok_or(Error::TooLarge)?;
                self.state = State { revision, ..next };
                Err(error)
            }
            result => result,
        }
    }
    fn commit(&mut self, mut next: State) -> Result<(), Error> {
        next.revision = self.state.revision.checked_add(1).ok_or(Error::TooLarge)?;
        validate(&next)?;
        let bytes = serde_json::to_vec(&next).map_err(|_| Error::InvalidState)?;
        let result = self.storage.write(&bytes);
        // Rename commits the new snapshot even when directory fsync fails.
        // Keep memory consistent with that file while reporting durability loss.
        if result.is_ok() || result == Err(Error::Durability) {
            self.state = next;
        }
        result
    }
}

fn validate(s: &State) -> Result<(), Error> {
    if s.version != VERSION
        || s.next_playlist == 0
        || s.roots.len() > 64
        || s.playlists.len() > MAX_PLAYLISTS
    {
        return Err(Error::InvalidState);
    }
    let mut roots = HashSet::new();
    for root in &s.roots {
        if root.id.is_empty()
            || root.id.len() > 128
            || !root
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            || !roots.insert(&root.id)
            || !root.path.is_absolute()
            || root
                .path
                .components()
                .any(|p| matches!(p, Component::ParentDir))
        {
            return Err(Error::InvalidState);
        }
    }
    let known: HashSet<_> = s.catalogue.tracks().into_iter().map(|t| t.id).collect();
    for (id, p) in &s.playlists {
        let sequence = id
            .strip_prefix("playlist-")
            .and_then(|n| n.parse::<u64>().ok());
        if sequence.is_none_or(|n| n == 0 || n >= s.next_playlist)
            || *id != p.id
            || id.len() > 64
            || p.title.is_empty()
            || text(&p.title) != p.title
            || p.tracks.len() > MAX_ENTRIES
            || p.tracks.iter().any(|t| !known.contains(t))
        {
            return Err(Error::InvalidState);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe;
    use std::{fs, path::PathBuf};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("arinn-music-lib-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            fs::create_dir(path.join("audio")).unwrap();
            for name in ["a.flac", "b.flac"] {
                fs::write(path.join("audio").join(name), b"fixture").unwrap();
            }
            Self(path)
        }
        fn root(&self) -> Root {
            Root {
                id: "root".into(),
                path: self.0.join("audio"),
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    struct Synthetic;
    impl Probe for Synthetic {
        fn inspect(&self, path: &Path) -> Result<Track, probe::Error> {
            Ok(Track {
                title: path.file_stem().unwrap().to_str().unwrap().into(),
                available: true,
                ..Track::default()
            })
        }
    }
    fn state_identity(f: &Fixture) -> (u64, i64, i64) {
        use std::os::unix::fs::MetadataExt;
        let m = fs::metadata(f.0.join("state/state.json")).unwrap();
        (m.ino(), m.mtime(), m.mtime_nsec())
    }
    /// Review of #710: every periodic rescan advanced the revision and
    /// rewrote the whole snapshot, resetting the shell's paging.
    #[test]
    fn an_unchanged_rescan_keeps_the_revision_and_writes_nothing() {
        let f = Fixture::new();
        let mut library = Library::open(&f.0.join("state")).unwrap();
        library.configure_roots(vec![f.root()]).unwrap();
        library.scan(&Synthetic).unwrap();
        let revision = library.revision();
        let written = state_identity(&f);
        std::thread::sleep(std::time::Duration::from_millis(20));
        library.scan(&Synthetic).unwrap();
        assert_eq!(library.revision(), revision);
        assert_eq!(state_identity(&f), written);
        fs::write(f.0.join("audio/c.flac"), b"fixture").unwrap();
        library.scan(&Synthetic).unwrap();
        assert!(library.revision() > revision);
        assert_ne!(state_identity(&f), written);
    }
    /// Review of #710: a scan whose snapshot did not fit was thrown away, so
    /// a large library stayed empty. It is now kept in memory and reported.
    #[test]
    fn a_scan_that_cannot_be_written_is_still_published_in_memory() {
        let f = Fixture::new();
        let mut library = Library::open_bounded(&f.0.join("state"), 512).unwrap();
        library.configure_roots(vec![f.root()]).unwrap();
        assert_eq!(library.scan(&Synthetic).unwrap_err(), Error::TooLarge);
        assert_eq!(library.tracks().len(), 2);
        assert!(library.tracks().iter().all(|t| t.available));
        assert!(library.open_track(&library.tracks()[0].id).is_ok());
    }
    /// Review of #710: records of removed roots were kept forever and
    /// counted toward the budget. Tracks a playlist refers to stay.
    #[test]
    fn removing_a_root_forgets_its_tracks_except_playlist_references() {
        let f = Fixture::new();
        let mut library = Library::open(&f.0.join("state")).unwrap();
        library.configure_roots(vec![f.root()]).unwrap();
        library.scan(&Synthetic).unwrap();
        let kept = library.tracks()[0].id.clone();
        library.create_playlist("Kept", vec![kept.clone()]).unwrap();
        library.configure_roots(vec![]).unwrap();
        let tracks = library.tracks();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, kept);
        assert!(!tracks[0].available);
    }
    /// The unreleased version-1 format keeps its roots; the next scan
    /// rebuilds the catalogue with the same track IDs.
    #[test]
    fn a_version_one_state_keeps_its_roots_and_rescans_to_the_same_ids() {
        let f = Fixture::new();
        let state = f.0.join("state");
        let ids = {
            let mut library = Library::open(&state).unwrap();
            library.configure_roots(vec![f.root()]).unwrap();
            library.scan(&Synthetic).unwrap();
            let mut ids: Vec<String> = library.tracks().into_iter().map(|t| t.id).collect();
            ids.sort();
            ids
        };
        let legacy = serde_json::json!({
            "version": 1, "revision": 4,
            "catalogue": {"roots": {}, "records": {"x": {"root_path": "/old"}}, "revision": 3},
            "roots": [{"id": "root", "path": f.root().path}],
            "playlists": {}, "next_playlist": 1
        });
        Storage::open(&state)
            .unwrap()
            .write(&serde_json::to_vec(&legacy).unwrap())
            .unwrap();
        let mut library = Library::open(&state).unwrap();
        assert_eq!(library.roots().len(), 1);
        assert!(library.tracks().is_empty());
        library.scan(&Synthetic).unwrap();
        let mut rescanned: Vec<String> = library.tracks().into_iter().map(|t| t.id).collect();
        rescanned.sort();
        assert_eq!(rescanned, ids);
    }
}
