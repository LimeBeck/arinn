// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.WebShortcuts1`: Arinn-owned web shortcuts (ADR-0043, ADR-0045).
//! Shortcut URLs are user configuration and are never logged.
use crate::{operations::Operations, storage::Store};
use arinn_protocol::{
    production::{self, PATH},
    validation::ErrorCode,
    web::{WebShortcut, WebShortcutList},
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, OwnedMutexGuard};
use zbus::{Connection, message::Header, object_server::SignalEmitter, zvariant::OwnedObjectPath};

const DEFAULTS_FILE: &str = "arinn/web-shortcuts.toml";
const MAX_DEFAULTS_BYTES: u64 = 64 * 1024;
const REQUEST_RETENTION: Duration = Duration::from_secs(300);
const MAX_REQUESTS: usize = 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DefaultsFile {
    #[serde(default)]
    shortcut: Vec<DefaultShortcut>,
    #[serde(default)]
    migration: Vec<PresetMigration>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DefaultShortcut {
    title: String,
    url: String,
    #[serde(default)]
    icon: String,
}

/// Versioned packaging data; core code has no product-specific matching.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresetMigration {
    pub id: String,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub icon: String,
    pub replacement_url: String,
}
impl PresetMigration {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        arinn_protocol::validation::id(&self.id)?;
        WebShortcut {
            id: self.id.clone(),
            title: self.title.clone(),
            url: self.url.clone(),
            icon: self.icon.clone(),
        }
        .validate()?;
        arinn_protocol::web::url(&self.replacement_url)
    }
}
fn load_file(roots: &[PathBuf]) -> Option<DefaultsFile> {
    let path = roots
        .iter()
        .map(|root| root.join(DEFAULTS_FILE))
        .find(|path| path.is_file())?;
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_DEFAULTS_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    if text.len() as u64 > MAX_DEFAULTS_BYTES {
        return None;
    }
    toml::from_str(&text).ok()
}
pub fn load_migrations(roots: &[PathBuf]) -> Vec<PresetMigration> {
    load_file(roots).map(|f| f.migration).unwrap_or_default()
}
/// The packaged default set: the first `arinn/web-shortcuts.toml` in the XDG
/// data home, then the data directories. `None` when there is none, so the
/// seed still happens once a package later provides it. Entries get fresh IDs.
pub fn load_defaults(roots: &[PathBuf]) -> Option<Vec<WebShortcut>> {
    let file = load_file(roots)?;
    Some(
        file.shortcut
            .into_iter()
            .map(|s| WebShortcut {
                id: uuid::Uuid::new_v4().to_string(),
                title: s.title,
                url: s.url,
                icon: s.icon,
            })
            .collect(),
    )
}
/// Packaged browser applications to launch when a URL must open and no
/// browser is connected (ADR-0055): `arinn/browser-applications.toml` in the
/// first data root that has it. Product IDs live in this packaging data,
/// never in core code (R-01).
pub const BROWSER_APPLICATIONS_FILE: &str = "arinn/browser-applications.toml";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserApplicationsFile {
    applications: Vec<String>,
}
pub fn browser_candidates(roots: &[PathBuf]) -> Vec<String> {
    let Some(path) = roots
        .iter()
        .map(|root| root.join(BROWSER_APPLICATIONS_FILE))
        .find(|path| path.is_file())
    else {
        return Vec::new();
    };
    let mut text = String::new();
    let read = std::fs::File::open(&path)
        .and_then(|f| f.take(MAX_DEFAULTS_BYTES + 1).read_to_string(&mut text));
    if read.is_err() || text.len() as u64 > MAX_DEFAULTS_BYTES {
        return Vec::new();
    }
    toml::from_str::<BrowserApplicationsFile>(&text)
        .map(|f| {
            f.applications
                .into_iter()
                .filter(|id| arinn_protocol::validation::id(id).is_ok())
                .take(16)
                .collect()
        })
        .unwrap_or_default()
}
/// Absolute XDG data roots in precedence order.
pub fn data_roots() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home.map(|h| h.join(".local/share")));
    let dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    data_home
        .into_iter()
        .chain(std::env::split_paths(&dirs))
        .filter(|p| p.is_absolute())
        .collect()
}

/// `Add` deduplication: (caller, request ID) -> (fingerprint, shortcut ID, admitted).
type Requests = HashMap<(String, String), (String, String, Instant)>;

#[derive(Clone)]
pub struct WebShortcuts {
    state: PathBuf,
    // Serializes read-modify-write sequences; SQLite also locks the database.
    lock: Arc<Mutex<()>>,
    requests: Arc<Mutex<Requests>>,
    operations: Operations,
    /// Opens URLs in the default browser provider; without it `Open` fails
    /// with `Unavailable` (fixtures and tests).
    browser: Option<crate::runtime::Core>,
}
impl WebShortcuts {
    pub fn new(state: &Path, operations: Operations) -> Self {
        Self {
            state: state.into(),
            lock: Arc::new(Mutex::new(())),
            requests: Arc::default(),
            operations,
            browser: None,
        }
    }
    /// Route `Open` through the daemon's browser providers (ADR-0048).
    pub fn with_browser(mut self, core: crate::runtime::Core) -> Self {
        self.browser = Some(core);
        self
    }
    /// Seeds the packaged defaults once.
    pub async fn seed(&self, defaults: Option<Vec<WebShortcut>>) -> Result<(), ErrorCode> {
        self.startup().await.seed(defaults).await
    }
    /// Holds the store lock until the returned guard is dropped. The daemon
    /// takes it before it exports this interface and owns its bus name, so a
    /// call that arrives early waits for the preset migrations and the seed
    /// instead of reading the store before them (#463).
    pub async fn startup(&self) -> Startup {
        Startup {
            state: self.state.clone(),
            _guard: Arc::clone(&self.lock).lock_owned().await,
        }
    }
    /// Snapshot lookup for the optional icon route. No caller-supplied URL.
    pub(crate) async fn icon_shortcut(
        &self,
        revision: u64,
        id: &str,
    ) -> Result<WebShortcut, ErrorCode> {
        let id = id.to_owned();
        self.store(move |store| {
            let list = store.web_shortcuts()?;
            if list.revision != revision {
                return Err(ErrorCode::Conflict);
            }
            list.shortcuts
                .into_iter()
                .find(|s| s.id == id)
                .ok_or(ErrorCode::NotFound)
        })
        .await
    }
    async fn store<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Store) -> Result<T, ErrorCode> + Send + 'static,
    ) -> Result<T, ErrorCode> {
        let _guard = self.lock.lock().await;
        run_store(&self.state, work).await
    }
    async fn changed(&self, connection: &Connection, revision: u64) {
        if let Ok(emitter) = SignalEmitter::new(connection, PATH) {
            let _ = Self::changed_signal(&emitter, revision).await;
        }
    }
}
/// Exclusive store access while the daemon starts; see [`WebShortcuts::startup`].
pub struct Startup {
    state: PathBuf,
    _guard: OwnedMutexGuard<()>,
}
impl Startup {
    pub async fn migrate_presets(&self, migrations: Vec<PresetMigration>) -> Result<(), ErrorCode> {
        run_store(&self.state, move |store| {
            store.migrate_web_presets(&migrations)
        })
        .await
    }
    /// Seeds the packaged defaults once.
    pub async fn seed(&self, defaults: Option<Vec<WebShortcut>>) -> Result<(), ErrorCode> {
        let Some(defaults) = defaults else {
            return Ok(());
        };
        run_store(&self.state, move |store| {
            store.seed_web_shortcuts(&defaults)
        })
        .await
        .map(|_| ())
    }
}
/// Callers hold the store lock.
async fn run_store<T: Send + 'static>(
    state: &Path,
    work: impl FnOnce(&mut Store) -> Result<T, ErrorCode> + Send + 'static,
) -> Result<T, ErrorCode> {
    let state = state.to_path_buf();
    tokio::task::spawn_blocking(move || work(&mut Store::open(&state)?))
        .await
        .map_err(|_| ErrorCode::Internal)?
}
fn caller(header: &Header<'_>) -> Result<String, ErrorCode> {
    header
        .sender()
        .map(ToString::to_string)
        .ok_or(ErrorCode::PermissionDenied)
}

#[zbus::interface(name = "org.arinn.WebShortcuts1")]
impl WebShortcuts {
    async fn list(&self) -> production::Result<(WebShortcutList,)> {
        Ok((self.store(|store| store.web_shortcuts()).await?,))
    }
    async fn add(
        &self,
        title: String,
        url: String,
        icon: String,
        request_id: String,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> production::Result<String> {
        uuid::Uuid::parse_str(&request_id).map_err(|_| ErrorCode::InvalidArgument)?;
        let key = (caller(&header)?, request_id);
        let fingerprint = format!("{title}\0{url}\0{icon}");
        let mut requests = self.requests.lock().await;
        requests.retain(|_, (_, _, at)| at.elapsed() < REQUEST_RETENTION);
        if let Some((known, id, _)) = requests.get(&key) {
            return if *known == fingerprint {
                Ok(id.clone())
            } else {
                Err(ErrorCode::Conflict.into())
            };
        }
        if requests.len() >= MAX_REQUESTS {
            return Err(ErrorCode::Unavailable.into());
        }
        let shortcut = WebShortcut {
            id: uuid::Uuid::new_v4().to_string(),
            title,
            url,
            icon,
        };
        let id = shortcut.id.clone();
        let revision = self
            .store(move |store| store.add_web_shortcut(&shortcut))
            .await?;
        requests.insert(key, (fingerprint, id.clone(), Instant::now()));
        drop(requests);
        self.changed(connection, revision).await;
        Ok(id)
    }
    async fn update(
        &self,
        id: String,
        revision: u64,
        title: String,
        url: String,
        icon: String,
        #[zbus(connection)] connection: &Connection,
    ) -> production::Result<()> {
        let shortcut = WebShortcut {
            id,
            title,
            url,
            icon,
        };
        let next = self
            .store(move |store| store.update_web_shortcut(revision, &shortcut))
            .await?;
        self.changed(connection, next).await;
        Ok(())
    }
    async fn remove(
        &self,
        id: String,
        revision: u64,
        #[zbus(connection)] connection: &Connection,
    ) -> production::Result<()> {
        let next = self
            .store(move |store| store.remove_web_shortcut(revision, &id))
            .await?;
        self.changed(connection, next).await;
        Ok(())
    }
    async fn open(
        &self,
        id: String,
        request_id: String,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> production::Result<OwnedObjectPath> {
        let list = self.store(|store| store.web_shortcuts()).await?;
        let url = list
            .shortcuts
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.url.clone())
            .ok_or(ErrorCode::NotFound)?;
        let (path, operation, fresh) = self
            .operations
            .admit(
                connection,
                &caller(&header)?,
                &request_id,
                &format!("open-web-shortcut\0{id}"),
                "",
                crate::operations::Label::new("shortcut.open"),
            )
            .await?;
        if fresh {
            let browser = self.browser.clone();
            let connection = connection.clone();
            let object = path.clone();
            tokio::spawn(async move {
                {
                    let mut value = operation.value.lock().await;
                    if value.state == "queued" {
                        value.state = "running".into();
                        value.started_at = crate::operations::now();
                    }
                }
                let result = match browser {
                    Some(core) => core.open_url(&url, &operation).await,
                    None => Err(ErrorCode::Unavailable),
                };
                operation
                    .finish(&connection, &object, result.map(|_| Vec::new()))
                    .await;
            });
        }
        Ok(path)
    }
    #[zbus(signal, name = "Changed")]
    async fn changed_signal(emitter: &SignalEmitter<'_>, revision: u64) -> zbus::Result<()>;
}

#[cfg(test)]
mod browser_candidate_tests {
    use super::*;

    #[test]
    fn packaged_browser_candidates_come_from_the_first_root_that_has_them() {
        let base = std::env::temp_dir().join(format!("arinn-browsers-{}", uuid::Uuid::new_v4()));
        let (first, second) = (base.join("home"), base.join("system"));
        for root in [&first, &second] {
            std::fs::create_dir_all(root.join("arinn")).unwrap();
        }
        assert!(browser_candidates(&[first.clone(), second.clone()]).is_empty());
        std::fs::write(
            second.join(BROWSER_APPLICATIONS_FILE),
            "applications = [\"a.desktop\", \"\", \"b.desktop\"]",
        )
        .unwrap();
        // Invalid (here: empty) IDs are dropped, order is kept.
        assert_eq!(
            browser_candidates(&[first.clone(), second.clone()]),
            ["a.desktop", "b.desktop"]
        );
        std::fs::write(
            first.join(BROWSER_APPLICATIONS_FILE),
            "applications = [\"c.desktop\"]",
        )
        .unwrap();
        assert_eq!(
            browser_candidates(&[first.clone(), second.clone()]),
            ["c.desktop"]
        );
        std::fs::write(first.join(BROWSER_APPLICATIONS_FILE), "unknown = 1").unwrap();
        assert!(browser_candidates(&[first, second]).is_empty());
        std::fs::remove_dir_all(base).unwrap();
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;

    // #463: the daemon exports this interface and owns its name before it
    // seeds, so an early call must wait for the seed instead of missing it.
    #[tokio::test]
    async fn calls_wait_for_the_startup_seed() {
        let state = std::env::temp_dir().join(format!("arinn-startup-{}", uuid::Uuid::new_v4()));
        let shortcuts = WebShortcuts::new(&state, Operations::default());
        let startup = shortcuts.startup().await;
        let early = tokio::spawn({
            let shortcuts = shortcuts.clone();
            async move { shortcuts.list().await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!early.is_finished());
        let defaults = vec![WebShortcut {
            id: "seeded".into(),
            title: "Example".into(),
            url: "https://example.invalid/".into(),
            icon: String::new(),
        }];
        startup.migrate_presets(Vec::new()).await.unwrap();
        startup.seed(Some(defaults)).await.unwrap();
        drop(startup);
        let (list,) = early.await.unwrap().unwrap();
        assert_eq!(list.revision, 2);
        assert_eq!(list.shortcuts[0].id, "seeded");
        std::fs::remove_dir_all(state).unwrap();
    }
}
