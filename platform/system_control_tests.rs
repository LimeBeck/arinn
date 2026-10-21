// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! ADR-0108 platform actions against a fake logind on a private bus and a
//! fake `pactl`. Nothing here suspends, reboots or touches a sound server.
use super::*;
use crate::status_sources::tests::Bus;
use std::{
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::{Arc, Mutex},
};

#[test]
fn logind_answers_map_to_the_contract() {
    for (answer, availability) in [
        ("yes", Availability::Available),
        ("no", Availability::Denied),
        ("challenge", Availability::Denied),
        ("na", Availability::Unsupported),
        ("maybe", Availability::Unknown),
        ("", Availability::Unknown),
    ] {
        assert_eq!(availability_from_can(answer), availability, "{answer}");
    }
    // Only block inhibitors of sleep and shutdown count; delay inhibitors
    // only delay, and other locks do not hold a power action.
    assert_eq!(
        inhibitors_from_record("sleep:shutdown:idle", "Player\n", " Playing ", "block"),
        vec![
            Inhibitor {
                what: "sleep".into(),
                who: "Player".into(),
                why: "Playing".into()
            },
            Inhibitor {
                what: "shutdown".into(),
                who: "Player".into(),
                why: "Playing".into()
            },
        ]
    );
    assert!(inhibitors_from_record("sleep", "Player", "Saving", "delay").is_empty());
    assert!(inhibitors_from_record("handle-power-key:idle", "Desktop", "Keys", "block").is_empty());
    let long = inhibitors_from_record("sleep", &"w".repeat(500), &"y".repeat(500), "block");
    assert_eq!(
        (long[0].who.len(), long[0].why.len()),
        (MAX_WHO_BYTES, MAX_WHY_BYTES)
    );

    for (name, blocked, code) in [
        (
            "org.freedesktop.login1.BlockedByInhibitorLock",
            false,
            ErrorCode::Conflict,
        ),
        (
            "org.freedesktop.login1.OperationInProgress",
            false,
            ErrorCode::Conflict,
        ),
        (
            "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired",
            false,
            ErrorCode::PermissionDenied,
        ),
        (
            "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired",
            true,
            ErrorCode::Conflict,
        ),
        (
            "org.freedesktop.DBus.Error.AccessDenied",
            false,
            ErrorCode::PermissionDenied,
        ),
        (
            "org.freedesktop.DBus.Error.AccessDenied",
            true,
            ErrorCode::Conflict,
        ),
        (
            "org.freedesktop.login1.SleepVerbNotSupported",
            false,
            ErrorCode::Unsupported,
        ),
        (
            "org.freedesktop.DBus.Error.NotSupported",
            false,
            ErrorCode::Unsupported,
        ),
        (
            "org.freedesktop.DBus.Error.ServiceUnknown",
            false,
            ErrorCode::Unavailable,
        ),
        (
            "org.freedesktop.DBus.Error.NoReply",
            false,
            ErrorCode::Timeout,
        ),
        (
            "org.freedesktop.DBus.Error.Failed",
            false,
            ErrorCode::Internal,
        ),
    ] {
        assert_eq!(error_from_logind(name, blocked), code, "{name} {blocked}");
    }
}

#[test]
fn pactl_levels() {
    let stereo = "Volume: front-left: 32768 /  50% / -18.06 dB,   front-right: 36045 /  55% / -15.58 dB\n        balance 0.10\n";
    assert_eq!(level_from_pactl(stereo), Some(55));
    assert_eq!(
        level_from_pactl("Volume: mono: 65536 / 100% / 0.00 dB\n"),
        Some(100)
    );
    assert_eq!(
        level_from_pactl("Volume: mono: 655360 / 1000% / 60.00 dB\n"),
        Some(MAX_LEVEL)
    );
    assert_eq!(level_from_pactl(""), None);
    assert_eq!(level_from_pactl("Failure: No such entity\n"), None);
    assert_eq!(
        level_from_pactl("Volume: front-left: 1 / x% / 0 dB\n"),
        None
    );
}

pub(crate) type Record = (String, String, String, String, u32, u32);

#[derive(Default)]
pub(crate) struct FakeLogin {
    pub(crate) can: [String; 3],
    pub(crate) inhibitors: Vec<Record>,
    /// Refuse every action with this fdo error.
    pub(crate) refuse: Option<fn(String) -> zbus::fdo::Error>,
    pub(crate) calls: Vec<(String, bool)>,
    /// Answer power requests only after this long.
    pub(crate) hold: Duration,
    /// ListInhibitors fails.
    pub(crate) list_fails: bool,
}
pub(crate) type Shared = Arc<Mutex<FakeLogin>>;

struct Manager(Shared);
impl Manager {
    async fn act(&self, name: &str, interactive: bool) -> zbus::fdo::Result<()> {
        let (refuse, hold) = {
            let mut fake = self.0.lock().unwrap();
            fake.calls.push((name.into(), interactive));
            (fake.refuse, fake.hold)
        };
        tokio::time::sleep(hold).await;
        match refuse {
            Some(error) => Err(error("refused by the fixture".into())),
            None => Ok(()),
        }
    }
}
#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Manager {
    fn can_suspend(&self) -> String {
        self.0.lock().unwrap().can[0].clone()
    }
    fn can_reboot(&self) -> String {
        self.0.lock().unwrap().can[1].clone()
    }
    fn can_power_off(&self) -> String {
        self.0.lock().unwrap().can[2].clone()
    }
    async fn suspend(&self, interactive: bool) -> zbus::fdo::Result<()> {
        self.act("suspend", interactive).await
    }
    async fn reboot(&self, interactive: bool) -> zbus::fdo::Result<()> {
        self.act("reboot", interactive).await
    }
    async fn power_off(&self, interactive: bool) -> zbus::fdo::Result<()> {
        self.act("power-off", interactive).await
    }
    fn list_inhibitors(&self) -> zbus::fdo::Result<Vec<Record>> {
        let fake = self.0.lock().unwrap();
        if fake.list_fails {
            return Err(zbus::fdo::Error::Failed("fixture".into()));
        }
        Ok(fake.inhibitors.clone())
    }
}

pub(crate) fn record(what: &str, mode: &str) -> Record {
    (
        what.into(),
        "Fixture player".into(),
        "Playing".into(),
        mode.into(),
        1000,
        4242,
    )
}

/// A fake logind; the returned connection owns its name.
pub(crate) async fn logind(bus: &Bus, fake: FakeLogin) -> (Connection, Shared) {
    let shared = Arc::new(Mutex::new(fake));
    let service = bus.connect().await;
    service
        .object_server()
        .at("/org/freedesktop/login1", Manager(shared.clone()))
        .await
        .unwrap();
    service
        .request_name("org.freedesktop.login1")
        .await
        .unwrap();
    (service, shared)
}

pub(crate) fn control(bus: &Bus, pactl: PathBuf) -> Control {
    Control {
        system_bus: Some(bus.1.clone()),
        pactl,
    }
}

#[tokio::test]
async fn power_follows_logind_and_respects_block_inhibitors() {
    let system = Bus::new();
    let (_service, fake) = logind(
        &system,
        FakeLogin {
            can: ["yes".into(), "challenge".into(), "na".into()],
            inhibitors: vec![record("sleep", "block"), record("shutdown", "delay")],
            ..FakeLogin::default()
        },
    )
    .await;
    let control = control(&system, PathBuf::from("/nonexistent/pactl"));

    let state = control.power().await;
    state.validate().unwrap();
    assert_eq!(
        PowerAction::ALL.map(|a| state.availability(a)),
        [
            Availability::Inhibited,
            Availability::Denied,
            Availability::Unsupported
        ]
    );
    // The delay inhibitor is not listed; no user or process ID leaves.
    assert_eq!(
        state.inhibitors,
        vec![Inhibitor {
            what: "sleep".into(),
            who: "Fixture player".into(),
            why: "Playing".into()
        }]
    );

    // A block inhibitor refuses Suspend before logind is asked.
    assert_eq!(
        control.act(PowerAction::Suspend).await,
        Err(ErrorCode::Conflict)
    );
    assert!(fake.lock().unwrap().calls.is_empty());

    // A delay inhibitor does not refuse; logind is never asked to prompt.
    assert_eq!(control.act(PowerAction::Reboot).await, Ok(()));
    assert_eq!(
        fake.lock().unwrap().calls,
        vec![("reboot".to_string(), false)]
    );

    // logind's refusals become typed errors.
    fake.lock().unwrap().refuse = Some(zbus::fdo::Error::InteractiveAuthorizationRequired);
    assert_eq!(
        control.act(PowerAction::PowerOff).await,
        Err(ErrorCode::PermissionDenied)
    );
    fake.lock().unwrap().refuse = Some(zbus::fdo::Error::AccessDenied);
    assert_eq!(
        control.act(PowerAction::Reboot).await,
        Err(ErrorCode::PermissionDenied)
    );
    fake.lock().unwrap().refuse = Some(zbus::fdo::Error::NotSupported);
    assert_eq!(
        control.act(PowerAction::PowerOff).await,
        Err(ErrorCode::Unsupported)
    );
    fake.lock().unwrap().refuse = Some(zbus::fdo::Error::Failed);
    assert_eq!(
        control.act(PowerAction::PowerOff).await,
        Err(ErrorCode::Internal)
    );

    // Without inhibitors every answer maps directly.
    {
        let mut fake = fake.lock().unwrap();
        fake.inhibitors.clear();
        fake.refuse = None;
        fake.can = ["yes".into(), "yes".into(), "no".into()];
    }
    let state = control.power().await;
    state.validate().unwrap();
    assert_eq!(
        PowerAction::ALL.map(|a| state.availability(a)),
        [
            Availability::Available,
            Availability::Available,
            Availability::Denied
        ]
    );
    assert_eq!(control.act(PowerAction::Suspend).await, Ok(()));
    assert_eq!(
        fake.lock().unwrap().calls.last(),
        Some(&("suspend".to_string(), false))
    );
}

#[tokio::test]
async fn many_inhibitors_are_bounded_and_name_each_lock() {
    let system = Bus::new();
    // Hundreds of irrelevant records first: the block that matters comes
    // last and is still found.
    let mut inhibitors: Vec<Record> = (0..300)
        .map(|i| record(if i % 2 == 0 { "idle" } else { "sleep" }, "delay"))
        .collect();
    inhibitors.extend((0..40).map(|i| {
        let mut holder = record("sleep", "block");
        holder.1 = format!("Holder {i:02}");
        holder
    }));
    inhibitors.push(record("shutdown", "block"));
    let (_service, _fake) = logind(
        &system,
        FakeLogin {
            can: ["yes".into(), "yes".into(), "yes".into()],
            inhibitors,
            ..FakeLogin::default()
        },
    )
    .await;
    let state = control(&system, PathBuf::from("/nonexistent/pactl"))
        .power()
        .await;
    state.validate().unwrap();
    assert_eq!(state.inhibitors.len(), MAX_INHIBITORS);
    assert_eq!(state.inhibitors_of(PowerAction::PowerOff).count(), 1);
    assert_eq!(
        state.availability(PowerAction::Reboot),
        Availability::Inhibited
    );
}

/// Review of #470: a single block far behind hundreds of irrelevant records
/// still refuses the action, and logind is never asked.
#[tokio::test]
async fn one_block_behind_many_records_still_refuses() {
    let system = Bus::new();
    let mut inhibitors: Vec<Record> = (0..300).map(|_| record("idle", "block")).collect();
    inhibitors.push(record("sleep", "block"));
    let (_service, fake) = logind(
        &system,
        FakeLogin {
            can: ["yes".into(), "yes".into(), "yes".into()],
            inhibitors,
            ..FakeLogin::default()
        },
    )
    .await;
    let control = control(&system, PathBuf::from("/nonexistent/pactl"));
    let state = control.power().await;
    state.validate().unwrap();
    assert_eq!(
        state.availability(PowerAction::Suspend),
        Availability::Inhibited
    );
    assert_eq!(
        control.act(PowerAction::Suspend).await,
        Err(ErrorCode::Conflict)
    );
    assert!(fake.lock().unwrap().calls.is_empty());
}

#[tokio::test]
async fn an_unreadable_inhibitor_list_refuses() {
    let system = Bus::new();
    let (_service, fake) = logind(
        &system,
        FakeLogin {
            can: ["yes".into(), "yes".into(), "yes".into()],
            list_fails: true,
            ..FakeLogin::default()
        },
    )
    .await;
    let control = control(&system, PathBuf::from("/nonexistent/pactl"));
    // Fail closed: nothing is called available, and nothing is requested.
    assert_eq!(control.power().await, PowerState::unknown());
    for action in PowerAction::ALL {
        assert_eq!(control.act(action).await, Err(ErrorCode::Unavailable));
    }
    assert!(fake.lock().unwrap().calls.is_empty());
    // Readable again: the request goes through.
    fake.lock().unwrap().list_fails = false;
    assert_eq!(control.act(PowerAction::Suspend).await, Ok(()));
}

#[tokio::test]
async fn without_logind_power_is_unknown_and_unavailable() {
    let system = Bus::new();
    let control = control(&system, PathBuf::from("/nonexistent/pactl"));
    let state = control.power().await;
    state.validate().unwrap();
    assert_eq!(state, PowerState::unknown());
    assert_eq!(
        control.act(PowerAction::Suspend).await,
        Err(ErrorCode::Unavailable)
    );
    // No system bus at all.
    let control = Control {
        system_bus: Some("unix:path=/nonexistent/arinn-test-bus".into()),
        pactl: PathBuf::from("/nonexistent/pactl"),
    };
    assert_eq!(control.power().await, PowerState::unknown());
    assert_eq!(
        control.act(PowerAction::PowerOff).await,
        Err(ErrorCode::Unavailable)
    );
}

/// A fake `pactl` with a stereo default sink: `mute` holds yes/no and
/// `volume` the two channel levels. It answers only in the C locale, logs
/// every call to `calls` and fails while `no-server` exists.
pub(crate) fn fake_pactl(folder: &Path) -> PathBuf {
    std::fs::create_dir_all(folder).unwrap();
    let script = folder.join("pactl");
    std::fs::write(
        &script,
        r#"#!/bin/sh
dir=$(dirname "$0")
[ -e "$dir/no-server" ] && exit 1
echo "$*" >> "$dir/calls"
case "$1" in
get-sink-mute)
  [ -e "$dir/fail-reads" ] && exit 1
  [ "$LC_ALL" = C ] || { echo 'Mute: да'; exit 0; }
  echo "Mute: $(cat "$dir/mute")" ;;
get-sink-volume)
  [ -e "$dir/fail-reads" ] && exit 1
  read l r < "$dir/volume"
  echo "Volume: front-left: 1 /  $l% / 0.00 dB,   front-right: 1 /  $r% / 0.00 dB"
  echo "        balance 0.00" ;;
set-sink-mute)
  [ "$2" = @DEFAULT_SINK@ ] || exit 1
  case "$3" in
  1) echo yes > "$dir/mute" ;;
  0) echo no > "$dir/mute" ;;
  toggle) if [ "$(cat "$dir/mute")" = yes ]; then echo no > "$dir/mute"; else echo yes > "$dir/mute"; fi ;;
  *) exit 1 ;;
  esac ;;
set-sink-volume)
  # As pactl: options end at the command, the next argument is the sink
  # and the one after it the volume.
  [ $# -eq 3 ] && [ "$2" = @DEFAULT_SINK@ ] || exit 1
  n=${3%\%}
  case "$n" in
  +*) n=${n#+} ;;
  -*) n=$((0 - ${n#-})) ;;
  *) exit 1 ;;
  esac
  read l r < "$dir/volume"
  l=$((l + n)); r=$((r + n))
  [ "$l" -lt 0 ] && l=0
  [ "$r" -lt 0 ] && r=0
  echo "$l $r" > "$dir/volume" ;;
*) exit 2 ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(folder.join("mute"), "no\n").unwrap();
    std::fs::write(folder.join("volume"), "40 50\n").unwrap();
    script
}
pub(crate) fn set_volume(folder: &Path, left: u32, right: u32) {
    std::fs::write(folder.join("volume"), format!("{left} {right}\n")).unwrap();
}

fn volume(audio: &str, level: u32) -> Volume {
    Volume {
        audio: audio.into(),
        level,
    }
}

#[tokio::test]
async fn volume_reads_and_changes_the_default_sink() {
    let folder = std::env::temp_dir().join(format!("arinn-control-{}", uuid::Uuid::new_v4()));
    let pactl = fake_pactl(&folder);
    let control = Control {
        system_bus: Some("unix:path=/nonexistent/arinn-test-bus".into()),
        pactl,
    };
    assert_eq!(control.volume().await, volume("unmuted", 50));
    assert_eq!(control.set_muted(true).await, Ok(volume("muted", 50)));
    assert_eq!(control.toggle_muted().await, Ok(volume("unmuted", 50)));
    assert_eq!(control.toggle_muted().await, Ok(volume("muted", 50)));
    assert_eq!(control.set_muted(false).await, Ok(volume("unmuted", 50)));

    // Steps move every channel by the same amount, so the balance stays,
    // and the loudest channel stops at 100 %.
    assert_eq!(control.step_volume(1).await, Ok(volume("unmuted", 55)));
    assert_eq!(
        std::fs::read_to_string(folder.join("volume")).unwrap(),
        "45 55\n"
    );
    assert_eq!(control.step_volume(20).await, Ok(volume("unmuted", 100)));
    assert_eq!(control.step_volume(-3).await, Ok(volume("unmuted", 85)));
    // A level above 100 % set elsewhere is kept when stepping up.
    set_volume(&folder, 120, 120);
    let calls = std::fs::read_to_string(folder.join("calls")).unwrap();
    assert_eq!(control.step_volume(1).await, Ok(volume("unmuted", 120)));
    let after = std::fs::read_to_string(folder.join("calls")).unwrap();
    assert!(
        !after[calls.len()..].contains("set-sink-volume"),
        "no change requested: {after}"
    );
    assert_eq!(control.step_volume(-1).await, Ok(volume("unmuted", 115)));
    set_volume(&folder, 3, 3);
    assert_eq!(control.step_volume(-20).await, Ok(volume("unmuted", 0)));
    // Stepping does not unmute; mute is its own action.
    control.set_muted(true).await.unwrap();
    assert_eq!(control.step_volume(2).await, Ok(volume("muted", 10)));

    // A change that succeeds but cannot be read back is an error, not an
    // unknown state reported as done.
    std::fs::write(folder.join("fail-reads"), "").unwrap();
    assert_eq!(control.set_muted(false).await, Err(ErrorCode::Unavailable));
    assert_eq!(control.toggle_muted().await, Err(ErrorCode::Unavailable));
    assert_eq!(control.volume().await, Volume::unknown());
    std::fs::remove_file(folder.join("fail-reads")).unwrap();

    // A server that goes away: reads are unknown, changes unavailable.
    std::fs::write(folder.join("no-server"), "").unwrap();
    assert_eq!(control.volume().await, Volume::unknown());
    assert_eq!(control.set_muted(false).await, Err(ErrorCode::Unavailable));
    assert_eq!(control.toggle_muted().await, Err(ErrorCode::Unavailable));
    assert_eq!(control.step_volume(1).await, Err(ErrorCode::Unavailable));
    let _ = std::fs::remove_dir_all(&folder);

    // No pactl at all.
    let control = Control {
        system_bus: None,
        pactl: PathBuf::from("/nonexistent/pactl"),
    };
    assert_eq!(control.volume().await, Volume::unknown());
    assert_eq!(control.toggle_muted().await, Err(ErrorCode::Unavailable));
}
