// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native account lifecycle. Credential-bearing operations stay in this process.
use crate::{
    auth::{self, CredentialStore, DeviceAuth},
    secrets::SecretService,
};
use arinn_music_common::{paths, storage::Storage};
use arinn_plugin_sdk::runtime::{
    MusicCatalogue, MusicHandler, MusicService, TextsHandler, run_music_on, serve_settings,
    serve_texts, startup_settings,
};
use arinn_protocol::{
    music::{Command, Prompt, Query, State},
    validation::ErrorCode,
};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};
use tokio::sync::Mutex;

// Default compatibility registration was explicitly authorized by the owner on 2026-11-04.
const DEFAULT_CLIENT_ID: &str = crate::compatibility::CLIENT_ID;
struct Account {
    state: State,
    epoch: u64,
    unlock_task: Option<tokio::task::AbortHandle>,
    prompt_deadline: Option<tokio::time::Instant>,
    browser_request: Option<tokio::sync::oneshot::Sender<()>>,
}
struct Service<S: CredentialStore + 'static = SecretService> {
    auth: Option<Arc<DeviceAuth<S>>>,
    account: Arc<Mutex<Account>>,
    runtime: Option<Arc<crate::runtime::Runtime>>,
    #[cfg(test)]
    test_api: Option<crate::Api>,
    browser: Option<Arc<dyn crate::browser::Handoff>>,
    connection_gate: Arc<Mutex<Option<String>>>,
    projection: Arc<Mutex<Option<State>>>,
}
impl<S: CredentialStore + 'static> Clone for Service<S> {
    fn clone(&self) -> Self {
        Self {
            auth: self.auth.clone(),
            account: self.account.clone(),
            runtime: self.runtime.clone(),
            #[cfg(test)]
            test_api: self.test_api.clone(),
            browser: self.browser.clone(),
            connection_gate: self.connection_gate.clone(),
            projection: self.projection.clone(),
        }
    }
}
impl<S: CredentialStore + 'static> Service<S> {
    fn new(auth: Option<Arc<DeviceAuth<S>>>) -> Self {
        let configured = auth.is_some();
        Self {
            auth,
            runtime: None,
            #[cfg(test)]
            test_api: None,
            browser: None,
            connection_gate: Arc::default(),
            projection: Arc::new(Mutex::new(None)),
            account: Arc::new(Mutex::new(Account {
                epoch: 0,
                unlock_task: None,
                prompt_deadline: None,
                browser_request: None,
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
    fn from_initialization(auth: Result<Option<Arc<DeviceAuth<S>>>, ErrorCode>) -> Self {
        match auth {
            Ok(auth) => Self::new(auth),
            Err(error) => Self {
                account: Arc::new(Mutex::new(Account {
                    epoch: 0,
                    unlock_task: None,
                    prompt_deadline: None,
                    browser_request: None,
                    state: State {
                        backend: "unavailable".into(),
                        account: "unconfigured".into(),
                        error: Some(error),
                        ..State::default()
                    },
                })),
                ..Self::new(None)
            },
        }
    }
    fn runtime(mut self, runtime: Arc<crate::runtime::Runtime>) -> Self {
        self.runtime = Some(runtime);
        self
    }
    fn browser(mut self, browser: Arc<dyn crate::browser::Handoff>) -> Self {
        self.browser = Some(browser);
        self
    }
    async fn snapshot(&self) -> State {
        // Serialize projection so overlapping polls cannot publish older lifecycle state.
        let mut projection = self.projection.lock().await;
        let account = {
            let data = self.account.lock().await;
            let mut state = data.state.clone();
            if let (Some(prompt), Some(deadline)) = (&mut state.prompt, data.prompt_deadline) {
                prompt.expires_in = deadline
                    .saturating_duration_since(tokio::time::Instant::now())
                    .as_secs()
                    .max(1);
            }
            state
        };
        let mut state = if account.account == "connected"
            && let Some(runtime) = &self.runtime
        {
            let mut state = runtime.state().await;
            state.revision = state.revision.saturating_add(account.revision);
            if state.account != "expired" {
                state.account = account.account;
            }
            state.actions.push("disconnect".into());
            state
        } else {
            account
        };
        state.revision = projection.as_ref().map_or(0, |previous| previous.revision);
        if projection.as_ref() != Some(&state) {
            state.revision = state.revision.saturating_add(1);
        }
        *projection = Some(state.clone());
        state
    }
    async fn reconcile(&self) {
        let Some(auth) = &self.auth else {
            return;
        };
        let preserve = {
            let account = self.account.lock().await;
            ["connected", "connecting", "restricted", "unavailable"]
                .contains(&account.state.account.as_str())
                || account
                    .state
                    .actions
                    .iter()
                    .any(|action| action == "unlock")
        };
        if preserve || auth.api().await.is_err() {
            return;
        }
        if let Err(error) = auth.disconnect().await {
            let mut a = self.account.lock().await;
            a.state.error = Some(normalize(error));
            a.state.revision += 1;
        }
    }
    async fn connection_api(&self) -> Result<crate::Api, auth::Error> {
        let api = self
            .auth
            .as_ref()
            .ok_or(auth::Error::InvalidInput)?
            .api()
            .await?;
        #[cfg(test)]
        if let Some(fixture) = &self.test_api {
            let mut fixture = fixture.clone();
            fixture.lease = api.lease;
            fixture.expires_at = api.expires_at;
            fixture.deadline = api.deadline;
            return Ok(fixture);
        }
        Ok(api)
    }
    async fn finish_connection(&self, epoch: u64, api: Result<crate::Api, auth::Error>) {
        let result = match api {
            Ok(api) => match tokio::time::timeout(
                std::time::Duration::from_secs(30),
                api.check_music_access(),
            )
            .await
            {
                Ok(Ok(())) => Ok(api),
                Ok(Err(error)) => Err(error),
                Err(_) => Err(crate::Error::Transport),
            },
            Err(_) => Err(crate::Error::Authentication),
        };
        let mut account = self.account.lock().await;
        if account.epoch != epoch {
            drop(account);
            self.reconcile().await;
            return;
        }
        account.browser_request = None;
        account.unlock_task = None;
        account.state.prompt = None;
        account.prompt_deadline = None;
        account.state.revision += 1;
        match result {
            Ok(api) => {
                if let Some(runtime) = &self.runtime {
                    runtime.connected(api).await;
                }
                account.state.backend = "available".into();
                account.state.account = "connected".into();
                account.state.actions = vec!["disconnect".into()];
                account.state.error = None;
            }
            Err(error) => {
                account.state.backend = if error == crate::Error::Forbidden {
                    "unavailable"
                } else {
                    "degraded"
                }
                .into();
                account.state.account = match error {
                    crate::Error::Forbidden => "restricted",
                    crate::Error::Authentication => "expired",
                    _ => "unavailable",
                }
                .into();
                account.state.error = Some(match error {
                    crate::Error::Forbidden | crate::Error::Authentication => {
                        ErrorCode::PermissionDenied
                    }
                    _ => ErrorCode::Unavailable,
                });
                account.state.actions = vec!["connect".into(), "disconnect".into()];
            }
        }
    }
    async fn restore(&self, start: Option<auth::Start>) {
        let Some(auth) = &self.auth else {
            return;
        };
        let Some(start) = start else {
            return;
        };
        let result = auth.restore_prepared(start).await;
        self.finish_restore(0, result).await;
    }
    async fn finish_restore(&self, epoch: u64, result: Result<bool, auth::Error>) {
        let mut account = self.account.lock().await;
        if account.epoch != epoch {
            drop(account);
            if result == Ok(true) {
                self.reconcile().await;
            }
            return;
        }
        match result {
            Ok(true) => {
                account.state.account = "connecting".into();
                if account.unlock_task.is_none() {
                    account.state.actions = vec!["cancel".into()];
                }
                account.state.revision += 1;
                drop(account);
                self.finish_connection(epoch, self.connection_api().await)
                    .await;
            }
            outcome => {
                account.unlock_task = None;
                account.state.account = "disconnected".into();
                account.state.actions = vec![if outcome == Err(auth::Error::Locked) {
                    "unlock".into()
                } else {
                    "connect".into()
                }];
                account.state.error = outcome.err().map(normalize);
                account.state.revision += 1;
            }
        }
    }
    async fn unlock(&self) -> Result<(), ErrorCode> {
        let auth = self.auth.clone().ok_or(ErrorCode::Unsupported)?;
        let mut account = self.account.lock().await;
        if !account
            .state
            .actions
            .iter()
            .any(|action| action == "unlock")
        {
            return Err(ErrorCode::Conflict);
        }
        account.epoch += 1;
        account.state.revision += 1;
        account.state.account = "connecting".into();
        account.state.actions.clear();
        account.state.error = None;
        let (epoch, start) = (account.epoch, auth.prepare());
        let this = self.clone();
        let task = tokio::spawn(async move {
            match auth.unlock_storage().await {
                Ok(()) => {
                    this.finish_restore(epoch, auth.restore_prepared(start).await)
                        .await
                }
                Err(error) => {
                    let mut account = this.account.lock().await;
                    if account.epoch == epoch {
                        account.unlock_task = None;
                        account.state.account = "disconnected".into();
                        account.state.actions = vec!["unlock".into()];
                        account.state.error = Some(if error == auth::Error::Cancelled {
                            ErrorCode::PermissionDenied
                        } else {
                            normalize(error)
                        });
                        account.state.revision += 1;
                    }
                }
            }
        });
        account.unlock_task = Some(task.abort_handle());
        Ok(())
    }
    async fn connect(&self) -> Result<(), ErrorCode> {
        let auth = self.auth.clone().ok_or(ErrorCode::Unsupported)?;
        let browser = self.browser.clone().ok_or(ErrorCode::Unsupported)?;
        let reconnect = if let Some(runtime) = &self.runtime {
            runtime.expired().await
        } else {
            false
        };
        let (epoch, start) = {
            let mut a = self.account.lock().await;
            if a.unlock_task.is_some()
                || a.state.actions.iter().any(|action| action == "unlock")
                || (a.state.account == "connected" && !reconnect)
            {
                return Err(ErrorCode::Conflict);
            }
            auth.cancel();
            a.epoch += 1;
            a.state.revision += 1;
            a.state.account = "connecting".into();
            a.state.backend = "available".into();
            a.state.prompt = None;
            a.state.actions = vec!["cancel".into()];
            a.state.error = None;
            (a.epoch, auth.prepare_reconnect())
        };
        if let Some(runtime) = &self.runtime {
            runtime.disconnected().await;
        }
        let this = self.clone();
        tokio::spawn(async move {
            let mut cleanup = this.connection_gate.lock().await;
            if let Some(previous) = cleanup.as_deref()
                && browser.close(previous).await.is_ok()
            {
                *cleanup = None;
            }
            if this.account.lock().await.epoch != epoch {
                return;
            }
            let request = uuid::Uuid::new_v4().to_string();
            let mut opened = false;
            let mut signin_started = false;
            let result=async {
                let attempt=auth.start_prepared(start).await?;
                signin_started = true;
                let url=attempt.prompt.verification_url.clone();
                let requested = {
                    let mut a=this.account.lock().await;
                    if a.epoch!=epoch {return Err(auth::Error::Cancelled);}
                    let remaining=attempt.remaining_seconds().max(1);
                    a.state.prompt=Some(Prompt {verification_url:url.clone(),user_code:attempt.prompt.user_code.clone(),expires_in:remaining});
                    a.prompt_deadline=Some(tokio::time::Instant::now()+std::time::Duration::from_secs(remaining));
                    a.state.revision += 1;
                    // A retained browser lease fences only another browser open.
                    // The code can still be confirmed on a phone.
                    if cleanup.is_none() {
                        let (open, requested) = tokio::sync::oneshot::channel();
                        a.browser_request = Some(open);
                        a.state.actions = vec!["authorize".into(), "cancel".into()];
                        Some(requested)
                    } else {
                        a.browser_request = None;
                        a.state.actions = vec!["cancel".into()];
                        None
                    }
                };
                let Some(requested) = requested else {
                    return auth.complete(attempt).await;
                };
                let completion=auth.complete(attempt);tokio::pin!(completion);
                tokio::select! {
                    result=&mut completion=>result,
                    requested=requested=>{
                        requested.map_err(|_|auth::Error::Cancelled)?;
                        *cleanup = Some(request.clone());
                        browser.open(&url,&request).await.map_err(|_|auth::Error::Unavailable)?;
                        opened=true;
                        if this.account.lock().await.epoch!=epoch {return Err(auth::Error::Cancelled);}
                        completion.await
                    }
                }
            }.await;
            // An already dispatched window mutation is settled and closed before Home.
            let closed = if cleanup.as_deref() == Some(request.as_str()) {
                let result = browser.close(&request).await;
                if result.is_ok() {
                    *cleanup = None;
                }
                result
            } else {
                Ok(())
            };
            let result = result.and_then(|_| {
                closed
                    .as_ref()
                    .map(|_| ())
                    .map_err(|_| auth::Error::Unavailable)
            });
            if this.account.lock().await.epoch != epoch {
                this.reconcile().await;
                return;
            }
            {
                let mut a = this.account.lock().await;
                a.browser_request = None;
                a.state.prompt = None;
                a.prompt_deadline = None;
                a.state.actions = vec!["cancel".into()];
                a.state.revision += 1;
            }
            match result {
                Ok(()) => {
                    this.finish_connection(epoch, this.connection_api().await)
                        .await
                }
                Err(error) => {
                    let mut a = this.account.lock().await;
                    if a.epoch == epoch {
                        a.state.revision += 1;
                        a.state.account = if error == auth::Error::Expired {
                            "expired"
                        } else {
                            "disconnected"
                        }
                        .into();
                        a.state.actions =
                            vec![if error == auth::Error::Locked && !signin_started {
                                "unlock".into()
                            } else {
                                "connect".into()
                            }];
                        a.state.error = Some(normalize(error));
                    }
                }
            }
            let current = this.account.lock().await.epoch == epoch;
            if opened
                && closed.is_ok()
                && current
                && let Err(error) = browser.home(&request).await
            {
                let mut a = this.account.lock().await;
                if a.epoch == epoch {
                    a.state.error = Some(error);
                    a.state.revision += 1;
                }
            }
            this.reconcile().await;
        });
        Ok(())
    }
    async fn authorize(&self) -> Result<(), ErrorCode> {
        let mut a = self.account.lock().await;
        if a.state.account != "connecting" {
            return Err(ErrorCode::Conflict);
        }
        a.browser_request
            .take()
            .ok_or(ErrorCode::Conflict)?
            .send(())
            .map_err(|_| ErrorCode::Conflict)?;
        a.state.actions = vec!["cancel".into()];
        a.state.revision += 1;
        Ok(())
    }
    async fn cancel(&self) {
        let mut a = self.account.lock().await;
        if a.state.account != "connecting" || a.unlock_task.is_some() {
            return;
        }
        if let Some(auth) = &self.auth {
            let error = auth.disconnect().await.err();
            a.state.error = error.map(normalize);
        }
        a.epoch += 1;
        a.state.revision += 1;
        a.state.prompt = None;
        a.browser_request = None;
        a.state.account = "disconnected".into();
        a.state.actions = vec!["connect".into()];
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
impl<S: CredentialStore + 'static> TextsHandler for Service<S> {
    fn catalog(&self) -> arinn_protocol::localization::Catalog {
        arinn_music_common::fixed_texts::catalog(
            "yandex-music",
            &[
                (
                    "provider.name",
                    "Yandex Music",
                    "The installed source name, not content metadata",
                ),
                (
                    "station.wave",
                    "My Wave",
                    "The provider's fixed personal radio station",
                ),
                (
                    "station.track-radio",
                    "Track radio",
                    "The fixed label of a live track radio session",
                ),
            ],
        )
    }
    fn translations(&self) -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([("ru".into(), include_bytes!("../i18n/ru.qm").to_vec())])
    }
    fn resolve(
        &self,
        _generation: String,
        requests: Vec<arinn_protocol::localization::TextRequest>,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<arinn_protocol::localization::ResolvedText>, ErrorCode>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            use arinn_music_common::fixed_texts::{provider_name, resolved};
            use arinn_protocol::localization::Scope;
            // Read the last assigned projection, never call snapshot() or
            // materialize a catalogue/HTTP request just for localization.
            let state = self.projection.lock().await.clone();
            let mut result = Vec::new();
            for (index, request) in requests.iter().enumerate() {
                let key = if provider_name(request, "Yandex Music") {
                    Some("provider.name")
                } else if let Some(runtime) = &self.runtime {
                    match request.scope {
                        Scope::MusicEntity => runtime.fixed_entity(request).await,
                        Scope::MusicStation => {
                            if let Some(state) = &state {
                                runtime.fixed_station(state, request).await
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(key) = key {
                    result.push(resolved(index, request, key));
                }
            }
            Ok(result)
        })
    }
}
impl<S: CredentialStore + 'static> MusicHandler for Service<S> {
    fn supports_assets(&self) -> bool {
        true
    }
    fn asset(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<arinn_protocol::assets::Asset, ErrorCode>> + Send + '_>>
    {
        Box::pin(async move {
            self.runtime
                .as_ref()
                .ok_or(ErrorCode::Unsupported)?
                .artwork(&id)
                .await
        })
    }
    fn state(&self) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>> {
        Box::pin(async move { Ok(self.snapshot().await) })
    }
    fn catalogue(
        &self,
        query: Query,
    ) -> Pin<Box<dyn Future<Output = Result<MusicCatalogue, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            if self.account.lock().await.state.account != "connected" {
                return Err(ErrorCode::PermissionDenied);
            }
            self.runtime
                .as_ref()
                .ok_or(ErrorCode::Unsupported)?
                .catalogue(query)
                .await
        })
    }
    fn command(
        &self,
        command: Command,
    ) -> Pin<Box<dyn Future<Output = Result<State, ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            match command.action.as_str() {
                "connect" => self.connect().await?,
                "unlock" => self.unlock().await?,
                "authorize" => self.authorize().await?,
                "cancel" => self.cancel().await,
                "disconnect" => {
                    let auth = self.auth.as_ref().ok_or(ErrorCode::Unsupported)?;
                    {
                        let mut a = self.account.lock().await;
                        if a.unlock_task.is_some()
                            || a.state.actions.iter().any(|action| action == "unlock")
                        {
                            return Err(ErrorCode::Conflict);
                        }
                        auth.cancel();
                        a.epoch += 1;
                        a.state.prompt = None;
                        a.state.account = "disconnected".into();
                        a.state.actions = vec!["connect".into()];
                        a.state.revision += 1;
                    }
                    if let Some(runtime) = &self.runtime {
                        runtime.disconnected().await;
                    }
                    if let Err(error) = auth.disconnect().await {
                        let mut account = self.account.lock().await;
                        account.state.error = Some(normalize(error));
                        account.state.revision += 1;
                        return Err(normalize(error));
                    }
                }
                _ => {
                    if self.account.lock().await.state.account != "connected" {
                        return Err(ErrorCode::PermissionDenied);
                    }
                    // Do not cancel a command after local commit has begun.
                    // Network preparation has its own pre-commit deadline;
                    // daemon retries settle long-running owned mutations.
                    self.runtime
                        .as_ref()
                        .ok_or(ErrorCode::Unsupported)?
                        .command(&command)
                        .await?;
                }
            }
            Ok(self.snapshot().await)
        })
    }
    fn reset(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            {
                let mut account = self.account.lock().await;
                if let Some(task) = account.unlock_task.take() {
                    task.abort();
                    account.epoch += 1;
                    account.state.revision += 1;
                    account.state.account = "disconnected".into();
                    account.state.actions = vec!["unlock".into()];
                    account.state.error = Some(ErrorCode::PermissionDenied);
                }
            }
            self.cancel().await;
            if let Some(runtime) = &self.runtime {
                let _ = runtime.playback.stop().await;
            }
        })
    }
}

pub async fn serve(connection: zbus::Connection) -> Result<(), ErrorCode> {
    let provider = std::env::var("ARINN_PROVIDER_ID").map_err(|_| ErrorCode::InvalidArgument)?;
    // Client ID is a public deployment identity, never a user token or a
    // borrowed third-party application identity. Secret-requiring clients need
    // separately qualified provisioning before being supported here.
    let client_id =
        Some(std::env::var("ARINN_YANDEX_CLIENT_ID").unwrap_or_else(|_| DEFAULT_CLIENT_ID.into()))
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
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
        let client_secret = (client_id == crate::compatibility::CLIENT_ID)
            .then_some(crate::compatibility::CLIENT_SECRET);
        DeviceAuth::new(&client_id, client_secret, &device, store)
            .map(|auth| Some(Arc::new(auth)))
            .map_err(normalize)
    })
    .await
    .map_err(|_| ErrorCode::Internal)?;
    let directory = paths::provider_state(
        &std::env::var("ARINN_PROVIDER_ID").map_err(|_| ErrorCode::InvalidArgument)?,
    )?;
    let pins = Storage::open(&directory.join("music-pins")).map_err(|_| ErrorCode::Unavailable)?;
    let player = arinn_music_common::player::Player::new(
        &paths::runtime()?,
        std::path::PathBuf::from("mpv"),
    )?;
    let runtime = crate::runtime::Runtime::new(player, pins)?;
    let values = startup_settings(&crate::settings::schema());
    runtime
        .set_prefetch_tracks(crate::settings::count(&values)?)
        .await?;
    let browser = Arc::new(crate::browser::Browser::new(connection.clone()).await?);
    let handler = Service::from_initialization(auth)
        .runtime(runtime.clone())
        .browser(browser);
    let restore_start = handler.auth.as_ref().map(|auth| auth.prepare());
    let restore = handler.clone();
    tokio::spawn(async move {
        restore.restore(restore_start).await;
    });
    let service = MusicService::new(handler.clone());
    serve_texts(&connection, handler.clone(), service.registrations())
        .await
        .map_err(|_| ErrorCode::Internal)?;
    serve_settings(
        &connection,
        crate::settings::Settings(runtime),
        service.registrations(),
    )
    .await
    .map_err(|_| ErrorCode::Internal)?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| ErrorCode::Unavailable)?;
    let result = tokio::select! {result=run_music_on(connection,service)=>result.map_err(|_|ErrorCode::Unavailable),_=terminate.recv()=>Ok(()),_=tokio::signal::ctrl_c()=>Ok(())};
    handler.cancel().await;
    if let Some(runtime) = &handler.runtime {
        runtime.disconnected().await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn fixed_provider_name_is_available_without_connecting_or_reading_content() {
        use arinn_protocol::localization::{Scope, TextRequest, validate_resolved};
        let service = Service::<crate::auth::tests::MemoryStore>::new(None);
        let name = TextRequest {
            scope: Scope::Provider,
            object_id: "self".into(),
            revision: 0,
            field: "name".into(),
            fallback_text: "Yandex Music".into(),
        };
        let requests = vec![
            name,
            TextRequest {
                scope: Scope::MusicEntity,
                object_id: "track:fixture".into(),
                revision: 1,
                field: "title".into(),
                fallback_text: "My Wave".into(),
            },
        ];
        let result = TextsHandler::resolve(&service, "authorized-by-sdk".into(), requests.clone())
            .await
            .unwrap();
        assert_eq!(
            result.iter().map(|r| r.request_index).collect::<Vec<_>>(),
            [0]
        );
        let catalog = TextsHandler::catalog(&service);
        validate_resolved(&catalog, &requests, &result).unwrap();
        assert_eq!(catalog.locales, ["ru"]);
        assert!(!TextsHandler::translations(&service)["ru"].is_empty());
        assert!(service.projection.lock().await.is_none());
    }
    #[tokio::test]
    async fn account_lifecycle_never_decreases_projected_revision() {
        let service: Service = Service::new(None);
        {
            let mut a = service.account.lock().await;
            a.state.account = "connected".into();
            a.state.revision = 100;
        }
        let connected = service.snapshot().await;
        {
            let mut a = service.account.lock().await;
            a.state.account = "disconnected".into();
            a.state.revision = 2;
        }
        let disconnected = service.snapshot().await;
        assert!(disconnected.revision > connected.revision);
        assert_eq!(service.snapshot().await.revision, disconnected.revision);
        {
            service.account.lock().await.state.account = "connecting".into();
        }
        assert!(service.snapshot().await.revision > disconnected.revision);
    }
    #[tokio::test]
    async fn missing_client_identity_has_no_connect_action_or_secret_prompt() {
        let service: Service = Service::new(None);
        service.restore(None).await;
        service.cancel().await;
        let state = service.snapshot().await;
        assert_eq!(state.account, "unconfigured");
        assert!(state.actions.is_empty());
        assert!(state.prompt.is_none());
        assert!(state.validate().is_ok());
        assert_eq!(service.connect().await, Err(ErrorCode::Unsupported));
    }
    #[tokio::test]
    async fn an_accepted_session_hidden_by_a_cancel_is_removed_again() {
        use crate::auth::tests::{MemoryStore, Mock, code, reply, token};
        let mock = Mock::new(vec![reply(200, code()), reply(200, token())]);
        let store = MemoryStore::default();
        let service = Service::new(Some(Arc::new(mock.client(store.clone()))));
        let auth = service.auth.clone().unwrap();
        let attempt = auth.start().await.unwrap();
        auth.complete(attempt).await.unwrap();
        service.account.lock().await.state.account = "connected".into();
        let auth = service.auth.clone().unwrap();
        assert!(auth.api().await.is_ok());
        assert!(store.bytes.lock().unwrap().is_some());
        // What a Cancel landing just after acceptance used to leave behind.
        {
            let mut a = service.account.lock().await;
            a.epoch += 1;
            a.state.account = "disconnected".into();
            a.state.actions = vec!["connect".into()];
        }
        service.reconcile().await;
        assert!(auth.api().await.is_err());
        assert!(store.bytes.lock().unwrap().is_none());
        // A shown connection is left alone.
        let state = service.snapshot().await;
        assert_eq!(state.account, "disconnected");
        mock.finish();
    }
    #[tokio::test]
    async fn invalid_client_configuration_projects_an_unavailable_source() {
        use crate::auth::tests::MemoryStore;
        let init = DeviceAuth::new(
            "invalid client",
            None,
            "synthetic-device",
            MemoryStore::default(),
        )
        .map(|auth| Some(Arc::new(auth)))
        .map_err(normalize);
        let service = Service::from_initialization(init);
        let state = service.snapshot().await;
        assert_eq!(state.backend, "unavailable");
        assert_eq!(state.account, "unconfigured");
        assert!(state.actions.is_empty());
        assert!(state.error.is_some());
        assert!(state.validate().is_ok());
        assert_eq!(service.connect().await, Err(ErrorCode::Unsupported));
    }
    #[tokio::test]
    async fn oauth_success_does_not_hide_music_api_forbidden() {
        use crate::auth::tests::MemoryStore;
        for account_denied in [true, false] {
            let mut responses = vec![];
            if !account_denied {
                responses.push((
                    200,
                    serde_json::json!({"result":{"account":{"uid":123}}}).to_string(),
                ));
            }
            responses.push((403, "{}".into()));
            let mock = crate::tests::Mock::new(responses);
            let auth = DeviceAuth::new(
                "synthetic-client",
                None,
                "synthetic-device",
                MemoryStore::default(),
            )
            .unwrap();
            let service = Service::new(Some(Arc::new(auth)));
            {
                let mut account = service.account.lock().await;
                account.epoch = 7;
                account.state.account = "connecting".into();
            }
            service.finish_connection(7, Ok(mock.api.clone())).await;
            let state = service.snapshot().await;
            assert_eq!(state.account, "restricted");
            assert_eq!(state.backend, "unavailable");
            assert_eq!(state.error, Some(ErrorCode::PermissionDenied));
            assert!(state.facets.is_empty());
            assert!(state.actions.contains(&"disconnect".to_owned()));
            assert!(state.validate().is_ok());
            let requests = mock.finish();
            assert_eq!(requests.len(), if account_denied { 1 } else { 2 });
        }
    }
    #[tokio::test]
    async fn only_a_confirmed_music_api_accepts_the_connection() {
        use crate::auth::tests::MemoryStore;
        let mock = crate::tests::Mock::new(vec![
            (
                200,
                serde_json::json!({"result":{"account":{"uid":123}}}).to_string(),
            ),
            (
                200,
                serde_json::json!({"result":{"library":{"tracks":[]}}}).to_string(),
            ),
        ]);
        let auth = DeviceAuth::new(
            "synthetic-client",
            None,
            "synthetic-device",
            MemoryStore::default(),
        )
        .unwrap();
        let service = Service::new(Some(Arc::new(auth)));
        service.finish_connection(0, Ok(mock.api.clone())).await;
        let state = service.snapshot().await;
        assert_eq!(state.account, "connected");
        assert_eq!(state.error, None);
        assert_eq!(mock.finish().len(), 2);
    }
    #[tokio::test]
    async fn locked_account_recovers_the_saved_grant_without_starting_sign_in() {
        use crate::auth::tests::MemoryStore;
        use std::sync::atomic::{AtomicU8, Ordering};

        struct LockedStore {
            saved: MemoryStore,
            prompt: Arc<AtomicU8>,
        }
        impl CredentialStore for LockedStore {
            async fn load(&self) -> Result<Option<auth::Credentials>, auth::Error> {
                if self.prompt.load(Ordering::SeqCst) < 2 {
                    return Err(auth::Error::Locked);
                }
                self.saved.load().await
            }
            async fn save(&self, _: &auth::Credentials) -> Result<(), auth::Error> {
                panic!("Recovery must never save an OAuth grant")
            }
            async fn clear(&self) -> Result<(), auth::Error> {
                panic!("Recovery must never clear a valid OAuth grant")
            }
            async fn unlock(&self, _: Option<&str>) -> Result<(), auth::Error> {
                match self.prompt.fetch_add(1, Ordering::SeqCst) {
                    0 => Err(auth::Error::Cancelled),
                    1 => Ok(()),
                    _ => panic!("Recovery must issue one prompt per click"),
                }
            }
        }

        let music = crate::tests::Mock::new(vec![
            (
                200,
                serde_json::json!({"result":{"account":{"uid":123}}}).to_string(),
            ),
            (
                200,
                serde_json::json!({"result":{"library":{"tracks":[]}}}).to_string(),
            ),
        ]);
        let bytes = serde_json::to_vec(&serde_json::json!({
            "nonce": uuid::Uuid::new_v4().to_string(),
            "access_token": "synthetic-token",
            "client_id": "synthetic-client",
        }))
        .unwrap();
        let saved = MemoryStore::default();
        *saved.bytes.lock().unwrap() = Some(bytes.clone());
        let prompt = Arc::new(AtomicU8::new(0));
        let store = LockedStore {
            saved: saved.clone(),
            prompt: prompt.clone(),
        };
        let auth = Arc::new(DeviceAuth::new("synthetic-client", None, "device", store).unwrap());
        let browser = Arc::new(BrowserFixture::default());
        let mut service = Service::new(Some(auth.clone())).browser(browser.clone());
        service.test_api = Some(music.api.clone());
        service.restore(Some(auth.prepare())).await;
        assert_eq!(service.snapshot().await.actions, ["unlock"]);
        assert_eq!(prompt.load(Ordering::SeqCst), 0);

        service.unlock().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while service.snapshot().await.actions != ["unlock"] {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(*saved.bytes.lock().unwrap(), Some(bytes.clone()));

        service.unlock().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while service.snapshot().await.account != "connected" {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(*saved.bytes.lock().unwrap(), Some(bytes));
        assert!(browser.calls.lock().unwrap().is_empty());
        assert_eq!(music.finish().len(), 2);
    }
    #[derive(Default)]
    struct BrowserFixture {
        calls: std::sync::Mutex<Vec<(String, String)>>,
        fail_close: std::sync::atomic::AtomicBool,
    }
    impl crate::browser::Handoff for BrowserFixture {
        fn open<'a>(&'a self, _url: &'a str, id: &'a str) -> crate::browser::Work<'a> {
            self.calls.lock().unwrap().push(("open".into(), id.into()));
            Box::pin(async { Ok(()) })
        }
        fn close<'a>(&'a self, id: &'a str) -> crate::browser::Work<'a> {
            self.calls.lock().unwrap().push(("close".into(), id.into()));
            let fail = self
                .fail_close
                .swap(false, std::sync::atomic::Ordering::AcqRel);
            Box::pin(async move {
                if fail {
                    Err(ErrorCode::Unavailable)
                } else {
                    Ok(())
                }
            })
        }
        fn home<'a>(&'a self, id: &'a str) -> crate::browser::Work<'a> {
            self.calls.lock().unwrap().push(("home".into(), id.into()));
            Box::pin(async { Ok(()) })
        }
    }
    async fn wait_for_prompt(service: &Service<crate::auth::tests::MemoryStore>) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while service.snapshot().await.prompt.is_none() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn browser_confirmation_checks_music_and_returns_only_after_window_cleanup() {
        use crate::auth::tests::{MemoryStore, Mock, code, reply, token};
        let oauth = Mock::new(vec![reply(200, code()), reply(200, token())]);
        let music = crate::tests::Mock::new(vec![
            (
                200,
                serde_json::json!({"result":{"account":{"uid":123}}}).to_string(),
            ),
            (
                200,
                serde_json::json!({"result":{"library":{"tracks":[]}}}).to_string(),
            ),
        ]);
        let browser = Arc::new(BrowserFixture::default());
        let store = MemoryStore::default();
        let mut service =
            Service::new(Some(Arc::new(oauth.client(store.clone())))).browser(browser.clone());
        service.test_api = Some(music.api.clone());
        service.connect().await.unwrap();
        wait_for_prompt(&service).await;
        assert!(
            service
                .snapshot()
                .await
                .actions
                .contains(&"authorize".into())
        );
        service.authorize().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            while !browser
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|(action, _)| action == "home")
            {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(service.snapshot().await.account, "connected");
        assert!(store.bytes.lock().unwrap().is_some());
        let calls = browser.calls.lock().unwrap().clone();
        assert_eq!(
            calls.iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(),
            vec!["open", "close", "home"]
        );
        assert!(calls.iter().all(|(_, id)| id == &calls[0].1));
        assert_eq!(oauth.finish().len(), 2);
        assert_eq!(music.finish().len(), 2);
    }
    #[tokio::test]
    async fn a_failed_close_is_retried_before_a_new_browser_attempt() {
        use crate::auth::tests::{MemoryStore, Mock, code, reply, token};
        let oauth = Mock::new(vec![
            reply(200, code()),
            reply(200, token()),
            reply(200, code()),
            reply(200, token()),
        ]);
        let music = crate::tests::Mock::new(vec![
            (
                200,
                serde_json::json!({"result":{"account":{"uid":123}}}).to_string(),
            ),
            (
                200,
                serde_json::json!({"result":{"library":{"tracks":[]}}}).to_string(),
            ),
        ]);
        let browser = Arc::new(BrowserFixture::default());
        browser
            .fail_close
            .store(true, std::sync::atomic::Ordering::Release);
        let mut service = Service::new(Some(Arc::new(oauth.client(MemoryStore::default()))))
            .browser(browser.clone());
        service.test_api = Some(music.api.clone());
        service.connect().await.unwrap();
        wait_for_prompt(&service).await;
        service.authorize().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            while service.snapshot().await.account == "connecting" {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(service.connection_gate.lock().await.is_some());
        assert_eq!(service.snapshot().await.account, "disconnected");
        service.connect().await.unwrap();
        wait_for_prompt(&service).await;
        service.authorize().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            while !browser
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|(action, _)| action == "home")
            {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let calls = browser.calls.lock().unwrap().clone();
        assert_eq!(
            calls.iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(),
            vec!["open", "close", "close", "open", "close", "home"]
        );
        assert_eq!(calls[0].1, calls[2].1);
        assert_ne!(calls[0].1, calls[3].1);
        assert!(service.connection_gate.lock().await.is_none());
        assert_eq!(oauth.finish().len(), 4);
        assert_eq!(music.finish().len(), 2);
    }

    #[tokio::test]
    async fn a_retained_browser_lease_does_not_block_phone_sign_in() {
        use crate::auth::tests::{MemoryStore, Mock, code, reply, token};
        let oauth = Mock::new(vec![reply(200, code()), reply(200, token())]);
        let music = crate::tests::Mock::new(vec![
            (
                200,
                serde_json::json!({"result":{"account":{"uid":123}}}).to_string(),
            ),
            (
                200,
                serde_json::json!({"result":{"library":{"tracks":[]}}}).to_string(),
            ),
        ]);
        let browser = Arc::new(BrowserFixture::default());
        browser
            .fail_close
            .store(true, std::sync::atomic::Ordering::Release);
        let mut service = Service::new(Some(Arc::new(oauth.client(MemoryStore::default()))))
            .browser(browser.clone());
        service.test_api = Some(music.api.clone());
        *service.connection_gate.lock().await = Some("previous-request".into());

        service.connect().await.unwrap();
        wait_for_prompt(&service).await;
        assert_eq!(service.snapshot().await.actions, vec!["cancel"]);
        assert_eq!(service.authorize().await, Err(ErrorCode::Conflict));
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            while service.snapshot().await.account == "connecting" {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(service.snapshot().await.account, "connected");
        assert_eq!(
            service.connection_gate.lock().await.as_deref(),
            Some("previous-request")
        );
        assert_eq!(
            *browser.calls.lock().unwrap(),
            vec![(String::from("close"), String::from("previous-request"))]
        );
        assert_eq!(oauth.finish().len(), 2);
        assert_eq!(music.finish().len(), 2);
    }
}
