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
use zbus::{Connection, export::futures_core::Stream, fdo::DBusProxy};

const SESSION: &str = "org.xfce.SessionManager";
const TARGET: &str = "arinn-ewmh-session.target";

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

async fn run() -> Result<()> {
    ensure!(
        env::var_os("DISPLAY").is_some_and(|value| !value.is_empty()),
        "an X11 display is required"
    );
    ensure!(
        env::var("XDG_SESSION_TYPE").map_or(true, |value| value.is_empty() || value == "x11"),
        "an X11 session is required"
    );
    let connection = Connection::session().await?;
    // Do not queue a second launcher: only the owner may stop the target.
    let reply = DBusProxy::new(&connection)
        .await?
        .request_name(
            "org.arinn.XfceSession".try_into()?,
            zbus::fdo::RequestNameFlags::DoNotQueue.into(),
        )
        .await?;
    ensure!(
        reply == zbus::fdo::RequestNameReply::PrimaryOwner,
        "the XFCE launcher is already running"
    );
    let bus = DBusProxy::new(&connection).await?;
    // Subscribe before reading the owner so logout during startup is observed.
    let mut owners = bus.receive_name_owner_changed().await?;
    let owner = bus.get_name_owner(SESSION.try_into()?).await?;
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
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
