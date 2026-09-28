// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Deterministic XDG discovery. No discovered executable is run by this module.
use arinn_protocol::validation::{self, ErrorCode, MAX_BODY_BYTES};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs::{self, File},
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

const MAX_MANIFESTS_PER_LEVEL: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(skip_serializing)]
    pub executable: PathBuf,
    #[serde(default, skip_serializing)]
    pub args: Vec<String>,
    pub capabilities: Vec<String>,
}

impl Manifest {
    fn validate(&mut self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        validation::text(&self.name)?;
        validation::id(&self.version)?;
        if self.name.is_empty() || self.capabilities.is_empty() {
            return Err(ErrorCode::InvalidArgument);
        }
        if self.api_version != 1 {
            return Err(ErrorCode::Unsupported);
        }
        let mut seen = BTreeSet::new();
        for capability in &self.capabilities {
            validation::capability(capability)?;
            if !seen.insert(capability) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        self.capabilities.sort();
        let Some(executable) = self.executable.to_str() else {
            return Err(ErrorCode::InvalidArgument);
        };
        validation::text(executable)?;
        if !self.executable.is_absolute() || self.args.len() > 256 {
            return Err(ErrorCode::InvalidArgument);
        }
        for arg in &self.args {
            validation::text(arg)?;
        }
        let metadata = fs::metadata(&self.executable).map_err(|_| ErrorCode::Unavailable)?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
            return Err(ErrorCode::PermissionDenied);
        }
        Ok(())
    }

    pub fn requires_restart(&self, replacement: &Self) -> bool {
        self.executable != replacement.executable
            || self.args != replacement.args
            || self.api_version != replacement.api_version
            || self.capabilities != replacement.capabilities
    }
}

/// Paths are exposed only through diagnostics; never copy them into regular logs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub source: PathBuf,
    pub error: ErrorCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Record {
    pub sources: Vec<PathBuf>,
    pub manifest: Option<Manifest>,
    pub error: Option<ErrorCode>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Registry {
    pub providers: BTreeMap<String, Record>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Ordered roots, highest priority first. Relative XDG values are ignored.
/// Accept values explicitly so tests never read or mutate the user's environment.
pub fn discovery_roots(
    home: &Path,
    data_home: Option<&OsStr>,
    data_dirs: Option<&OsStr>,
) -> Vec<PathBuf> {
    let user = data_home
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    let defaults = OsStr::new("/usr/local/share:/usr/share");
    let dirs = data_dirs.filter(|s| !s.is_empty()).unwrap_or(defaults);
    let mut roots = vec![user.join("arinn/plugins")];
    for root in std::env::split_paths(dirs).filter(|p| p.is_absolute()) {
        let root = root.join("arinn/plugins");
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

impl Registry {
    pub fn discover(roots: &[PathBuf]) -> Self {
        let mut registry = Self::default();
        for root in roots {
            let mut paths = Vec::new();
            let entries = match fs::read_dir(root) {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => {
                    registry.diagnostics.push(Diagnostic {
                        source: root.clone(),
                        error: ErrorCode::Unavailable,
                    });
                    break;
                }
            };
            let mut unreadable = false;
            for entry in entries {
                match entry {
                    Ok(entry) if entry.path().extension() == Some(OsStr::new("toml")) => {
                        paths.push(entry.path())
                    }
                    Ok(_) => (),
                    Err(_) => {
                        unreadable = true;
                        break;
                    }
                }
                if paths.len() > MAX_MANIFESTS_PER_LEVEL {
                    unreadable = true;
                    break;
                }
            }
            if unreadable {
                registry.diagnostics.push(Diagnostic {
                    source: root.clone(),
                    error: ErrorCode::Unavailable,
                });
                break;
            }
            paths.sort();
            let mut level = BTreeMap::<String, Record>::new();
            let mut unidentified = false;
            for path in paths {
                let (id, result) = read_manifest(&path);
                let Some(id) = id else {
                    registry.diagnostics.push(Diagnostic {
                        source: path,
                        error: result.err().unwrap_or(ErrorCode::InvalidArgument),
                    });
                    unidentified = true;
                    continue;
                };
                let record = match result {
                    Ok(manifest) => Record {
                        sources: vec![path.clone()],
                        manifest: Some(manifest),
                        error: None,
                    },
                    Err(error) => Record {
                        sources: vec![path.clone()],
                        manifest: None,
                        error: Some(error),
                    },
                };
                if let Some(duplicate) = level.get_mut(&id) {
                    duplicate.sources.push(path);
                    duplicate.manifest = None;
                    duplicate.error = Some(ErrorCode::Conflict);
                } else {
                    level.insert(id, record);
                }
            }
            for (id, record) in level {
                registry.providers.entry(id).or_insert(record);
            }
            // With an unreadable ID we cannot prove a system manifest is not the
            // overridden provider. Fail closed instead of silently launching it.
            if unidentified {
                break;
            }
        }
        registry
    }

    /// A declarative reload plan. A runtime must stop before starting replacements;
    /// metadata-only changes never restart a provider. Invalid entries stay visible.
    pub fn changes(&self, next: &Self) -> Vec<Change> {
        let ids: BTreeSet<_> = self.providers.keys().chain(next.providers.keys()).collect();
        ids.into_iter()
            .filter_map(|id| {
                let old = self.providers.get(id).and_then(|r| r.manifest.as_ref());
                let new = next.providers.get(id).and_then(|r| r.manifest.as_ref());
                let kind = match (old, new) {
                    (None, Some(_)) => ChangeKind::Start,
                    (Some(_), None) => ChangeKind::Stop,
                    (Some(old), Some(new)) if old.requires_restart(new) => ChangeKind::Restart,
                    (Some(old), Some(new)) if old != new => ChangeKind::Metadata,
                    _ => return None,
                };
                Some(Change {
                    provider_id: id.clone(),
                    kind,
                })
            })
            .collect()
    }
}

fn read_manifest(path: &Path) -> (Option<String>, Result<Manifest, ErrorCode>) {
    let read = || -> Result<String, ErrorCode> {
        // O_NONBLOCK prevents a FIFO manifest from hanging discovery. Metadata is
        // checked on the opened descriptor to avoid a check/open replacement race.
        use std::os::unix::fs::OpenOptionsExt;
        let file = File::options()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .map_err(|_| ErrorCode::Unavailable)?;
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
    };
    let body = match read() {
        Ok(body) => body,
        Err(error) => return (None, Err(error)),
    };
    let value = match toml::from_str::<toml::Value>(&body) {
        Ok(value) => value,
        Err(_) => return (None, Err(ErrorCode::InvalidArgument)),
    };
    let id = value
        .get("id")
        .and_then(toml::Value::as_str)
        .filter(|id| validation::id(id).is_ok())
        .map(str::to_owned);
    let manifest = value
        .try_into::<Manifest>()
        .map_err(|_| ErrorCode::InvalidArgument)
        .and_then(|mut manifest| {
            manifest.validate()?;
            Ok(manifest)
        });
    (id, manifest)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ChangeKind {
    Start,
    Stop,
    Restart,
    Metadata,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub provider_id: String,
    pub kind: ChangeKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("arinn-registry-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(path.join("user")).unwrap();
            fs::create_dir_all(path.join("system")).unwrap();
            Self(path)
        }
        fn roots(&self) -> Vec<PathBuf> {
            vec![self.0.join("user"), self.0.join("system")]
        }
        fn put(&self, path: &str, id: &str, extra: &str) {
            fs::write(self.0.join(path), format!("id = {id:?}\nname = 'Test'\nversion = '0.1.0'\napi_version = 1\nexecutable = '/bin/true'\ncapabilities = ['application']\n{extra}")).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn xdg_precedence_defaults_and_relative_paths() {
        let home = Path::new("/home/test");
        assert_eq!(
            discovery_roots(home, None, None),
            vec![
                PathBuf::from("/home/test/.local/share/arinn/plugins"),
                PathBuf::from("/usr/local/share/arinn/plugins"),
                PathBuf::from("/usr/share/arinn/plugins")
            ]
        );
        assert_eq!(
            discovery_roots(
                home,
                Some(OsStr::new("relative")),
                Some(OsStr::new("/system::relative:/system:/other"))
            ),
            vec![
                PathBuf::from("/home/test/.local/share/arinn/plugins"),
                PathBuf::from("/system/arinn/plugins"),
                PathBuf::from("/other/arinn/plugins")
            ]
        );
    }

    #[test]
    fn ac11_discovery_override_removal_and_reload_plan() {
        let f = Fixture::new();
        f.put("system/a.toml", "opaque/id", "");
        let system = Registry::discover(&f.roots());
        assert_eq!(
            Registry::default().changes(&system)[0].kind,
            ChangeKind::Start
        );
        f.put(
            "user/a.toml",
            "opaque/id",
            "args = ['a b', '; no shell', '$(no execution)']",
        );
        let user = Registry::discover(&f.roots());
        assert_eq!(system.changes(&user)[0].kind, ChangeKind::Restart);
        assert_eq!(
            user.providers["opaque/id"].sources,
            vec![f.0.join("user/a.toml")]
        );
        fs::remove_file(f.0.join("user/a.toml")).unwrap();
        assert_eq!(Registry::discover(&f.roots()), system);
        fs::remove_file(f.0.join("system/a.toml")).unwrap();
        assert_eq!(
            system.changes(&Registry::discover(&f.roots()))[0].kind,
            ChangeKind::Stop
        );
    }

    #[test]
    fn invalid_user_manifest_disables_valid_system_provider() {
        let f = Fixture::new();
        f.put("system/a.toml", "test", "");
        fs::write(f.0.join("user/a.toml"), "id = 'test'\nname = 42\n").unwrap();
        let registry = Registry::discover(&f.roots());
        assert_eq!(
            registry.providers["test"].error,
            Some(ErrorCode::InvalidArgument)
        );
        assert!(registry.providers["test"].manifest.is_none());
        fs::write(f.0.join("user/a.toml"), "not valid TOML\n").unwrap();
        let registry = Registry::discover(&f.roots());
        assert!(registry.providers.is_empty());
        assert_eq!(registry.diagnostics.len(), 1);
    }

    #[test]
    fn same_level_duplicates_conflict_without_fallback() {
        let f = Fixture::new();
        for path in ["user/a.toml", "user/b.toml", "system/a.toml"] {
            f.put(path, "test", "");
        }
        let registry = Registry::discover(&f.roots());
        assert_eq!(registry.providers["test"].error, Some(ErrorCode::Conflict));
        assert_eq!(registry.providers["test"].sources.len(), 2);
    }

    #[test]
    fn metadata_updates_do_not_restart_but_args_do() {
        let f = Fixture::new();
        f.put("user/a.toml", "test", "");
        let old = Registry::discover(&f.roots());
        let mut next = old.clone();
        next.providers
            .get_mut("test")
            .unwrap()
            .manifest
            .as_mut()
            .unwrap()
            .name = "Renamed".into();
        assert_eq!(old.changes(&next)[0].kind, ChangeKind::Metadata);
        next.providers
            .get_mut("test")
            .unwrap()
            .manifest
            .as_mut()
            .unwrap()
            .args
            .push("argument".into());
        assert_eq!(old.changes(&next)[0].kind, ChangeKind::Restart);
    }

    #[test]
    fn g13_validation_and_bounded_reads() {
        let f = Fixture::new();
        f.put("user/a.toml", "test", "");
        let valid = fs::read_to_string(f.0.join("user/a.toml")).unwrap();
        for (body, expected) in [
            (
                valid.replace("api_version = 1", "api_version = 2"),
                ErrorCode::Unsupported,
            ),
            (
                valid.replace("['application']", "['unknown']"),
                ErrorCode::Unsupported,
            ),
            (
                valid.replace("['application']", "['application', 'application']"),
                ErrorCode::InvalidArgument,
            ),
            (
                valid.replace("/bin/true", "relative"),
                ErrorCode::InvalidArgument,
            ),
            (valid.replace("/bin/true", "/"), ErrorCode::PermissionDenied),
        ] {
            fs::write(f.0.join("user/a.toml"), body).unwrap();
            assert_eq!(
                Registry::discover(&f.roots()).providers["test"].error,
                Some(expected)
            );
        }
        fs::write(f.0.join("user/a.toml"), " ".repeat(MAX_BODY_BYTES + 1)).unwrap();
        assert_eq!(
            Registry::discover(&f.roots()).diagnostics[0].error,
            ErrorCode::InvalidArgument
        );
    }

    #[test]
    fn discovery_is_not_recursive_and_missing_directories_are_normal() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("user/nested")).unwrap();
        f.put("user/nested/a.toml", "hidden", "");
        let registry = Registry::discover(&[f.0.join("missing"), f.0.join("user")]);
        assert_eq!(registry, Registry::default());
    }
}
