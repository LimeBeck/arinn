// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native account lifecycle. Credential-bearing operations stay in this process.
use crate::{
    auth::{self, DeviceAuth},
    secrets::SecretService,
};
use arinn_music_common::{paths, storage::Storage};
use arinn_plugin_sdk::runtime::{MusicCatalogue, MusicHandler, MusicService, run_music_on};
use arinn_protocol::{
    music::{Command, Prompt, Query, State},
    validation::ErrorCode,
};
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::sync::Mutex;

type Auth = DeviceAuth<SecretService>;
struct Account {
    state: State,
    epoch: u64,
}
#[derive(Clone)]
struct Service {
    auth: Option<Arc<Auth>>,
    account: Arc<Mutex<Account>>,
}
impl Service {
    fn new(auth: Option<Arc<Auth>>) -> Self {
        let configured = auth.is_some();
        Self {
            auth,
            account: Arc::new(Mutex::new(Account {
                epoch: 0,
                state: State {
                    backend: if configured {
                        "available"
                    } else {
                        "unavailable"
                    }
                    .into(),
                    account: if configured {
                        "disconnected"
                    } else {
                        "unconfigured"
                    }
                    .into(),
                    actions: if configured {
                        vec!["connect".into()]
                    } else {
                        vec![]
                    },
                    ..State::default()
                },
            })),
        }
    }
    async fn snapshot(&self) -> State {
        self.account.lock().await.state.clone()
    }
    async fn restore(&self, start: Option<auth::Start>) {
        let Some(auth) = &self.auth else {
            return;
        };
        let Some(start) = start else {
            return;
        };
        let result = auth.restore_prepared(start).await;
        let mut account = self.account.lock().await;
        if account.epoch != 0 {
            return;
        }
        match result {
            Ok(true) => {
                account.state.account = "connected".into();
                account.state.actions = vec!["disconnect".into()];
            }
            Ok(false) => {}
            Err(error) => account.state.error = Some(normalize(error)),
        }
        account.state.revision += 1;
    }
    async fn connect(&self) -> Result<(), ErrorCode> {
        let auth = self.auth.clone().ok_or(ErrorCode::Unsupported)?;
        let (epoch, start) = {
            let mut a = self.account.lock().await;
            if a.state.account == "connected" {
                return Err(ErrorCode::Conflict);
            }
            auth.cancel();
            a.epoch += 1;
            a.state.revision += 1;
            a.state.account = "connecting".into();
            a.state.prompt = None;
            a.state.actions = vec!["cancel".into()];
            a.state.error = None;
            (a.epoch, auth.prepare())
        };
        let this = self.clone();
        tokio::spawn(async move {
            let result = async {
                let attempt = auth.start_prepared(start).await?;
                {
                    let mut a = this.account.lock().await;
                    if a.epoch != epoch {
                        return Err(auth::Error::Cancelled);
                    }
                    a.state.prompt = Some(Prompt {
                        verification_url: attempt.prompt.verification_url.clone(),
                        user_code: attempt.prompt.user_code.clone(),
                        expires_in: attempt.prompt.expires_in,
                    });
                    a.state.revision += 1;
                }
                auth.complete(attempt).await
            }
            .await;
            let mut a = this.account.lock().await;
            if a.epoch != epoch {
                return;
            }
            a.state.prompt = None;
            a.state.revision += 1;
            match result {
                Ok(()) => {
                    a.state.account = "connected".into();
                    a.state.actions = vec!["disconnect".into()];
                    a.state.error = None;
                }
                Err(error) => {
                    a.state.account = if error == auth::Error::Expired {
                        "expired"
                    } else {
                        "disconnected"
                    }
                    .into();
                    a.state.actions = vec!["connect".into()];
                    a.state.error = Some(normalize(error));
                }
            }
        });
        Ok(())
    }
    async fn cancel(&self) {
        let mut a = self.account.lock().await;
        if a.state.account != "connecting" {
            return;
        }
        if let Some(auth) = &self.auth {
            auth.cancel();
        }
        a.epoch += 1;
        a.state.revision += 1;
        a.state.prompt = None;
        a.state.account = "disconnected".into();
        a.state.actions = vec!["connect".into()];
        a.state.error = None;
    }
}
fn normalize(error: auth::Error) -> ErrorCode {
    match error {
        auth::Error::Denied | auth::Error::Locked => ErrorCode::PermissionDenied,
        auth::Error::InvalidInput | auth::Error::ClientRejected => ErrorCode::InvalidArgument,
        auth::Error::Cancelled => ErrorCode::Conflict,
        _ => ErrorCode::Unavailable,
    }
}
impl MusicHandler for Service {
    fn state(&self) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>> {
        Box::pin(async move { Ok(self.snapshot().await) })
    }
    fn catalogue(
        &self,
        _query: Query,
    ) -> Pin<Box<dyn Future<Output = Result<MusicCatalogue, ErrorCode>> + Send + '_>> {
        Box::pin(async { Err(ErrorCode::Unsupported) })
    }
    fn command(
        &self,
        command: Command,
    ) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            match command.action.as_str() {
                "connect" => self.connect().await?,
                "cancel" => self.cancel().await,
                "disconnect" => {
                    let auth = self.auth.as_ref().ok_or(ErrorCode::Unsupported)?;
                    {
                        let mut a = self.account.lock().await;
                        auth.cancel();
                        a.epoch += 1;
                        a.state.prompt = None;
                        a.state.account = "disconnected".into();
                        a.state.actions = vec!["connect".into()];
                        a.state.revision += 1;
                    }
                    if let Err(error) = auth.disconnect().await {
                        self.account.lock().await.state.error = Some(normalize(error));
                        return Err(normalize(error));
                    }
                }
                _ => return Err(ErrorCode::Unsupported),
            }
            Ok(self.snapshot().await)
        })
    }
    fn reset(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.cancel().await;
        })
    }
}

pub async fn serve(connection: zbus::Connection) -> Result<(), ErrorCode> {
    let provider = std::env::var("ARINN_PROVIDER_ID").map_err(|_| ErrorCode::InvalidArgument)?;
    // Client ID is a public deployment identity, never a user token or a
    // borrowed third-party application identity. Secret-requiring clients need
    // separately qualified provisioning before being supported here.
    let client_id = std::env::var("ARINN_YANDEX_CLIENT_ID")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let bus = connection.clone();
    let auth = tokio::task::spawn_blocking(move || {
        let Some(client_id) = client_id else {
            return Ok(None);
        };
        let directory = paths::provider_state(&provider)?;
        let identity =
            Storage::open(&directory.join("oauth-device")).map_err(|_| ErrorCode::Unavailable)?;
        let device = match identity.read().map_err(|_| ErrorCode::Unavailable)? {
            Some(bytes) => String::from_utf8(bytes).map_err(|_| ErrorCode::Unavailable)?,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                identity
                    .write(id.as_bytes())
                    .map_err(|_| ErrorCode::Unavailable)?;
                id
            }
        };
        let reference = paths::group("account", &provider).replace(':', "-");
        let store = SecretService::new(bus, &reference, &directory.join("oauth-approval"))
            .map_err(normalize)?;
        DeviceAuth::new(&client_id, None, &device, store)
            .map(|auth| Some(Arc::new(auth)))
            .map_err(normalize)
    })
    .await
    .map_err(|_| ErrorCode::Internal)??;
    let handler = Service::new(auth);
    let restore_start = handler.auth.as_ref().map(|auth| auth.prepare());
    let restore = handler.clone();
    tokio::spawn(async move {
        restore.restore(restore_start).await;
    });
    let service = MusicService::new(handler.clone());
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| ErrorCode::Unavailable)?;
    let result = tokio::select! {result=run_music_on(connection,service)=>result.map_err(|_|ErrorCode::Unavailable),_=terminate.recv()=>Ok(()),_=tokio::signal::ctrl_c()=>Ok(())};
    handler.cancel().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn missing_client_identity_has_no_connect_action_or_secret_prompt() {
        let service = Service::new(None);
        service.restore(None).await;
        service.cancel().await;
        let state = service.snapshot().await;
        assert_eq!(state.account, "unconfigured");
        assert!(state.actions.is_empty());
        assert!(state.prompt.is_none());
        assert!(state.validate().is_ok());
        assert_eq!(service.connect().await, Err(ErrorCode::Unsupported));
    }
}
