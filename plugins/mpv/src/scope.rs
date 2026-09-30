// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One Arinn-owned mpv in its own transient user unit (ADR-0057).
//! The unit is not referenced by this process, so a provider crash does not
//! stop it, and no other mpv is signalled.

use crate::paths::is_public_tmp;
use arinn_protocol::validation::ErrorCode;
use std::{path::Path, time::Duration};
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedObjectPath, Value},
};

pub const UNIT: &str = "app-arinn-mpv.service";
const DEADLINE: Duration = Duration::from_secs(5);

/// argv after the executable. Key bindings, the OSC and watch-later are off.
pub fn arguments(socket: &Path) -> Vec<String> {
    vec![
        "--no-config".into(),
        "--load-scripts=no".into(),
        "--ytdl=no".into(),
        "--load-stats-overlay=no".into(),
        "--load-osd-console=no".into(),
        "--load-auto-profiles=no".into(),
        "--idle=yes".into(),
        "--keep-open=yes".into(),
        "--force-window=yes".into(),
        "--no-resume-playback".into(),
        "--save-position-on-quit=no".into(),
        "--input-default-bindings=no".into(),
        "--input-vo-keyboard=no".into(),
        "--input-media-keys=no".into(),
        "--no-input-terminal".into(),
        "--osc=no".into(),
        "--osd-level=0".into(),
        "--no-terminal".into(),
        format!("--input-ipc-server={}", socket.display()),
    ]
}

pub fn socket_is_private(path: &Path) -> bool {
    path.is_absolute() && !is_public_tmp(path)
}

/// Start `app-arinn-mpv.service` when it is not already active.
/// An active unit is left alone.
pub async fn ensure_unit(
    connection: &Connection,
    binary: &Path,
    socket: &Path,
) -> Result<(), ErrorCode> {
    if !socket_is_private(socket) || !binary.is_absolute() {
        return Err(ErrorCode::InvalidArgument);
    }
    let manager = Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .map_err(|_| ErrorCode::Unavailable)?;
    if unit_active(connection, &manager).await? {
        return Ok(());
    }
    let _: Result<(), zbus::Error> = manager.call("ResetFailedUnit", &(UNIT,)).await;
    let binary_text = binary.to_str().ok_or(ErrorCode::InvalidArgument)?;
    let mut argv = vec![binary_text.to_string()];
    argv.extend(arguments(socket));
    let command = vec![(
        binary_text.to_string(),
        argv,
        vec!["no-env-expand".to_string()],
    )];
    let properties: Vec<(&str, Value<'_>)> = vec![
        ("Description", "Arinn playback".into()),
        ("Type", "exec".into()),
        ("ExitType", "cgroup".into()),
        ("Slice", "app.slice".into()),
        ("CollectMode", "inactive-or-failed".into()),
        ("AddRef", false.into()),
        ("ExecStartEx", Value::new(command)),
        ("StandardOutput", "null".into()),
        ("StandardError", "null".into()),
    ];
    let auxiliaries: Vec<(&str, Vec<(&str, Value<'_>)>)> = Vec::new();
    let started: Result<OwnedObjectPath, ErrorCode> = tokio::time::timeout(DEADLINE, async {
        manager
            .call(
                "StartTransientUnit",
                &(UNIT, "fail", properties, auxiliaries),
            )
            .await
            .map_err(|_| ErrorCode::Unavailable)
    })
    .await
    .unwrap_or(Err(ErrorCode::Timeout));
    started.map(|_| ())
}

async fn unit_active(connection: &Connection, manager: &Proxy<'_>) -> Result<bool, ErrorCode> {
    let object: OwnedObjectPath = match manager.call("GetUnit", &(UNIT,)).await {
        Ok(path) => path,
        Err(_) => return Ok(false),
    };
    let unit = Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        object.as_str(),
        "org.freedesktop.systemd1.Unit",
    )
    .await
    .map_err(|_| ErrorCode::Unavailable)?;
    let state: String = unit
        .get_property("ActiveState")
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    Ok(state == "active" || state == "activating")
}
