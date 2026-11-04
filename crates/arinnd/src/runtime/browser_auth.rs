// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Caller-scoped temporary browser leases; no address is written to history.
use super::*;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};
use tokio::sync::watch;

type Target = (String, String, String, String); // provider, original owner, generation, internal lease
pub(super) struct Launch {
    pub(super) id: String,
    revoked: AtomicBool,
    target: watch::Sender<Option<Target>>,
}
impl Launch {
    pub(super) fn cancelled(&self) -> bool {
        self.revoked.load(Ordering::Acquire)
    }
    pub(super) fn bind(&self, target: Target) {
        self.target.send_replace(Some(target));
    }
}
struct Session {
    launch: Launch,
    result: watch::Sender<Option<Result<(), ErrorCode>>>,
    closed: watch::Sender<bool>,
}
type Sessions = Arc<Mutex<HashMap<(String, String), Arc<Session>>>>;
struct BrowserAuthEndpoint {
    core: Core,
    sessions: Sessions,
}
impl Core {
    pub async fn export_browser_auth(&self) -> Result<(), ErrorCode> {
        self.connection
            .object_server()
            .at(
                production::PATH,
                BrowserAuthEndpoint {
                    core: self.clone(),
                    sessions: Arc::default(),
                },
            )
            .await
            .map(|_| ())
            .map_err(|_| ErrorCode::Internal)
    }
    async fn close_auth_window(&self, target: &Target) -> Result<(), ErrorCode> {
        let (id, owner, generation, lease) = target;
        let current = self
            .state
            .lock()
            .await
            .providers
            .get(id)
            .is_some_and(|p| &p.owner == owner && &p.info.generation == generation);
        // Browser SDK reset revokes the old generation's private temporary leases.
        // Never send stale cleanup to the replacement provider/daemon.
        if !current {
            return Ok(());
        }
        let proxy = BrowserProviderProxy::builder(&self.connection)
            .destination(owner.clone())
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        tokio::time::timeout(
            Duration::from_secs(10),
            proxy.close_temporary_window(generation, lease),
        )
        .await
        .map_err(|_| ErrorCode::Timeout)?
        .map_err(normalize_error)
    }
    async fn settle_auth(&self, session: &Session) -> Result<(), ErrorCode> {
        // Intent precedes every wait, including browser startup before a target exists.
        session.launch.revoked.store(true, Ordering::Release);
        let target = session.launch.target.borrow().clone();
        if let Some(target) = target {
            self.close_auth_window(&target).await?;
        }
        let mut result = session.result.subscribe();
        tokio::time::timeout(Duration::from_secs(60), result.wait_for(|r| r.is_some()))
            .await
            .map_err(|_| ErrorCode::Timeout)?
            .map_err(|_| ErrorCode::Unavailable)?;
        let target = session.launch.target.borrow().clone();
        // Reconcile an uncertain create as well as a confirmed one. The provider
        // retains the internal lease even if the create acknowledgement timed out.
        if let Some(target) = target {
            self.close_auth_window(&target).await?;
        }
        session.closed.send_replace(true);
        Ok(())
    }
}
#[zbus::interface(name = "org.arinn.BrowserAuth1")]
impl BrowserAuthEndpoint {
    async fn open(
        &self,
        url: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        arinn_protocol::web::open_url(url)?;
        if !url.starts_with("https://") {
            return Err(ErrorCode::InvalidArgument.into());
        }
        let caller = sender(&h)?;
        let (path, operation, fresh) = self
            .core
            .operations
            .admit(
                &self.core.connection,
                &caller,
                request_id,
                &format!("browser-auth\0{url}"),
                "",
                Label::new("browse.open"),
            )
            .await?;
        if !fresh {
            return Ok(path);
        }
        let session = Arc::new(Session {
            launch: Launch {
                id: Uuid::new_v4().to_string(),
                revoked: AtomicBool::new(false),
                target: watch::channel(None).0,
            },
            result: watch::channel(None).0,
            closed: watch::channel(false).0,
        });
        let key = (caller.clone(), request_id.to_owned());
        let admitted = {
            let mut sessions = self.sessions.lock().await;
            if sessions.len() >= 16 || sessions.keys().any(|(owner, _)| owner == &caller) {
                false
            } else {
                sessions.insert(key.clone(), session.clone());
                true
            }
        };
        if !admitted {
            operation
                .finish(&self.core.connection, &path, Err(ErrorCode::Conflict))
                .await;
            return Ok(path);
        }
        let owned = session.clone();
        let core = self.core.clone();
        let url = url.to_owned();
        let object = path.clone();
        tokio::spawn(async move {
            {
                let mut value = operation.value.lock().await;
                value.state = "running".into();
                value.started_at = now();
            }
            let result = core
                .open_browser_url(&url, &operation, Some(&owned.launch))
                .await;
            owned.result.send_replace(Some(result));
            operation
                .finish(&core.connection, &object, result.map(|_| Vec::new()))
                .await;
            // Even a failed open retains its lease until Close or caller loss settles it.
        });
        let core = self.core.clone();
        let sessions = self.sessions.clone();
        tokio::spawn(async move {
            let Ok(bus) = zbus::fdo::DBusProxy::new(&core.connection).await else {
                return;
            };
            let Ok(mut gone) = bus
                .receive_name_owner_changed_with_args(&[(0, caller.as_str())])
                .await
            else {
                return;
            };
            let mut closed = session.closed.subscribe();
            let alive = bus
                .name_has_owner(match caller.as_str().try_into() {
                    Ok(n) => n,
                    Err(_) => return,
                })
                .await
                .unwrap_or(false);
            if alive {
                tokio::select! {_=closed.wait_for(|v|*v)=>return,_=gone.next()=>{}}
            }
            // Caller loss revokes immediately, and failed cleanup remains owned/bounded.
            session.launch.revoked.store(true, Ordering::Release);
            while core.settle_auth(&session).await.is_err() {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            sessions.lock().await.remove(&key);
        });
        Ok(path)
    }
    async fn close(
        &self,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        validation::id(request_id)?;
        let key = (sender(&h)?, request_id.to_owned());
        let session = self.sessions.lock().await.get(&key).cloned();
        if let Some(session) = session {
            self.core.settle_auth(&session).await?;
            self.sessions.lock().await.remove(&key);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
