// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Peer lookups on an auxiliary session-bus connection.
//!
//! Calling `org.freedesktop.DBus` on the same [`zbus::Connection`] that is
//! handling an incoming method (for example `Register`) can deadlock: the
//! handler waits for the reply while the connection cannot dispatch it.
use arinn_protocol::validation::ErrorCode;
use zbus::Connection;

async fn auxiliary_session() -> Result<Connection, ErrorCode> {
    zbus::connection::Builder::session()
        .map_err(|_| ErrorCode::Unavailable)?
        .build()
        .await
        .map_err(|_| ErrorCode::Unavailable)
}

/// UID and PID of the D-Bus client that owns `bus_name`.
pub async fn unix_peer(bus_name: &str) -> Result<(u32, u32), ErrorCode> {
    let connection = auxiliary_session().await?;
    let bus = zbus::fdo::DBusProxy::new(&connection)
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    let name: zbus::names::BusName<'_> = bus_name
        .try_into()
        .map_err(|_| ErrorCode::PermissionDenied)?;
    let uid = bus
        .get_connection_unix_user(name.clone())
        .await
        .map_err(|_| ErrorCode::PermissionDenied)?;
    let pid = bus
        .get_connection_unix_process_id(name)
        .await
        .map_err(|_| ErrorCode::PermissionDenied)?;
    Ok((uid, pid))
}

pub async fn name_has_owner(bus_name: &str) -> Result<bool, ErrorCode> {
    let connection = auxiliary_session().await?;
    let bus = zbus::fdo::DBusProxy::new(&connection)
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    let name: zbus::names::BusName<'_> = bus_name
        .try_into()
        .map_err(|_| ErrorCode::PermissionDenied)?;
    bus.name_has_owner(name)
        .await
        .map_err(|_| ErrorCode::Unavailable)
}
