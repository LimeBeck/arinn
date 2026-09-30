// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Blocking persistence primitives. Call these from a dedicated blocking task.
use arinn_protocol::{
    validation::{self, ErrorCode, MAX_BODY_BYTES},
    web::{MAX_SHORTCUTS, WebShortcut, WebShortcutList},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const SCHEMA: u32 = 1;
const MAX_RECENTS: i64 = 100;
const RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub state: PathBuf,
    pub data: PathBuf,
}
impl Paths {
    pub fn from_env() -> Result<Self, ErrorCode> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or(ErrorCode::InvalidArgument)?;
        let root = |key: &str, fallback: &str| {
            std::env::var_os(key)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(fallback))
                .join("arinn")
        };
        Ok(Self {
            config: root("XDG_CONFIG_HOME", ".config"),
            state: root("XDG_STATE_HOME", ".local/state"),
            data: root("XDG_DATA_HOME", ".local/share"),
        })
    }
    pub fn provider_dirs(&self, provider_id: &str) -> Result<(PathBuf, PathBuf), ErrorCode> {
        let digest = provider_digest(provider_id)?;
        let data = self.data.join("providers").join(&digest);
        let state = self.state.join("providers").join(&digest);
        for root in [&self.data, &self.state] {
            private_dir(root)?;
            private_dir(&root.join("providers"))?;
        }
        private_dir(&data)?;
        private_dir(&state)?;
        Ok((data, state))
    }
}

pub fn provider_digest(provider_id: &str) -> Result<String, ErrorCode> {
    validation::id(provider_id)?;
    Ok(format!("{:x}", Sha256::digest(provider_id.as_bytes())))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceConfig {
    pub plugin_id: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Provider-defined settings, opaque to the daemon. They reach the
    /// provider as `ARINN_PROVIDER_SETTINGS` (JSON) at its next start
    /// (ADR-0064).
    #[serde(default, skip_serializing_if = "toml::Table::is_empty")]
    pub settings: toml::Table,
}
/// Largest `ARINN_PROVIDER_SETTINGS` value in bytes.
pub const MAX_PROVIDER_SETTINGS_BYTES: usize = 64 * 1024;

/// The settings of one provider as the compact JSON the provider receives.
/// TOML datetimes have no JSON form and are rejected, as are settings over
/// [`MAX_PROVIDER_SETTINGS_BYTES`].
pub fn settings_json(settings: &toml::Table) -> Result<String, ErrorCode> {
    fn plain(value: &toml::Value) -> bool {
        match value {
            toml::Value::Datetime(_) => false,
            toml::Value::Array(items) => items.iter().all(plain),
            toml::Value::Table(table) => table.values().all(plain),
            _ => true,
        }
    }
    if !settings.values().all(plain) {
        return Err(ErrorCode::InvalidArgument);
    }
    let json = serde_json::to_string(settings).map_err(|_| ErrorCode::InvalidArgument)?;
    if json.len() > MAX_PROVIDER_SETTINGS_BYTES || json.contains('\0') {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(json)
}
fn yes() -> bool {
    true
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    #[serde(default)]
    pub providers: BTreeMap<String, InstanceConfig>,
    #[serde(default)]
    pub defaults: BTreeMap<String, String>,
    #[serde(default = "home_shortcut")]
    pub home_shortcut: String,
    #[serde(default)]
    pub home_output: String,
    #[serde(default = "yes")]
    pub animations: bool,
    /// Application ID an ApplicationProvider launches when a URL must open
    /// and no browser is connected (ADR-0055). Empty: the packaged candidate
    /// list decides.
    #[serde(default)]
    pub browser_application: String,
}
fn home_shortcut() -> String {
    "Meta+F12".into()
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA,
            providers: BTreeMap::new(),
            defaults: BTreeMap::new(),
            home_shortcut: home_shortcut(),
            home_output: String::new(),
            animations: true,
            browser_application: String::new(),
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.schema_version != SCHEMA {
            return Err(ErrorCode::Unsupported);
        }
        if self.providers.len() > 1024 || self.defaults.len() > validation::CAPABILITIES.len() {
            return Err(ErrorCode::InvalidArgument);
        }
        validation::text(&self.home_shortcut)?;
        validation::text(&self.home_output)?;
        if !self.browser_application.is_empty() {
            validation::id(&self.browser_application)?;
        }
        for (id, instance) in &self.providers {
            validation::id(id)?;
            validation::id(&instance.plugin_id)?;
            settings_json(&instance.settings)?;
        }
        for (capability, id) in &self.defaults {
            validation::capability(capability)?;
            validation::id(id)?;
        }
        Ok(())
    }
    pub fn load(directory: &Path) -> Result<Self, ErrorCode> {
        private_dir(directory)?;
        let path = directory.join("config.toml");
        let contents = match bounded_read(&path) {
            Ok(contents) => contents,
            Err(ErrorCode::NotFound) => {
                let config = Self::default();
                config.save(directory)?;
                return Ok(config);
            }
            Err(error) => return Err(error),
        };
        let mut document: toml::Value =
            toml::from_str(&contents).map_err(|_| ErrorCode::InvalidArgument)?;
        let table = document.as_table_mut().ok_or(ErrorCode::InvalidArgument)?;
        // Version 0 is the unversioned pre-production format with the same fields.
        let version = match table.get("schema_version") {
            None => 0,
            Some(value) => value.as_integer().ok_or(ErrorCode::InvalidArgument)?,
        };
        if version < 0 || version > i64::from(SCHEMA) {
            return Err(ErrorCode::Unsupported);
        }
        table.insert(
            "schema_version".into(),
            toml::Value::Integer(i64::from(SCHEMA)),
        );
        let config: Self = document
            .try_into()
            .map_err(|_| ErrorCode::InvalidArgument)?;
        config.validate()?;
        if version != i64::from(SCHEMA) {
            config.save(directory)?;
        }
        Ok(config)
    }
    pub fn save(&self, directory: &Path) -> Result<(), ErrorCode> {
        self.validate()?;
        let body = toml::to_string_pretty(self).map_err(|_| ErrorCode::Internal)?;
        if body.len() > MAX_BODY_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        private_dir(directory)?;
        atomic_write(&directory.join("config.toml"), body.as_bytes())
    }
}

pub(crate) fn private_dir(path: &Path) -> Result<(), ErrorCode> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(ErrorCode::PermissionDenied),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(|_| ErrorCode::Unavailable),
        Err(_) => Err(ErrorCode::Unavailable),
    }
}
fn bounded_read(path: &Path) -> Result<String, ErrorCode> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ErrorCode::NotFound
            } else {
                ErrorCode::Unavailable
            }
        })?;
    if !file
        .metadata()
        .map_err(|_| ErrorCode::Unavailable)?
        .is_file()
    {
        return Err(ErrorCode::InvalidArgument);
    }
    let mut bytes = Vec::new();
    file.take((MAX_BODY_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| ErrorCode::Unavailable)?;
    if bytes.len() > MAX_BODY_BYTES {
        return Err(ErrorCode::InvalidArgument);
    }
    String::from_utf8(bytes).map_err(|_| ErrorCode::InvalidArgument)
}
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ErrorCode> {
    let parent = path.parent().ok_or(ErrorCode::InvalidArgument)?;
    let temporary = parent.join(format!(".write-{}", Uuid::new_v4().simple()));
    let work = || -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()
    };
    let result = work().map_err(|_| ErrorCode::Unavailable);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub provider_id: String,
    pub capability: String,
    pub local_id: String,
}
impl Target {
    fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.provider_id)?;
        validation::capability(&self.capability)?;
        validation::id(&self.local_id)
    }
}

pub struct Store {
    connection: Connection,
}
impl Store {
    pub fn open(directory: &Path) -> Result<Self, ErrorCode> {
        private_dir(directory)?;
        let path = directory.join("state.sqlite3");
        // Pre-create with private permissions; never follow a symlink to a DB.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
            .map_err(|_| ErrorCode::Unavailable)?;
        if !file
            .metadata()
            .map_err(|_| ErrorCode::Unavailable)?
            .is_file()
        {
            return Err(ErrorCode::InvalidArgument);
        }
        let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
            | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let mut connection =
            Connection::open_with_flags(path, flags).map_err(|_| ErrorCode::Unavailable)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(2))
            .map_err(|_| ErrorCode::Unavailable)?;
        Self::migrate(&mut connection)?;
        Ok(Self { connection })
    }
    fn migrate(connection: &mut Connection) -> Result<(), ErrorCode> {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ErrorCode::Unavailable)?;
        let version: u32 = transaction
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|_| ErrorCode::Internal)?;
        if version > 3 {
            return Err(ErrorCode::Unsupported);
        }
        if version == 0 {
            transaction.execute_batch("CREATE TABLE providers (provider_id TEXT PRIMARY KEY, registry_uuid TEXT NOT NULL UNIQUE, restart_count INTEGER NOT NULL DEFAULT 0 CHECK (restart_count >= 0));
                CREATE TABLE favorites (position INTEGER PRIMARY KEY, provider_id TEXT NOT NULL, capability TEXT NOT NULL, local_id TEXT NOT NULL, UNIQUE(provider_id, capability, local_id));
                CREATE TABLE recents (provider_id TEXT NOT NULL, capability TEXT NOT NULL, local_id TEXT NOT NULL, launched_at INTEGER NOT NULL, PRIMARY KEY(provider_id, capability, local_id));
                PRAGMA user_version = 1;").map_err(|_| ErrorCode::Internal)?;
        }
        if version <= 1 {
            transaction.execute_batch("ALTER TABLE providers ADD COLUMN supervisor_restarts INTEGER NOT NULL DEFAULT 0; PRAGMA user_version = 2;").map_err(|_|ErrorCode::Internal)?;
        }
        if version <= 2 {
            // Web shortcuts (ADR-0043). The revision is persisted so a client's
            // stale revision is never accepted again after a daemon restart.
            transaction.execute_batch("CREATE TABLE web_shortcuts (position INTEGER NOT NULL UNIQUE, id TEXT PRIMARY KEY, title TEXT NOT NULL, url TEXT NOT NULL, icon TEXT NOT NULL);
                CREATE TABLE web_shortcut_state (singleton INTEGER PRIMARY KEY CHECK (singleton = 0), revision INTEGER NOT NULL CHECK (revision >= 1), defaults_seeded INTEGER NOT NULL);
                INSERT INTO web_shortcut_state VALUES (0, 1, 0);
                PRAGMA user_version = 3;").map_err(|_| ErrorCode::Internal)?;
        }
        transaction.commit().map_err(|_| ErrorCode::Internal)
    }
    pub fn observe_restarts(
        &mut self,
        provider_id: &str,
        observed: u64,
    ) -> Result<(u64, u64), ErrorCode> {
        let observed = i64::try_from(observed).map_err(|_| ErrorCode::InvalidArgument)?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ErrorCode::Unavailable)?;
        let (previous, total): (i64, i64) = transaction
            .query_row(
                "SELECT supervisor_restarts,restart_count FROM providers WHERE provider_id=?1",
                [provider_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| ErrorCode::NotFound)?;
        let delta = if observed >= previous {
            observed - previous
        } else {
            observed
        };
        let total = total.checked_add(delta).ok_or(ErrorCode::Conflict)?;
        transaction
            .execute(
                "UPDATE providers SET supervisor_restarts=?2,restart_count=?3 WHERE provider_id=?1",
                params![provider_id, observed, total],
            )
            .map_err(|_| ErrorCode::Internal)?;
        transaction.commit().map_err(|_| ErrorCode::Internal)?;
        Ok((total as u64, delta as u64))
    }
    pub fn reset_supervisor_count(&mut self, provider_id: &str) -> Result<(), ErrorCode> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ErrorCode::Unavailable)?;
        transaction
            .execute(
                "UPDATE providers SET supervisor_restarts=0 WHERE provider_id=?1",
                [provider_id],
            )
            .map_err(|_| ErrorCode::Internal)?;
        transaction.commit().map_err(|_| ErrorCode::Internal)
    }
    pub fn provider_uuid(&mut self, provider_id: &str) -> Result<String, ErrorCode> {
        validation::id(provider_id)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ErrorCode::Unavailable)?;
        let uuid = Uuid::new_v4().to_string();
        transaction
            .execute(
                "INSERT OR IGNORE INTO providers(provider_id,registry_uuid) VALUES (?1,?2)",
                params![provider_id, uuid],
            )
            .map_err(|_| ErrorCode::Internal)?;
        let value: String = transaction
            .query_row(
                "SELECT registry_uuid FROM providers WHERE provider_id=?1",
                [provider_id],
                |row| row.get(0),
            )
            .map_err(|_| ErrorCode::Internal)?;
        Uuid::parse_str(&value).map_err(|_| ErrorCode::Internal)?;
        transaction.commit().map_err(|_| ErrorCode::Internal)?;
        Ok(value)
    }
    pub fn restart_count(&self, provider_id: &str) -> Result<u64, ErrorCode> {
        validation::id(provider_id)?;
        let count: Option<i64> = self
            .connection
            .query_row(
                "SELECT restart_count FROM providers WHERE provider_id=?1",
                [provider_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| ErrorCode::Internal)?;
        count
            .ok_or(ErrorCode::NotFound)?
            .try_into()
            .map_err(|_| ErrorCode::Internal)
    }
    pub fn record_restart(&mut self, provider_id: &str) -> Result<u64, ErrorCode> {
        validation::id(provider_id)?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ErrorCode::Unavailable)?;
        let updated = transaction.execute("UPDATE providers SET restart_count=restart_count+1 WHERE provider_id=?1 AND restart_count<9223372036854775807", [provider_id]).map_err(|_| ErrorCode::Internal)?;
        if updated != 1 {
            return Err(ErrorCode::Conflict);
        }
        transaction.commit().map_err(|_| ErrorCode::Internal)?;
        self.restart_count(provider_id)
    }
    pub fn favorites(&self) -> Result<Vec<Target>, ErrorCode> {
        self.targets(
            "SELECT provider_id,capability,local_id FROM favorites ORDER BY position",
            [],
        )
    }
    /// Callers admit only provider-declared stable launch targets. The storage
    /// layer never stores titles, payloads or browser history.
    pub fn set_favorites(&mut self, favorites: &[Target]) -> Result<(), ErrorCode> {
        if favorites.len() > 1024 {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut seen = BTreeSet::new();
        for target in favorites {
            target.validate()?;
            if !seen.insert((&target.provider_id, &target.capability, &target.local_id)) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ErrorCode::Unavailable)?;
        transaction
            .execute("DELETE FROM favorites", [])
            .map_err(|_| ErrorCode::Internal)?;
        for (position, target) in favorites.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO favorites VALUES (?1,?2,?3,?4)",
                    params![
                        position as i64,
                        target.provider_id,
                        target.capability,
                        target.local_id
                    ],
                )
                .map_err(|_| ErrorCode::Internal)?;
        }
        transaction.commit().map_err(|_| ErrorCode::Internal)
    }
    pub fn record_launch(&mut self, target: &Target, now: i64) -> Result<(), ErrorCode> {
        target.validate()?;
        if now < 0 {
            return Err(ErrorCode::InvalidArgument);
        }
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ErrorCode::Unavailable)?;
        transaction.execute("INSERT INTO recents VALUES (?1,?2,?3,?4) ON CONFLICT(provider_id,capability,local_id) DO UPDATE SET launched_at=excluded.launched_at", params![target.provider_id, target.capability, target.local_id, now]).map_err(|_| ErrorCode::Internal)?;
        prune(&transaction, now)?;
        transaction.commit().map_err(|_| ErrorCode::Internal)
    }
    pub fn recents(&mut self, now: i64) -> Result<Vec<Target>, ErrorCode> {
        if now < 0 {
            return Err(ErrorCode::InvalidArgument);
        }
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ErrorCode::Unavailable)?;
        prune(&transaction, now)?;
        transaction.commit().map_err(|_| ErrorCode::Internal)?;
        self.targets("SELECT provider_id,capability,local_id FROM recents ORDER BY launched_at DESC,provider_id,capability,local_id", [])
    }
    fn targets<P: rusqlite::Params>(
        &self,
        query: &str,
        params: P,
    ) -> Result<Vec<Target>, ErrorCode> {
        self.connection
            .prepare(query)
            .map_err(|_| ErrorCode::Internal)?
            .query_map(params, |row| {
                Ok(Target {
                    provider_id: row.get(0)?,
                    capability: row.get(1)?,
                    local_id: row.get(2)?,
                })
            })
            .map_err(|_| ErrorCode::Internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ErrorCode::Internal)
    }
}
/// Web shortcuts (ADR-0043, ADR-0045). Every mutation validates, runs in one
/// immediate transaction and increments the persisted list revision.
impl Store {
    pub fn web_shortcuts(&self) -> Result<WebShortcutList, ErrorCode> {
        let revision: i64 = self
            .connection
            .query_row("SELECT revision FROM web_shortcut_state", [], |row| {
                row.get(0)
            })
            .map_err(|_| ErrorCode::Internal)?;
        let shortcuts = self
            .connection
            .prepare("SELECT id,title,url,icon FROM web_shortcuts ORDER BY position")
            .map_err(|_| ErrorCode::Internal)?
            .query_map([], |row| {
                Ok(WebShortcut {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    url: row.get(2)?,
                    icon: row.get(3)?,
                })
            })
            .map_err(|_| ErrorCode::Internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ErrorCode::Internal)?;
        let list = WebShortcutList {
            revision: u64::try_from(revision).map_err(|_| ErrorCode::Internal)?,
            shortcuts,
        };
        // A corrupted or hand-edited database is reported, never published.
        list.validate().map_err(|_| ErrorCode::Internal)?;
        Ok(list)
    }
    /// Appends a validated shortcut and returns the new revision.
    pub fn add_web_shortcut(&mut self, shortcut: &WebShortcut) -> Result<u64, ErrorCode> {
        shortcut.validate()?;
        self.mutate_web_shortcuts(None, |transaction| {
            append(transaction, shortcut)?;
            Ok(true)
        })
    }
    pub fn update_web_shortcut(
        &mut self,
        expected: u64,
        shortcut: &WebShortcut,
    ) -> Result<u64, ErrorCode> {
        shortcut.validate()?;
        self.mutate_web_shortcuts(Some(expected), |transaction| {
            let changed = transaction
                .execute(
                    "UPDATE web_shortcuts SET title=?2,url=?3,icon=?4 WHERE id=?1",
                    params![shortcut.id, shortcut.title, shortcut.url, shortcut.icon],
                )
                .map_err(|_| ErrorCode::Internal)?;
            if changed == 0 {
                return Err(ErrorCode::NotFound);
            }
            Ok(true)
        })
    }
    pub fn remove_web_shortcut(&mut self, expected: u64, id: &str) -> Result<u64, ErrorCode> {
        validation::id(id)?;
        self.mutate_web_shortcuts(Some(expected), |transaction| {
            let changed = transaction
                .execute("DELETE FROM web_shortcuts WHERE id=?1", [id])
                .map_err(|_| ErrorCode::Internal)?;
            if changed == 0 {
                return Err(ErrorCode::NotFound);
            }
            Ok(true)
        })
    }
    /// Seeds the packaged defaults exactly once per state database, so an
    /// upgrade never re-adds a shortcut the user removed. Invalid defaults and
    /// defaults beyond the capacity are skipped. Returns the new revision when
    /// anything was added.
    pub fn seed_web_shortcuts(
        &mut self,
        defaults: &[WebShortcut],
    ) -> Result<Option<u64>, ErrorCode> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ErrorCode::Unavailable)?;
        let (revision, seeded): (i64, bool) = transaction
            .query_row(
                "SELECT revision,defaults_seeded FROM web_shortcut_state",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| ErrorCode::Internal)?;
        if seeded {
            return Ok(None);
        }
        let mut added = false;
        for shortcut in defaults {
            if shortcut.validate().is_ok() && append(&transaction, shortcut).is_ok() {
                added = true;
            }
        }
        let revision = revision + i64::from(added);
        transaction
            .execute(
                "UPDATE web_shortcut_state SET revision=?1, defaults_seeded=1",
                [revision],
            )
            .map_err(|_| ErrorCode::Internal)?;
        transaction.commit().map_err(|_| ErrorCode::Internal)?;
        Ok(added.then_some(revision as u64))
    }
    fn mutate_web_shortcuts(
        &mut self,
        expected: Option<u64>,
        change: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<bool, ErrorCode>,
    ) -> Result<u64, ErrorCode> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| ErrorCode::Unavailable)?;
        let revision: i64 = transaction
            .query_row("SELECT revision FROM web_shortcut_state", [], |row| {
                row.get(0)
            })
            .map_err(|_| ErrorCode::Internal)?;
        let revision = u64::try_from(revision).map_err(|_| ErrorCode::Internal)?;
        if expected.is_some_and(|expected| expected != revision) {
            return Err(ErrorCode::Conflict);
        }
        change(&transaction)?;
        let next = revision.checked_add(1).ok_or(ErrorCode::Internal)?;
        transaction
            .execute(
                "UPDATE web_shortcut_state SET revision=?1",
                [i64::try_from(next).map_err(|_| ErrorCode::Internal)?],
            )
            .map_err(|_| ErrorCode::Internal)?;
        transaction.commit().map_err(|_| ErrorCode::Internal)?;
        Ok(next)
    }
}
/// Appends at the end; `InvalidArgument` when full or the ID is taken.
fn append(
    transaction: &rusqlite::Transaction<'_>,
    shortcut: &WebShortcut,
) -> Result<(), ErrorCode> {
    let (count, next): (i64, i64) = transaction
        .query_row(
            "SELECT COUNT(*), COALESCE(MAX(position) + 1, 0) FROM web_shortcuts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| ErrorCode::Internal)?;
    if count as usize >= MAX_SHORTCUTS {
        return Err(ErrorCode::InvalidArgument);
    }
    let inserted = transaction
        .execute(
            "INSERT OR IGNORE INTO web_shortcuts VALUES (?1,?2,?3,?4,?5)",
            params![
                next,
                shortcut.id,
                shortcut.title,
                shortcut.url,
                shortcut.icon
            ],
        )
        .map_err(|_| ErrorCode::Internal)?;
    if inserted == 0 {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(())
}
fn prune(transaction: &rusqlite::Transaction<'_>, now: i64) -> Result<(), ErrorCode> {
    transaction
        .execute(
            "DELETE FROM recents WHERE launched_at<=?1",
            [now.saturating_sub(RETENTION_SECONDS)],
        )
        .map_err(|_| ErrorCode::Internal)?;
    transaction.execute("DELETE FROM recents WHERE rowid IN (SELECT rowid FROM recents ORDER BY launched_at DESC,provider_id,capability,local_id LIMIT -1 OFFSET ?1)", [MAX_RECENTS]).map_err(|_| ErrorCode::Internal)?;
    Ok(())
}
