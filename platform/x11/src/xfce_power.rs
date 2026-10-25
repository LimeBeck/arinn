// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! xfce4-power-manager's power button in the dedicated Arinn XFCE session
//! (spec §5.7 *Power button*, owner decision of 2026-10-24, ADR-0111, #481).
//!
//! xfce4-power-manager grabs `XF86PowerOff` itself and acts on its
//! `power-button-action` setting. While Arinn runs in the dedicated Arinn
//! XFCE session (`ARINN_SESSION_ENTRY=xfce`, ADR-0107), the adapter sets that
//! action to "do nothing" (0) through xfconf, so only Arinn acts on the key;
//! the adapter then reads the key from raw events. The ordinary XFCE desktop
//! with opt-in autostart is never changed.
//!
//! The user's value is never lost: before any change it is written to
//! `$XDG_STATE_HOME/arinn/xfce-power-button` (durably, before the setting
//! changes), and it stays there until it has been restored. `release`
//! restores it on a clean stop, from `ExecStopPost=` of `arinn-ewmh.service`
//! (`arinn-platform-x11 --release-power-key`), on the next start of the
//! adapter in any session and at every XFCE login (`restore_at_login`). A
//! value the user set in the meantime is kept. Without xfconf nothing is
//! done.
//!
//! The restore also survives removing Arinn after an unclean end (review of
//! #516, Codex P1): with the copy, the adapter writes a per-user XDG
//! autostart entry, `$XDG_CONFIG_HOME/autostart/arinn-power-button-restore.desktop`
//! (`OnlyShowIn=XFCE;`), and a small `sh` script next to it that holds the
//! value. It uses only `sh` and XFCE's own `xfconf-query`. While Arinn is
//! installed it does nothing (Arinn restores the value itself, with its
//! lock); once Arinn's adapter is gone, it gives the value back at the next
//! XFCE login and removes itself and the copy. Every release removes it.
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use zbus::{Connection, zvariant::Value};

const SERVICE: &str = "org.xfce.Xfconf";
const PATH: &str = "/org/xfce/Xfconf";
const INTERFACE: &str = "org.xfce.Xfconf";
const CHANNEL: &str = "xfce4-power-manager";
const PROPERTY: &str = "/xfce4-power-manager/power-button-action";
/// xfce4-power-manager's "Do nothing".
const NOTHING: u32 = 0;
const TIMEOUT: Duration = Duration::from_secs(5);
/// The adapter's bus name: while it is owned, the adapter decides.
const ADAPTER: &str = "org.arinn.X11";
/// The packaged adapter, when this process cannot name its own binary.
const INSTALLED: &str = "/usr/bin/arinn-platform-x11";
/// xfce4-power-manager's channel file below a configuration directory.
const CHANNEL_FILE: &str = "xfce4/xfconf/xfce-perchannel-xml/xfce4-power-manager.xml";
/// The per-user restore entry in `$XDG_CONFIG_HOME/autostart`.
const ENTRY: &str = "autostart/arinn-power-button-restore.desktop";

/// The user's setting before Arinn changed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
    /// Not set: xfce4-power-manager uses its default.
    Absent,
    Uint(u32),
    Int(i32),
}
impl Saved {
    fn encode(self) -> String {
        match self {
            Saved::Absent => "absent\n".into(),
            Saved::Uint(value) => format!("uint {value}\n"),
            Saved::Int(value) => format!("int {value}\n"),
        }
    }
    fn decode(text: &str) -> Option<Self> {
        let text = text.strip_suffix('\n').unwrap_or(text);
        if text == "absent" {
            return Some(Saved::Absent);
        }
        let (kind, value) = text.split_once(' ')?;
        match kind {
            "uint" => value.parse().ok().map(Saved::Uint),
            "int" => value.parse().ok().map(Saved::Int),
            _ => None,
        }
    }
    fn is_nothing(self) -> bool {
        matches!(self, Saved::Uint(0) | Saved::Int(0))
    }
    /// The `xfconf-query` arguments that give this value back.
    fn query(self) -> String {
        match self {
            Saved::Absent => "-r".into(),
            Saved::Uint(value) => format!("-n -t uint --set={value}"),
            Saved::Int(value) => format!("-n -t int --set={value}"),
        }
    }
}

/// Whether this adapter runs in the dedicated Arinn XFCE session.
pub fn dedicated(entry: Option<&str>) -> bool {
    entry == Some("xfce")
}

/// This adapter's own binary, so an install outside `/usr` (or a
/// development build) is recognised as present too (review of #516). After
/// an upgrade replaced the file, the kernel names it with " (deleted)"; the
/// path is the same.
fn adapter_binary() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            let text = path.to_str()?;
            Some(PathBuf::from(
                text.strip_suffix(" (deleted)").unwrap_or(text),
            ))
        })
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| PathBuf::from(INSTALLED))
}

/// The files behind the user's setting.
#[derive(Clone, Debug)]
pub struct Files {
    /// `$XDG_STATE_HOME/arinn/xfce-power-button`: the user's value while
    /// Arinn holds the key. A lock file next to it orders the adapter and
    /// the login restore.
    pub record: PathBuf,
    /// The user's own xfconf channel file. Only a property named there is
    /// the user's value; xfconf otherwise returns a system default.
    pub channel: PathBuf,
    /// The per-user restore entry; its script is next to it (`.sh`).
    pub entry: PathBuf,
    /// The system default channel files (`$XDG_CONFIG_DIRS`, most
    /// important first).
    pub defaults: Vec<PathBuf>,
    /// The adapter binary that wrote the entry (`adapter_binary`). While it
    /// exists, the per-user restore entry leaves the setting to Arinn.
    pub installed: PathBuf,
}
impl Files {
    pub fn from_env(
        home: Option<&std::ffi::OsStr>,
        state: Option<&std::ffi::OsStr>,
        config: Option<&std::ffi::OsStr>,
    ) -> Option<Self> {
        let base = |value: Option<&std::ffi::OsStr>, fallback: &str| {
            value
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| {
                    home.map(PathBuf::from)
                        .filter(|p| p.is_absolute())
                        .map(|p| p.join(fallback))
                })
        };
        let config = base(config, ".config")?;
        Some(Self {
            record: base(state, ".local/state")?.join("arinn/xfce-power-button"),
            channel: config.join(CHANNEL_FILE),
            entry: config.join(ENTRY),
            defaults: std::env::var("XDG_CONFIG_DIRS")
                .ok()
                .filter(|dirs| !dirs.is_empty())
                .unwrap_or_else(|| "/etc/xdg".into())
                .split(':')
                .map(PathBuf::from)
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join(CHANNEL_FILE))
                .collect(),
            installed: adapter_binary(),
        })
    }
    /// From an environment `var`: the login's directories that
    /// `arinn-xfce-session` passes to the user manager
    /// (`ARINN_LOGIN_STATE_HOME`, `ARINN_LOGIN_CONFIG_HOME`) win over the
    /// process's own `XDG_*`, so the adapter, its `ExecStopPost=` helper and
    /// the system login entry (which runs in the login and has its own
    /// `XDG_*`) find the same copy and the same autostart folder, also when
    /// the login and the user manager disagree (review of #516, Codex P1).
    /// With `require_login` (the dedicated session takes the key) both must
    /// be set and absolute, or there are no files and nothing is taken.
    pub fn from_vars<V>(var: V, require_login: bool) -> Option<Self>
    where
        V: Fn(&str) -> Option<std::ffi::OsString>,
    {
        let absolute = |name: &str| var(name).filter(|value| Path::new(value).is_absolute());
        let (state, config) = (
            absolute("ARINN_LOGIN_STATE_HOME"),
            absolute("ARINN_LOGIN_CONFIG_HOME"),
        );
        if require_login && (state.is_none() || config.is_none()) {
            return None;
        }
        Self::from_env(
            var("HOME").as_deref(),
            state.or_else(|| var("XDG_STATE_HOME")).as_deref(),
            config.or_else(|| var("XDG_CONFIG_HOME")).as_deref(),
        )
    }
    /// The script the restore entry runs.
    pub fn script(&self) -> PathBuf {
        self.entry.with_extension("sh")
    }
    fn lock_path(&self) -> PathBuf {
        self.record.with_extension("lock")
    }
    /// The user set the property: their channel file names it.
    fn explicit(&self) -> bool {
        std::fs::read_to_string(&self.channel)
            .is_ok_and(|text| text.contains(r#"name="power-button-action""#))
    }
    /// The value the system default files give the property, if any.
    fn system_default(&self) -> Option<i64> {
        self.defaults.iter().find_map(|path| {
            let text = std::fs::read_to_string(path).ok()?;
            let at = text.find(r#"name="power-button-action""#)?;
            let tag = &text[at..at + text[at..].find('>')?];
            let value = &tag[tag.find(r#"value=""#)? + 7..];
            value[..value.find('"')?].parse().ok()
        })
    }
    /// Whether the live value xfconf answered is the user's own. The
    /// user's channel file names it, or the value differs from the system
    /// default (xfce4-power-manager's built-in default is "do nothing"):
    /// xfconfd writes the file a few seconds after a change, so a value
    /// the user set just before has to be recognised from the live value
    /// (Codex P2 on #516). Otherwise it is the default, recorded as "not
    /// set".
    fn users(&self, live: Saved) -> Saved {
        let value = match live {
            Saved::Uint(value) => i64::from(value),
            Saved::Int(value) => i64::from(value),
            Saved::Absent => return Saved::Absent,
        };
        if self.explicit() || value != self.system_default().unwrap_or(0) {
            live
        } else {
            Saved::Absent
        }
    }
    /// An exclusive lock next to the record, held while it is read or
    /// changed; released when dropped.
    fn lock(&self) -> Option<std::fs::File> {
        use std::os::fd::AsRawFd;
        let folder = self.record.parent()?;
        std::fs::create_dir_all(folder).ok()?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.lock_path())
            .ok()?;
        // SAFETY: flock on a descriptor this function owns.
        (unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0).then_some(file)
    }
}

fn log(message: &str) {
    eprintln!("arinn-platform-x11: {message}");
}

async fn call<B>(c: &Connection, method: &str, body: &B) -> zbus::Result<zbus::Message>
where
    B: zbus::export::serde::Serialize + zbus::zvariant::DynamicType,
{
    tokio::time::timeout(
        TIMEOUT,
        c.call_method(Some(SERVICE), PATH, Some(INTERFACE), method, body),
    )
    .await
    .map_err(|_| zbus::Error::Failure("xfconf timed out".into()))?
}

/// xfconf is running or can be started on this bus.
async fn available(c: &Connection) -> bool {
    let Ok(bus) = zbus::fdo::DBusProxy::new(c).await else {
        return false;
    };
    let name = SERVICE.try_into().expect("valid name");
    if bus.name_has_owner(name).await.unwrap_or(false) {
        return true;
    }
    bus.list_activatable_names()
        .await
        .is_ok_and(|names| names.iter().any(|n| n.as_str() == SERVICE))
}

/// The current setting; `Ok(None)` for a type Arinn cannot restore.
async fn current(c: &Connection) -> zbus::Result<Option<Saved>> {
    match call(c, "GetProperty", &(CHANNEL, PROPERTY)).await {
        Ok(reply) => {
            let value: zbus::zvariant::OwnedValue = reply.body().deserialize()?;
            Ok(match &*value {
                Value::U32(value) => Some(Saved::Uint(*value)),
                Value::I32(value) => Some(Saved::Int(*value)),
                _ => None,
            })
        }
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str().ends_with("PropertyNotFound") =>
        {
            Ok(Some(Saved::Absent))
        }
        Err(error) => Err(error),
    }
}

async fn write_setting(c: &Connection, value: Saved) -> zbus::Result<()> {
    match value {
        Saved::Absent => call(c, "ResetProperty", &(CHANNEL, PROPERTY, false)).await,
        Saved::Uint(value) => call(c, "SetProperty", &(CHANNEL, PROPERTY, Value::U32(value))).await,
        Saved::Int(value) => call(c, "SetProperty", &(CHANNEL, PROPERTY, Value::I32(value))).await,
    }
    .map(drop)
}

/// Writes `saved` so that it survives a crash or a power loss.
fn store(path: &Path, saved: Saved) -> std::io::Result<()> {
    write_durably(path, &saved.encode())
}

/// Writes `text` to `path` so that it survives a crash or a power loss.
fn write_durably(path: &Path, text: &str) -> std::io::Result<()> {
    let folder = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    std::fs::create_dir_all(folder)?;
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);
    let mut file = std::fs::File::create(&partial)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&partial, path)?;
    std::fs::File::open(folder)?.sync_all()
}

/// `path` as text that is safe both in single quotes in `sh` and in double
/// quotes in a desktop entry's `Exec`; `None` for a path that would need
/// escaping there. Such a path changes nothing.
fn quotable(path: &Path) -> Option<&str> {
    let text = path.to_str()?;
    (!text
        .chars()
        .any(|c| c.is_control() || matches!(c, '\'' | '"' | '`' | '$' | '\\' | '%')))
    .then_some(text)
}

/// The script of the per-user restore entry for `saved`: only `sh` and
/// `xfconf-query`, so it works after Arinn is removed.
fn restore_script(files: &Files, saved: Saved) -> Option<String> {
    let installed = quotable(&files.installed)?;
    let entry = quotable(&files.entry)?;
    let script = files.script();
    let script = quotable(&script)?;
    let record = quotable(&files.record)?;
    let lock = files.lock_path();
    let lock = quotable(&lock)?;
    let query = saved.query();
    Some(format!(
        r#"#!/bin/sh
# Written by Arinn (ADR-0111) while it held the power key in the dedicated
# Arinn XFCE session. If Arinn was removed before it could give the XFCE
# power button setting back, this gives it back at the next XFCE login and
# removes itself. While Arinn's adapter is installed, Arinn does that
# itself. Anything it cannot read is left for the next login.
[ -e '{installed}' ] && exit 0
c=xfce4-power-manager
p=/xfce4-power-manager/power-button-action
list=$(xfconf-query -c "$c" -l 2>/dev/null) || exit 0
case "$list" in
*"$p"*)
    now=$(xfconf-query -c "$c" -p "$p" 2>/dev/null) || exit 0
    # Anything but Arinn's "do nothing" was chosen by the user since: kept.
    if [ "$now" = 0 ]; then
        xfconf-query -c "$c" -p "$p" {query} || exit 0
    fi
    ;;
esac
rm -f '{entry}' '{script}' '{record}' '{lock}'
"#
    ))
}

/// The per-user XDG autostart entry that runs `script`.
fn restore_entry(script: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Arinn power button restore\n\
         Comment=Gives back the XFCE power button setting if Arinn was removed\n\
         Exec=sh \"{script}\"\n\
         OnlyShowIn=XFCE;\n\
         NoDisplay=true\n\
         Terminal=false\n"
    )
}

/// Writes the per-user restore entry for `saved`, the script first.
fn write_restore(files: &Files, saved: Saved) -> std::io::Result<()> {
    let invalid = || std::io::Error::from(std::io::ErrorKind::InvalidInput);
    let text = restore_script(files, saved).ok_or_else(invalid)?;
    let script = files.script();
    write_durably(&script, &text)?;
    write_durably(
        &files.entry,
        &restore_entry(quotable(&script).ok_or_else(invalid)?),
    )
}

/// Removes the per-user restore entry, the entry first.
fn remove_restore(files: &Files) {
    for path in [files.entry.clone(), files.script()] {
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                log("cannot remove the XFCE power button restore entry");
            }
            _ => {}
        }
    }
}

fn load(path: &Path) -> std::io::Result<Option<Saved>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Saved::decode(&text)
            .map(Some)
            .ok_or_else(|| std::io::ErrorKind::InvalidData.into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Sets the power button action to "do nothing" for the time Arinn runs.
/// Returns whether xfce4-power-manager now leaves the key alone. Never
/// changes the setting without a stored copy of the user's value. A copy an
/// earlier run left is given back first (review of #517): a value the user
/// chose since then is theirs and must survive this run.
pub async fn take(c: &Connection, files: &Files) -> bool {
    let Some(_lock) = files.lock() else {
        log("cannot lock the XFCE power button record; left unchanged");
        return false;
    };
    if !available(c).await || !release_locked(c, files).await {
        return false;
    }
    let path = files.record.as_path();
    let original = match current(c).await {
        Ok(Some(saved)) if saved.is_nothing() => return true,
        Ok(Some(saved)) => {
            // A value that is only the system default is recorded as "not
            // set" and reset afterwards, never pinned as the user's.
            let saved = files.users(saved);
            if store(path, saved).is_err() {
                log("cannot keep the XFCE power button setting; left unchanged");
                return false;
            }
            // The restore must survive removing Arinn: without the entry,
            // nothing changes.
            if write_restore(files, saved).is_err() {
                log("cannot write the XFCE power button restore entry; left unchanged");
                remove_restore(files);
                let _ = std::fs::remove_file(path);
                return false;
            }
            saved
        }
        Ok(None) => {
            log("unexpected XFCE power button setting type; left unchanged");
            return false;
        }
        Err(_) => {
            log("xfconf did not answer; the XFCE power button is unchanged");
            return false;
        }
    };
    let nothing = match original {
        Saved::Int(_) => Saved::Int(NOTHING as i32),
        _ => Saved::Uint(NOTHING),
    };
    if write_setting(c, nothing).await.is_err() {
        log("xfconf refused the XFCE power button setting (locked?); left unchanged");
        // The setting is as it was: the copy goes.
        let _ = release_locked(c, files).await;
        return false;
    }
    true
}

/// Gives the user's setting back and forgets the stored copy. A setting
/// that no longer holds Arinn's "do nothing" was changed by the user in the
/// meantime and is kept. Returns false when a copy remains to restore later.
pub async fn release(c: &Connection, files: &Files) -> bool {
    match files.lock() {
        Some(_lock) => release_locked(c, files).await,
        None => false,
    }
}

/// At an XFCE login (`--restore-power-button`, system XDG autostart): gives
/// back a copy an unclean end of the dedicated session left, unless the
/// adapter runs and owns the key in this session.
pub async fn restore_at_login(c: &Connection, files: &Files) -> bool {
    let Some(_lock) = files.lock() else {
        return false;
    };
    let adapter = async {
        let bus = zbus::fdo::DBusProxy::new(c).await.ok()?;
        bus.name_has_owner(ADAPTER.try_into().ok()?).await.ok()
    };
    if adapter.await.unwrap_or(false) {
        return true;
    }
    release_locked(c, files).await
}

async fn release_locked(c: &Connection, files: &Files) -> bool {
    let path = files.record.as_path();
    let saved = match load(path) {
        Ok(Some(saved)) => saved,
        Ok(None) => {
            // A restore entry an interrupted release left behind.
            remove_restore(files);
            return true;
        }
        Err(_) => {
            log("unreadable stored XFCE power button setting; kept for the user");
            return false;
        }
    };
    if !available(c).await {
        return false;
    }
    match current(c).await {
        Ok(Some(now)) if now.is_nothing() => {
            if write_setting(c, saved).await.is_err() {
                return false;
            }
        }
        Ok(_) => {}
        Err(_) => return false,
    }
    remove_restore(files);
    std::fs::remove_file(path).is_ok()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::{Arc, Mutex},
    };

    pub(crate) struct Bus(Child, pub(crate) String);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn bus() -> Bus {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Bus(child, address.trim().to_owned())
    }
    async fn connect(address: &str) -> Connection {
        zbus::connection::Builder::address(address)
            .unwrap()
            .build()
            .await
            .unwrap()
    }

    /// xfconfd's properties and the calls that changed them.
    #[derive(Default)]
    struct Store {
        properties: HashMap<(String, String), zbus::zvariant::OwnedValue>,
        writes: Vec<String>,
        locked: bool,
    }
    type Shared = Arc<Mutex<Store>>;
    /// The part of xfconfd's interface Arinn uses. A missing property
    /// answers xfconfd's `PropertyNotFound`, a locked one `PermissionDenied`.
    struct Xfconf(Shared);
    #[zbus::interface(name = "org.xfce.Xfconf")]
    impl Xfconf {
        fn get_property(
            &self,
            channel: String,
            property: String,
        ) -> Result<zbus::zvariant::OwnedValue, XfconfError> {
            self.0
                .lock()
                .unwrap()
                .properties
                .get(&(channel, property))
                .map(|v| v.try_clone().unwrap())
                .ok_or(XfconfError::PropertyNotFound("no such property".into()))
        }
        fn set_property(
            &self,
            channel: String,
            property: String,
            value: zbus::zvariant::OwnedValue,
        ) -> Result<(), XfconfError> {
            let mut store = self.0.lock().unwrap();
            if store.locked {
                return Err(XfconfError::PermissionDenied("locked".into()));
            }
            store.writes.push(format!("set {property} {value:?}"));
            store.properties.insert((channel, property), value);
            Ok(())
        }
        fn reset_property(&self, channel: String, property: String, _recursive: bool) {
            let mut store = self.0.lock().unwrap();
            store.writes.push(format!("reset {property}"));
            store.properties.remove(&(channel, property));
        }
    }
    #[derive(Debug, zbus::DBusError)]
    #[zbus(prefix = "org.xfce.Xfconf.Error")]
    enum XfconfError {
        #[zbus(error)]
        ZBus(zbus::Error),
        PropertyNotFound(String),
        PermissionDenied(String),
    }

    pub(crate) struct Fixture {
        pub(crate) bus: Bus,
        _service: Connection,
        pub(crate) adapter: Connection,
        store: Shared,
        pub(crate) files: Files,
        root: PathBuf,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    /// A fake xfconf holding `value`; `explicit`: the user's channel file
    /// names the property (otherwise the value is a system default).
    async fn fixture_with(value: Option<Value<'static>>, explicit: bool) -> Fixture {
        let bus = bus();
        let store = Shared::default();
        if let Some(value) = value {
            store
                .lock()
                .unwrap()
                .properties
                .insert((CHANNEL.into(), PROPERTY.into()), value.try_into().unwrap());
        }
        let service = connect(&bus.1).await;
        service
            .object_server()
            .at(PATH, Xfconf(store.clone()))
            .await
            .unwrap();
        service.request_name(SERVICE).await.unwrap();
        let root = std::env::temp_dir().join(format!("arinn-xfce-{}", uuid::Uuid::new_v4()));
        let mut files = Files::from_env(
            None,
            Some(root.join("state").as_os_str()),
            Some(root.join("config").as_os_str()),
        )
        .unwrap();
        // Hermetic system defaults: none unless a test writes one.
        files.defaults = vec![root.join("etc/xdg").join(CHANNEL_FILE)];
        if explicit {
            std::fs::create_dir_all(files.channel.parent().unwrap()).unwrap();
            std::fs::write(
                &files.channel,
                r#"<channel name="xfce4-power-manager" version="1.0"><property name="xfce4-power-manager" type="empty"><property name="power-button-action" type="uint" value="3"/></property></channel>"#,
            )
            .unwrap();
        }
        Fixture {
            adapter: connect(&bus.1).await,
            bus,
            _service: service,
            store,
            files,
            root,
        }
    }
    pub(crate) async fn fixture(value: Option<Value<'static>>) -> Fixture {
        let explicit = value.is_some();
        fixture_with(value, explicit).await
    }
    impl Fixture {
        pub(crate) fn value(&self) -> Option<Saved> {
            let store = self.store.lock().unwrap();
            store
                .properties
                .get(&(CHANNEL.into(), PROPERTY.into()))
                .map(|v| match &**v {
                    Value::U32(n) => Saved::Uint(*n),
                    Value::I32(n) => Saved::Int(*n),
                    other => panic!("{other:?}"),
                })
        }
        fn set(&self, value: Value<'static>) {
            self.store
                .lock()
                .unwrap()
                .properties
                .insert((CHANNEL.into(), PROPERTY.into()), value.try_into().unwrap());
        }
        /// The value, readable later from another place.
        pub(crate) fn value_fn(&self) -> std::sync::Arc<dyn Fn() -> Option<Saved> + Send + Sync> {
            let store = self.store.clone();
            std::sync::Arc::new(move || {
                store
                    .lock()
                    .unwrap()
                    .properties
                    .get(&(CHANNEL.into(), PROPERTY.into()))
                    .map(|v| match &**v {
                        Value::U32(n) => Saved::Uint(*n),
                        Value::I32(n) => Saved::Int(*n),
                        other => panic!("{other:?}"),
                    })
            })
        }
        /// Locks the property: xfconf refuses changes.
        pub(crate) fn lock(&self) {
            self.store.lock().unwrap().locked = true;
        }
        pub(crate) fn unlock(&self) {
            self.store.lock().unwrap().locked = false;
        }
        pub(crate) fn writes(&self) -> usize {
            self.store.lock().unwrap().writes.len()
        }
        fn stored(&self) -> Option<Saved> {
            load(&self.files.record).unwrap()
        }
        fn path(&self) -> &Path {
            &self.files.record
        }
    }

    /// An environment from `pairs`.
    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<std::ffi::OsString> + use<> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v.into())
    }
    fn same(a: &Files, b: &Files) {
        assert_eq!(a.record, b.record);
        assert_eq!(a.channel, b.channel);
        assert_eq!(a.entry, b.entry);
    }

    /// Review of #516 (Codex P1): the login sets its own XDG directories,
    /// the user manager does not. The adapter (user manager, with the
    /// directories `arinn-xfce-session` passed) and the system login entry
    /// (the login) find the same copy and the same autostart folder.
    #[test]
    fn the_login_directories_win_when_only_the_login_sets_them() {
        let login = vars(&[
            ("HOME", "/home/u"),
            ("XDG_STATE_HOME", "/login/state"),
            ("XDG_CONFIG_HOME", "/login/config"),
        ]);
        let manager = vars(&[
            ("HOME", "/home/u"),
            ("ARINN_LOGIN_STATE_HOME", "/login/state"),
            ("ARINN_LOGIN_CONFIG_HOME", "/login/config"),
        ]);
        let adapter = Files::from_vars(manager, true).unwrap();
        same(&adapter, &Files::from_vars(login, false).unwrap());
        assert!(adapter.record.starts_with("/login/state"));
        assert!(adapter.entry.starts_with("/login/config/autostart"));
    }

    /// The other direction: the user manager sets its own XDG directories,
    /// the login uses the defaults. The login's defaults win.
    #[test]
    fn the_login_directories_win_when_only_the_manager_sets_them() {
        let login = vars(&[("HOME", "/home/u")]);
        let manager = vars(&[
            ("HOME", "/home/u"),
            ("XDG_STATE_HOME", "/manager/state"),
            ("XDG_CONFIG_HOME", "/manager/config"),
            ("ARINN_LOGIN_STATE_HOME", "/home/u/.local/state"),
            ("ARINN_LOGIN_CONFIG_HOME", "/home/u/.config"),
        ]);
        let adapter = Files::from_vars(manager, true).unwrap();
        same(&adapter, &Files::from_vars(login, false).unwrap());
        assert!(!adapter.record.starts_with("/manager"));
    }

    /// The dedicated session without the login's directories takes
    /// nothing; a relative value does not count. Other sessions fall back
    /// to their own `XDG_*` for giving a stale copy back.
    #[test]
    fn the_dedicated_session_needs_the_login_directories() {
        let manager = vars(&[("HOME", "/home/u"), ("XDG_STATE_HOME", "/manager/state")]);
        assert!(Files::from_vars(&manager, true).is_none());
        assert!(
            Files::from_vars(&manager, false)
                .unwrap()
                .record
                .starts_with("/manager/state")
        );
        let relative = vars(&[
            ("HOME", "/home/u"),
            ("ARINN_LOGIN_STATE_HOME", "state"),
            ("ARINN_LOGIN_CONFIG_HOME", "/c"),
        ]);
        assert!(Files::from_vars(relative, true).is_none());
    }

    #[test]
    fn only_the_dedicated_xfce_session_and_a_valid_record() {
        assert!(dedicated(Some("xfce")));
        for entry in [None, Some("plasma"), Some(""), Some("XFCE")] {
            assert!(!dedicated(entry), "{entry:?}");
        }
        for saved in [Saved::Absent, Saved::Uint(3), Saved::Int(-1)] {
            assert_eq!(Saved::decode(&saved.encode()), Some(saved));
        }
        for text in ["", "uint", "uint x", "string 3", "absent extra"] {
            assert_eq!(Saved::decode(text), None, "{text}");
        }
        let files = Files::from_env(Some("/home/u".as_ref()), None, None).unwrap();
        assert_eq!(
            files.record,
            PathBuf::from("/home/u/.local/state/arinn/xfce-power-button")
        );
        assert_eq!(
            files.channel,
            PathBuf::from(
                "/home/u/.config/xfce4/xfconf/xfce-perchannel-xml/xfce4-power-manager.xml"
            )
        );
        let files = Files::from_env(
            Some("/home/u".as_ref()),
            Some("relative".as_ref()),
            Some("/c".as_ref()),
        )
        .unwrap();
        assert_eq!(
            files.record,
            PathBuf::from("/home/u/.local/state/arinn/xfce-power-button")
        );
        assert!(files.channel.starts_with("/c/xfce4"));
        assert!(Files::from_env(None, None, None).is_none());
    }

    /// Owner decision of 2026-10-24: "do nothing" while Arinn runs, the
    /// user's value back when it stops; the copy exists before the change.
    #[tokio::test]
    async fn sets_nothing_while_arinn_runs_and_restores_the_users_value() {
        let f = fixture(Some(Value::U32(3))).await;
        assert!(take(&f.adapter, &f.files).await);
        assert_eq!(f.value(), Some(Saved::Uint(0)));
        assert_eq!(f.stored(), Some(Saved::Uint(3)));
        // The restore that survives removing Arinn is written with the copy.
        let entry = std::fs::read_to_string(&f.files.entry).unwrap();
        assert!(entry.contains("\nOnlyShowIn=XFCE;\n"), "{entry}");
        assert!(
            entry.contains(&format!("\nExec=sh \"{}\"\n", f.files.script().display())),
            "{entry}"
        );
        assert!(f.files.script().exists());
        assert!(release(&f.adapter, &f.files).await);
        assert_eq!(f.value(), Some(Saved::Uint(3)));
        assert!(!f.path().exists());
        assert!(!f.files.entry.exists());
        assert!(!f.files.script().exists());
        // A second release has nothing left to do.
        let writes = f.writes();
        assert!(release(&f.adapter, &f.files).await);
        assert_eq!(f.writes(), writes);
    }

    /// A crash or power loss leaves the copy; the next take gives it back
    /// first, never reads Arinn's "do nothing" as the user's value, and the
    /// next release restores it.
    #[tokio::test]
    async fn a_stale_copy_survives_a_crash_and_is_never_overwritten() {
        let f = fixture(Some(Value::U32(4))).await;
        assert!(take(&f.adapter, &f.files).await);
        // Crash: no release. The next run takes over again.
        assert!(take(&f.adapter, &f.files).await);
        assert_eq!(f.stored(), Some(Saved::Uint(4)));
        assert_eq!(f.value(), Some(Saved::Uint(0)));
        assert!(release(&f.adapter, &f.files).await);
        assert_eq!(f.value(), Some(Saved::Uint(4)));
    }

    /// Review of #517 (Codex P2, the reviewer's probe): a power loss leaves
    /// copy 3; in the plain desktop the user then chooses 4. The next
    /// dedicated run must keep 4, not write the old copy back.
    #[tokio::test]
    async fn a_stale_copy_never_overwrites_the_users_newer_choice() {
        let f = fixture(Some(Value::U32(3))).await;
        assert!(take(&f.adapter, &f.files).await);
        f.set(Value::U32(4));
        assert!(take(&f.adapter, &f.files).await);
        assert_eq!(f.stored(), Some(Saved::Uint(4)));
        assert_eq!(f.value(), Some(Saved::Uint(0)));
        assert!(release(&f.adapter, &f.files).await);
        assert_eq!(f.value(), Some(Saved::Uint(4)));
    }

    /// An unset setting is unset again afterwards; one the user set to
    /// "do nothing" needs no change and no copy.
    #[tokio::test]
    async fn unset_and_nothing_settings() {
        let f = fixture(None).await;
        assert!(take(&f.adapter, &f.files).await);
        assert_eq!(f.value(), Some(Saved::Uint(0)));
        assert_eq!(f.stored(), Some(Saved::Absent));
        assert!(release(&f.adapter, &f.files).await);
        assert_eq!(f.value(), None);
        let f = fixture(Some(Value::I32(0))).await;
        assert!(take(&f.adapter, &f.files).await);
        assert_eq!(f.writes(), 0);
        assert!(!f.path().exists());
        assert!(release(&f.adapter, &f.files).await);
        assert_eq!(f.value(), Some(Saved::Int(0)));
        // An int setting stays an int.
        let f = fixture(Some(Value::I32(1))).await;
        assert!(take(&f.adapter, &f.files).await);
        assert_eq!(f.value(), Some(Saved::Int(0)));
        assert!(release(&f.adapter, &f.files).await);
        assert_eq!(f.value(), Some(Saved::Int(1)));
    }

    /// Writes a system default channel file with `value`.
    fn system_default(f: &Fixture, value: u32) {
        let path = &f.files.defaults[0];
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            format!(
                r#"<channel name="xfce4-power-manager" version="1.0"><property name="xfce4-power-manager" type="empty"><property name="power-button-action" type="uint" value="{value}"/></property></channel>"#
            ),
        )
        .unwrap();
    }

    /// Codex P2 on #516: xfconfd writes the user's channel file a few
    /// seconds after a change. A live value that differs from the system
    /// default (here none, so xfce4-power-manager's built-in "do nothing",
    /// and then a default of 2) is the user's even before the file names
    /// it, and is restored, not reset.
    #[tokio::test]
    async fn a_fresh_user_value_not_yet_in_the_file_is_kept() {
        let f = fixture_with(Some(Value::U32(3)), false).await;
        assert!(take(&f.adapter, &f.files).await);
        assert_eq!(f.stored(), Some(Saved::Uint(3)));
        assert!(release(&f.adapter, &f.files).await);
        assert_eq!(f.value(), Some(Saved::Uint(3)));
        let f = fixture_with(Some(Value::U32(3)), false).await;
        system_default(&f, 2);
        assert!(take(&f.adapter, &f.files).await);
        assert_eq!(f.stored(), Some(Saved::Uint(3)));
    }

    /// Review of #516: xfconf answers a system default for a property the
    /// user never set. It is recorded as "not set" and reset afterwards,
    /// never pinned as a user value.
    #[tokio::test]
    async fn a_system_default_is_reset_not_pinned() {
        let f = fixture_with(Some(Value::U32(3)), false).await;
        system_default(&f, 3);
        assert!(take(&f.adapter, &f.files).await);
        assert_eq!(f.stored(), Some(Saved::Absent));
        assert_eq!(f.value(), Some(Saved::Uint(0)));
        assert!(release(&f.adapter, &f.files).await);
        assert_eq!(f.value(), None);
        assert!(
            f.store
                .lock()
                .unwrap()
                .writes
                .last()
                .unwrap()
                .starts_with("reset ")
        );
    }

    /// Review of #516: at an XFCE login the system autostart entry gives a
    /// copy back that an unclean end of the dedicated session left, but
    /// never while the adapter owns its name and holds the key.
    #[tokio::test]
    async fn the_login_restore_gives_a_stale_copy_back_unless_the_adapter_runs() {
        let f = fixture(Some(Value::U32(3))).await;
        assert!(restore_at_login(&f.adapter, &f.files).await);
        assert_eq!(f.writes(), 0);
        assert!(take(&f.adapter, &f.files).await);
        // The adapter runs: the login restore leaves the key alone.
        let adapter = connect(&f.bus.1).await;
        adapter.request_name(ADAPTER).await.unwrap();
        assert!(restore_at_login(&f.adapter, &f.files).await);
        assert_eq!(f.value(), Some(Saved::Uint(0)));
        assert!(f.path().exists());
        // A power cut: the next login in the plain desktop restores it.
        adapter.release_name(ADAPTER).await.unwrap();
        adapter_gone(&f).await;
        assert!(restore_at_login(&f.adapter, &f.files).await);
        assert_eq!(f.value(), Some(Saved::Uint(3)));
        assert!(!f.path().exists());
    }
    async fn adapter_gone(f: &Fixture) {
        let bus = zbus::fdo::DBusProxy::new(&f.adapter).await.unwrap();
        for _ in 0..100 {
            if !bus
                .name_has_owner(ADAPTER.try_into().unwrap())
                .await
                .unwrap()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the adapter's name stayed");
    }

    /// Runs the restore entry's script as an XFCE login would, with a fake
    /// `xfconf-query` holding `value` (`None`: not set). Returns the value
    /// afterwards and the fake's calls. The script and the fake run under
    /// `SHELL`.
    fn run_restore(f: &Fixture, value: Option<&str>) -> (Option<String>, Vec<String>) {
        run_restore_with(f, value, false)
    }
    /// `read_fails`: xfconf lists the property but cannot read it.
    fn run_restore_with(
        f: &Fixture,
        value: Option<&str>,
        read_fails: bool,
    ) -> (Option<String>, Vec<String>) {
        let bin = f.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let fake = bin.join("xfconf-query");
        std::fs::write(
            &fake,
            format!(
                "#!/usr/bin/env {}\n{}",
                SHELL.with(|s| s.get()),
                r#"args="$*"
echo "$args" >> "$FAKE_XFCONF.calls"
case "$args" in
*" -l") [ -e "$FAKE_XFCONF" ] && echo /xfce4-power-manager/power-button-action; exit 0 ;;
*" -r") rm -f "$FAKE_XFCONF"; exit 0 ;;
*--set=*) echo "${args##*--set=}" > "$FAKE_XFCONF"; exit 0 ;;
esac
[ -n "$FAKE_READ_FAILS" ] && exit 1
[ -e "$FAKE_XFCONF" ] || exit 1
cat "$FAKE_XFCONF"
"#
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let state = f.root.join("xfconf");
        let _ = std::fs::remove_file(&state);
        let _ = std::fs::remove_file(state.with_extension("calls"));
        if let Some(value) = value {
            std::fs::write(&state, format!("{value}\n")).unwrap();
        }
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        let status = Command::new(SHELL.with(|s| s.get()))
            .arg(f.files.script())
            .env("PATH", path)
            .env("FAKE_XFCONF", &state)
            .env("FAKE_READ_FAILS", if read_fails { "1" } else { "" })
            .status()
            .unwrap();
        assert!(status.success());
        let calls = std::fs::read_to_string(state.with_extension("calls")).unwrap_or_default();
        (
            std::fs::read_to_string(&state)
                .ok()
                .map(|v| v.trim().to_owned()),
            calls.lines().map(str::to_owned).collect(),
        )
    }

    thread_local! {
        /// The shell for the restore script and the fake `xfconf-query`.
        static SHELL: std::cell::Cell<&'static str> = const { std::cell::Cell::new("sh") };
    }
    /// `sh`, and `bash` where it is installed: Arch's `/bin/sh` is bash,
    /// Debian's and Ubuntu's is dash, and both must run the script (review
    /// of #517: a bash-only difference turned CI red).
    fn shells() -> Vec<&'static str> {
        let mut shells = vec!["sh"];
        if Command::new("bash")
            .args(["-c", "true"])
            .status()
            .is_ok_and(|s| s.success())
        {
            shells.push("bash");
        }
        shells
    }
    /// Runs `test` once per shell.
    async fn in_each_shell<F: AsyncFn()>(test: F) {
        for shell in shells() {
            SHELL.with(|s| s.set(shell));
            test().await;
        }
        SHELL.with(|s| s.set("sh"));
    }

    /// Review of #516 (Codex P1): an unclean end of the dedicated session,
    /// then Arinn is removed before the next XFCE login. The per-user entry
    /// gives the user's value back with `xfconf-query` alone and removes
    /// itself and the copy; while Arinn is installed it leaves the value to
    /// Arinn.
    #[tokio::test]
    async fn the_restore_survives_removing_arinn() {
        in_each_shell(restore_survives_removing_arinn).await;
    }
    async fn restore_survives_removing_arinn() {
        let mut f = fixture(Some(Value::U32(3))).await;
        f.files.installed = f.root.join("usr/bin/arinn-platform-x11");
        assert!(take(&f.adapter, &f.files).await);
        // Arinn is installed: nothing changes and the entry stays.
        std::fs::create_dir_all(f.files.installed.parent().unwrap()).unwrap();
        std::fs::write(&f.files.installed, "").unwrap();
        assert_eq!(run_restore(&f, Some("0")), (Some("0".into()), vec![]));
        assert!(f.files.entry.exists() && f.path().exists());
        // Arinn is removed: the value comes back and everything goes.
        std::fs::remove_file(&f.files.installed).unwrap();
        let (value, calls) = run_restore(&f, Some("0"));
        assert_eq!(value.as_deref(), Some("3"));
        assert!(
            calls.iter().any(|c| c.ends_with(" -n -t uint --set=3")),
            "{calls:?}"
        );
        for path in [
            f.files.entry.clone(),
            f.files.script(),
            f.files.record.clone(),
            f.files.lock_path(),
        ] {
            assert!(!path.exists(), "{}", path.display());
        }
    }

    /// Review of #516: the script recognises the adapter binary that wrote
    /// it, not only `/usr/bin` (here the test binary, as a development build
    /// or an install elsewhere would be), so it never restores while that
    /// adapter may hold the key. A value it cannot read is left for the next
    /// login, with the entry and the copy.
    #[tokio::test]
    async fn the_removal_restore_knows_its_adapter_and_keeps_what_it_cannot_read() {
        in_each_shell(removal_restore_knows_its_adapter).await;
    }
    async fn removal_restore_knows_its_adapter() {
        let mut f = fixture(Some(Value::U32(3))).await;
        assert_eq!(f.files.installed, std::env::current_exe().unwrap());
        assert!(!f.files.installed.starts_with("/usr"));
        assert!(take(&f.adapter, &f.files).await);
        assert_eq!(run_restore(&f, Some("0")), (Some("0".into()), vec![]));
        assert!(f.files.entry.exists() && f.path().exists());
        f.files.installed = f.root.join("removed");
        assert!(take(&f.adapter, &f.files).await);
        let (value, calls) = run_restore_with(&f, Some("0"), true);
        assert_eq!(value.as_deref(), Some("0"));
        assert!(!calls.iter().any(|c| c.contains("--set")), "{calls:?}");
        assert!(f.files.entry.exists() && f.files.script().exists() && f.path().exists());
        // The next login reads it and restores.
        assert_eq!(run_restore(&f, Some("0")).0.as_deref(), Some("3"));
        assert!(!f.files.entry.exists() && !f.path().exists());
    }

    /// The same script keeps a value the user chose since, and resets a
    /// value the user never set; an int stays an int.
    #[tokio::test]
    async fn the_removal_restore_keeps_the_users_newer_choice() {
        in_each_shell(removal_restore_keeps_the_users_newer_choice).await;
    }
    async fn removal_restore_keeps_the_users_newer_choice() {
        let mut f = fixture(Some(Value::U32(3))).await;
        f.files.installed = f.root.join("missing");
        assert!(take(&f.adapter, &f.files).await);
        let (value, calls) = run_restore(&f, Some("1"));
        assert_eq!(value.as_deref(), Some("1"));
        assert!(
            !calls
                .iter()
                .any(|c| c.contains("--set") || c.ends_with(" -r"))
        );
        assert!(!f.files.entry.exists() && !f.path().exists());
        let mut f = fixture(None).await;
        f.files.installed = f.root.join("missing");
        assert!(take(&f.adapter, &f.files).await);
        assert_eq!(run_restore(&f, Some("0")).0, None);
        let mut f = fixture(Some(Value::I32(-1))).await;
        f.files.installed = f.root.join("missing");
        assert!(take(&f.adapter, &f.files).await);
        assert_eq!(run_restore(&f, Some("0")).0.as_deref(), Some("-1"));
    }

    /// A path the entry cannot quote safely, or an entry that cannot be
    /// written, changes nothing: without a restore that survives removal,
    /// the key stays the desktop's.
    #[tokio::test]
    async fn without_a_restore_entry_nothing_changes() {
        let mut f = fixture(Some(Value::U32(3))).await;
        f.files.entry = f
            .root
            .join("config/auto$start/arinn-power-button-restore.desktop");
        assert!(!take(&f.adapter, &f.files).await);
        assert_eq!(f.writes(), 0);
        assert!(!f.path().exists());
        let mut f = fixture(Some(Value::U32(3))).await;
        std::fs::create_dir_all(f.root.join("config")).unwrap();
        std::fs::write(f.root.join("config/autostart"), "a file, not a folder").unwrap();
        f.files.entry = f
            .root
            .join("config/autostart/arinn-power-button-restore.desktop");
        assert!(!take(&f.adapter, &f.files).await);
        assert_eq!(f.writes(), 0);
        assert!(!f.path().exists());
    }

    /// A value the user chose while Arinn ran is kept.
    #[tokio::test]
    async fn a_change_the_user_made_meanwhile_is_kept() {
        let f = fixture(Some(Value::U32(3))).await;
        assert!(take(&f.adapter, &f.files).await);
        f.set(Value::U32(1));
        assert!(release(&f.adapter, &f.files).await);
        assert_eq!(f.value(), Some(Saved::Uint(1)));
        assert!(!f.path().exists());
    }

    /// A locked setting, a value of another type or a missing xfconf: no
    /// change, and the key stays the desktop's.
    #[tokio::test]
    async fn changes_nothing_it_cannot_restore() {
        let f = fixture(Some(Value::U32(3))).await;
        f.store.lock().unwrap().locked = true;
        assert!(!take(&f.adapter, &f.files).await);
        assert_eq!(f.value(), Some(Saved::Uint(3)));
        assert!(!f.path().exists());
        let f = fixture(Some(Value::from("suspend"))).await;
        assert!(!take(&f.adapter, &f.files).await);
        assert_eq!(f.writes(), 0);
        assert!(!f.path().exists());
        // An unreadable copy is kept for the user and nothing changes.
        let f = fixture(Some(Value::U32(3))).await;
        std::fs::create_dir_all(f.path().parent().unwrap()).unwrap();
        std::fs::write(f.path(), "garbage").unwrap();
        assert!(!take(&f.adapter, &f.files).await);
        assert!(!release(&f.adapter, &f.files).await);
        assert!(!restore_at_login(&f.adapter, &f.files).await);
        assert_eq!(f.writes(), 0);
        assert!(f.path().exists());
        // No xfconf on the bus: nothing is started or written; a copy waits.
        let bus = bus();
        let adapter = connect(&bus.1).await;
        assert!(!take(&adapter, &f.files).await);
        std::fs::write(f.path(), Saved::Uint(3).encode()).unwrap();
        assert!(!release(&adapter, &f.files).await);
        assert!(f.path().exists());
    }
}
