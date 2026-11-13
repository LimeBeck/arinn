// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider-private device-code OAuth. No tokens or device credentials in UI state.
use crate::Api;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{Mutex, watch},
    time::Instant,
};

const RESPONSE_LIMIT: usize = 16 * 1024;
// Keep the canonical address within the shell's verification QR byte limit.
const VERIFICATION_URL_LIMIT: usize = 2048;
const PENDING: u8 = 0;
pub(crate) const ACCEPTED: u8 = 1;
const REVOKED: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    InvalidResponse,
    Transport,
    Unavailable,
    Denied,
    Expired,
    Cancelled,
    ClientRejected,
    Storage,
    Locked,
}

/// Sensitive, deliberately without Debug or public token accessors.
#[derive(Serialize, Deserialize)]
pub struct Credentials {
    nonce: String,
    access_token: String,
    expires_at: Option<u64>,
    #[serde(default)]
    client_id: Option<String>,
}
impl Credentials {
    pub(crate) fn nonce(&self) -> &str {
        &self.nonce
    }
    fn valid(&self) -> bool {
        secret(&self.access_token) && self.expires_at.is_none_or(|at| at > epoch())
    }
    pub(crate) fn encode(&self) -> Result<Vec<u8>, Error> {
        serde_json::to_vec(self).map_err(|_| Error::Storage)
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > RESPONSE_LIMIT {
            return Err(Error::Storage);
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| Error::Storage)?;
        if !secret(&value.access_token) || uuid::Uuid::parse_str(&value.nonce).is_err() {
            return Err(Error::Storage);
        }
        Ok(value)
    }
}

/// Implementations must never log credentials or fall back to plaintext files.
pub trait CredentialStore: Send + Sync {
    fn load(&self) -> impl Future<Output = Result<Option<Credentials>, Error>> + Send;
    fn save(&self, credentials: &Credentials) -> impl Future<Output = Result<(), Error>> + Send;
    fn clear(&self) -> impl Future<Output = Result<(), Error>> + Send;
    fn application_secret(
        &self,
        _client_id: &str,
    ) -> impl Future<Output = Result<Option<String>, Error>> + Send {
        async { Ok(None) }
    }
    fn unlock(&self, _client_id: Option<&str>) -> impl Future<Output = Result<(), Error>> + Send {
        async { Err(Error::Locked) }
    }
    /// Synchronous local approval checkpoint after the remote write succeeds.
    fn commit(&self, _credentials: &Credentials) -> Result<(), Error> {
        Ok(())
    }
    /// Revoke local approval before any fallible/late remote removal. A nonce
    /// limits cleanup to its own attempt; None revokes the current account.
    fn revoke(&self, _nonce: Option<&str>) -> Result<(), Error> {
        Ok(())
    }
}

/// Safe bounded projection for the connection screen, not a secret-bearing session.
#[derive(Clone, PartialEq, Eq)]
pub struct Prompt {
    pub verification_url: String,
    pub user_code: String,
    pub expires_in: u64,
}

/// An attempt reserved synchronously before spawning network work.
pub struct Start {
    replace: bool,
    owner: Arc<()>,
    generation: u64,
}

/// Caller retains an attempt, not its device code. A newer start invalidates it.
pub struct Attempt {
    owner: Arc<()>,
    generation: u64,
    device_code: String,
    client_secret: Option<String>,
    pub prompt: Prompt,
    deadline: Instant,
    interval: Duration,
}
impl Attempt {
    pub fn remaining_seconds(&self) -> u64 {
        self.deadline
            .saturating_duration_since(Instant::now())
            .as_secs()
    }

    fn slow_down(&mut self) {
        // Expiry already bounds sleep; never retain a rate rejected by OAuth.
        self.interval = self.interval.saturating_add(Duration::from_secs(5));
    }
}

/// The caller supplies the client identity; the controller bundles none.
pub struct DeviceAuth<S> {
    client: Client,
    origin: Url,
    client_id: String,
    client_secret: Option<String>,
    device_id: String,
    owner: Arc<()>,
    session: Arc<Session<S>>,
}

struct Active {
    lease: Arc<AtomicU8>,
    generation: u64,
    credentials: Credentials,
}
struct Session<S> {
    generation: watch::Sender<u64>,
    gate: Mutex<Option<Active>>,
    store: S,
}
impl<S> Session<S> {
    fn current(&self, generation: u64) -> Result<(), Error> {
        if *self.generation.borrow() == generation {
            Ok(())
        } else {
            Err(Error::Cancelled)
        }
    }
}

// Dropping a caller must not abandon a Secret Service write already submitted.
// Owned commit/cleanup tasks outlive that caller and serialize through gate.
struct Lease<S: CredentialStore + 'static> {
    session: Arc<Session<S>>,
    generation: u64,
    completed: bool,
    nonce: Option<String>,
    state: Arc<AtomicU8>,
}
impl<S: CredentialStore + 'static> Drop for Lease<S> {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        self.state.store(REVOKED, Ordering::Release); // revoked before deferred cleanup
        let session = self.session.clone();
        let generation = self.generation;
        session.generation.send_if_modified(|n| {
            if *n == generation {
                *n = n.wrapping_add(1);
                true
            } else {
                false
            }
        });
        if let Some(nonce) = &self.nonce {
            let _ = session.store.revoke(Some(nonce));
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let mut active = session.gate.lock().await;
                if active.as_ref().is_some_and(|a| a.generation == generation) {
                    *active = None;
                    let _ = session.store.clear().await;
                }
            });
        }
    }
}

impl<S: CredentialStore + 'static> DeviceAuth<S> {
    pub fn new(
        client_id: &str,
        client_secret: Option<&str>,
        device_id: &str,
        store: S,
    ) -> Result<Self, Error> {
        if !secret(client_id)
            || client_secret.is_some_and(|s| !secret(s))
            || !secret(device_id)
            || device_id.len() > 128
        {
            return Err(Error::InvalidInput);
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .user_agent("arinn-yandex-provider")
            .build()
            .map_err(|_| Error::Transport)?;
        let (generation, _) = watch::channel(0);
        Ok(Self {
            client,
            origin: Url::parse("https://oauth.yandex.ru/").map_err(|_| Error::InvalidInput)?,
            client_id: client_id.into(),
            client_secret: client_secret.map(str::to_owned),
            device_id: device_id.into(),
            owner: Arc::new(()),
            session: Arc::new(Session {
                generation,
                gate: Mutex::new(None),
                store,
            }),
        })
    }

    /// Cancels requests and polling immediately; it does not disconnect an
    /// already connected account. Use disconnect to remove stored credentials.
    pub fn cancel(&self) {
        self.advance();
    }
    fn advance(&self) -> u64 {
        let mut generation = 0;
        self.session.generation.send_modify(|n| {
            *n = n.wrapping_add(1);
            generation = *n;
        });
        generation
    }
    fn current(&self, generation: u64) -> Result<(), Error> {
        if *self.session.generation.borrow() == generation {
            Ok(())
        } else {
            Err(Error::Cancelled)
        }
    }

    pub fn prepare(&self) -> Start {
        Start {
            replace: false,
            owner: self.owner.clone(),
            generation: self.advance(),
        }
    }
    pub fn prepare_reconnect(&self) -> Start {
        Start {
            replace: true,
            owner: self.owner.clone(),
            generation: self.advance(),
        }
    }
    pub async fn start(&self) -> Result<Attempt, Error> {
        self.start_prepared(self.prepare()).await
    }
    pub async fn start_prepared(&self, start: Start) -> Result<Attempt, Error> {
        if !Arc::ptr_eq(&self.owner, &start.owner) {
            return Err(Error::InvalidInput);
        }
        let generation = start.generation;
        let client_secret = self.guard(generation, self.application_secret()).await?;
        {
            let mut active = self.session.gate.lock().await;
            self.current(generation)?;
            if active.as_ref().is_some_and(|a| {
                !start.replace
                    && a.lease.load(Ordering::Acquire) == ACCEPTED
                    && a.credentials.valid()
            }) {
                return Err(Error::InvalidInput);
            }
            if let Some(previous) = active.take() {
                previous.lease.store(REVOKED, Ordering::Release);
                self.session
                    .store
                    .revoke(Some(previous.credentials.nonce()))?;
                self.session.store.clear().await?;
                self.current(generation)?;
            }
        }
        let begun = Instant::now();
        let value = self
            .guard(
                generation,
                self.post(
                    "device/code",
                    &[
                        ("client_id", self.client_id.as_str()),
                        ("device_id", &self.device_id),
                        ("device_name", "Arinn"),
                    ],
                ),
            )
            .await?;
        if let Some(error) = value.get("error").and_then(Value::as_str) {
            return Err(match error {
                "invalid_client" | "unauthorized_client" => Error::ClientRejected,
                "access_denied" => Error::Denied,
                _ => Error::Unavailable,
            });
        }
        let mut attempt = parse_code(value, generation)?;
        attempt.owner = self.owner.clone();
        attempt.client_secret = client_secret;
        // Include network time in code lifetime; never extend the server grant.
        attempt.deadline = begun + Duration::from_secs(attempt.prompt.expires_in);
        if Instant::now() >= attempt.deadline {
            return Err(Error::Expired);
        }
        Ok(attempt)
    }

    /// Wait for confirmation, save it in the provider's store, then connect.
    /// Callers never receive a token that they could accidentally export to QML.
    pub async fn complete(&self, mut attempt: Attempt) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.owner, &attempt.owner) {
            return Err(Error::InvalidInput);
        }
        let mut lease = Lease {
            session: self.session.clone(),
            generation: attempt.generation,
            completed: false,
            nonce: None,
            state: Arc::new(AtomicU8::new(PENDING)), // pending until completion is acknowledged
        };
        loop {
            self.current(attempt.generation)?;
            if Instant::now() >= attempt.deadline {
                return Err(Error::Expired);
            }
            self.guard(attempt.generation, async {
                tokio::time::sleep_until((Instant::now() + attempt.interval).min(attempt.deadline))
                    .await;
                Ok(())
            })
            .await?;
            if Instant::now() >= attempt.deadline {
                return Err(Error::Expired);
            }
            let mut fields = vec![
                ("grant_type", "device_code"),
                ("code", attempt.device_code.as_str()),
                ("client_id", self.client_id.as_str()),
            ];
            if let Some(secret) = &attempt.client_secret {
                fields.push(("client_secret", secret));
            }
            let response = self
                .guard(attempt.generation, async {
                    tokio::time::timeout_at(attempt.deadline, self.post("token", &fields))
                        .await
                        .map_err(|_| Error::Expired)?
                })
                .await?;
            match response.get("error").and_then(Value::as_str) {
                Some("authorization_pending") => continue,
                Some("slow_down") => {
                    attempt.slow_down();
                    continue;
                }
                Some("access_denied") => return Err(Error::Denied),
                Some("expired_token") => return Err(Error::Expired),
                Some("invalid_client" | "unauthorized_client") => {
                    return Err(Error::ClientRejected);
                }
                Some(_) => return Err(Error::Unavailable),
                None => {}
            }
            let mut credentials = parse_token(&response)?;
            credentials.client_id = Some(self.client_id.clone());
            lease.nonce = Some(credentials.nonce.clone());
            let session = self.session.clone();
            let generation = attempt.generation;
            let deadline = attempt.deadline;
            let lease_state = lease.state.clone();
            let task = tokio::spawn(async move {
                let mut active = session.gate.lock().await;
                session.current(generation)?;
                if Instant::now() >= deadline {
                    return Err(Error::Expired);
                }
                if let Err(error) = session.store.save(&credentials).await {
                    *active = None;
                    // A failed IPC reply does not prove that the service did
                    // not commit the secret. Remove any ambiguous result.
                    session.store.clear().await?;
                    return Err(error);
                }
                if session.current(generation).is_err() || Instant::now() >= deadline {
                    *active = None;
                    session.store.clear().await?;
                    return Err(Error::Cancelled);
                }
                // A failed checkpoint commit must not leave a live token in
                // the keyring that nothing will activate or remove.
                if let Err(error) = session.store.commit(&credentials) {
                    *active = None;
                    let _ = session.store.revoke(Some(&credentials.nonce));
                    session.store.clear().await?;
                    return Err(error);
                }
                if session.current(generation).is_err() {
                    session.store.revoke(Some(&credentials.nonce))?;
                    session.store.clear().await?;
                    return Err(Error::Cancelled);
                }
                *active = Some(Active {
                    lease: lease_state,
                    generation,
                    credentials,
                });
                Ok(())
            });
            task.await.map_err(|_| Error::Unavailable)??;
            let mut acknowledged = false;
            self.session.generation.send_if_modified(|current| {
                if *current == generation && lease.state.load(Ordering::Acquire) == PENDING {
                    lease.state.store(ACCEPTED, Ordering::Release); // accepted
                    acknowledged = true;
                }
                false
            });
            if !acknowledged {
                return Err(Error::Cancelled);
            }
            lease.completed = true;
            return Ok(());
        }
    }

    pub async fn restore(&self) -> Result<bool, Error> {
        self.restore_prepared(self.prepare()).await
    }
    pub async fn restore_prepared(&self, start: Start) -> Result<bool, Error> {
        if !Arc::ptr_eq(&self.owner, &start.owner) {
            return Err(Error::InvalidInput);
        }
        let generation = start.generation;
        self.guard(generation, self.application_secret()).await?;
        let mut active = self.session.gate.lock().await;
        self.current(generation)?;
        if active
            .as_ref()
            .is_some_and(|a| a.lease.load(Ordering::Acquire) != ACCEPTED)
        {
            let previous = active.take().ok_or(Error::Unavailable)?;
            previous.lease.store(REVOKED, Ordering::Release);
            self.session
                .store
                .revoke(Some(previous.credentials.nonce()))?;
            self.session.store.clear().await?;
            return Ok(false);
        }
        let saved = self.session.store.load().await?;
        self.current(generation)?;
        if saved
            .as_ref()
            .is_some_and(|s| !s.valid() || s.client_id.as_deref() != Some(self.client_id.as_str()))
        {
            self.session.store.clear().await?;
            *active = None;
            return Ok(false);
        }
        *active = saved.map(|credentials| Active {
            lease: Arc::new(AtomicU8::new(ACCEPTED)),
            generation,
            credentials,
        });
        Ok(active.is_some())
    }

    pub async fn unlock_storage(&self) -> Result<(), Error> {
        self.session
            .store
            .unlock(
                self.client_secret
                    .is_none()
                    .then_some(self.client_id.as_str()),
            )
            .await
    }

    async fn application_secret(&self) -> Result<Option<String>, Error> {
        if let Some(secret) = &self.client_secret {
            return Ok(Some(secret.clone()));
        }
        let value = self
            .session
            .store
            .application_secret(&self.client_id)
            .await?;
        if value.as_deref().is_some_and(|value| !secret(value)) {
            return Err(Error::InvalidInput);
        }
        Ok(value)
    }

    pub async fn disconnect(&self) -> Result<(), Error> {
        let generation = self.advance();
        let session = self.session.clone();
        tokio::spawn(async move {
            let mut active = session.gate.lock().await;
            session.current(generation)?;
            *active = None;
            session.store.revoke(None)?;
            session.store.clear().await
        })
        .await
        .map_err(|_| Error::Unavailable)?
    }

    /// Constructs the existing private API client without exposing its token.
    pub async fn api(&self) -> Result<Api, Error> {
        let active = self.session.gate.lock().await;
        let credentials = active
            .as_ref()
            .filter(|s| s.lease.load(Ordering::Acquire) == ACCEPTED)
            .map(|s| &s.credentials)
            .filter(|s| s.valid())
            .ok_or(Error::Expired)?;
        let lease = active.as_ref().ok_or(Error::Expired)?.lease.clone();
        Api::leased(&credentials.access_token, credentials.expires_at, lease)
            .map_err(|_| Error::Unavailable)
    }

    async fn guard<T>(
        &self,
        generation: u64,
        work: impl Future<Output = Result<T, Error>>,
    ) -> Result<T, Error> {
        let mut changed = self.session.generation.subscribe();
        self.current(generation)?;
        tokio::select! {
            biased;
            _ = changed.changed() => Err(Error::Cancelled),
            result = work => { self.current(generation)?; result }
        }
    }

    async fn post(&self, path: &str, fields: &[(&str, &str)]) -> Result<Value, Error> {
        let url = self.origin.join(path).map_err(|_| Error::InvalidInput)?;
        let mut response = self
            .client
            .post(url)
            .form(fields)
            .send()
            .await
            .map_err(|_| Error::Transport)?;
        let status = response.status();
        if status.is_redirection() || !(status.is_success() || status == StatusCode::BAD_REQUEST) {
            return Err(Error::Unavailable);
        }
        if response
            .content_length()
            .is_some_and(|n| n > RESPONSE_LIMIT as u64)
        {
            return Err(Error::InvalidResponse);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if chunk.len() > RESPONSE_LIMIT.saturating_sub(bytes.len()) {
                return Err(Error::InvalidResponse);
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?;
        if !value.is_object()
            || (!status.is_success() && !value.get("error").is_some_and(Value::is_string))
        {
            return Err(Error::InvalidResponse);
        }
        Ok(value)
    }
}

fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(u64::MAX)
}
fn secret(s: &str) -> bool {
    !s.is_empty() && s.len() <= 8192 && s.bytes().all(|b| b.is_ascii_graphic())
}
fn parse_code(v: Value, generation: u64) -> Result<Attempt, Error> {
    let device_code = v
        .get("device_code")
        .and_then(Value::as_str)
        .filter(|s| secret(s))
        .ok_or(Error::InvalidResponse)?
        .to_owned();
    let user_code = v
        .get("user_code")
        .and_then(Value::as_str)
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 64
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        .ok_or(Error::InvalidResponse)?
        .to_owned();
    let raw = v
        .get("verification_url")
        .and_then(Value::as_str)
        .ok_or(Error::InvalidResponse)?;
    // OAuth also returns this exact static alias for its device page. Keep
    // the projected destination canonical without following redirects.
    let raw = if raw == "https://ya.ru/device" {
        "https://oauth.yandex.ru/device"
    } else {
        raw
    };
    if raw.len() > VERIFICATION_URL_LIMIT {
        return Err(Error::InvalidResponse);
    }
    let url = Url::parse(raw).map_err(|_| Error::InvalidResponse)?;
    if url.as_str().len() > VERIFICATION_URL_LIMIT
        || url.scheme() != "https"
        || url.host_str() != Some("oauth.yandex.ru")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::InvalidResponse);
    }
    let expires = v
        .get("expires_in")
        .and_then(Value::as_u64)
        .filter(|n| (1..=1800).contains(n))
        .ok_or(Error::InvalidResponse)?;
    let interval = v
        .get("interval")
        .and_then(Value::as_u64)
        .filter(|n| (1..=120).contains(n))
        .ok_or(Error::InvalidResponse)?;
    Ok(Attempt {
        owner: Arc::new(()),
        generation,
        device_code,
        client_secret: None,
        prompt: Prompt {
            verification_url: url.to_string(),
            user_code,
            expires_in: expires,
        },
        deadline: Instant::now() + Duration::from_secs(expires),
        interval: Duration::from_secs(interval),
    })
}
fn parse_token(v: &Value) -> Result<Credentials, Error> {
    let token = v
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|s| secret(s))
        .ok_or(Error::InvalidResponse)?;
    if v.get("token_type")
        .is_some_and(|v| v.as_str().is_none_or(|s| !s.eq_ignore_ascii_case("bearer")))
    {
        return Err(Error::InvalidResponse);
    }
    let expires_at = match v.get("expires_in") {
        None => None,
        Some(v) => Some(
            epoch()
                .checked_add(
                    v.as_u64()
                        .filter(|s| *s > 0)
                        .ok_or(Error::InvalidResponse)?,
                )
                .ok_or(Error::InvalidResponse)?,
        ),
    };
    Ok(Credentials {
        nonce: uuid::Uuid::new_v4().to_string(),
        access_token: token.into(),
        expires_at,
        client_id: None,
    })
}

#[cfg(test)]
pub(crate) mod tests;
