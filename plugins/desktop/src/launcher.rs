// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Delegate desktop-entry interpretation to GIO in a distinct application unit.
use arinn_protocol::validation::ErrorCode;
use std::{path::Path, time::Duration};
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedObjectPath, Value},
};

pub async fn launch(path: &Path) -> Result<(), ErrorCode> {
    // A dedicated connection owns the temporary AddRef reference. Cancellation
    // or provider death releases the reference, never stops the application unit.
    let connection = Connection::session()
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    launch_on(&connection, path).await
}
pub async fn launch_on(connection: &Connection, path: &Path) -> Result<(), ErrorCode> {
    let path = path
        .to_str()
        .filter(|_| path.is_absolute())
        .ok_or(ErrorCode::InvalidArgument)?;
    let unit = format!("app-arinn-{}.service", uuid::Uuid::new_v4().simple());
    let manager = Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .map_err(|_| ErrorCode::Unavailable)?;
    let command = vec![(
        "/usr/bin/gio",
        vec!["/usr/bin/gio", "launch", path],
        vec!["no-env-expand"],
    )];
    let properties: Vec<(&str, Value<'_>)> = vec![
        ("Description", "Arinn desktop application".into()),
        ("Type", "exec".into()),
        ("ExitType", "cgroup".into()),
        ("Slice", "app.slice".into()),
        ("CollectMode", "inactive-or-failed".into()),
        ("AddRef", true.into()),
        ("ExecStartEx", Value::new(command)),
        // Do not journal launcher diagnostics containing desktop paths/argv.
        ("StandardOutput", "null".into()),
        ("StandardError", "null".into()),
    ];
    let auxiliaries: Vec<(&str, Vec<(&str, Value<'_>)>)> = Vec::new();
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let _: OwnedObjectPath = manager
            .call(
                "StartTransientUnit",
                &(&unit, "fail", properties, auxiliaries),
            )
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let object: OwnedObjectPath = manager
            .call("GetUnit", &(&unit,))
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let service = zbus::proxy::Builder::<Proxy<'_>>::new(connection)
            .destination("org.freedesktop.systemd1")
            .map_err(|_| ErrorCode::Internal)?
            .path(object)
            .map_err(|_| ErrorCode::Internal)?
            .interface("org.freedesktop.systemd1.Service")
            .map_err(|_| ErrorCode::Internal)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        loop {
            // GIO's exit acknowledges launch dispatch, not window creation or
            // application lifetime. AddRef keeps even a fast failed job readable.
            let ended: u64 = service
                .get_property("ExecMainExitTimestampMonotonic")
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            if ended != 0 {
                let code: i32 = service
                    .get_property("ExecMainCode")
                    .await
                    .map_err(|_| ErrorCode::Unavailable)?;
                let status: i32 = service
                    .get_property("ExecMainStatus")
                    .await
                    .map_err(|_| ErrorCode::Unavailable)?;
                return if code == libc::CLD_EXITED && status == 0 {
                    Ok(())
                } else {
                    Err(ErrorCode::Unavailable)
                };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or(Err(ErrorCode::Timeout));
    let _ = tokio::time::timeout(
        Duration::from_secs(1),
        manager.call::<_, _, ()>("UnrefUnit", &(&unit,)),
    )
    .await;
    result
}
