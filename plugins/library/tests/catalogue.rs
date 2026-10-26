// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Catalogue scans on temporary folders: names, NFO, availability, rename,
//! resume and import. No network mounts.

use arinn_plugin_library::{
    MountTables, Settings, Store, scan_shared, scan_shared_with, to_file_uri,
};
use arinn_protocol::{library::ImportCandidate, production::EntityRef, validation::ErrorCode};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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

// ADR-0085 (#311): anime `TVn` season folders beside each other form one
// series with a season each.
#[test]
fn tv_season_folders_form_one_series() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir_all(root.join("Show TV1")).unwrap();
    fs::create_dir_all(root.join("Show TV2")).unwrap();
    fs::write(root.join("Show TV1/01.mkv"), b"episode").unwrap();
    fs::write(
        root.join("Show TV1/[Group] Show - 02 [1080p].mkv"),
        b"episode",
    )
    .unwrap();
    fs::write(root.join("Show TV2/Show TV2 [01].mkv"), b"episode").unwrap();

    let mut store = open(&scratch);
    store
        .scan(&settings(&root, false, PathBuf::from("ffprobe")))
        .unwrap();
    let (_, items) = store.snapshot().unwrap();
    let series: Vec<_> = items.iter().filter(|item| item.kind == "series").collect();
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].title, "Show");
    let mut episodes: Vec<(u32, u32)> = items
        .iter()
        .filter(|item| item.kind == "episode")
        .map(|item| {
            assert_eq!(item.parent_id, series[0].id);
            (item.season, item.episode)
        })
        .collect();
    episodes.sort();
    assert_eq!(episodes, vec![(1, 1), (1, 2), (2, 1)]);
    assert!(items.iter().all(|item| item.kind != "film"));
}

// ADR-0109 (#361): release folders `<Title> [...] S01` and `<Title> S02`
// form one series; their files' ` - NN` numbers are the episodes.
#[test]
fn season_only_folders_form_one_series() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    let first = root.join("Show Name [WEB-DL 1080p] S01");
    let second = root.join("Show Name S02");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    for number in 1..=2 {
        fs::write(
            first.join(format!("Short - {number:02} [1080p AVC] [ENG SUB].mkv")),
            b"episode",
        )
        .unwrap();
    }
    fs::write(second.join("Short - 01 [1080p].mkv"), b"episode").unwrap();

    let mut store = open(&scratch);
    store
        .scan(&settings(&root, false, PathBuf::from("ffprobe")))
        .unwrap();
    let (_, items) = store.snapshot().unwrap();
    let series: Vec<_> = items.iter().filter(|item| item.kind == "series").collect();
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].title, "Show Name");
    let mut episodes: Vec<(u32, u32)> = items
        .iter()
        .filter(|item| item.kind == "episode")
        .map(|item| {
            assert_eq!(item.parent_id, series[0].id);
            (item.season, item.episode)
        })
        .collect();
    episodes.sort();
    assert_eq!(episodes, vec![(1, 1), (1, 2), (2, 1)]);
    assert!(items.iter().all(|item| item.kind != "film"));
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
fn an_unmounted_share_is_unavailable_not_empty() {
    // #556 (ADR-0115, AC-15): a share listed in fstab with `nofail` leaves an
    // empty mount point behind. The mount tables are given to the scan, so
    // no real mount is needed.
    let scratch = Scratch::new();
    let root = scratch.path().join("share");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Kept (2004).mkv"), b"a").unwrap();
    fs::write(root.join("Other (2005).mkv"), b"b").unwrap();
    let fstab = format!("//nas/films {} cifs nofail 0 0\n", root.display());
    let line = |point: &Path| format!("1 0 0:1 / {} rw - ext4 /dev/x rw\n", point.display());
    let mounted = MountTables::from_text(
        &fstab,
        Some(&format!("{}{}", line(Path::new("/")), line(&root))),
    );
    let away = MountTables::from_text(&fstab, Some(&line(Path::new("/"))));
    let store = Mutex::new(open(&scratch));
    let configured = settings(&root, false, PathBuf::from("ffprobe"));
    assert_eq!(
        scan_shared_with(&store, &configured, &mounted).unwrap(),
        vec![(root.clone(), true)]
    );
    let before = store.lock().unwrap().snapshot().unwrap().1;
    assert_eq!(before.len(), 2);
    assert!(before.iter().all(|item| item.available));

    // The share goes away: its mount point is an empty folder.
    fs::remove_file(root.join("Kept (2004).mkv")).unwrap();
    fs::remove_file(root.join("Other (2005).mkv")).unwrap();
    assert_eq!(
        scan_shared_with(&store, &configured, &away).unwrap(),
        vec![(root.clone(), false)]
    );
    let after = store.lock().unwrap().snapshot().unwrap().1;
    assert_eq!(after.len(), 2);
    assert!(after.iter().all(|item| !item.available));
    for item in &before {
        assert!(after.iter().any(|kept| kept.id == item.id));
    }

    // The share is back with its files: the same items are available again.
    fs::write(root.join("Kept (2004).mkv"), b"a").unwrap();
    fs::write(root.join("Other (2005).mkv"), b"b").unwrap();
    store
        .lock()
        .unwrap()
        .scan_with(&configured, &mounted)
        .unwrap();
    let back = store.lock().unwrap().snapshot().unwrap().1;
    assert_eq!(back.len(), 2);
    assert!(back.iter().all(|item| item.available));
    for item in &before {
        assert!(back.iter().any(|kept| kept.id == item.id));
    }
}

#[test]
fn a_really_empty_folder_stays_available() {
    // No fstab entry covers it: emptied by the user, it stays available and
    // its items are missing one by one, as before (ADR-0056).
    let scratch = Scratch::new();
    let root = scratch.path().join("plain");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Kept (2004).mkv"), b"a").unwrap();
    let store = Mutex::new(open(&scratch));
    let configured = settings(&root, false, PathBuf::from("ffprobe"));
    let tables = MountTables::from_text(
        "UUID=1 / ext4 defaults 0 1\n//nas/films /mnt/elsewhere cifs nofail 0 0\n",
        Some("1 0 0:1 / / rw - ext4 /dev/x rw\n"),
    );
    scan_shared_with(&store, &configured, &tables).unwrap();
    fs::remove_file(root.join("Kept (2004).mkv")).unwrap();
    assert_eq!(
        scan_shared_with(&store, &configured, &tables).unwrap(),
        vec![(root.clone(), true)]
    );
    let items = store.lock().unwrap().snapshot().unwrap().1;
    assert_eq!(items.len(), 1);
    assert!(!items[0].available);
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

// #373 / ADR-0090: playing a completed item again is a rewatch. Its position
// is kept when the viewer stops in the middle, and the end completes it again.
#[test]
fn rewatching_a_completed_item_resumes_where_it_stopped() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Show S01E01.mkv"), b"episode").unwrap();
    let mut store = open(&scratch);
    store
        .scan(&settings(&root, false, PathBuf::from("ffprobe")))
        .unwrap();
    let id = store
        .snapshot()
        .unwrap()
        .1
        .into_iter()
        .find(|item| item.kind == "episode")
        .unwrap()
        .id;
    let duration = 24 * 60 * 1_000_000;
    store.set_progress(&id, duration, duration, true).unwrap();
    assert!(store.resumes().unwrap()[0].completed);

    let (source, restarted) = store.launch_for_playback(&id).unwrap();
    assert_eq!(source.start_position_us, 0);
    assert!(restarted);
    let resume = &store.resumes().unwrap()[0];
    assert!(!resume.completed);
    assert_eq!(resume.position_us, 0);

    // Watched to the middle, then stopped: Continue resumes there.
    store
        .set_progress(&id, 10_000_000, duration, false)
        .unwrap();
    store
        .set_progress(&id, 16 * 60 * 1_000_000, duration, false)
        .unwrap();
    let (again, restarted) = store.launch_for_playback(&id).unwrap();
    assert!(!restarted);
    assert_eq!(again.start_position_us, 16 * 60 * 1_000_000);

    // Reaching the end completes it again.
    store.set_progress(&id, duration, duration, true).unwrap();
    assert!(store.resumes().unwrap()[0].completed);
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
fn missing_ffprobe_keeps_the_catalogue_and_unknown_duration() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Feature (2016).mkv"), b"film").unwrap();
    let mut store = open(&scratch);
    store
        .scan(&settings(
            &root,
            true,
            scratch.path().join("no-such-ffprobe"),
        ))
        .unwrap();
    let film = store
        .snapshot()
        .unwrap()
        .1
        .into_iter()
        .find(|item| item.kind == "film")
        .unwrap();
    assert!(film.available);
    assert_eq!(film.title, "Feature");
    assert_eq!(film.duration_us, -1);
    let source = store.launch(&film.id).unwrap();
    assert_eq!(source.audio_language, "");
    assert_eq!(source.subtitle_language, "");
    assert_eq!(source.start_position_us, 0);
}

#[test]
fn an_unreadable_subdirectory_does_not_fail_the_scan() {
    if !run_without_root("an_unreadable_subdirectory_does_not_fail_the_scan") {
        return;
    }
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    let locked = root.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(root.join("Visible (2001).mkv"), b"visible").unwrap();
    fs::write(locked.join("Hidden (2002).mkv"), b"hidden").unwrap();
    let mut store = open(&scratch);
    let configured = settings(&root, false, PathBuf::from("ffprobe"));
    store.scan(&configured).unwrap();
    let hidden_id = store
        .snapshot()
        .unwrap()
        .1
        .into_iter()
        .find(|item| item.title == "Hidden")
        .unwrap()
        .id;
    let _restore = ModeGuard(locked.clone());
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    store.scan(&configured).unwrap();
    let items = store.snapshot().unwrap().1;
    let visible = items.iter().find(|item| item.title == "Visible").unwrap();
    assert!(visible.available);
    let hidden = items.iter().find(|item| item.id == hidden_id).unwrap();
    assert!(!hidden.available);
}

#[test]
fn a_fifo_reached_through_a_symlink_is_not_catalogued() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    let fifo = root.join("payload");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    std::os::unix::fs::symlink(&fifo, root.join("Clip (1998).mkv")).unwrap();
    fs::write(root.join("Real (1999).mkv"), b"film").unwrap();
    let mut store = open(&scratch);
    store
        .scan(&settings(&root, false, PathBuf::from("ffprobe")))
        .unwrap();
    let items = store.snapshot().unwrap().1;
    assert!(items.iter().all(|item| item.title != "Clip"));
    assert!(
        items
            .iter()
            .any(|item| item.title == "Real" && item.available)
    );
}

#[test]
fn sidecar_symlinks_outside_the_root_are_ignored() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Feature (2004).mkv"), b"film").unwrap();
    let outside = scratch.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(
        outside.join("leak.nfo"),
        b"<movie><title>LEAKED TITLE</title><year>1991</year></movie>",
    )
    .unwrap();
    fs::write(outside.join("poster.jpg"), b"jpg").unwrap();
    std::os::unix::fs::symlink(outside.join("leak.nfo"), root.join("Feature (2004).nfo")).unwrap();
    std::os::unix::fs::symlink(outside.join("poster.jpg"), root.join("poster.jpg")).unwrap();
    let mut store = open(&scratch);
    store
        .scan(&settings(&root, false, PathBuf::from("ffprobe")))
        .unwrap();
    let film = store
        .snapshot()
        .unwrap()
        .1
        .into_iter()
        .find(|item| item.kind == "film")
        .unwrap();
    assert_eq!(film.title, "Feature");
    assert_eq!(film.year, 2004);
    assert!(film.poster.is_empty());
    assert!(film.fanart.is_empty());
}

#[test]
fn a_duplicate_import_batch_writes_nothing() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    let video = root.join("Batch (2014).mkv");
    let mut store = open(&scratch);
    store
        .scan(&settings(&root, false, PathBuf::from("ffprobe")))
        .unwrap();
    fs::write(&video, b"film").unwrap();
    let candidate = |completion_id: &str| ImportCandidate {
        source: EntityRef {
            provider_id: "downloads".into(),
            capability: "download".into(),
            local_id: "job-batch".into(),
        },
        file_uri: file_uri(&video),
        completion_id: completion_id.into(),
    };
    let first = candidate("done-a");
    let second = candidate("done-b");
    assert_eq!(
        store.import(&[first.clone(), second.clone()]),
        Err(ErrorCode::InvalidArgument)
    );
    assert_eq!(
        store
            .snapshot()
            .unwrap()
            .1
            .iter()
            .filter(|item| item.kind == "film")
            .count(),
        0
    );
    let once = store.import(std::slice::from_ref(&first)).unwrap();
    assert_eq!(once.len(), 1);
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
    assert_eq!(store.import(std::slice::from_ref(&second)).unwrap(), once);
}

#[test]
fn scan_does_not_hold_the_catalogue_during_ffprobe() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Slow (2003).mkv"), b"film").unwrap();
    let marker = scratch.path().join("probe-started");
    let probe = scratch.path().join("ffprobe");
    fs::write(
        &probe,
        format!(
            "#!/bin/sh\ntouch '{}'\nsleep 2\nprintf '%s\\n' '{{\"format\":{{\"duration\":\"1\"}}}}'\n",
            marker.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o755)).unwrap();
    let store = Arc::new(Mutex::new(open(&scratch)));
    let worker = {
        let store = Arc::clone(&store);
        let configured = settings(&root, true, probe);
        std::thread::spawn(move || scan_shared(&store, &configured).unwrap())
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while !marker.exists() {
        if Instant::now() > deadline {
            panic!("ffprobe did not start");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let guard = store
        .try_lock()
        .expect("scan held the catalogue lock during ffprobe");
    assert!(guard.snapshot().is_ok());
    drop(guard);
    worker.join().unwrap();
}

#[test]
fn import_during_ffprobe_stays_available() {
    let scratch = Scratch::new();
    let root = scratch.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Slow (2003).mkv"), b"film").unwrap();
    let mut store = open(&scratch);
    store
        .scan(&settings(&root, false, PathBuf::from("ffprobe")))
        .unwrap();
    let marker = scratch.path().join("probe-started");
    let probe = scratch.path().join("ffprobe");
    fs::write(
        &probe,
        format!(
            "#!/bin/sh\ntouch '{}'\nsleep 2\nprintf '%s\\n' '{{\"format\":{{\"duration\":\"1\"}}}}'\n",
            marker.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o755)).unwrap();
    let store = Arc::new(Mutex::new(store));
    let worker = {
        let store = Arc::clone(&store);
        let configured = settings(&root, true, probe);
        std::thread::spawn(move || scan_shared(&store, &configured).unwrap())
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while !marker.exists() {
        if Instant::now() > deadline {
            panic!("ffprobe did not start");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let imported = root.join("Imported (2011).mkv");
    fs::write(&imported, b"imported").unwrap();
    let candidate = ImportCandidate {
        source: EntityRef {
            provider_id: "downloads".into(),
            capability: "download".into(),
            local_id: "job-during-probe".into(),
        },
        file_uri: file_uri(&imported),
        completion_id: "done-during-probe".into(),
    };
    let imported_id = {
        let mut guard = store
            .lock()
            .expect("catalogue stayed locked during ffprobe");
        guard.import(std::slice::from_ref(&candidate)).unwrap()
    };
    worker.join().unwrap();
    let guard = store.lock().unwrap();
    let item = guard
        .snapshot()
        .unwrap()
        .1
        .into_iter()
        .find(|item| item.id == imported_id[0])
        .expect("imported item missing after the scan");
    assert!(item.available);
    assert_eq!(item.title, "Imported");
}

struct ModeGuard(PathBuf);
impl Drop for ModeGuard {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

fn run_without_root(test: &str) -> bool {
    let uid = std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .unwrap_or_default();
    if uid.trim() != "0" {
        return true;
    }
    if std::env::var_os("ARINN_LIBRARY_UNPRIVILEGED").is_some() {
        unsafe {
            assert_eq!(libc_setuid(), 0);
        }
        return true;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("ARINN_LIBRARY_UNPRIVILEGED", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

unsafe fn libc_setuid() -> i32 {
    unsafe extern "C" {
        fn setgid(gid: u32) -> i32;
        fn setuid(uid: u32) -> i32;
    }
    unsafe {
        if setgid(65534) != 0 {
            return -1;
        }
        setuid(65534)
    }
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
