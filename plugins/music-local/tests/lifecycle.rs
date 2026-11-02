// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_music_common::Track;
use arinn_plugin_music_local::{
    catalogue::Root,
    library::{Error, Library},
    probe::{self, Probe},
};
use std::{
    cell::Cell,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("arinn-music-life-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct SyntheticProbe(Cell<u32>);
impl Probe for SyntheticProbe {
    fn inspect(&self, _: &Path) -> Result<Track, probe::Error> {
        self.0.set(self.0.get() + 1);
        Ok(Track {
            title: "Synthetic song".into(),
            available: true,
            ..Track::default()
        })
    }
}

#[test]
fn restart_rescans_before_playback_and_preserves_playlists_through_missing_roots() {
    let f = Fixture::new();
    let root = f.0.join("audio");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("track.wav"), b"fixture").unwrap();
    let state = f.0.join("state");
    let probe = SyntheticProbe(Cell::new(0));
    let mut library = Library::open(&state).unwrap();
    library
        .configure_roots(vec![Root {
            id: "root-1".into(),
            path: root.clone(),
        }])
        .unwrap();
    library.scan(&probe).unwrap();
    let id = library.tracks()[0].id.clone();
    let playlist = library
        .create_playlist("Synthetic playlist", vec![id.clone(), id.clone()])
        .unwrap();
    let playlist_revision = library.revision();
    assert!(library.open_track(&id).is_ok());
    assert_eq!(probe.0.get(), 1);
    drop(library);
    let mut library = Library::open(&state).unwrap();
    assert!(!library.tracks()[0].available);
    assert!(library.open_track(&id).is_err());
    assert_eq!(library.playlists()[0].tracks, [id.clone(), id.clone()]);
    library.scan(&probe).unwrap();
    assert_eq!(probe.0.get(), 1); // unchanged metadata restored from the cache
    assert!(library.open_track(&id).is_ok());
    fs::rename(&root, f.0.join("unmounted")).unwrap();
    assert!(!library.scan(&probe).unwrap()[0].available);
    assert!(!library.tracks()[0].available);
    assert_eq!(library.playlists()[0].id, playlist);
    drop(library);
    let mut library = Library::open(&state).unwrap();
    assert_eq!(library.search("synthetic")[0].id, id);
    fs::rename(f.0.join("unmounted"), &root).unwrap();
    library.scan(&probe).unwrap();
    assert!(library.open_track(&id).is_ok());
    library
        .replace_playlist(&playlist, "Renamed", vec![id])
        .unwrap();
    assert!(library.revision() > playlist_revision);
    let replacement_revision = library.revision();
    assert_eq!(library.playlists()[0].title, "Renamed");
    library.remove_playlist(&playlist).unwrap();
    assert!(library.revision() > replacement_revision);
    drop(library);
    assert!(Library::open(&state).unwrap().playlists().is_empty());
    assert_eq!(
        fs::metadata(&state).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(state.join("state.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn unsafe_or_corrupt_state_is_rejected_without_reset_or_link_following() {
    let f = Fixture::new();
    let state = f.0.join("state");
    let library = Library::open(&state).unwrap();
    assert!(matches!(Library::open(&state), Err(Error::Busy)));
    drop(library);
    let file = state.join("state.json");
    fs::write(&file, b"corrupt state").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(Library::open(&state), Err(Error::InvalidState)));
    assert_eq!(fs::read(&file).unwrap(), b"corrupt state");
    fs::remove_file(&file).unwrap();
    let target = f.0.join("outside");
    fs::write(&target, b"preserve").unwrap();
    symlink(&target, &file).unwrap();
    assert!(Library::open(&state).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"preserve");
    fs::remove_file(&file).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(Library::open(&state), Err(Error::UnsafeStorage)));
}

#[test]
fn failed_snapshot_commit_does_not_publish_mutations_or_leave_temp_files() {
    let f = Fixture::new();
    let state = f.0.join("state");
    let mut library = Library::open(&state).unwrap();
    // An owned synthetic obstruction forces rename failure without disk faults.
    fs::create_dir(state.join("state.json")).unwrap();
    assert_eq!(
        library.create_playlist("Synthetic", vec![]),
        Err(Error::Unavailable)
    );
    assert!(library.playlists().is_empty());
    assert!(
        fs::read_dir(&state).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp"))
    );
    fs::remove_dir(state.join("state.json")).unwrap();
    assert_eq!(
        library.create_playlist("Synthetic", vec![]).unwrap(),
        "playlist-1"
    );
    assert_eq!(
        library.create_playlist("Bad reference", vec!["unknown".into()]),
        Err(Error::InvalidState)
    );
    assert_eq!(library.playlists().len(), 1);
}
