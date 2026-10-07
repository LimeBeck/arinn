// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! systemd user-manager adapter for provider units.
use crate::registry::Manifest;
use arinn_protocol::validation::ErrorCode;
use std::{path::Path, time::Duration};
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedObjectPath, Value},
};

use super::{Attachment, running_executable_matches};

#[derive(Clone)]
pub struct Backend {
    connection: Connection,
}

impl Backend {
    pub fn new(connection: &Connection) -> Self {
        Self {
            connection: connection.clone(),
        }
    }

    pub async fn available(connection: &Connection) -> bool {
        let bus = match zbus::fdo::DBusProxy::new(connection).await {
            Ok(bus) => bus,
            Err(_) => return false,
        };
        let systemd = match "org.freedesktop.systemd1".try_into() {
            Ok(name) => name,
            Err(_) => return false,
        };
        if !bus.name_has_owner(systemd).await.unwrap_or(false) {
            return false;
        }
        Self::new(connection).manager().await.is_ok()
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
            let (uid, pid) = crate::bus_peer::unix_peer(owner).await?;
            // SAFETY: geteuid has no arguments or memory preconditions.
            if uid != unsafe { libc::geteuid() } {
                return Err(ErrorCode::PermissionDenied);
            }
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

    async fn main_pid(&self, name: &str) -> Result<u32, ErrorCode> {
        let path = self.unit(name).await?;
        Proxy::new(
            &self.connection,
            "org.freedesktop.systemd1",
            path,
            "org.freedesktop.systemd1.Service",
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)?
        .get_property("MainPID")
        .await
        .map_err(|_| ErrorCode::Unavailable)
    }

    pub async fn executable_current(
        &self,
        name: &str,
        executable: &Path,
    ) -> Result<bool, ErrorCode> {
        let work = async {
            match self.state(name).await {
                Err(ErrorCode::NotFound) => Ok(true),
                Err(error) => Err(error),
                Ok(state) if matches!(state.as_str(), "active" | "activating" | "reloading") => {
                    self.process_matches(name, executable).await
                }
                Ok(_) => Ok(true),
            }
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }

    async fn process_matches(&self, name: &str, executable: &Path) -> Result<bool, ErrorCode> {
        let pid = self.main_pid(name).await?;
        if pid == 0 {
            return Ok(true);
        }
        let executable = executable.to_path_buf();
        tokio::task::spawn_blocking(move || running_executable_matches(pid, &executable))
            .await
            .map_err(|_| ErrorCode::Internal)
    }

    pub async fn start(
        &self,
        name: &str,
        manifest: &Manifest,
        environment: Vec<String>,
    ) -> Result<Attachment, ErrorCode> {
        let replace = match tokio::time::timeout(Duration::from_secs(3), self.state(name)).await {
            Ok(Ok(state)) if matches!(state.as_str(), "active" | "activating" | "reloading") => {
                let matches = tokio::time::timeout(
                    Duration::from_secs(3),
                    self.process_matches(name, &manifest.executable),
                )
                .await
                .map_err(|_| ErrorCode::Timeout)??;
                if matches {
                    return Ok(Attachment::Attached);
                }
                true
            }
            Ok(Ok(state)) if state == "failed" => return Err(ErrorCode::Unavailable),
            Ok(Ok(_)) | Ok(Err(ErrorCode::NotFound)) => false,
            Ok(Err(error)) => return Err(error),
            Err(_) => return Err(ErrorCode::Timeout),
        };
        if replace {
            self.discard(name).await?;
        }
        self.spawn_transient(name, manifest, environment).await?;
        Ok(if replace {
            Attachment::Replaced
        } else {
            Attachment::Created
        })
    }

    async fn spawn_transient(
        &self,
        name: &str,
        manifest: &Manifest,
        environment: Vec<String>,
    ) -> Result<(), ErrorCode> {
        let work = async {
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
            Ok(())
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
                    Ok(state) if state == "inactive" || state == "failed" => break,
                    _ => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            }
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(6), work)
            .await
            .map_err(|_| ErrorCode::Timeout)??;
        if self.state(name).await.as_deref() != Ok("failed") {
            let _ = self.unloaded(name).await;
        }
        Ok(())
    }

    async fn unloaded(&self, name: &str) -> Result<(), ErrorCode> {
        let work = async {
            loop {
                match self.unit(name).await {
                    Err(ErrorCode::NotFound) => return Ok(()),
                    Err(error) => return Err(error),
                    Ok(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }

    pub async fn discard(&self, name: &str) -> Result<(), ErrorCode> {
        self.stop(name).await?;
        match self.unit(name).await {
            Err(ErrorCode::NotFound) => return Ok(()),
            Err(error) => return Err(error),
            Ok(_) => (),
        }
        self.reset_failed(name).await?;
        self.unloaded(name).await
    }

    pub async fn reset_failed(&self, name: &str) -> Result<(), ErrorCode> {
        let work = async {
            let _: () = self
                .manager()
                .await?
                .call("ResetFailedUnit", &(name,))
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(3), work)
            .await
            .map_err(|_| ErrorCode::Timeout)?
    }

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
