// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! systemd user-manager adapter. Process supervision is never reimplemented here.
use crate::registry::Manifest;
use arinn_protocol::validation::ErrorCode;
use std::time::Duration;
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedObjectPath, Value},
};

#[derive(Clone)]
pub struct Supervisor {
    connection: Connection,
}
impl Supervisor {
    pub fn new(connection: &Connection) -> Self {
        Self {
            connection: connection.clone(),
        }
    }
    async fn manager(&self) -> Result<Proxy<'_>, ErrorCode> {
        Proxy::new(
            &self.connection,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)
    }
    pub async fn unit(&self, name: &str) -> Result<OwnedObjectPath, ErrorCode> {
        self.manager()
            .await?
            .call("GetUnit", &(name,))
            .await
            .map_err(|error| match error {
                zbus::Error::MethodError(name, _, _)
                    if name.as_str() == "org.freedesktop.systemd1.NoSuchUnit"
                        || name.as_str() == "org.freedesktop.DBus.Error.UnknownObject" =>
                {
                    ErrorCode::NotFound
                }
                _ => ErrorCode::Unavailable,
            })
    }
    pub async fn state(&self, name: &str) -> Result<String, ErrorCode> {
        let path = self.unit(name).await?;
        Proxy::new(
            &self.connection,
            "org.freedesktop.systemd1",
            path,
            "org.freedesktop.systemd1.Unit",
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)?
        .get_property("ActiveState")
        .await
        .map_err(|_| ErrorCode::Unavailable)
    }
    pub async fn restarts(&self, name: &str) -> Result<u32, ErrorCode> {
        let work = async {
            let path = self.unit(name).await?;
            Proxy::new(
                &self.connection,
                "org.freedesktop.systemd1",
                path,
                "org.freedesktop.systemd1.Service",
            )
            .await
            .map_err(|_| ErrorCode::Unavailable)?
            .get_property("NRestarts")
            .await
            .map_err(|_| ErrorCode::Unavailable)
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
    pub async fn authenticate(&self, owner: &str, unit: &str) -> Result<(), ErrorCode> {
        let work = async {
            let bus = zbus::fdo::DBusProxy::new(&self.connection)
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            let name = owner.try_into().map_err(|_| ErrorCode::PermissionDenied)?;
            let uid = bus
                .get_connection_unix_user(name)
                .await
                .map_err(|_| ErrorCode::PermissionDenied)?;
            // SAFETY: geteuid has no arguments or memory preconditions.
            if uid != unsafe { libc::geteuid() } {
                return Err(ErrorCode::PermissionDenied);
            }
            let pid = bus
                .get_connection_unix_process_id(
                    owner.try_into().map_err(|_| ErrorCode::PermissionDenied)?,
                )
                .await
                .map_err(|_| ErrorCode::PermissionDenied)?;
            let actual: OwnedObjectPath = self
                .manager()
                .await?
                .call("GetUnitByPID", &(pid,))
                .await
                .map_err(|_| ErrorCode::PermissionDenied)?;
            if actual != self.unit(unit).await? {
                return Err(ErrorCode::PermissionDenied);
            }
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
    pub async fn start(
        &self,
        name: &str,
        manifest: &Manifest,
        environment: Vec<String>,
    ) -> Result<bool, ErrorCode> {
        let work = async {
            // On daemon restart, attach to the surviving unit. A failed unit is
            // left failed; only a manual restart clears its start-limit state.
            if let Ok(state) = self.state(name).await {
                if ["active", "activating", "reloading"].contains(&state.as_str()) {
                    return Ok(false);
                }
                if state == "failed" {
                    return Err(ErrorCode::Unavailable);
                }
            }
            let executable = manifest
                .executable
                .to_str()
                .ok_or(ErrorCode::InvalidArgument)?
                .to_string();
            let mut argv = vec![executable.clone()];
            argv.extend(manifest.args.clone());
            let commands = vec![(executable, argv, false)];
            let properties = vec![
                ("Type", Value::from("exec")),
                ("Restart", Value::from("on-failure")),
                ("RestartUSec", Value::from(2_000_000u64)),
                ("StartLimitIntervalUSec", Value::from(60_000_000u64)),
                ("StartLimitBurst", Value::from(5u32)),
                ("TimeoutStopUSec", Value::from(5_000_000u64)),
                ("ExecStart", Value::new(commands)),
                ("Environment", Value::new(environment)),
            ];
            let auxiliary: Vec<(String, Vec<(String, Value<'_>)>)> = Vec::new();
            let _: OwnedObjectPath = self
                .manager()
                .await?
                .call("StartTransientUnit", &(name, "fail", properties, auxiliary))
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            Ok(true)
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
    pub async fn stop(&self, name: &str) -> Result<(), ErrorCode> {
        match self.unit(name).await {
            Err(ErrorCode::NotFound) => return Ok(()),
            Err(error) => return Err(error),
            Ok(_) => (),
        }
        let work = async {
            let _: OwnedObjectPath = self
                .manager()
                .await?
                .call("StopUnit", &(name, "replace"))
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            loop {
                match self.state(name).await {
                    Err(_) => return Ok(()),
                    Ok(state) if state == "inactive" || state == "failed" => return Ok(()),
                    _ => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(6), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
    pub async fn restart(&self, name: &str) -> Result<(), ErrorCode> {
        let work = async {
            let manager = self.manager().await?;
            let _: () = manager
                .call("ResetFailedUnit", &(name,))
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            let _: OwnedObjectPath = manager
                .call("RestartUnit", &(name, "replace"))
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
    /// A hung handshake is a failed process, so systemd's existing restart budget
    /// applies. StopUnit would mark it an intentional stop and bypass that budget.
    pub async fn fail_handshake(&self, name: &str) -> Result<(), ErrorCode> {
        let work = async {
            let _: () = self
                .manager()
                .await?
                .call("KillUnit", &(name, "all", libc::SIGKILL))
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }
}
