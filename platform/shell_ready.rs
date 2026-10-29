// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whether the shell is ready to show the power menu (ADR-0111).
//!
//! The shell owns `org.arinn.Shell1` before its window exists
//! (`arinn-shell.service` is `Type=dbus`). It answers a property read on
//! `/org/arinn/Shell1` only once its event loop runs, after its QML has
//! loaded; `arinn-open` uses the same probe. The power key follows the
//! shell only from that answer on, so a press while the shell starts is
//! left to the desktop (review of #517, Codex). Each owner is probed until
//! it answers once.
use std::time::Duration;
use zbus::Connection;

/// The shell's bus name; its owner is the live shell.
pub const SHELL: &str = "org.arinn.Shell1";
const PATH: &str = "/org/arinn/Shell1";
/// A loaded shell answers at once; a starting one does not answer.
const PROBE: Duration = Duration::from_millis(250);

/// The shell owner that answered, if any.
#[derive(Clone, Debug, Default)]
pub struct ShellReady {
    owner: Option<String>,
}
impl ShellReady {
    /// Whether a ready shell owns `org.arinn.Shell1` on `bus` now.
    pub async fn check(&mut self, bus: &Connection) -> bool {
        let owner = match zbus::fdo::DBusProxy::new(bus).await {
            Ok(dbus) => match zbus::names::BusName::try_from(SHELL) {
                Ok(name) => dbus.get_name_owner(name).await.ok().map(|o| o.to_string()),
                Err(_) => None,
            },
            Err(_) => None,
        };
        let Some(owner) = owner else {
            self.owner = None;
            return false;
        };
        if self.owner.as_deref() == Some(owner.as_str()) {
            return true;
        }
        let answered = tokio::time::timeout(
            PROBE,
            bus.call_method(
                Some(owner.as_str()),
                PATH,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &(SHELL, "connected"),
            ),
        )
        .await
        .is_ok_and(|reply| reply.is_ok());
        self.owner = answered.then_some(owner);
        answered
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::power_inhibitor::tests::{bus, connect};

    /// A shell whose window has loaded: it answers `connected`.
    pub(crate) struct Shell;
    #[zbus::interface(name = "org.arinn.Shell1")]
    impl Shell {
        /// Qt exports the property under its own name.
        #[zbus(property, name = "connected")]
        fn connected(&self) -> bool {
            true
        }
    }
    /// `connection` becomes a ready shell.
    pub(crate) async fn serve(connection: &Connection) {
        connection.object_server().at(PATH, Shell).await.unwrap();
        connection.request_name(SHELL).await.unwrap();
    }

    /// Review of #517 (Codex): owning the name is not enough; only an
    /// answer to the property read counts, and a new owner is probed again.
    #[tokio::test]
    async fn the_shell_counts_once_it_answers() {
        let bus = bus();
        let adapter = connect(&bus.1).await;
        let mut ready = ShellReady::default();
        assert!(!ready.check(&adapter).await);
        // Starting: the name is owned, the window is not there yet.
        let starting = connect(&bus.1).await;
        starting.request_name(SHELL).await.unwrap();
        assert!(!ready.check(&adapter).await);
        // Loaded.
        starting.object_server().at(PATH, Shell).await.unwrap();
        assert!(ready.check(&adapter).await);
        assert!(ready.check(&adapter).await);
        // Another shell: probed again.
        starting.release_name(SHELL).await.unwrap();
        let next = connect(&bus.1).await;
        serve(&next).await;
        assert!(ready.check(&adapter).await);
        // A name owned without an answer never counts.
        next.release_name(SHELL).await.unwrap();
        let silent = connect(&bus.1).await;
        silent.request_name(SHELL).await.unwrap();
        assert!(!ready.check(&adapter).await);
    }
}
