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
            version: 1,
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

impl Library {
    /// Requires a private directory; a second writer fails without changing it.
    /// Corrupt/unsupported snapshots are errors, never silently reset.
    pub fn open(directory: &Path) -> Result<Self, Error> {
        let storage = Storage::open(directory)?;
        let mut state: State = match storage.read()? {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| Error::InvalidState)?,
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

    pub fn configure_roots(&mut self, roots: Vec<Root>) -> Result<(), Error> {
        let mut next = self.state.clone();
        next.roots = roots;
        next.catalogue.invalidate();
        self.commit(next)
    }
    pub fn scan(&mut self, probe: &impl Probe) -> Result<Vec<RootStatus>, Error> {
        let mut next = self.state.clone();
        let status = next.catalogue.scan(&next.roots, probe);
        self.commit(next)?;
        Ok(status)
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
    if s.version != 1
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
