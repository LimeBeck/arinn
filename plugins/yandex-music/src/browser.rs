// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider-owned, serialized handoff through the daemon; no shell or desktop commands.
use arinn_protocol::{
    browser::authorization::BrowserAuthProxy,
    production::{ProductionDaemonProxy, ProductionOperationProxy},
    validation::ErrorCode,
};
use std::{future::Future, pin::Pin, time::Duration};
pub(crate) type Work<'a> = Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + 'a>>;
pub(crate) trait Handoff: Send + Sync {
    fn open<'a>(&'a self, url: &'a str, id: &'a str) -> Work<'a>;
    fn close<'a>(&'a self, id: &'a str) -> Work<'a>;
    fn home<'a>(&'a self, id: &'a str) -> Work<'a>;
}
pub(crate) struct Browser {
    connection: zbus::Connection,
    owner: tokio::sync::Mutex<Option<(String, String)>>,
}
impl Browser {
    pub(crate) async fn new(connection: zbus::Connection) -> Result<Self, ErrorCode> {
        Ok(Self {
            connection,
            owner: tokio::sync::Mutex::new(None),
        })
    }
    async fn current_owner(&self) -> Result<String, ErrorCode> {
        zbus::fdo::DBusProxy::new(&self.connection)
            .await
            .map_err(|_| ErrorCode::Unavailable)?
            .get_name_owner(
                "org.arinn.Daemon1"
                    .try_into()
                    .map_err(|_| ErrorCode::Internal)?,
            )
            .await
            .map(|n| n.to_string())
            .map_err(|_| ErrorCode::Unavailable)
    }
    async fn owner(&self, id: &str, begin: bool) -> Result<Option<String>, ErrorCode> {
        let mut record = self.owner.lock().await;
        if let Some((request, owner)) = record.as_ref()
            && request == id
        {
            return Ok(Some(owner.clone()));
        }
        if !begin {
            return Ok(None);
        }
        let owner = self.current_owner().await?;
        *record = Some((id.into(), owner.clone()));
        Ok(Some(owner))
    }
    async fn proxy(&self, owner: &str) -> Result<BrowserAuthProxy<'_>, ErrorCode> {
        BrowserAuthProxy::builder(&self.connection)
            .destination(owner.to_owned())
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)
    }
}
impl Handoff for Browser {
    fn open<'a>(&'a self, url: &'a str, id: &'a str) -> Work<'a> {
        Box::pin(async move {
            let owner = self.owner(id, true).await?.ok_or(ErrorCode::Unavailable)?;
            let path = self
                .proxy(&owner)
                .await?
                .open(url, id)
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            let operation = ProductionOperationProxy::builder(&self.connection)
                .destination(owner.clone())
                .map_err(|_| ErrorCode::Internal)?
                .path(path)
                .map_err(|_| ErrorCode::Internal)?
                .build()
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            // This await owns the dispatched launch. Cancellation is handled only after
            // it settles, then Close waits before the caller can request Home.
            tokio::time::timeout(Duration::from_secs(60), async {
                loop {
                    let (info,) = operation.get().await.map_err(|_| ErrorCode::Unavailable)?;
                    match info.state.as_str() {
                        "succeeded" => return Ok(()),
                        "failed" | "cancelled" => return Err(ErrorCode::Unavailable),
                        "queued" | "running" => {
                            tokio::time::sleep(Duration::from_millis(100)).await
                        }
                        _ => return Err(ErrorCode::Unavailable),
                    }
                }
            })
            .await
            .map_err(|_| ErrorCode::Timeout)?
        })
    }
    fn close<'a>(&'a self, id: &'a str) -> Work<'a> {
        Box::pin(async move {
            let Some(owner) = self.owner(id, false).await? else {
                return Ok(());
            };
            let result = self
                .proxy(&owner)
                .await?
                .close(id)
                .await
                .map_err(|_| ErrorCode::Unavailable);
            if result.is_err() && self.current_owner().await.ok().as_deref() != Some(owner.as_str())
            {
                // The old daemon's browser registration owns revocation. Never
                // retarget an old cleanup at the new daemon's independent leases.
                return Ok(());
            }
            result
        })
    }
    fn home<'a>(&'a self, id: &'a str) -> Work<'a> {
        Box::pin(async move {
            let owner = self.owner(id, false).await?.ok_or(ErrorCode::Conflict)?;
            if self.current_owner().await? != owner {
                return Err(ErrorCode::Unavailable);
            }
            ProductionDaemonProxy::builder(&self.connection)
                .destination(owner)
                .map_err(|_| ErrorCode::Internal)?
                .build()
                .await
                .map_err(|_| ErrorCode::Unavailable)?
                .home()
                .await
                .map_err(|_| ErrorCode::Unavailable)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::{Arc, Mutex},
    };
    struct Bus(Child, String);
    impl Bus {
        fn new() -> Self {
            let mut child = Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut address = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut address)
                .unwrap();
            Self(child, address.trim().into())
        }
        async fn connection(&self) -> zbus::Connection {
            zbus::connection::Builder::address(self.1.as_str())
                .unwrap()
                .method_timeout(Duration::from_secs(3))
                .build()
                .await
                .unwrap()
        }
    }
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[derive(Clone, Default)]
    struct Endpoint(Arc<Mutex<Vec<String>>>);
    #[zbus::interface(name = "org.arinn.BrowserAuth1")]
    impl Endpoint {
        fn close(&self, request: &str) {
            self.0.lock().unwrap().push(request.into());
        }
    }
    #[tokio::test]
    async fn each_attempt_pins_its_own_daemon_and_old_cleanup_never_targets_the_replacement() {
        let bus = Bus::new();
        let first = bus.connection().await;
        let second = bus.connection().await;
        let client = bus.connection().await;
        let a = Endpoint::default();
        let b = Endpoint::default();
        first
            .object_server()
            .at(arinn_protocol::production::PATH, a.clone())
            .await
            .unwrap();
        second
            .object_server()
            .at(arinn_protocol::production::PATH, b.clone())
            .await
            .unwrap();
        first
            .request_name(arinn_protocol::production::SERVICE)
            .await
            .unwrap();
        let browser = Browser::new(client).await.unwrap();
        assert_eq!(
            browser.owner("old", true).await.unwrap().as_deref(),
            first.unique_name().map(|n| n.as_str())
        );
        first
            .release_name(arinn_protocol::production::SERVICE)
            .await
            .unwrap();
        second
            .request_name(arinn_protocol::production::SERVICE)
            .await
            .unwrap();
        assert_eq!(browser.home("old").await, Err(ErrorCode::Unavailable));
        browser.close("old").await.unwrap();
        assert_eq!(*a.0.lock().unwrap(), vec!["old"]);
        assert!(b.0.lock().unwrap().is_empty());
        assert_eq!(
            browser.owner("new", true).await.unwrap().as_deref(),
            second.unique_name().map(|n| n.as_str())
        );
        browser.close("new").await.unwrap();
        assert_eq!(*b.0.lock().unwrap(), vec!["new"]);
    }
}
