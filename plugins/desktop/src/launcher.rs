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

/// A dispatched application unit. The connection that started it holds the
/// AddRef reference, so the unit stays readable after its processes exit;
/// dropping the connection releases the reference, never stops the unit.
pub struct LaunchUnit {
    connection: Connection,
    name: String,
    unit: Proxy<'static>,
}
impl LaunchUnit {
    /// Whether processes of the launch still run. With `ExitType=cgroup`
    /// the unit stays active after GIO exits until its cgroup is empty, so
    /// `inactive` or `failed` means the application and everything it started
    /// have exited, as does a unit that systemd has already collected. A read
    /// that fails some other way says nothing, so the launch keeps waiting;
    /// callers bound how long.
    pub async fn running(&self) -> bool {
        exited(&self.unit.get_property::<String>("ActiveState").await).is_none_or(|exited| !exited)
    }
    /// Stop this unit only: the processes of this launch, nothing else.
    pub async fn stop(&self) -> Result<(), ErrorCode> {
        let manager = manager(&self.connection).await?;
        tokio::time::timeout(
            Duration::from_secs(5),
            manager.call::<_, _, OwnedObjectPath>("StopUnit", &(&self.name, "replace")),
        )
        .await
        .map_err(|_| ErrorCode::Timeout)?
        .map(|_| ())
        .map_err(|_| ErrorCode::Unavailable)
    }
    /// Release the AddRef reference; the unit and its processes keep running.
    pub async fn release(self) {
        if let Ok(manager) = manager(&self.connection).await {
            let _ = tokio::time::timeout(
                Duration::from_secs(1),
                manager.call::<_, _, ()>("UnrefUnit", &(&self.name,)),
            )
            .await;
        }
    }
}
/// What one read of a launch unit's `ActiveState` says: `Some(true)` when
/// its processes have exited (inactive, failed, or the unit is gone after
/// garbage collection), `Some(false)` while they run, `None` when the read
/// failed and nothing is known.
fn exited(read: &zbus::Result<String>) -> Option<bool> {
    use zbus::DBusError as _;
    match read {
        Ok(state) => Some(matches!(state.as_str(), "inactive" | "failed")),
        Err(zbus::Error::MethodError(name, _, _)) if gone(name.as_str()) => Some(true),
        Err(zbus::Error::FDO(error)) if gone(error.name().as_str()) => Some(true),
        Err(_) => None,
    }
}
/// D-Bus errors that mean the unit no longer exists.
fn gone(name: &str) -> bool {
    matches!(
        name,
        "org.freedesktop.DBus.Error.UnknownObject"
            | "org.freedesktop.DBus.Error.UnknownInterface"
            | "org.freedesktop.DBus.Error.UnknownProperty"
            | "org.freedesktop.DBus.Error.UnknownMethod"
            | "org.freedesktop.systemd1.NoSuchUnit"
            | "org.freedesktop.systemd1.LoadFailed"
    )
}
async fn manager(connection: &Connection) -> Result<Proxy<'static>, ErrorCode> {
    Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await
    .map_err(|_| ErrorCode::Unavailable)
}

/// Dispatch through GIO and release the unit at once: success acknowledges
/// dispatch only.
pub async fn launch_on(connection: &Connection, path: &Path) -> Result<(), ErrorCode> {
    start_on(connection, path).await?.release().await;
    Ok(())
}

/// Start GIO in a new application unit and wait, within the dispatch
/// deadline, until GIO has exited. The returned unit stays referenced.
pub async fn start_on(connection: &Connection, path: &Path) -> Result<LaunchUnit, ErrorCode> {
    let path = path
        .to_str()
        .filter(|_| path.is_absolute())
        .ok_or(ErrorCode::InvalidArgument)?;
    let name = format!("app-arinn-{}.service", uuid::Uuid::new_v4().simple());
    let manager = manager(connection).await?;
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
                &(&name, "fail", properties, auxiliaries),
            )
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let object: OwnedObjectPath = manager
            .call("GetUnit", &(&name,))
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let unit = |interface: &'static str| {
            let object = object.clone();
            async move {
                zbus::proxy::Builder::<Proxy<'static>>::new(connection)
                    .destination("org.freedesktop.systemd1")
                    .map_err(|_| ErrorCode::Internal)?
                    .path(object)
                    .map_err(|_| ErrorCode::Internal)?
                    .interface(interface)
                    .map_err(|_| ErrorCode::Internal)?
                    .cache_properties(zbus::proxy::CacheProperties::No)
                    .build()
                    .await
                    .map_err(|_| ErrorCode::Unavailable)
            }
        };
        let service = unit("org.freedesktop.systemd1.Service").await?;
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
                    unit("org.freedesktop.systemd1.Unit").await
                } else {
                    Err(ErrorCode::Unavailable)
                };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or(Err(ErrorCode::Timeout));
    match result {
        Ok(unit) => Ok(LaunchUnit {
            connection: connection.clone(),
            name,
            unit,
        }),
        Err(error) => {
            let _ = tokio::time::timeout(
                Duration::from_secs(1),
                manager.call::<_, _, ()>("UnrefUnit", &(&name,)),
            )
            .await;
            Err(error)
        }
    }
}

#[cfg(test)]
mod state_tests {
    use super::{exited, gone};
    use zbus::fdo;

    /// A failed read is not an exit: a cancel must not be confirmed, nor a
    /// launch failed, while the processes may still run.
    #[test]
    fn only_a_known_state_or_a_collected_unit_counts_as_exited() {
        assert_eq!(exited(&Ok("active".into())), Some(false));
        assert_eq!(exited(&Ok("deactivating".into())), Some(false));
        assert_eq!(exited(&Ok("inactive".into())), Some(true));
        assert_eq!(exited(&Ok("failed".into())), Some(true));
        let fdo_error = |error| Err(zbus::Error::FDO(Box::new(error)));
        assert_eq!(
            exited(&fdo_error(fdo::Error::UnknownObject("gone".into()))),
            Some(true)
        );
        assert_eq!(exited(&fdo_error(fdo::Error::NoReply("slow".into()))), None);
        assert_eq!(
            exited(&fdo_error(fdo::Error::TimedOut("slow".into()))),
            None
        );
        assert_eq!(
            exited(&Err(zbus::Error::InputOutput(std::sync::Arc::new(
                std::io::Error::other("bus")
            )))),
            None
        );
        assert!(gone("org.freedesktop.systemd1.NoSuchUnit"));
        assert!(!gone("org.freedesktop.DBus.Error.NoReply"));
        assert!(!gone("org.freedesktop.DBus.Error.ServiceUnknown"));
    }
}
