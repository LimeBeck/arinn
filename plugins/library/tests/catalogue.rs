// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Catalogue scans on temporary folders: names, NFO, availability, rename,
//! resume and import. No network mounts.

use arinn_plugin_library::{Settings, Store, to_file_uri};
use arinn_protocol::{library::ImportCandidate, production::EntityRef, validation::ErrorCode};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "arinn-library-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn settings(root: &Path, probe: bool, ffprobe: PathBuf) -> Settings {
    Settings {
        autoscan: false,
        roots: vec![root.to_path_buf()],
        probe,
        ffprobe,
    }
}

fn open(scratch: &Scratch) -> Store {
    Store::open(&scratch.path().join("library.sqlite3")).unwrap()
}

fn file_uri(path: &Path) -> String {
    to_file_uri(path).unwrap()
}

fn kinds(store: &Store) -> Vec<String> {
    store
        .snapshot()
        .unwrap()
        .0
        .into_iter()
        .map(|collection| collection.kind)
        .collect()
}

#[test]
fn scans_films_series_artwork_and_hostile_nfo() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir_all(root.join("Show Name/Season 01")).unwrap();
    fs::write(root.join("Example (1999).mkv"), b"film").unwrap();
    fs::write(root.join("Example (1999)-poster.jpg"), b"poster").unwrap();
    fs::write(root.join("fanart.jpg"), b"fanart").unwrap();
    fs::write(root.join("Movie.1080p.mkv"), b"film").unwrap();
    fs::write(
        root.join("Show Name/Season 01/Show Name - S01E02 - Pilot.mkv"),
        b"episode",
    )
    .unwrap();
    fs::write(
        root.join("Show Name/tvshow.nfo"),
        b"<tvshow><title>Example Show</title><year>2010</year></tvshow>",
    )
    .unwrap();
    fs::write(
        root.join("Show Name/Season 01/Show Name - S01E02 - Pilot.nfo"),
        b"<episodedetails><title>The Pilot</title><season>1</season><episode>2</episode></episodedetails>",
    )
    .unwrap();
    let secret = scratch.path().join("secret.txt");
    fs::write(&secret, b"SECRET_FROM_ENTITY").unwrap();
    fs::write(root.join("Plain (2001).mkv"), b"film").unwrap();
    fs::write(
        root.join("Plain (2001).nfo"),
        format!(
            "<!DOCTYPE movie [<!ENTITY xxe SYSTEM \"file://{}\">]><movie><title>&xxe;</title><year>1999</year></movie>",
            secret.display()
        ),
    )
    .unwrap();
    let outside = scratch.path().join("outside.mkv");
    fs::write(&outside, b"outside").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("linked.mkv")).unwrap();

    let mut store = open(&scratch);
    store
        .scan(&settings(&root, false, PathBuf::from("ffprobe")))
        .unwrap();
    let (collections, items) = store.snapshot().unwrap();
    assert_eq!(
        collections
            .iter()
            .map(|collection| collection.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["film", "series", "other"]
    );
    assert!(
        collections
            .iter()
            .all(|collection| !collection.id.is_empty())
    );

    let film = items.iter().find(|item| item.title == "Example").unwrap();
    assert_eq!(film.kind, "film");
    assert!(film.has_year && film.year == 1999);
    assert!(film.available);
    assert!(!film.poster.is_empty());
    assert!(!film.fanart.is_empty());
    assert_eq!(film.duration_us, -1);

    let resolution = items
        .iter()
        .find(|item| item.title == "Movie 1080p")
        .unwrap();
    assert!(!resolution.has_year);

    let episode = items.iter().find(|item| item.title == "The Pilot").unwrap();
    assert_eq!(episode.kind, "episode");
    assert_eq!(episode.season, 1);
    assert_eq!(episode.episode, 2);
    let series = items
        .iter()
        .find(|item| item.id == episode.parent_id)
        .unwrap();
    assert_eq!(series.kind, "series");
    assert_eq!(series.title, "Example Show");
    assert!(series.has_year && series.year == 2010);
    assert!(series.parent_id.is_empty());

    let plain = items.iter().find(|item| item.title == "Plain").unwrap();
    assert_eq!(plain.year, 2001);
    assert!(
        items
            .iter()
            .all(|item| !item.title.contains("SECRET_FROM_ENTITY"))
    );
    assert!(items.iter().all(|item| item.title != "linked"));
    assert_eq!(fs::read(&secret).unwrap(), b"SECRET_FROM_ENTITY");
}

#[test]
fn in_root_symlink_is_one_catalogue_item() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    let video = root.join("Feature (2008).mkv");
    fs::write(&video, b"film").unwrap();
    std::os::unix::fs::symlink(&video, root.join("alias.mkv")).unwrap();
    let mut store = open(&scratch);
    store
        .scan(&settings(&root, false, PathBuf::from("ffprobe")))
        .unwrap();
    let films: Vec<_> = store
        .snapshot()
        .unwrap()
        .1
        .into_iter()
        .filter(|item| item.kind == "film")
        .collect();
    assert_eq!(films.len(), 1);
    assert!(films[0].available);
}

#[test]
fn missing_file_and_unavailable_root_keep_the_uuid() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Kept (2004).mkv"), b"a").unwrap();
    fs::write(root.join("Gone (2005).mkv"), b"b").unwrap();
    let mut store = open(&scratch);
    let configured = settings(&root, false, PathBuf::from("ffprobe"));
    store.scan(&configured).unwrap();
    let (_, items) = store.snapshot().unwrap();
    let kept = items
        .iter()
        .find(|item| item.title == "Kept")
        .unwrap()
        .id
        .clone();
    let gone = items
        .iter()
        .find(|item| item.title == "Gone")
        .unwrap()
        .id
        .clone();
    store
        .set_progress(&gone, 5_000_000, 60_000_000, false)
        .unwrap();

    fs::remove_file(root.join("Gone (2005).mkv")).unwrap();
    store.scan(&configured).unwrap();
    let (_, items) = store.snapshot().unwrap();
    let missing = items.iter().find(|item| item.id == gone).unwrap();
    assert!(!missing.available);
    assert!(items.iter().find(|item| item.id == kept).unwrap().available);
    let resume = store
        .resumes()
        .unwrap()
        .into_iter()
        .find(|resume| resume.content_id == gone)
        .unwrap();
    assert_eq!(resume.position_us, 5_000_000);

    fs::write(root.join("Gone (2005).mkv"), b"b").unwrap();
    store.scan(&configured).unwrap();
    assert!(
        store
            .snapshot()
            .unwrap()
            .1
            .iter()
            .find(|item| item.id == gone)
            .unwrap()
            .available
    );

    let away = scratch.path().join("away");
    fs::rename(&root, &away).unwrap();
    store.scan(&configured).unwrap();
    let (_, items) = store.snapshot().unwrap();
    assert!(items.iter().all(|item| !item.available));
    assert!(items.iter().any(|item| item.id == kept));
    assert_eq!(
        store
            .resumes()
            .unwrap()
            .iter()
            .find(|resume| resume.content_id == gone)
            .unwrap()
            .position_us,
        5_000_000
    );
    fs::rename(&away, &root).unwrap();
    store.scan(&configured).unwrap();
    assert!(
        store
            .snapshot()
            .unwrap()
            .1
            .iter()
            .find(|item| item.id == kept)
            .unwrap()
            .available
    );
}

#[test]
fn rename_inside_a_root_keeps_the_uuid() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Before (1991).mkv"), b"same").unwrap();
    let mut store = open(&scratch);
    let configured = settings(&root, false, PathBuf::from("ffprobe"));
    store.scan(&configured).unwrap();
    let id = store
        .snapshot()
        .unwrap()
        .1
        .into_iter()
        .find(|item| item.kind == "film")
        .unwrap()
        .id;
    fs::rename(
        root.join("Before (1991).mkv"),
        root.join("After (1991).mkv"),
    )
    .unwrap();
    store.scan(&configured).unwrap();
    let items = store.snapshot().unwrap().1;
    let renamed = items.iter().find(|item| item.id == id).unwrap();
    assert_eq!(renamed.title, "After");
    assert!(renamed.available);
    assert_eq!(items.iter().filter(|item| item.kind == "film").count(), 1);
}

#[test]
fn resume_rules_and_launch_position() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Feature (2020).mkv"), b"film").unwrap();
    let mut store = open(&scratch);
    store
        .scan(&settings(&root, false, PathBuf::from("ffprobe")))
        .unwrap();
    let id = store
        .snapshot()
        .unwrap()
        .1
        .into_iter()
        .find(|item| item.kind == "film")
        .unwrap()
        .id;
    let duration = 2 * 60 * 60 * 1_000_000;
    store
        .set_progress(&id, duration - 1_000_000, duration, false)
        .unwrap();
    assert!(!store.resumes().unwrap()[0].completed);
    store
        .set_progress(&id, duration - 100_000_000, duration, false)
        .unwrap();
    store
        .set_progress(&id, duration - 86_000_000, duration, false)
        .unwrap();
    assert!(store.resumes().unwrap()[0].completed);
    let source = store.launch(&id).unwrap();
    assert_eq!(source.local_id, id);
    assert!(source.uri.starts_with("file:///"));
    assert!(!source.uri.contains(' '));
    assert_eq!(source.start_position_us, 0);

    store.restart(&id).unwrap();
    let resume = &store.resumes().unwrap()[0];
    assert!(!resume.completed);
    assert_eq!(resume.position_us, 0);
    store.set_progress(&id, 8_000_000, duration, false).unwrap();
    assert_eq!(store.launch(&id).unwrap().start_position_us, 8_000_000);

    store.set_progress(&id, 1_000, 60_000_000, false).unwrap();
    assert!(!store.resumes().unwrap()[0].completed);
    store
        .set_progress(&id, 60_000_000, 60_000_000, true)
        .unwrap();
    assert!(store.resumes().unwrap()[0].completed);
    assert_eq!(store.launch(&id).unwrap().start_position_us, 0);
}

#[test]
fn import_is_idempotent_per_completion_id() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    let video = root.join("Imported (2012).mkv");
    fs::write(&video, b"imported").unwrap();
    let mut store = open(&scratch);
    store
        .scan(&settings(&root, false, PathBuf::from("ffprobe")))
        .unwrap();
    let candidate = ImportCandidate {
        source: EntityRef {
            provider_id: "downloads".into(),
            capability: "download".into(),
            local_id: "job-1".into(),
        },
        file_uri: file_uri(&video),
        completion_id: "done-1".into(),
    };
    let first = store.import(std::slice::from_ref(&candidate)).unwrap();
    let second = store.import(std::slice::from_ref(&candidate)).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        store
            .snapshot()
            .unwrap()
            .1
            .iter()
            .filter(|item| item.kind == "film")
            .count(),
        1
    );
    let mut again = candidate.clone();
    again.completion_id = "done-2".into();
    let third = store.import(std::slice::from_ref(&again)).unwrap();
    assert_eq!(third, first);
    assert_eq!(
        store
            .snapshot()
            .unwrap()
            .1
            .iter()
            .filter(|item| item.kind == "film")
            .count(),
        1
    );
    let outside = scratch.path().join("outside.mkv");
    fs::write(&outside, b"nope").unwrap();
    let mut rejected = candidate.clone();
    rejected.completion_id = "done-3".into();
    rejected.file_uri = file_uri(&outside);
    assert_eq!(
        store.import(std::slice::from_ref(&rejected)),
        Err(ErrorCode::InvalidArgument)
    );
}

#[test]
fn import_rejects_a_root_removed_from_settings() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    let video = root.join("Kept (2011).mkv");
    fs::write(&video, b"film").unwrap();
    let mut store = open(&scratch);
    let configured = settings(&root, false, PathBuf::from("ffprobe"));
    store.scan(&configured).unwrap();
    let mut cleared = configured;
    cleared.roots.clear();
    store.scan(&cleared).unwrap();
    let candidate = ImportCandidate {
        source: EntityRef {
            provider_id: "downloads".into(),
            capability: "download".into(),
            local_id: "job-removed".into(),
        },
        file_uri: file_uri(&video),
        completion_id: "done-removed".into(),
    };
    assert_eq!(
        store.import(std::slice::from_ref(&candidate)),
        Err(ErrorCode::InvalidArgument)
    );
}

#[test]
fn ffprobe_supplies_duration_and_languages() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Timed (2018).mkv"), b"film").unwrap();
    let probe = scratch.path().join("ffprobe");
    fs::write(
        &probe,
        "#!/bin/sh\nprintf '%s\\n' '{\"format\":{\"duration\":\"12.5\"},\"streams\":[{\"codec_type\":\"audio\",\"tags\":{\"language\":\"en\"}},{\"codec_type\":\"subtitle\",\"tags\":{\"language\":\"de\"}}]}'\n",
    )
    .unwrap();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o755)).unwrap();
    let mut store = open(&scratch);
    store.scan(&settings(&root, true, probe)).unwrap();
    let film = store
        .snapshot()
        .unwrap()
        .1
        .into_iter()
        .find(|item| item.kind == "film")
        .unwrap();
    assert_eq!(film.duration_us, 12_500_000);
    let source = store.launch(&film.id).unwrap();
    assert_eq!(source.audio_language, "en");
    assert_eq!(source.subtitle_language, "de");
    assert_eq!(source.start_position_us, 0);
}

#[test]
fn collections_exist_before_a_scan() {
    let scratch = Scratch::new();
    let store = open(&scratch);
    assert_eq!(
        kinds(&store),
        vec!["film".to_string(), "series".into(), "other".into()]
    );
    assert!(store.snapshot().unwrap().1.is_empty());
}
