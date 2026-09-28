// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Delegate desktop-entry interpretation to GIO in a distinct application unit,
//! or activate a D-Bus activatable application through the session bus.
use crate::discovery::Entry;
use arinn_protocol::validation::ErrorCode;
use std::{collections::HashMap, path::Path, time::Duration};
use zbus::{
    Connection, Message, Proxy,
    message::Flags,
    names::WellKnownName,
    zvariant::{OwnedObjectPath, Value},
};

const DEADLINE: Duration = Duration::from_secs(30);

/// Launch a discovered entry: D-Bus activation when GLib would use it,
/// otherwise GIO in a transient application unit.
pub async fn dispatch(entry: &Entry) -> Result<(), ErrorCode> {
    // A dedicated connection owns the temporary AddRef reference. Cancellation
    // or provider death releases the reference, never stops the application unit.
    let connection = Connection::session()
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    dispatch_on(&connection, entry).await
}
pub async fn dispatch_on(connection: &Connection, entry: &Entry) -> Result<(), ErrorCode> {
    match &entry.dbus_name {
        Some(name) => activate_on(connection, name).await,
        None => launch_on(connection, &entry.path).await,
    }
}

/// Send `org.freedesktop.Application.Activate` as GLib does, from this
/// connection. `gio launch` sends it asynchronously and may exit before the
/// message leaves the process, losing the activation (issue #59). zbus writes
/// the message to the socket before `send` returns, so dispatch is complete
/// even if this connection closes immediately afterwards. The bus, not the
/// provider, starts the application, so it never joins the provider's cgroup
/// (with systemd activation it gets its own unit). Like GIO,
/// success acknowledges dispatch, not handling by the application.
pub async fn activate_on(connection: &Connection, name: &str) -> Result<(), ErrorCode> {
    let name = WellKnownName::try_from(name).map_err(|_| ErrorCode::InvalidArgument)?;
    let path = format!("/{}", name.as_str().replace('.', "/").replace('-', "_"));
    tokio::time::timeout(DEADLINE, async {
        let bus = zbus::fdo::DBusProxy::new(connection)
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        // Without a reply the bus cannot report an unknown name, so check that
        // the application is running or activatable before dispatching.
        let running = bus
            .name_has_owner(name.as_ref().into())
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        if !running
            && !bus
                .list_activatable_names()
                .await
                .map_err(|_| ErrorCode::Unavailable)?
                .iter()
                .any(|n| n.as_str() == name.as_str())
        {
            return Err(ErrorCode::Unavailable);
        }
        let platform_data: HashMap<&str, Value<'_>> = HashMap::new();
        let message = Message::method_call(path.as_str(), "Activate")
            .and_then(|m| m.destination(name.as_ref()))
            .and_then(|m| m.interface("org.freedesktop.Application"))
            .and_then(|m| m.with_flags(Flags::NoReplyExpected))
            .and_then(|m| m.build(&(platform_data,)))
            .map_err(|_| ErrorCode::Internal)?;
        connection
            .send(&message)
            .await
            .map_err(|_| ErrorCode::Unavailable)
    })
    .await
    .unwrap_or(Err(ErrorCode::Timeout))
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
    let result = tokio::time::timeout(DEADLINE, async {
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
