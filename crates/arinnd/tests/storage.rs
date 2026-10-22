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
                "DROP TABLE web_shortcuts; DROP TABLE web_shortcut_state; DROP TABLE browse_recent; DROP TABLE download_imports; DROP TABLE web_preset_migrations; DROP TABLE favorite_state; PRAGMA user_version=2;
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

/// ADR-0112: a file written before the shell preferences existed loads with
/// their defaults, and only the offered ambient timeouts are valid.
#[test]
fn shell_preferences_default_and_validate() {
    let dir = std::env::temp_dir().join(format!("arinn-prefs-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    std::fs::write(dir.join("config.toml"), "schema_version = 1\n").unwrap();
    let loaded = arinnd::storage::Config::load(&dir).unwrap();
    assert_eq!((loaded.light_field, loaded.ambient_timeout), (true, 10));
    let mut config = loaded;
    for minutes in arinnd::storage::AMBIENT_TIMEOUTS {
        config.ambient_timeout = minutes;
        config.validate().unwrap();
    }
    for minutes in [0, 7, 61] {
        config.ambient_timeout = minutes;
        assert!(config.validate().is_err(), "{minutes}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

/// Owner decision on #488: an invalid hand-edited preference reads as its
/// default with a warning; the rest of the file still loads. Other keys keep
/// their strict rules.
#[test]
fn invalid_hand_edited_preferences_fall_back_to_defaults() {
    let dir = std::env::temp_dir().join(format!("arinn-prefs-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    for (text, light, minutes, invalid) in [
        ("ambient_timeout = 7\n", true, 10, vec!["ambient_timeout"]),
        (
            "ambient_timeout = \"30\"\n",
            true,
            10,
            vec!["ambient_timeout"],
        ),
        ("ambient_timeout = -5\n", true, 10, vec!["ambient_timeout"]),
        ("light_field = \"off\"\n", true, 10, vec!["light_field"]),
        (
            "light_field = 0\nambient_timeout = 9999999999\n",
            true,
            10,
            vec!["light_field", "ambient_timeout"],
        ),
        (
            "light_field = false\nambient_timeout = 30\n",
            false,
            30,
            vec![],
        ),
    ] {
        std::fs::write(
            dir.join("config.toml"),
            format!("schema_version = 1\nsearch_engine = \"kept\"\n{text}"),
        )
        .unwrap();
        let (loaded, seen) = arinnd::storage::Config::load_reporting(&dir).unwrap();
        assert_eq!(
            (loaded.light_field, loaded.ambient_timeout),
            (light, minutes),
            "{text}"
        );
        // The other keys are read as written.
        assert_eq!(loaded.search_engine, "kept");
        // Field order in the file decides the order they are seen.
        let mut seen = seen;
        seen.sort_unstable();
        let mut invalid = invalid;
        invalid.sort_unstable();
        assert_eq!(seen, invalid, "{text}");
    }
    // Repeated loads keep no state: each reports only its own file.
    std::fs::write(
        dir.join("config.toml"),
        "schema_version = 1\nambient_timeout = 7\n",
    )
    .unwrap();
    for _ in 0..3 {
        let (_, seen) = arinnd::storage::Config::load_reporting(&dir).unwrap();
        assert_eq!(seen, vec!["ambient_timeout"]);
    }
    // Other keys stay strict: a wrong type is still an invalid file.
    std::fs::write(
        dir.join("config.toml"),
        "schema_version = 1\nanimations = \"no\"\n",
    )
    .unwrap();
    assert!(arinnd::storage::Config::load(&dir).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn provider_settings_round_trip_and_old_configs_stay_valid() {
    let dir = std::env::temp_dir().join(format!("arinn-settings-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    // An old file without settings still loads.
    let mut config = arinnd::storage::Config::default();
    config.providers.insert(
        "library".into(),
        toml::from_str("plugin_id = \"library\"").unwrap(),
    );
    config.save(&dir).unwrap();
    let loaded = arinnd::storage::Config::load(&dir).unwrap();
    assert!(loaded.providers["library"].settings.is_empty());
    // Settings survive a save and load, and serialize as compact JSON.
    let instance: arinnd::storage::InstanceConfig = toml::from_str(
        "plugin_id = \"library\"\n[settings]\nroots = [\"/srv/films\", \"/mnt/nas/series\"]\nscan = true",
    )
    .unwrap();
    config.providers.insert("library".into(), instance);
    config.save(&dir).unwrap();
    let loaded = arinnd::storage::Config::load(&dir).unwrap();
    assert_eq!(loaded, config);
    assert_eq!(
        arinnd::storage::settings_json(&loaded.providers["library"].settings).unwrap(),
        r#"{"roots":["/srv/films","/mnt/nas/series"],"scan":true}"#
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn provider_settings_without_a_json_form_or_too_large_are_rejected() {
    let dated: toml::Table = toml::from_str("since = 2026-09-30T00:00:00Z").unwrap();
    assert!(arinnd::storage::settings_json(&dated).is_err());
    let mut large = toml::Table::new();
    large.insert(
        "blob".into(),
        toml::Value::String("x".repeat(arinnd::storage::MAX_PROVIDER_SETTINGS_BYTES)),
    );
    assert!(arinnd::storage::settings_json(&large).is_err());
    assert_eq!(
        arinnd::storage::settings_json(&toml::Table::new()).unwrap(),
        "{}"
    );
    // A config with invalid settings does not validate.
    let mut config = arinnd::storage::Config::default();
    config.providers.insert(
        "library".into(),
        arinnd::storage::InstanceConfig {
            plugin_id: "library".into(),
            enabled: true,
            settings: dated,
        },
    );
    assert!(config.validate().is_err());
}

#[test]
fn browse_recent_migrates_from_schema_three_and_keeps_five() {
    let fixture = Fixture::new();
    let state = fixture.paths().state;
    drop(Store::open(&state).unwrap());
    {
        // Recreate a schema-3 database.
        let connection = rusqlite::Connection::open(state.join("state.sqlite3")).unwrap();
        connection
            .execute_batch(
                "DROP TABLE browse_recent; DROP TABLE download_imports; DROP TABLE web_preset_migrations; DROP TABLE favorite_state; PRAGMA user_version=3;",
            )
            .unwrap();
    }
    let mut store = Store::open(&state).unwrap();
    assert!(store.browse_recent().unwrap().is_empty());
    for text in ["a", "b", "a", "c", "d", "e", "f"] {
        store.push_browse_recent(text).unwrap();
    }
    assert_eq!(store.browse_recent().unwrap(), ["f", "e", "d", "c", "a"]);
    drop(store);
    let mut store = Store::open(&state).unwrap();
    assert_eq!(store.browse_recent().unwrap().len(), 5);
    store.clear_browse_recent().unwrap();
    assert!(store.browse_recent().unwrap().is_empty());
}

#[test]
fn download_checkpoints_migrate_from_four_and_prune_only_the_selected_provider() {
    use arinnd::storage::DownloadImport;
    let fixture = Fixture::new();
    let state = fixture.paths().state;
    {
        let mut store = Store::open(&state).unwrap();
        store.push_browse_recent("synthetic search").unwrap();
    }
    {
        let connection = rusqlite::Connection::open(state.join("state.sqlite3")).unwrap();
        connection
            .execute_batch("DROP TABLE download_imports; DROP TABLE web_preset_migrations; DROP TABLE favorite_state; PRAGMA user_version=4;")
            .unwrap();
    }
    let mut store = Store::open(&state).unwrap();
    assert_eq!(store.browse_recent().unwrap(), ["synthetic search"]);
    let record = DownloadImport {
        fingerprint: "digest".into(),
        library: "library".into(),
        state: "pending".into(),
        refs: vec![],
    };
    store.save_download_import("one", "keep", &record).unwrap();
    store.save_download_import("one", "gone", &record).unwrap();
    store.save_download_import("two", "gone", &record).unwrap();
    store
        .prune_download_imports("one", &["keep".into()])
        .unwrap();
    drop(store);
    let store = Store::open(&state).unwrap();
    assert_eq!(
        store.download_imports("one").unwrap(),
        [("keep".into(), record.clone())].into()
    );
    assert_eq!(
        store.download_imports("two").unwrap(),
        [("gone".into(), record)].into()
    );
}

fn favorite(id: &str) -> arinn_protocol::production::EntityRef {
    arinn_protocol::production::EntityRef {
        provider_id: "fixture.desktop".into(),
        capability: "application".into(),
        local_id: id.into(),
    }
}

#[test]
fn favorites_cas_order_empty_and_revision_survive_reopen() {
    let fixture = Fixture::new();
    let state = fixture.paths().state;
    let mut first = Store::open(&state).unwrap();
    let mut second = Store::open(&state).unwrap();
    assert_eq!(first.favorite_list().unwrap().revision, 1);
    assert!(first.favorite_list().unwrap().entries.is_empty());
    let entries = vec![favorite("one"), favorite("two")];
    assert_eq!(first.replace_favorites(1, &entries), Ok((2, true)));
    assert_eq!(second.replace_favorites(1, &[]), Err(ErrorCode::Conflict));
    assert_eq!(second.favorite_list().unwrap().entries, entries);
    assert_eq!(second.replace_favorites(2, &entries), Ok((2, false)));
    let reverse = vec![favorite("two"), favorite("one")];
    assert_eq!(second.replace_favorites(2, &reverse), Ok((3, true)));
    drop(first);
    drop(second);
    let mut reopened = Store::open(&state).unwrap();
    assert_eq!(reopened.favorite_list().unwrap().entries, reverse);
    assert_eq!(reopened.favorite_list().unwrap().revision, 3);
    assert_eq!(reopened.replace_favorites(3, &[]), Ok((4, true)));
    assert!(
        Store::open(&state)
            .unwrap()
            .favorite_list()
            .unwrap()
            .entries
            .is_empty()
    );
    assert_eq!(
        Store::open(&state)
            .unwrap()
            .favorite_list()
            .unwrap()
            .revision,
        4
    );
}

#[test]
fn favorites_missing_shortcut_is_retained_but_not_newly_admitted() {
    use arinn_protocol::{
        favorites::{SHORTCUT_CAPABILITY, SHORTCUT_PROVIDER},
        production::EntityRef,
        web::WebShortcut,
    };
    let fixture = Fixture::new();
    let mut store = Store::open(&fixture.paths().state).unwrap();
    let shortcut = WebShortcut {
        id: "fixture-site".into(),
        title: "Example".into(),
        url: "https://example.org/".into(),
        icon: String::new(),
    };
    let entry = EntityRef {
        provider_id: SHORTCUT_PROVIDER.into(),
        capability: SHORTCUT_CAPABILITY.into(),
        local_id: shortcut.id.clone(),
    };
    assert_eq!(
        store.replace_favorites(1, std::slice::from_ref(&entry)),
        Err(ErrorCode::NotFound)
    );
    let web_revision = store.add_web_shortcut(&shortcut).unwrap();
    assert_eq!(
        store.replace_favorites(1, &[entry.clone(), favorite("one")]),
        Ok((2, true))
    );
    store
        .remove_web_shortcut(web_revision, &shortcut.id)
        .unwrap();
    assert_eq!(store.favorite_list().unwrap().entries[0], entry);
    assert_eq!(
        store.replace_favorites(2, &[favorite("one"), entry.clone()]),
        Ok((3, true))
    );
    assert_eq!(
        store.replace_favorites(3, &[favorite("one")]),
        Ok((4, true))
    );
    assert_eq!(
        store.replace_favorites(4, &[entry]),
        Err(ErrorCode::NotFound)
    );
    assert_eq!(store.favorite_list().unwrap().revision, 4);
}

#[test]
fn favorites_invalid_writes_and_overflow_leave_selection_intact() {
    let fixture = Fixture::new();
    let state = fixture.paths().state;
    let mut store = Store::open(&state).unwrap();
    assert_eq!(
        store.replace_favorites(1, &[favorite("one"), favorite("one")]),
        Err(ErrorCode::InvalidArgument)
    );
    let mut transient = favorite("tab");
    transient.capability = "browser".into();
    assert_eq!(
        store.replace_favorites(1, &[transient]),
        Err(ErrorCode::InvalidArgument)
    );
    assert!(store.favorite_list().unwrap().entries.is_empty());
    let db = rusqlite::Connection::open(state.join("state.sqlite3")).unwrap();
    db.execute(
        "UPDATE favorite_state SET revision=?1",
        [(u64::MAX - 1).to_string()],
    )
    .unwrap();
    assert_eq!(
        store.replace_favorites(u64::MAX - 1, &[favorite("one")]),
        Ok((u64::MAX, true))
    );
    assert_eq!(
        store.replace_favorites(u64::MAX, &[favorite("two")]),
        Err(ErrorCode::Conflict)
    );
    let list = store.favorite_list().unwrap();
    assert_eq!(list.revision, u64::MAX);
    assert_eq!(list.entries, vec![favorite("one")]);
    assert_eq!(
        store.replace_favorites(u64::MAX, &[favorite("one")]),
        Ok((u64::MAX, false))
    );
}

#[test]
fn favorites_schema_six_migration_preserves_existing_order() {
    let fixture = Fixture::new();
    let state = fixture.paths().state;
    let mut store = Store::open(&state).unwrap();
    store
        .set_favorites(&[target("two"), target("one")])
        .unwrap();
    drop(store);
    let db = rusqlite::Connection::open(state.join("state.sqlite3")).unwrap();
    db.execute_batch("DROP TABLE favorite_state; PRAGMA user_version=6;")
        .unwrap();
    drop(db);
    let store = Store::open(&state).unwrap();
    assert_eq!(
        store.favorites().unwrap(),
        vec![target("two"), target("one")]
    );
    assert_eq!(store.favorite_list().unwrap().revision, 1);
}
