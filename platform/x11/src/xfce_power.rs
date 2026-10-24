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
//! (`arinn-platform-x11 --release-power-key`) and on the next start of the
//! adapter in any session. A value the user set in the meantime is kept.
//! Without xfconf nothing is done.
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
}

/// Whether this adapter runs in the dedicated Arinn XFCE session.
pub fn dedicated(entry: Option<&str>) -> bool {
    entry == Some("xfce")
}

/// `$XDG_STATE_HOME/arinn/xfce-power-button`, or under `~/.local/state`.
pub fn record(home: Option<&std::ffi::OsStr>, state: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    state
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            home.map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|p| p.join(".local/state"))
        })
        .map(|root| root.join("arinn/xfce-power-button"))
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
    let folder = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    std::fs::create_dir_all(folder)?;
    let partial = path.with_extension("partial");
    let mut file = std::fs::File::create(&partial)?;
    file.write_all(saved.encode().as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&partial, path)?;
    std::fs::File::open(folder)?.sync_all()
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

/// The adapter starts: in the dedicated Arinn XFCE session (`entry`) take
/// the power button; in any other session only give back a copy an earlier
/// dedicated run left. Returns whether the power button was taken.
pub async fn start(c: &Connection, path: &Path, entry: Option<&str>) -> bool {
    if dedicated(entry) {
        take(c, path).await
    } else {
        release(c, path).await;
        false
    }
}

/// Sets the power button action to "do nothing" for the time Arinn runs.
/// Returns whether xfce4-power-manager now leaves the key alone, so the
/// adapter may read it from raw events. Never changes the setting without a
/// stored copy of the user's value.
pub async fn take(c: &Connection, path: &Path) -> bool {
    if !available(c).await {
        return false;
    }
    let original = match load(path) {
        // A copy left by an earlier run is the user's value; the setting
        // still holds Arinn's "do nothing".
        Ok(Some(saved)) => saved,
        Ok(None) => match current(c).await {
            Ok(Some(saved)) if saved.is_nothing() => return true,
            Ok(Some(saved)) => {
                if store(path, saved).is_err() {
                    log("cannot keep the XFCE power button setting; left unchanged");
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
        },
        Err(_) => {
            log("unreadable stored XFCE power button setting; left unchanged");
            return false;
        }
    };
    let nothing = match original {
        Saved::Int(_) => Saved::Int(NOTHING as i32),
        _ => Saved::Uint(NOTHING),
    };
    if write_setting(c, nothing).await.is_err() {
        log("xfconf refused the XFCE power button setting (locked?); left unchanged");
        // The copy stays until a release finds the setting as it was.
        let _ = release(c, path).await;
        return false;
    }
    true
}

/// Gives the user's setting back and forgets the stored copy. A setting
/// that no longer holds Arinn's "do nothing" was changed by the user in the
/// meantime and is kept. Returns false when a copy remains to restore later.
pub async fn release(c: &Connection, path: &Path) -> bool {
    let saved = match load(path) {
        Ok(Some(saved)) => saved,
        Ok(None) => return true,
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
    std::fs::remove_file(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::{Arc, Mutex},
    };

    struct Bus(Child, String);
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

    struct Fixture {
        _bus: Bus,
        _service: Connection,
        adapter: Connection,
        store: Shared,
        path: PathBuf,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.path.parent().unwrap().parent().unwrap());
        }
    }
    async fn fixture(value: Option<Value<'static>>) -> Fixture {
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
        let path = record(None, Some(root.as_os_str())).unwrap();
        Fixture {
            adapter: connect(&bus.1).await,
            _bus: bus,
            _service: service,
            store,
            path,
        }
    }
    impl Fixture {
        fn value(&self) -> Option<Saved> {
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
        fn writes(&self) -> usize {
            self.store.lock().unwrap().writes.len()
        }
        fn stored(&self) -> Option<Saved> {
            load(&self.path).unwrap()
        }
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
        assert_eq!(
            record(Some("/home/u".as_ref()), None),
            Some(PathBuf::from(
                "/home/u/.local/state/arinn/xfce-power-button"
            ))
        );
        assert_eq!(
            record(Some("/home/u".as_ref()), Some("relative".as_ref())),
            Some(PathBuf::from(
                "/home/u/.local/state/arinn/xfce-power-button"
            ))
        );
        assert_eq!(record(None, None), None);
    }

    /// Owner decision of 2026-10-24: "do nothing" while Arinn runs, the
    /// user's value back when it stops; the copy exists before the change.
    #[tokio::test]
    async fn sets_nothing_while_arinn_runs_and_restores_the_users_value() {
        let f = fixture(Some(Value::U32(3))).await;
        assert!(take(&f.adapter, &f.path).await);
        assert_eq!(f.value(), Some(Saved::Uint(0)));
        assert_eq!(f.stored(), Some(Saved::Uint(3)));
        assert!(release(&f.adapter, &f.path).await);
        assert_eq!(f.value(), Some(Saved::Uint(3)));
        assert!(!f.path.exists());
        // A second release has nothing left to do.
        let writes = f.writes();
        assert!(release(&f.adapter, &f.path).await);
        assert_eq!(f.writes(), writes);
    }

    /// A crash or power loss leaves the copy; the next start keeps it as the
    /// user's value instead of reading Arinn's "do nothing", and the next
    /// release restores it.
    #[tokio::test]
    async fn a_stale_copy_survives_a_crash_and_is_never_overwritten() {
        let f = fixture(Some(Value::U32(4))).await;
        assert!(take(&f.adapter, &f.path).await);
        // Crash: no release. The next run takes over again.
        assert!(take(&f.adapter, &f.path).await);
        assert_eq!(f.stored(), Some(Saved::Uint(4)));
        assert_eq!(f.value(), Some(Saved::Uint(0)));
        assert!(release(&f.adapter, &f.path).await);
        assert_eq!(f.value(), Some(Saved::Uint(4)));
    }

    /// An unset setting is unset again afterwards; one the user set to
    /// "do nothing" needs no change and no copy.
    #[tokio::test]
    async fn unset_and_nothing_settings() {
        let f = fixture(None).await;
        assert!(take(&f.adapter, &f.path).await);
        assert_eq!(f.value(), Some(Saved::Uint(0)));
        assert_eq!(f.stored(), Some(Saved::Absent));
        assert!(release(&f.adapter, &f.path).await);
        assert_eq!(f.value(), None);
        let f = fixture(Some(Value::I32(0))).await;
        assert!(take(&f.adapter, &f.path).await);
        assert_eq!(f.writes(), 0);
        assert!(!f.path.exists());
        assert!(release(&f.adapter, &f.path).await);
        assert_eq!(f.value(), Some(Saved::Int(0)));
        // An int setting stays an int.
        let f = fixture(Some(Value::I32(1))).await;
        assert!(take(&f.adapter, &f.path).await);
        assert_eq!(f.value(), Some(Saved::Int(0)));
        assert!(release(&f.adapter, &f.path).await);
        assert_eq!(f.value(), Some(Saved::Int(1)));
    }

    /// Owner decision of 2026-10-24: the ordinary XFCE desktop (opt-in
    /// autostart) is never changed, but a copy a crashed dedicated session
    /// left is given back there too.
    #[tokio::test]
    async fn only_the_dedicated_session_takes_the_power_button() {
        let f = fixture(Some(Value::U32(3))).await;
        for entry in [None, Some("plasma")] {
            assert!(!start(&f.adapter, &f.path, entry).await);
        }
        assert_eq!(f.writes(), 0);
        assert_eq!(f.value(), Some(Saved::Uint(3)));
        assert!(start(&f.adapter, &f.path, Some("xfce")).await);
        assert_eq!(f.value(), Some(Saved::Uint(0)));
        // Crash, then a login to the ordinary desktop with autostart.
        assert!(!start(&f.adapter, &f.path, None).await);
        assert_eq!(f.value(), Some(Saved::Uint(3)));
        assert!(!f.path.exists());
    }

    /// A value the user chose while Arinn ran is kept.
    #[tokio::test]
    async fn a_change_the_user_made_meanwhile_is_kept() {
        let f = fixture(Some(Value::U32(3))).await;
        assert!(take(&f.adapter, &f.path).await);
        f.store.lock().unwrap().properties.insert(
            (CHANNEL.into(), PROPERTY.into()),
            Value::U32(1).try_into().unwrap(),
        );
        assert!(release(&f.adapter, &f.path).await);
        assert_eq!(f.value(), Some(Saved::Uint(1)));
        assert!(!f.path.exists());
    }

    /// A locked setting, a value of another type or a missing xfconf: no
    /// change, and the key stays the desktop's.
    #[tokio::test]
    async fn changes_nothing_it_cannot_restore() {
        let f = fixture(Some(Value::U32(3))).await;
        f.store.lock().unwrap().locked = true;
        assert!(!take(&f.adapter, &f.path).await);
        assert_eq!(f.value(), Some(Saved::Uint(3)));
        assert!(!f.path.exists());
        let f = fixture(Some(Value::from("suspend"))).await;
        assert!(!take(&f.adapter, &f.path).await);
        assert_eq!(f.writes(), 0);
        assert!(!f.path.exists());
        // An unreadable copy is kept for the user and nothing changes.
        let f = fixture(Some(Value::U32(3))).await;
        std::fs::create_dir_all(f.path.parent().unwrap()).unwrap();
        std::fs::write(&f.path, "garbage").unwrap();
        assert!(!take(&f.adapter, &f.path).await);
        assert!(!release(&f.adapter, &f.path).await);
        assert_eq!(f.writes(), 0);
        assert!(f.path.exists());
        // No xfconf on the bus: nothing is started or written; a copy waits.
        let bus = bus();
        let adapter = connect(&bus.1).await;
        assert!(!take(&adapter, &f.path).await);
        std::fs::write(&f.path, Saved::Uint(3).encode()).unwrap();
        assert!(!release(&adapter, &f.path).await);
        assert!(f.path.exists());
    }
}
