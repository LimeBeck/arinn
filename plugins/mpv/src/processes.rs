// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Optional ADR-0096 process evidence. Only the registered daemon reads it.
use crate::Player;
use arinn_plugin_sdk::runtime::RegistrationContext;
use arinn_protocol::{media_association, production, validation::ErrorCode};
use tokio::sync::watch;
use zbus::Connection;

struct Processes {
    player: Player,
    registrations: watch::Receiver<Option<RegistrationContext>>,
}

pub(super) async fn export(
    connection: &Connection,
    player: Player,
    registrations: watch::Receiver<Option<RegistrationContext>>,
) -> Result<(), ErrorCode> {
    connection
        .object_server()
        .at(
            production::PROVIDER_PATH,
            Processes {
                player,
                registrations,
            },
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    Ok(())
}

impl Processes {
    fn check(&self, sender: &str, generation: &str, id: &str) -> Result<(), ErrorCode> {
        let current = self.registrations.borrow();
        let current = current.as_ref().ok_or(ErrorCode::PermissionDenied)?;
        if current.daemon_owner != sender {
            return Err(ErrorCode::PermissionDenied);
        }
        media_association::validate_process_query(generation, id)?;
        if current.generation != generation {
            return Err(ErrorCode::Conflict);
        }
        Ok(())
    }
}

#[zbus::interface(name = "org.arinn.MediaProcess1")]
impl Processes {
    async fn get_process_id(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> production::Result<u32> {
        let sender = header.sender().ok_or(ErrorCode::PermissionDenied)?.as_str();
        self.check(sender, generation, id)?;
        let process = self.player.process_id(id.into()).await;
        self.check(sender, generation, id)?;
        Ok(process?)
    }
}
