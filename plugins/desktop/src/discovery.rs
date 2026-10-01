// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Desktop IDs and visibility only. The session launcher interprets Exec.
use arinn_protocol::{
    application::Application,
    validation::{self, ErrorCode},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const MAX_FILE: u64 = 256 * 1024;
const MAX_ENTRIES: usize = 100_000;
#[derive(Clone)]
pub struct Settings {
    pub roots: Vec<PathBuf>,
    pub locale: String,
    pub desktops: Vec<String>,
    pub executable_paths: Vec<PathBuf>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub application: Application,
    pub path: PathBuf,
    /// Well-known bus name for a D-Bus activatable entry, derived from the
    /// file basename exactly as GLib does. `None` launches through GIO.
    pub dbus_name: Option<String>,
}
impl Settings {
    pub fn from_env() -> Result<Self, ErrorCode> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or(ErrorCode::Unavailable)?;
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".local/share"));
        let mut roots = vec![data.join("applications")];
        let dirs = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        roots.extend(
            dirs.split(':')
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|p| p.join("applications")),
        );
        let mut seen = BTreeSet::new();
        roots.retain(|p| seen.insert(p.clone()));
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .find_map(|key| std::env::var(key).ok().filter(|s| !s.is_empty()))
            .unwrap_or_default();
        Ok(Self {
            roots,
            locale,
            desktops: std::env::var("XDG_CURRENT_DESKTOP")
                .unwrap_or_default()
                .split(':')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect(),
            executable_paths: std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .filter(|p| p.is_absolute())
                .collect(),
        })
    }
    pub fn discover(&self) -> Result<BTreeMap<String, Entry>, ErrorCode> {
        let mut resolved = BTreeSet::new();
        let mut output = BTreeMap::new();
        let mut visited = 0;
        let mut bytes = 0usize;
        for root in &self.roots {
            let mut candidates = BTreeMap::<String, Vec<PathBuf>>::new();
            collect(root, root, 0, &mut visited, &mut candidates)?;
            for (id, paths) in candidates {
                if !resolved.insert(id.clone()) || paths.len() != 1 {
                    continue;
                }
                if let Some(entry) = self.entry(&id, &paths[0]) {
                    // Conservative allowance includes D-Bus strings, structure
                    // alignment and per-page generation/token overhead.
                    bytes += entry.application.id.len()
                        + entry.application.title.len()
                        + entry.application.icon.len()
                        + 64;
                    if bytes > 16 * 1024 * 1024 {
                        return Err(ErrorCode::InvalidArgument);
                    }
                    output.insert(id, entry);
                }
            }
        }
        Ok(output)
    }
    fn entry(&self, id: &str, path: &Path) -> Option<Entry> {
        validation::id(id).ok()?;
        // O_NONBLOCK prevents a hostile FIFO from hanging discovery. Only a
        // regular file is read; directory symlinks are not traversed.
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .ok()?;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file() || metadata.len() > MAX_FILE {
            return None;
        }
        let mut text = String::new();
        (&mut file)
            .take(MAX_FILE + 1)
            .read_to_string(&mut text)
            .ok()?;
        if text.len() as u64 > MAX_FILE || text.contains('\0') {
            return None;
        }
        let values = parse(&text)?;
        if values.get("Type")?.as_str() != "Application" {
            return None;
        }
        if boolean(&values, "Hidden")? || boolean(&values, "NoDisplay")? {
            return None;
        }
        if values.contains_key("OnlyShowIn") && values.contains_key("NotShowIn") {
            return None;
        }
        if let Some(only) = values.get("OnlyShowIn")
            && !only
                .split(';')
                .any(|d| self.desktops.iter().any(|current| current == d))
        {
            return None;
        }
        if let Some(excluded) = values.get("NotShowIn")
            && excluded
                .split(';')
                .any(|d| self.desktops.iter().any(|current| current == d))
        {
            return None;
        }
        if let Some(executable) = values.get("TryExec") {
            let executable = unescape(executable)?;
            let p = Path::new(&executable);
            let usable = |p: &Path| {
                fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            };
            if if p.is_absolute() {
                !usable(p)
            } else {
                executable.is_empty()
                    || executable.contains('/')
                    || !self.executable_paths.iter().any(|dir| usable(&dir.join(p)))
            } {
                return None;
            }
        }
        let dbus = boolean(&values, "DBusActivatable")?;
        if !dbus && values.get("Exec").is_none_or(|v| v.trim().is_empty()) {
            return None;
        }
        // Validate ordinary booleans even when they do not affect visibility.
        boolean(&values, "Terminal")?;
        let base = unescape(values.get("Name")?)?;
        let title = locale_keys(&self.locale)
            .iter()
            .find_map(|locale| values.get(&format!("Name[{locale}]")))
            .map(|s| unescape(s))
            .unwrap_or(Some(base))?;
        if title.is_empty() {
            return None;
        }
        let application = Application {
            id: id.into(),
            title,
            can_launch: true,
            group: group(values.get("Categories").map(String::as_str)).into(),
            // A malformed optional icon must not hide an otherwise usable app.
            icon: values
                .get("Icon")
                .and_then(|s| unescape(s))
                .filter(|s| arinn_protocol::application::icon_reference(s).is_ok())
                .unwrap_or_default(),
        };
        application.validate().ok()?;
        Some(Entry {
            application,
            path: path.into(),
            dbus_name: dbus.then(|| dbus_name(path)).flatten(),
        })
    }
}
/// Applications-screen group from freedesktop main categories (spec §5.5,
/// #63): browsers are Online, then Game, then audio/video; the first match in
/// that order wins and everything else is Other.
fn group(categories: Option<&str>) -> &'static str {
    let categories: Vec<&str> = categories
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .collect();
    let has = |names: &[&str]| categories.iter().any(|c| names.contains(c));
    if has(&["WebBrowser"]) {
        "online"
    } else if has(&["Game"]) {
        "games"
    } else if has(&["AudioVideo", "Audio", "Video"]) {
        "media"
    } else {
        "other"
    }
}
/// GLib activates over D-Bus only when the basename without `.desktop` is a
/// valid well-known bus name; otherwise it falls back to Exec.
fn dbus_name(path: &Path) -> Option<String> {
    let stem = path.file_name()?.to_str()?.strip_suffix(".desktop")?;
    zbus::names::WellKnownName::try_from(stem).ok()?;
    Some(stem.into())
}
fn collect(
    root: &Path,
    dir: &Path,
    depth: usize,
    visited: &mut usize,
    candidates: &mut BTreeMap<String, Vec<PathBuf>>,
) -> Result<(), ErrorCode> {
    if depth > 32 {
        return Err(ErrorCode::InvalidArgument);
    }
    let list = match fs::read_dir(dir) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(ErrorCode::Unavailable),
    };
    for item in list {
        *visited += 1;
        if *visited > MAX_ENTRIES {
            return Err(ErrorCode::InvalidArgument);
        }
        let item = item.map_err(|_| ErrorCode::Unavailable)?;
        let kind = item.file_type().map_err(|_| ErrorCode::Unavailable)?;
        let path = item.path();
        if kind.is_dir() {
            collect(root, &path, depth + 1, visited, candidates)?;
        } else if path.extension().is_some_and(|e| e == "desktop") {
            let relative = path.strip_prefix(root).map_err(|_| ErrorCode::Internal)?;
            let Some(relative) = relative.to_str() else {
                continue;
            };
            candidates
                .entry(relative.replace('/', "-"))
                .or_default()
                .push(path);
        }
    }
    Ok(())
}
/// The `[Desktop Entry]` keys. A repeated key keeps its last value, as GLib
/// (and so the session launcher) reads it; packaged files such as Google
/// Chrome's repeat `StartupWMClass`. A repeated group is still malformed.
fn parse(text: &str) -> Option<BTreeMap<String, String>> {
    let mut group = "";
    let mut groups = BTreeSet::new();
    let mut values = BTreeMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            group = line.strip_prefix('[')?.strip_suffix(']')?;
            if group.is_empty() || !groups.insert(group) {
                return None;
            }
        } else {
            if group.is_empty() {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            if group == "Desktop Entry" {
                values.insert(key.into(), value.trim().into());
            }
        }
    }
    groups.contains("Desktop Entry").then_some(values)
}
fn boolean(values: &BTreeMap<String, String>, key: &str) -> Option<bool> {
    match values.get(key).map(String::as_str) {
        None | Some("false") => Some(false),
        Some("true") => Some(true),
        _ => None,
    }
}
fn unescape(input: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        out.push(if c == '\\' {
            match chars.next()? {
                's' => ' ',
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '\\' => '\\',
                _ => return None,
            }
        } else {
            c
        });
    }
    Some(out)
}
fn locale_keys(locale: &str) -> Vec<String> {
    let (base, modifier) = locale
        .split_once('@')
        .map_or((locale, None), |(a, b)| (a, Some(b)));
    let base = base.split('.').next().unwrap_or("");
    if base.is_empty() || ["C", "POSIX"].contains(&base) {
        return Vec::new();
    }
    let (language, country) = base
        .split_once('_')
        .map_or((base, None), |(a, b)| (a, Some(b)));
    let mut keys = Vec::new();
    if let (Some(country), Some(modifier)) = (country, modifier) {
        keys.push(format!("{language}_{country}@{modifier}"));
    }
    if let Some(country) = country {
        keys.push(format!("{language}_{country}"));
    }
    if let Some(modifier) = modifier {
        keys.push(format!("{language}@{modifier}"));
    }
    keys.push(language.into());
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Tree(PathBuf);
    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn setup() -> (Tree, Settings) {
        let root = std::env::temp_dir().join(format!("arinn-desktop-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("user")).unwrap();
        fs::create_dir(root.join("system")).unwrap();
        let settings = Settings {
            roots: vec![root.join("user"), root.join("system")],
            locale: "sr_RS.UTF-8@latin".into(),
            desktops: vec!["KDE".into()],
            executable_paths: vec!["/usr/bin".into()],
        };
        (Tree(root), settings)
    }
    fn put(root: &Path, file: &str, body: &str) {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }
    const ENTRY: &str = "[Desktop Entry]\nType=Application\nName=Base\nExec=fixture %U\n";
    #[test]
    fn optional_icon_references_are_discovered_without_hiding_apps() {
        let (_tree, s) = setup();
        for (value, expected) in [
            ("fixture-app", "fixture-app"),
            ("/opt/fixture/icon.png", "/opt/fixture/icon.png"),
            ("https://example.test/icon.png", ""),
            ("../icon.png", ""),
            ("/tmp/../icon.png", ""),
        ] {
            put(
                &s.roots[0],
                "fixture.desktop",
                &format!("{ENTRY}Icon={value}\n"),
            );
            let entries = s.discover().unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries.values().next().unwrap().application.icon, expected);
        }
    }
    #[test]
    fn precedence_visibility_locale_and_deletion() {
        let (_tree, s) = setup();
        put(&s.roots[1], "org.test.desktop", ENTRY);
        put(
            &s.roots[0],
            "org.test.desktop",
            &format!("{ENTRY}Hidden=true\n"),
        );
        assert!(s.discover().unwrap().is_empty());
        fs::remove_file(s.roots[0].join("org.test.desktop")).unwrap();
        assert_eq!(s.discover().unwrap().len(), 1);
        put(&s.roots[0], "org.test.desktop", "malformed");
        assert!(s.discover().unwrap().is_empty());
        put(
            &s.roots[0],
            "org.test.desktop",
            &format!(
                "{ENTRY}Name[sr]=Language\nName[sr_RS]=Country\nName[sr_RS@latin]=Localized\\sname\nOnlyShowIn=KDE;\n"
            ),
        );
        assert_eq!(
            s.discover().unwrap()["org.test.desktop"].application.title,
            "Localized name"
        );
        for extra in [
            "NoDisplay=true",
            "NotShowIn=KDE;",
            "OnlyShowIn=GNOME;",
            "Hidden=invalid",
            "TryExec=/does-not-exist",
            "Hidden=false\nHidden=true",
        ] {
            put(
                &s.roots[0],
                "org.test.desktop",
                &format!("{ENTRY}{extra}\n"),
            );
            assert!(s.discover().unwrap().is_empty(), "{extra}");
        }
    }
    #[test]
    fn a_repeated_key_keeps_its_last_value_as_in_glib() {
        let (_tree, s) = setup();
        // As shipped by the google-chrome package (#201).
        put(
            &s.roots[0],
            "google-chrome.desktop",
            "[Desktop Entry]\nName=First\nStartupWMClass=Google-chrome\nExec=fixture %U\nStartupWMClass=google-chrome\nName=Browser\nType=Application\n[Desktop Action new-window]\nName=New Window\nExec=fixture\n",
        );
        let found = s.discover().unwrap();
        assert_eq!(found["google-chrome.desktop"].application.title, "Browser");
        // A repeated group is still malformed.
        put(
            &s.roots[0],
            "google-chrome.desktop",
            &format!("{ENTRY}[Desktop Entry]\nName=Again\n"),
        );
        assert!(s.discover().unwrap().is_empty());
    }
    #[test]
    fn nested_ids_duplicates_dbus_and_untrusted_files() {
        let (_tree, s) = setup();
        put(&s.roots[0], "vendor/app.desktop", ENTRY);
        assert!(s.discover().unwrap().contains_key("vendor-app.desktop"));
        put(&s.roots[0], "vendor-app.desktop", ENTRY);
        assert!(s.discover().unwrap().is_empty());
        put(
            &s.roots[0],
            "dbus.desktop",
            "[Desktop Entry]\nType=Application\nName=DBus\nDBusActivatable=true\n",
        );
        assert_eq!(s.discover().unwrap().len(), 1);
        // Groups come from Categories=, first match in Online, Games, Media order.
        for (categories, expected) in [
            ("Network;WebBrowser;", "online"),
            ("Game;ActionGame;", "games"),
            ("AudioVideo;Video;Player;", "media"),
            ("Audio;", "media"),
            ("Game;Video;WebBrowser;", "online"),
            ("Video;Game;", "games"),
            ("Utility;TextEditor;", "other"),
            ("", "other"),
        ] {
            put(
                &s.roots[0],
                "org.test.Group.desktop",
                &format!("{ENTRY}Categories={categories}\n"),
            );
            assert_eq!(
                s.discover().unwrap()["org.test.Group.desktop"]
                    .application
                    .group,
                expected,
                "{categories}"
            );
        }
        put(&s.roots[0], "org.test.Group.desktop", ENTRY);
        assert_eq!(
            s.discover().unwrap()["org.test.Group.desktop"]
                .application
                .group,
            "other"
        );
        std::fs::remove_file(s.roots[0].join("org.test.Group.desktop")).unwrap();
        // A one-element basename is not a bus name; GLib falls back to Exec.
        assert_eq!(s.discover().unwrap()["dbus.desktop"].dbus_name, None);
        put(
            &s.roots[0],
            "org.test.Bus.desktop",
            "[Desktop Entry]\nType=Application\nName=DBus\nDBusActivatable=true\n",
        );
        put(&s.roots[0], "org.test.Plain.desktop", ENTRY);
        let found = s.discover().unwrap();
        assert_eq!(
            found["org.test.Bus.desktop"].dbus_name.as_deref(),
            Some("org.test.Bus")
        );
        assert_eq!(found["org.test.Plain.desktop"].dbus_name, None);
        std::fs::remove_file(s.roots[0].join("org.test.Bus.desktop")).unwrap();
        std::fs::remove_file(s.roots[0].join("org.test.Plain.desktop")).unwrap();
        put(
            &s.roots[0],
            "large.desktop",
            &"x".repeat(MAX_FILE as usize + 1),
        );
        std::os::unix::fs::symlink(&s.roots[0], s.roots[0].join("loop")).unwrap();
        let fifo = s.roots[0].join("fifo.desktop");
        let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert_eq!(s.discover().unwrap().len(), 1);
    }
}
