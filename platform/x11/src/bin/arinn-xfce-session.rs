// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Opt-in XDG autostart bridge; the XFCE session manager owns its lifetime.
use anyhow::{Result, bail, ensure};
use std::{
    env,
    future::poll_fn,
    path::PathBuf,
    pin::Pin,
    process::{Command, Stdio},
    time::Duration,
};
use zbus::{
    Connection,
    export::futures_core::Stream,
    fdo::{DBusProxy, RequestNameFlags, RequestNameReply},
};

const SESSION: &str = "org.xfce.SessionManager";
const TARGET: &str = "arinn-ewmh-session.target";
/// Only the owner of this name may stop the target.
const LAUNCHER: &str = "org.arinn.XfceSession";
/// After a fast re-login, the previous login's launcher can still own the
/// name while it stops the target (#483). Its cleanup is two `systemctl`
/// calls of at most 30 s each, so it releases the name within this time.
const HANDOVER: Duration = Duration::from_secs(60);

struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn systemctl(args: &[&str]) -> Result<bool> {
    let mut child = Child(
        Command::new("systemctl")
            .args(["--user", "--no-ask-password"])
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(status) = child.0.try_wait()? {
                return Ok(status.success());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await?
}

async fn start() -> Result<()> {
    ensure!(
        systemctl(&["import-environment", "DISPLAY"]).await?,
        "display import failed"
    );
    if env::var_os("XAUTHORITY").is_some_and(|value| !value.is_empty()) {
        ensure!(
            systemctl(&["import-environment", "XAUTHORITY"]).await?,
            "authority import failed"
        );
    } else {
        // LightDM can use Xlib's default authority file; remove stale manager data.
        ensure!(
            systemctl(&["unset-environment", "XAUTHORITY"]).await?,
            "authority reset failed"
        );
    }
    ensure!(
        systemctl(&[
            "set-environment",
            "XDG_SESSION_TYPE=x11",
            "ARINN_PLATFORM_BACKEND=ewmh",
            "ARINN_SESSION_BACKEND=x11"
        ])
        .await?,
        "backend selection failed"
    );
    // ADR-0111: the dedicated-session marker belongs to this login. The
    // dedicated session's wrapper puts it into the desktop's own
    // environment; a stock XFCE login lacks it, so a marker a killed
    // wrapper left in a lingering user manager is cleared here, and the
    // ordinary desktop keeps its power key.
    let marked = if env::var("ARINN_SESSION_ENTRY").as_deref() == Ok("xfce") {
        systemctl(&["set-environment", "ARINN_SESSION_ENTRY=xfce"]).await?
    } else {
        systemctl(&["unset-environment", "ARINN_SESSION_ENTRY"]).await?
    };
    ensure!(marked, "session marker update failed");
    // ADR-0111: the login's own state and config folders, for the XFCE
    // power button's saved copy and restore entry. The system login entry
    // runs in this login and uses these; the adapter runs in the user
    // manager, which may disagree, and prefers these Arinn-only values.
    // No XDG_* value changes for other units.
    let folders = match (
        login_folder("XDG_STATE_HOME", ".local/state"),
        login_folder("XDG_CONFIG_HOME", ".config"),
    ) {
        (Some(state), Some(config)) => {
            systemctl(&[
                "set-environment",
                &format!("ARINN_LOGIN_STATE_HOME={state}"),
                &format!("ARINN_LOGIN_CONFIG_HOME={config}"),
            ])
            .await?
        }
        // Unknown: the dedicated session then takes no power key.
        _ => {
            systemctl(&[
                "unset-environment",
                "ARINN_LOGIN_STATE_HOME",
                "ARINN_LOGIN_CONFIG_HOME",
            ])
            .await?
        }
    };
    ensure!(folders, "login folder update failed");
    // ADR-0111: this login's logind session, so the dedicated session can
    // tell whether the user has another graphical session sharing xfconfd.
    let session = match env::var("XDG_SESSION_ID")
        .ok()
        .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()))
    {
        Some(id) => {
            systemctl(&["set-environment", &format!("ARINN_LOGIN_SESSION_ID={id}")]).await?
        }
        // Unknown: the dedicated session then takes no power key.
        None => systemctl(&["unset-environment", "ARINN_LOGIN_SESSION_ID"]).await?,
    };
    ensure!(session, "login session update failed");
    ensure!(systemctl(&["start", TARGET]).await?, "session start failed");
    Ok(())
}

/// `variable` of this login as an absolute path, or the XDG default below
/// `$HOME`; `None` when neither is known or it is not UTF-8.
fn login_folder(variable: &str, fallback: &str) -> Option<String> {
    let absolute =
        |value: std::ffi::OsString| Some(PathBuf::from(value)).filter(|p| p.is_absolute());
    env::var_os(variable)
        .and_then(absolute)
        .or_else(|| {
            env::var_os("HOME")
                .and_then(absolute)
                .map(|home| home.join(fallback))
        })
        .and_then(|path| path.into_os_string().into_string().ok())
}

/// Owns [`LAUNCHER`], never queued: a queued launcher could become the owner
/// long after it gave up. While another launcher owns the name, waits up to
/// `wait` for it to change hands and asks again (#483).
async fn acquire(bus: &DBusProxy<'_>, wait: Duration) -> Result<()> {
    let deadline = tokio::time::Instant::now() + wait;
    // Subscribe before asking, so a release right after a refusal is seen.
    let mut changes = bus
        .receive_name_owner_changed_with_args(&[(0, LAUNCHER)])
        .await?;
    loop {
        let reply = bus
            .request_name(LAUNCHER.try_into()?, RequestNameFlags::DoNotQueue.into())
            .await?;
        if reply == RequestNameReply::PrimaryOwner {
            return Ok(());
        }
        let changed =
            tokio::time::timeout_at(deadline, poll_fn(|cx| Pin::new(&mut changes).poll_next(cx)))
                .await;
        match changed {
            Err(_) => bail!("the XFCE launcher is already running"),
            Ok(None) => bail!("session bus disconnected"),
            Ok(Some(_)) => {}
        }
    }
}

async fn run() -> Result<()> {
    ensure!(
        env::var_os("DISPLAY").is_some_and(|value| !value.is_empty()),
        "an X11 display is required"
    );
    ensure!(
        env::var("XDG_SESSION_TYPE").map_or(true, |value| value.is_empty() || value == "x11"),
        "an X11 session is required"
    );
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let connection = Connection::session().await?;
    let bus = DBusProxy::new(&connection).await?;
    // Subscribe before reading the owner so logout during startup is observed.
    let mut owners = bus.receive_name_owner_changed().await?;
    let owner = bus.get_name_owner(SESSION.try_into()?).await?;
    let lifetime = async {
        loop {
            let event = poll_fn(|cx| Pin::new(&mut owners).poll_next(cx)).await;
            let Some(event) = event else {
                bail!("session bus disconnected");
            };
            let args = event.args()?;
            if args.name.as_str() == SESSION && args.old_owner.as_ref() == Some(&owner) {
                return Ok(());
            }
        }
    };
    tokio::pin!(lifetime);
    // Nothing in the user manager is touched before the name is owned, so a
    // launcher that gives up or is stopped while it waits needs no cleanup.
    // Its own login ending ends the wait too: it must never take over a
    // later login with this login's display and session (#483).
    tokio::select! {
        result = acquire(&bus, HANDOVER) => result?,
        _ = &mut lifetime => bail!("the XFCE session ended while waiting for the launcher name"),
        _ = term.recv() => bail!("stopped while waiting for the launcher name"),
        _ = interrupt.recv() => bail!("stopped while waiting for the launcher name"),
    }
    // No supported concurrent graphical sessions share this user manager.
    for unit in [
        "arinn-session.target",
        TARGET,
        "arinnd.service",
        "arinn-shell.service",
        "arinn-ewmh.service",
    ] {
        ensure!(
            !systemctl(&["is-active", "--quiet", unit]).await?,
            "an Arinn session is already running"
        );
    }
    let mut startup = Box::pin(start());
    let outcome = tokio::select! {
        result = &mut startup => {
            match result {
                Err(error) => Err(error),
                Ok(()) => tokio::select! {
                    result = &mut lifetime => result,
                    _ = term.recv() => Ok(()),
                    _ = interrupt.recv() => Ok(()),
                }
            }
        },
        result = &mut lifetime => result,
        _ = term.recv() => Ok(()),
        _ = interrupt.recv() => Ok(()),
    };
    drop(startup);
    // A failed or interrupted start can already have activated some services.
    let stopped = systemctl(&["stop", TARGET]).await;
    let cleared = systemctl(&[
        "unset-environment",
        "DISPLAY",
        "XAUTHORITY",
        "XDG_SESSION_TYPE",
        "ARINN_PLATFORM_BACKEND",
        "ARINN_SESSION_BACKEND",
        "ARINN_LOGIN_STATE_HOME",
        "ARINN_LOGIN_CONFIG_HOME",
        "ARINN_LOGIN_SESSION_ID",
    ])
    .await;
    outcome?;
    ensure!(stopped? && cleared?, "session cleanup failed");
    Ok(())
}

#[tokio::main]
async fn main() {
    if run().await.is_err() {
        // Fixed diagnostic: never disclose DISPLAY, authority paths or bus errors.
        eprintln!("arinn-xfce-session: session integration failed");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};

    struct Bus(std::process::Child, String);
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
    async fn connect(bus: &Bus) -> Connection {
        zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }

    /// #483: the previous login's launcher releases the name while the new
    /// one waits; a name that stays taken ends the wait after its bound.
    #[tokio::test]
    async fn the_launcher_name_is_taken_over_within_the_bound() {
        let bus = bus();
        let previous = connect(&bus).await;
        previous.request_name(LAUNCHER).await.unwrap();
        let next = connect(&bus).await;
        let proxy = DBusProxy::new(&next).await.unwrap();

        let started = tokio::time::Instant::now();
        let refused = acquire(&proxy, Duration::from_millis(300)).await;
        assert!(refused.is_err());
        assert!(started.elapsed() >= Duration::from_millis(300));

        let release = async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            previous.release_name(LAUNCHER).await.unwrap();
        };
        let (taken, ()) = tokio::join!(acquire(&proxy, Duration::from_secs(10)), release);
        taken.unwrap();
        let owner = proxy
            .get_name_owner(LAUNCHER.try_into().unwrap())
            .await
            .unwrap();
        assert_eq!(owner.as_str(), next.unique_name().unwrap().as_str());
    }
}
