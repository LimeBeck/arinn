// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_protocol::validation::ErrorCode;
use arinnd::storage::{Config, Paths, Store, Target, provider_digest};
use std::{fs, path::PathBuf};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("arinn-storage-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn paths(&self) -> Paths {
        Paths {
            config: self.0.join("config"),
            state: self.0.join("state"),
            data: self.0.join("data"),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn target(id: &str) -> Target {
    Target {
        provider_id: "provider".into(),
        capability: "application".into(),
        local_id: id.into(),
    }
}

#[test]
fn oq06_fixed_vectors_and_opaque_ids() {
    assert_eq!(
        provider_digest("abc").unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(provider_digest(""), Err(ErrorCode::InvalidArgument));
    assert_ne!(provider_digest("é"), provider_digest("e\u{301}"));
    assert_ne!(provider_digest("ABC"), provider_digest("abc"));
    let hash = provider_digest("../opaque/provider").unwrap();
    assert_eq!(hash.len(), 64);
    assert!(
        hash.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
}
#[test]
fn oq06_registry_reset_preserves_provider_owned_data() {
    let fixture = Fixture::new();
    let paths = fixture.paths();
    let mut store = Store::open(&paths.state).unwrap();
    let old = store.provider_uuid("../opaque").unwrap();
    let dirs = paths.provider_dirs("../opaque").unwrap();
    fs::write(dirs.0.join("library"), b"owned data").unwrap();
    fs::write(dirs.1.join("resume"), b"owned state").unwrap();
    drop(store);
    fs::remove_file(paths.state.join("state.sqlite3")).unwrap();
    let mut store = Store::open(&paths.state).unwrap();
    assert_ne!(store.provider_uuid("../opaque").unwrap(), old);
    assert_eq!(paths.provider_dirs("../opaque").unwrap(), dirs);
    assert_eq!(fs::read(dirs.0.join("library")).unwrap(), b"owned data");
    assert_eq!(fs::read(dirs.1.join("resume")).unwrap(), b"owned state");
    assert_ne!(paths.provider_dirs("second instance").unwrap(), dirs);
}
#[test]
fn sqlite_mapping_counters_and_favorites_survive_reopen() {
    let fixture = Fixture::new();
    let paths = fixture.paths();
    let mut store = Store::open(&paths.state).unwrap();
    let id = store.provider_uuid("provider").unwrap();
    assert_eq!(store.provider_uuid("provider").unwrap(), id);
    assert_eq!(store.record_restart("provider").unwrap(), 1);
    store.set_favorites(&[target("b"), target("a")]).unwrap();
    assert!(store.set_favorites(&[target("a"), target("a")]).is_err());
    drop(store);
    let mut store = Store::open(&paths.state).unwrap();
    assert_eq!(store.provider_uuid("provider").unwrap(), id);
    assert_eq!(store.restart_count("provider").unwrap(), 1);
    assert_eq!(store.favorites().unwrap(), vec![target("b"), target("a")]);
}
#[test]
fn supervisor_restart_observations_survive_reopen_and_counter_reset() {
    let fixture = Fixture::new();
    let paths = fixture.paths();
    let mut store = Store::open(&paths.state).unwrap();
    store.provider_uuid("provider").unwrap();
    assert_eq!(store.observe_restarts("provider", 3).unwrap(), (3, 3));
    drop(store);
    let mut store = Store::open(&paths.state).unwrap();
    assert_eq!(store.observe_restarts("provider", 3).unwrap(), (3, 0));
    assert_eq!(store.observe_restarts("provider", 4).unwrap(), (4, 1));
    assert_eq!(store.observe_restarts("provider", 1).unwrap(), (5, 1));
    store.reset_supervisor_count("provider").unwrap();
    assert_eq!(store.observe_restarts("provider", 2).unwrap(), (7, 2));
    assert_eq!(store.record_restart("provider").unwrap(), 8);
}
#[test]
fn recents_are_bounded_expire_and_deduplicate() {
    let fixture = Fixture::new();
    let paths = fixture.paths();
    let mut store = Store::open(&paths.state).unwrap();
    for i in 0..101 {
        store
            .record_launch(&target(&i.to_string()), 1_000_000 + i)
            .unwrap();
    }
    let recent = store.recents(1_000_101).unwrap();
    assert_eq!(recent.len(), 100);
    assert!(!recent.contains(&target("0")));
    store.record_launch(&target("1"), 1_000_102).unwrap();
    let recent = store.recents(1_000_102).unwrap();
    assert_eq!(recent.len(), 100);
    assert_eq!(recent[0], target("1"));
    assert!(
        store
            .recents(1_000_102 + 30 * 24 * 60 * 60)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn failed_sqlite_migration_preserves_old_database() {
    let fixture = Fixture::new();
    let paths = fixture.paths();
    fs::create_dir(&paths.state).unwrap();
    let db = paths.state.join("state.sqlite3");
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE favorites(valuable TEXT); INSERT INTO favorites VALUES ('preserve');",
        )
        .unwrap();
    drop(connection);
    let before = fs::read(&db).unwrap();
    assert!(matches!(
        Store::open(&paths.state),
        Err(ErrorCode::Internal)
    ));
    assert_eq!(fs::read(&db).unwrap(), before);
    let connection = rusqlite::Connection::open(&db).unwrap();
    let tables: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='providers'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0);
    connection
        .execute_batch("PRAGMA user_version=999;")
        .unwrap();
    drop(connection);
    let before = fs::read(&db).unwrap();
    assert!(matches!(
        Store::open(&paths.state),
        Err(ErrorCode::Unsupported)
    ));
    assert_eq!(fs::read(&db).unwrap(), before);
}
#[test]
fn config_migration_atomic_write_and_invalid_input_preservation() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let paths = fixture.paths();
    let config = Config::load(&paths.config).unwrap();
    assert_eq!(Config::load(&paths.config).unwrap(), config);
    assert_eq!(
        fs::metadata(paths.config.join("config.toml"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let path = paths.config.join("config.toml");
    fs::write(&path, "animations = false\n").unwrap();
    assert!(!Config::load(&paths.config).unwrap().animations);
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("schema_version = 1")
    );
    for invalid in [
        "schema_version = 999\n",
        "schema_version = 'wrong'\n",
        "broken = [\n",
        "secret = 'unsupported'\n",
    ] {
        fs::write(&path, invalid).unwrap();
        assert!(Config::load(&paths.config).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), invalid);
    }
}
#[test]
fn storage_refuses_symlinked_database_and_provider_directory() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let paths = fixture.paths();
    fs::create_dir(&paths.state).unwrap();
    fs::write(fixture.0.join("valuable"), b"untouched").unwrap();
    symlink(
        fixture.0.join("valuable"),
        paths.state.join("state.sqlite3"),
    )
    .unwrap();
    assert!(Store::open(&paths.state).is_err());
    assert_eq!(fs::read(fixture.0.join("valuable")).unwrap(), b"untouched");
    fs::create_dir_all(paths.data.join("providers")).unwrap();
    symlink(
        &fixture.0,
        paths
            .data
            .join("providers")
            .join(provider_digest("provider").unwrap()),
    )
    .unwrap();
    assert!(paths.provider_dirs("provider").is_err());
}

fn shortcut(id: &str, url: &str) -> arinn_protocol::web::WebShortcut {
    arinn_protocol::web::WebShortcut {
        id: id.into(),
        title: format!("Title {id}"),
        url: url.into(),
        icon: String::new(),
    }
}

#[test]
fn web_shortcuts_revisions_conflicts_and_reopen() {
    let fixture = Fixture::new();
    let state = fixture.paths().state;
    let mut store = Store::open(&state).unwrap();
    let empty = store.web_shortcuts().unwrap();
    assert_eq!((empty.revision, empty.shortcuts.len()), (1, 0));
    assert_eq!(
        store.add_web_shortcut(&shortcut("a", "https://a.example/")),
        Ok(2)
    );
    assert_eq!(
        store.add_web_shortcut(&shortcut("b", "http://nas.local:8096/")),
        Ok(3)
    );
    // Invalid input and duplicate IDs change nothing.
    for bad in [
        shortcut("c", "http://public.example/"),
        shortcut("a", "https://again.example/"),
    ] {
        assert_eq!(
            store.add_web_shortcut(&bad),
            Err(ErrorCode::InvalidArgument)
        );
    }
    assert_eq!(
        store.update_web_shortcut(2, &shortcut("a", "https://new.example/")),
        Err(ErrorCode::Conflict)
    );
    assert_eq!(
        store.update_web_shortcut(3, &shortcut("zz", "https://new.example/")),
        Err(ErrorCode::NotFound)
    );
    assert_eq!(
        store.update_web_shortcut(3, &shortcut("a", "https://new.example/")),
        Ok(4)
    );
    assert_eq!(store.remove_web_shortcut(3, "b"), Err(ErrorCode::Conflict));
    assert_eq!(
        store.remove_web_shortcut(4, "missing"),
        Err(ErrorCode::NotFound)
    );
    drop(store);
    // Revision and order survive a restart, so a stale revision stays stale.
    let mut store = Store::open(&state).unwrap();
    let list = store.web_shortcuts().unwrap();
    assert_eq!(list.revision, 4);
    assert_eq!(
        list.shortcuts
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(list.shortcuts[0].url, "https://new.example/");
    assert_eq!(store.remove_web_shortcut(4, "a"), Ok(5));
    assert_eq!(
        store.add_web_shortcut(&shortcut("c", "https://c.example/")),
        Ok(6)
    );
    let ids: Vec<_> = store
        .web_shortcuts()
        .unwrap()
        .shortcuts
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, ["b", "c"]);
}

#[test]
fn web_shortcut_defaults_seed_once_and_capacity_is_enforced() {
    let fixture = Fixture::new();
    let state = fixture.paths().state;
    let mut store = Store::open(&state).unwrap();
    let defaults = vec![
        shortcut("d1", "https://one.example/"),
        shortcut("bad", "ftp://two.example/"),
        shortcut("d3", "https://three.example/"),
    ];
    assert_eq!(store.seed_web_shortcuts(&defaults), Ok(Some(2)));
    let seeded = store.web_shortcuts().unwrap();
    assert_eq!(seeded.shortcuts.len(), 2);
    // A removed default is never re-added, even by a later package.
    let revision = store.remove_web_shortcut(2, "d1").unwrap();
    assert_eq!(store.seed_web_shortcuts(&defaults), Ok(None));
    drop(store);
    let mut store = Store::open(&state).unwrap();
    assert_eq!(store.seed_web_shortcuts(&defaults), Ok(None));
    assert_eq!(store.web_shortcuts().unwrap().revision, revision);
    for i in 1..arinn_protocol::web::MAX_SHORTCUTS {
        store
            .add_web_shortcut(&shortcut(&format!("n{i}"), "https://n.example/"))
            .unwrap();
    }
    assert_eq!(
        store.add_web_shortcut(&shortcut("over", "https://over.example/")),
        Err(ErrorCode::InvalidArgument)
    );
    assert_eq!(
        store.web_shortcuts().unwrap().shortcuts.len(),
        arinn_protocol::web::MAX_SHORTCUTS
    );
}

#[test]
fn web_shortcuts_migrate_from_schema_two_and_reject_corrupted_rows() {
    let fixture = Fixture::new();
    let state = fixture.paths().state;
    drop(Store::open(&state).unwrap());
    let database = state.join("state.sqlite3");
    {
        // Recreate a schema-2 database with existing data.
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection
            .execute_batch(
                "DROP TABLE web_shortcuts; DROP TABLE web_shortcut_state; PRAGMA user_version=2;
                 INSERT INTO providers (provider_id, registry_uuid) VALUES ('kept', 'uuid-1');",
            )
            .unwrap();
    }
    let mut store = Store::open(&state).unwrap();
    assert_eq!(store.web_shortcuts().unwrap().revision, 1);
    assert_eq!(store.restart_count("kept"), Ok(0));
    store
        .add_web_shortcut(&shortcut("a", "https://a.example/"))
        .unwrap();
    drop(store);
    rusqlite::Connection::open(&database)
        .unwrap()
        .execute("UPDATE web_shortcuts SET url='javascript:alert(1)'", [])
        .unwrap();
    assert_eq!(
        Store::open(&state).unwrap().web_shortcuts(),
        Err(ErrorCode::Internal)
    );
}
