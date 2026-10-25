// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KDE settings the global on-screen keyboard needs (ADR-0105, #434).
//!
//! When the KWin adapter connects, the daemon makes sure that:
//!
//! - KWin starts Arinn's input method: `kwinrc [Wayland] InputMethod` names
//!   `org.arinn.InputMethod.desktop`, but only when no input method is set,
//!   and only once: a marker in Arinn's state directory records that the
//!   value was written (or found), so a later *None* in Plasma's settings,
//!   which deletes the key, is respected. Another choice is never replaced;
//!   the keyboard then reports `Unavailable` and this module logs
//!   `Conflict`. Uninstalling Arinn reverses neither the value nor the rule.
//! - The shell's panel window never takes focus: one KWin rule,
//!   `arinn-keyboard-panel` in `kwinrulesrc`, forces *Accept focus: No* for
//!   the window class `org.arinn.Shell` with the exact title
//!   `org.arinn.KeyboardPanel`.
//!
//! Only these entries are written, and a file this editor cannot handle
//! safely (immutable markers) is left alone. Values never reach the log.
use std::path::{Path, PathBuf};
use zbus::Connection;

/// The desktop file KWin reads the input method's command from.
pub const INPUT_METHOD_DESKTOP: &str = "org.arinn.InputMethod.desktop";
/// The title of the shell's keyboard panel window.
pub const PANEL_TITLE: &str = "org.arinn.KeyboardPanel";
const RULE: &str = "arinn-keyboard-panel";
/// The marker in Arinn's state directory: `kwinrc` names Arinn's input
/// method, written by Arinn or found there, and is never written again.
pub const INPUT_METHOD_MARKER: &str = "kde-input-method-selected";
/// Forced (`…rule=2`): no focus, above other windows, no border, and out of
/// the taskbar, pager and switcher. The KWin script places the window.
const RULE_ENTRIES: [(&str, &str); 17] = [
    ("Description", "Arinn on-screen keyboard panel"),
    ("above", "true"),
    ("aboverule", "2"),
    ("acceptfocus", "false"),
    ("acceptfocusrule", "2"),
    ("noborder", "true"),
    ("noborderrule", "2"),
    ("skippager", "true"),
    ("skippagerrule", "2"),
    ("skipswitcher", "true"),
    ("skipswitcherrule", "2"),
    ("skiptaskbar", "true"),
    ("skiptaskbarrule", "2"),
    ("title", PANEL_TITLE),
    ("titlematch", "1"),
    ("wmclass", "org.arinn.Shell"),
    ("wmclassmatch", "1"),
];

fn log(level: &str, operation: &str, error: Option<&str>) {
    eprintln!(
        "{}",
        serde_json::json!({"level":level,"component":"arinnd","provider_id":null,"request_id":null,"operation":operation,"error":error,"duration_ms":0})
    );
}

/// What a preparation pass changed, for tests.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub input_method: bool,
    pub rule: bool,
}

/// Prepares KDE in the background of an adapter connection. `arinn_config`
/// is the daemon's own configuration directory (`$XDG_CONFIG_HOME/arinn`);
/// KDE's files are next to it. `arinn_state` holds the one-time marker.
pub async fn prepare(connection: &Connection, arinn_config: &Path, arinn_state: &Path) {
    let Some(config) = arinn_config.parent().filter(|p| p.is_absolute()) else {
        return;
    };
    if !arinn_state.is_absolute() {
        return;
    }
    let data = data_dirs();
    let marker = arinn_state.join(INPUT_METHOD_MARKER);
    let _ = prepare_in(connection, config, &data, &marker).await;
}

/// Records that `kwinrc` names Arinn's input method. A marker that cannot
/// be written means another write on a later connection, never a lost
/// choice of the user's: the value is written only when it is empty.
fn mark(marker: &Path) {
    let written = marker
        .parent()
        .is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
        && std::fs::write(marker, "").is_ok();
    if !written {
        log("warn", "keyboard-input-method", Some("Unavailable"));
    }
}

/// Prepares the files under `config` (the XDG config home) and notifies KWin
/// of what changed. The input method is set at most once, recorded by
/// `marker`; the panel rule is kept on every pass.
pub async fn prepare_in(
    connection: &Connection,
    config: &Path,
    data: &[PathBuf],
    marker: &Path,
) -> Outcome {
    let mut outcome = Outcome::default();
    let desktop = data
        .iter()
        .map(|dir| dir.join("applications").join(INPUT_METHOD_DESKTOP))
        .find(|path| path.is_file());
    match desktop {
        // Arinn chose its input method once; whatever the user set since,
        // an empty value (Plasma's None) included, stays.
        _ if marker.exists() => (),
        None => log("warn", "keyboard-input-method", Some("NotFound")),
        Some(desktop) => {
            let path = config.join("kwinrc");
            let text = read(&path);
            match input_method(text.as_deref().unwrap_or(""), &desktop.to_string_lossy()) {
                Edit::Unchanged => mark(marker),
                Edit::Refused(error) => log("warn", "keyboard-input-method", Some(error)),
                Edit::Changed(next) => {
                    if write(&path, &next).is_ok() {
                        mark(marker);
                        outcome.input_method = true;
                        notify(connection).await;
                        log("info", "keyboard-input-method", None);
                    } else {
                        log("warn", "keyboard-input-method", Some("Unavailable"));
                    }
                }
            }
        }
    }
    let path = config.join("kwinrulesrc");
    let text = read(&path);
    match rule(text.as_deref().unwrap_or("")) {
        Edit::Unchanged => (),
        Edit::Refused(error) => log("warn", "keyboard-panel-rule", Some(error)),
        Edit::Changed(next) => {
            if write(&path, &next).is_ok() {
                outcome.rule = true;
                let _ = connection
                    .call_method(
                        Some("org.kde.KWin"),
                        "/KWin",
                        Some("org.kde.KWin"),
                        "reconfigure",
                        &(),
                    )
                    .await;
                log("info", "keyboard-panel-rule", None);
            } else {
                log("warn", "keyboard-panel-rule", Some("Unavailable"));
            }
        }
    }
    outcome
}

fn data_dirs() -> Vec<PathBuf> {
    std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into())
        .split(':')
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .collect()
}

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Replaces the file atomically, keeping its permissions.
fn write(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("arinn-tmp");
    std::fs::write(&tmp, text)?;
    if let Ok(meta) = std::fs::metadata(path) {
        std::fs::set_permissions(&tmp, meta.permissions())?;
    }
    std::fs::rename(&tmp, path)
}

/// KConfig's change notification, as `KConfig::Notify` sends it, so KWin's
/// config watcher starts the input method without a restart.
async fn notify(connection: &Connection) {
    let mut changed = std::collections::HashMap::new();
    changed.insert("Wayland".to_owned(), vec![b"InputMethod".to_vec()]);
    let _ = connection
        .emit_signal(
            None::<&str>,
            "/kwinrc",
            "org.kde.kconfig.notify",
            "ConfigChanged",
            &(changed,),
        )
        .await;
}

#[derive(Debug, PartialEq, Eq)]
enum Edit {
    Unchanged,
    Changed(String),
    Refused(&'static str),
}

/// A KConfig file as lines; groups are `[Name]` headers. Only the subset
/// needed here: plain group names and `key=value` entries.
struct File {
    lines: Vec<String>,
}

impl File {
    fn parse(text: &str) -> Result<Self, &'static str> {
        // `[$i]` makes a file, group or entry immutable for KConfig; never
        // work around it.
        if text.contains("[$i]") {
            return Err("PermissionDenied");
        }
        Ok(Self {
            lines: text.lines().map(str::to_owned).collect(),
        })
    }
    fn text(&self) -> String {
        let mut text = self.lines.join("\n");
        text.push('\n');
        text
    }
    /// The line range of a group's entries, after its header.
    fn group(&self, name: &str) -> Option<(usize, usize)> {
        let header = format!("[{name}]");
        let start = self.lines.iter().position(|l| l.trim() == header)? + 1;
        let end = self.lines[start..]
            .iter()
            .position(|l| l.trim_start().starts_with('['))
            .map_or(self.lines.len(), |i| start + i);
        Some((start, end))
    }
    fn key_of(line: &str) -> Option<&str> {
        let line = line.trim_start();
        if line.starts_with('#') {
            return None;
        }
        let key = line.split_once('=')?.0.trim();
        Some(key.split_once('[').map_or(key, |(base, _)| base))
    }
    fn get(&self, group: &str, key: &str) -> Option<String> {
        let (start, end) = self.group(group)?;
        self.lines[start..end].iter().find_map(|line| {
            (Self::key_of(line) == Some(key))
                .then(|| line.split_once('=').map(|(_, v)| v.trim().to_owned()))
                .flatten()
        })
    }
    /// Sets `key` in `group`, replacing every existing spelling of the key.
    fn set(&mut self, group: &str, key: &str, value: &str) {
        let entry = format!("{key}={value}");
        match self.group(group) {
            Some((start, end)) => {
                let mut kept: Vec<String> = Vec::new();
                let mut placed = false;
                for line in self.lines.drain(start..end) {
                    if Self::key_of(&line) == Some(key) {
                        if !placed {
                            kept.push(entry.clone());
                            placed = true;
                        }
                    } else {
                        kept.push(line);
                    }
                }
                if !placed {
                    // After the last entry, before trailing blank lines.
                    let at = kept
                        .iter()
                        .rposition(|l| !l.trim().is_empty())
                        .map_or(0, |i| i + 1);
                    kept.insert(at, entry);
                }
                self.lines.splice(start..start, kept);
            }
            None => {
                if self.lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    self.lines.push(String::new());
                }
                self.lines.push(format!("[{group}]"));
                self.lines.push(entry);
            }
        }
    }
}

/// The kwinrc edit: set `[Wayland] InputMethod` when it is empty.
fn input_method(text: &str, desktop: &str) -> Edit {
    let mut file = match File::parse(text) {
        Ok(file) => file,
        Err(error) => return Edit::Refused(error),
    };
    match file.get("Wayland", "InputMethod") {
        Some(value) if value == desktop => Edit::Unchanged,
        Some(value) if !value.is_empty() => Edit::Refused("Conflict"),
        _ => {
            file.set("Wayland", "InputMethod", desktop);
            Edit::Changed(file.text())
        }
    }
}

/// The kwinrulesrc edit: the panel rule with exactly its entries, listed in
/// `[General] rules` and counted in `count`.
fn rule(text: &str) -> Edit {
    let mut file = match File::parse(text) {
        Ok(file) => file,
        Err(error) => return Edit::Refused(error),
    };
    let before = file.text();
    for (key, value) in RULE_ENTRIES {
        file.set(RULE, key, value);
    }
    let mut rules: Vec<String> = file
        .get("General", "rules")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_owned)
        .collect();
    if !rules.iter().any(|r| r == RULE) {
        rules.push(RULE.to_owned());
    }
    file.set("General", "rules", &rules.join(","));
    file.set("General", "count", &rules.len().to_string());
    let after = file.text();
    if after == before {
        Edit::Unchanged
    } else {
        Edit::Changed(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_method_is_set_only_when_empty() {
        let desktop = "/usr/share/applications/org.arinn.InputMethod.desktop";
        let Edit::Changed(text) = input_method("", desktop) else {
            panic!("missing file is set");
        };
        assert_eq!(text, format!("[Wayland]\nInputMethod={desktop}\n"));
        assert_eq!(input_method(&text, desktop), Edit::Unchanged);
        let other =
            "[Wayland]\nInputMethod=/usr/share/applications/com.github.maliit.keyboard.desktop\n";
        assert_eq!(input_method(other, desktop), Edit::Refused("Conflict"));
        let empty = "[Xwayland]\nScale=1\n\n[Wayland]\nInputMethod=\nVirtualKeyboardEnabled=true\n";
        let Edit::Changed(text) = input_method(empty, desktop) else {
            panic!("empty value is set");
        };
        assert_eq!(
            text,
            format!(
                "[Xwayland]\nScale=1\n\n[Wayland]\nInputMethod={desktop}\nVirtualKeyboardEnabled=true\n"
            )
        );
        let other_groups = "[Compositing]\nBackend=OpenGL\n";
        let Edit::Changed(text) = input_method(other_groups, desktop) else {
            panic!("new group");
        };
        assert_eq!(
            text,
            format!("[Compositing]\nBackend=OpenGL\n\n[Wayland]\nInputMethod={desktop}\n")
        );
        assert_eq!(
            input_method("[Wayland][$i]\nInputMethod=\n", desktop),
            Edit::Refused("PermissionDenied")
        );
    }

    #[test]
    fn the_panel_rule_is_added_once_next_to_other_rules() {
        let Edit::Changed(text) = rule("") else {
            panic!("rule added");
        };
        assert!(text.contains("[arinn-keyboard-panel]\nDescription=Arinn on-screen keyboard panel\nabove=true\naboverule=2\nacceptfocus=false\nacceptfocusrule=2\n"));
        assert!(text.contains("\ntitle=org.arinn.KeyboardPanel\ntitlematch=1\nwmclass=org.arinn.Shell\nwmclassmatch=1\n"));
        assert!(text.contains("[General]\nrules=arinn-keyboard-panel\ncount=1\n"));
        assert_eq!(rule(&text), Edit::Unchanged);

        let user = "[1f3e]\nDescription=Firefox\nwmclass=firefox\nwmclassmatch=1\nminimizerule=2\n\n[General]\ncount=1\nrules=1f3e\n";
        let Edit::Changed(text) = rule(user) else {
            panic!("rule added");
        };
        assert!(text.starts_with(
            "[1f3e]\nDescription=Firefox\nwmclass=firefox\nwmclassmatch=1\nminimizerule=2\n"
        ));
        assert!(text.contains("[General]\ncount=2\nrules=1f3e,arinn-keyboard-panel\n"));
        assert_eq!(rule(&text), Edit::Unchanged);

        // A user edit of the rule is put back; other entries stay.
        let edited = text.replace("acceptfocus=false", "acceptfocus=true");
        let Edit::Changed(fixed) = rule(&edited) else {
            panic!("rule restored");
        };
        assert_eq!(fixed, text);
        assert_eq!(
            rule("[$i]\n[General]\ncount=0\n"),
            Edit::Refused("PermissionDenied")
        );
    }

    #[tokio::test]
    async fn prepare_writes_both_files_and_tells_kwin() {
        use futures_util::StreamExt;
        use std::sync::{Arc, Mutex};
        let dir = std::env::temp_dir().join(format!("arinn-kbd-{}", uuid::Uuid::new_v4()));
        let config = dir.join("config");
        let data = dir.join("data");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(data.join("applications")).unwrap();
        std::fs::write(
            data.join("applications").join(INPUT_METHOD_DESKTOP),
            "[Desktop Entry]\n",
        )
        .unwrap();
        std::fs::write(
            config.join("kwinrc"),
            "[Wayland]\nVirtualKeyboardEnabled=true\n",
        )
        .unwrap();

        /// Kills the private bus also when an assertion fails, so it never
        /// outlives the test and holds its output open (review of #484).
        struct Bus(std::process::Child);
        impl Drop for Bus {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut bus = Bus(std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap());
        let mut address = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(bus.0.stdout.take().unwrap()),
            &mut address,
        )
        .unwrap();
        let connect = || async {
            zbus::connection::Builder::address(address.trim())
                .unwrap()
                .build()
                .await
                .unwrap()
        };
        struct KWin(Arc<Mutex<u32>>);
        #[zbus::interface(name = "org.kde.KWin")]
        impl KWin {
            #[zbus(name = "reconfigure")]
            fn reconfigure(&self) {
                *self.0.lock().unwrap() += 1;
            }
        }
        let reconfigured = Arc::new(Mutex::new(0));
        let kwin = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .name("org.kde.KWin")
            .unwrap()
            .serve_at("/KWin", KWin(reconfigured.clone()))
            .unwrap()
            .build()
            .await
            .unwrap();
        let watcher = connect().await;
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.kde.kconfig.notify")
            .unwrap()
            .build();
        let mut notes = zbus::MessageStream::for_match_rule(rule, &watcher, None)
            .await
            .unwrap();
        let daemon = connect().await;
        let marker = dir.join("state").join(INPUT_METHOD_MARKER);
        let outcome = prepare_in(&daemon, &config, std::slice::from_ref(&data), &marker).await;
        assert_eq!(
            outcome,
            Outcome {
                input_method: true,
                rule: true
            }
        );
        let kwinrc = std::fs::read_to_string(config.join("kwinrc")).unwrap();
        assert!(kwinrc.contains(&format!(
            "InputMethod={}",
            data.join("applications").join(INPUT_METHOD_DESKTOP).display()
        )));
        assert!(
            std::fs::read_to_string(config.join("kwinrulesrc"))
                .unwrap()
                .contains("[arinn-keyboard-panel]")
        );
        let note = tokio::time::timeout(std::time::Duration::from_secs(5), notes.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(note.header().path().unwrap().as_str(), "/kwinrc");
        let (changed,): (std::collections::HashMap<String, Vec<Vec<u8>>>,) =
            note.body().deserialize().unwrap();
        assert_eq!(changed["Wayland"], [b"InputMethod".to_vec()]);
        assert_eq!(*reconfigured.lock().unwrap(), 1);
        assert!(marker.exists());
        // A second pass changes nothing and does not reconfigure KWin.
        assert_eq!(
            prepare_in(&daemon, &config, std::slice::from_ref(&data), &marker).await,
            Outcome::default()
        );
        assert_eq!(*reconfigured.lock().unwrap(), 1);
        // The user picks None in Plasma's settings, which deletes the key:
        // Arinn wrote the value once and never writes it again (review of
        // #484). The panel rule is still kept.
        std::fs::write(
            config.join("kwinrc"),
            "[Wayland]\nVirtualKeyboardEnabled=true\n",
        )
        .unwrap();
        std::fs::remove_file(config.join("kwinrulesrc")).unwrap();
        assert_eq!(
            prepare_in(&daemon, &config, std::slice::from_ref(&data), &marker).await,
            Outcome {
                input_method: false,
                rule: true
            }
        );
        assert!(
            !std::fs::read_to_string(config.join("kwinrc"))
                .unwrap()
                .contains("InputMethod")
        );
        // A value Arinn finds already in place is recorded the same way.
        let found = dir.join("found-marker");
        std::fs::write(config.join("kwinrc"), &kwinrc).unwrap();
        assert_eq!(
            prepare_in(&daemon, &config, std::slice::from_ref(&data), &found).await,
            Outcome::default()
        );
        assert!(found.exists());
        drop(kwin);
        drop(bus);
        let _ = std::fs::remove_dir_all(dir);
    }
}
