// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Staged Secret Service grants plus an atomic local approval checkpoint.
//! The checkpoint contains a random nonce, never a token or account identity.
use crate::auth::{CredentialStore, Credentials, Error};
use arinn_music_common::storage::Storage;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Mutex,
    time::Duration,
};
use zbus::{
    Connection, Message,
    zvariant::{DynamicType, OwnedObjectPath, OwnedValue, Str, Type, Value},
};

const SERVICE: &str = "org.freedesktop.secrets";
const PATH: &str = "/org/freedesktop/secrets";
const INTERFACE: &str = "org.freedesktop.Secret.Service";
const CALL_LIMIT: Duration = Duration::from_secs(3);

#[derive(Serialize, Deserialize, Type)]
struct Secret(OwnedObjectPath, Vec<u8>, Vec<u8>, String);
#[derive(Serialize, Deserialize)]
struct Checkpoint {
    version: u32,
    nonce: Option<String>,
}

struct SessionGuard {
    connection: Connection,
    path: OwnedObjectPath,
    closed: bool,
}
impl SessionGuard {
    async fn close(&mut self) {
        let _ = tokio::time::timeout(
            CALL_LIMIT,
            self.connection.call_method(
                Some(SERVICE),
                self.path.as_str(),
                Some("org.freedesktop.Secret.Session"),
                "Close",
                &(),
            ),
        )
        .await;
        self.closed = true;
    }
}
impl Drop for SessionGuard {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let connection = self.connection.clone();
            let path = self.path.clone();
            runtime.spawn(async move {
                let _ = tokio::time::timeout(
                    CALL_LIMIT,
                    connection.call_method(
                        Some(SERVICE),
                        path.as_str(),
                        Some("org.freedesktop.Secret.Session"),
                        "Close",
                        &(),
                    ),
                )
                .await;
            });
        }
    }
}

struct Journal {
    storage: Storage,
    revoked: HashSet<String>,
    pending: HashSet<String>,
    deny_all: bool,
}

pub struct SecretService {
    connection: Connection,
    reference: String,
    journal: Mutex<Journal>,
}
impl SecretService {
    pub fn new(
        connection: Connection,
        reference: &str,
        checkpoint_directory: &Path,
    ) -> Result<Self, Error> {
        if reference.is_empty()
            || reference.len() > 128
            || !reference
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        {
            return Err(Error::InvalidInput);
        }
        let store = Self {
            connection,
            reference: reference.into(),
            journal: Mutex::new(Journal {
                storage: Storage::open(checkpoint_directory).map_err(|_| Error::Storage)?,
                revoked: HashSet::new(),
                pending: HashSet::new(),
                deny_all: false,
            }),
        };
        store.checkpoint()?; // Reject corrupt journals rather than reconnecting/resetting.
        Ok(store)
    }
    fn checkpoint(&self) -> Result<Option<String>, Error> {
        let storage = self.journal.lock().map_err(|_| Error::Storage)?;
        read_checkpoint(&storage.storage)
    }
    fn attributes(&self, nonce: Option<&str>) -> HashMap<String, String> {
        let mut attrs = HashMap::from([
            ("arinn".into(), "music-yandex".into()),
            ("credential".into(), self.reference.clone()),
        ]);
        if let Some(nonce) = nonce {
            attrs.insert("grant".into(), nonce.into());
        }
        attrs
    }
    async fn call<B: Serialize + DynamicType>(
        &self,
        path: &str,
        interface: &str,
        method: &str,
        body: &B,
    ) -> Result<Message, Error> {
        tokio::time::timeout(
            CALL_LIMIT,
            self.connection
                .call_method(Some(SERVICE), path, Some(interface), method, body),
        )
        .await
        .map_err(|_| Error::Storage)?
        .map_err(|_| Error::Storage)
    }
    async fn session(&self) -> Result<SessionGuard, Error> {
        let reply = self
            .call(
                PATH,
                INTERFACE,
                "OpenSession",
                &("plain", OwnedValue::from(Str::from(""))),
            )
            .await?;
        let (_, session): (OwnedValue, OwnedObjectPath) =
            reply.body().deserialize().map_err(|_| Error::Storage)?;
        Ok(SessionGuard {
            connection: self.connection.clone(),
            path: session,
            closed: false,
        })
    }
    async fn items(&self, nonce: Option<&str>) -> Result<Vec<OwnedObjectPath>, Error> {
        let reply = self
            .call(PATH, INTERFACE, "SearchItems", &self.attributes(nonce))
            .await?;
        let (unlocked, locked): (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) =
            reply.body().deserialize().map_err(|_| Error::Storage)?;
        if !locked.is_empty() {
            return Err(Error::Locked);
        }
        if unlocked.len() > if nonce.is_some() { 1 } else { 256 } {
            return Err(Error::Storage);
        }
        Ok(unlocked)
    }
}
fn read_checkpoint(storage: &Storage) -> Result<Option<String>, Error> {
    let Some(bytes) = storage.read().map_err(|_| Error::Storage)? else {
        return Ok(None);
    };
    if bytes.len() > 1024 {
        return Err(Error::Storage);
    }
    let state: Checkpoint = serde_json::from_slice(&bytes).map_err(|_| Error::Storage)?;
    if state.version != 1
        || state
            .nonce
            .as_ref()
            .is_some_and(|n| uuid::Uuid::parse_str(n).is_err())
    {
        return Err(Error::Storage);
    }
    Ok(state.nonce)
}
impl CredentialStore for SecretService {
    async fn load(&self) -> Result<Option<Credentials>, Error> {
        let Some(nonce) = self.checkpoint()? else {
            return Ok(None);
        };
        let items = self.items(Some(&nonce)).await?;
        let Some(item) = items.first() else {
            return Ok(None);
        };
        let mut session = self.session().await?;
        let result = async {
            let reply = self
                .call(
                    item.as_str(),
                    "org.freedesktop.Secret.Item",
                    "GetSecret",
                    &session.path,
                )
                .await?;
            let (secret,): (Secret,) = reply.body().deserialize().map_err(|_| Error::Storage)?;
            if secret.0 != session.path || !secret.1.is_empty() || secret.3 != "application/json" {
                return Err(Error::Storage);
            }
            let credentials = Credentials::decode(&secret.2)?;
            if credentials.nonce() != nonce || self.checkpoint()?.as_deref() != Some(&nonce) {
                return Err(Error::Storage);
            }
            Ok(Some(credentials))
        }
        .await;
        session.close().await;
        result
    }
    async fn save(&self, credentials: &Credentials) -> Result<(), Error> {
        {
            let mut journal = self.journal.lock().map_err(|_| Error::Storage)?;
            if journal.deny_all || journal.revoked.contains(credentials.nonce()) {
                return Err(Error::Cancelled);
            }
            if journal.pending.len() >= 256 {
                return Err(Error::Storage);
            }
            journal.pending.insert(credentials.nonce().into());
        }
        // Every attempt uses a fresh grant. Late remote writes remain staged
        // and cannot overwrite another attempt or authorize a future restart.
        let all = self.items(None).await?;
        let existing = self.items(Some(credentials.nonce())).await?;
        if existing.is_empty() && all.len() >= 256 {
            return Err(Error::Storage);
        }
        let mut session = self.session().await?;
        let result = async {
            let secret = Secret(
                session.path.clone(),
                Vec::new(),
                credentials.encode()?,
                "application/json".into(),
            );
            if let Some(item) = existing.first() {
                self.call(
                    item.as_str(),
                    "org.freedesktop.Secret.Item",
                    "SetSecret",
                    &(secret,),
                )
                .await?;
                return Ok(());
            }
            let attrs =
                OwnedValue::try_from(Value::from(self.attributes(Some(credentials.nonce()))))
                    .map_err(|_| Error::Storage)?;
            let properties = HashMap::from([
                (
                    "org.freedesktop.Secret.Item.Label".to_owned(),
                    OwnedValue::from(Str::from("Arinn music account")),
                ),
                ("org.freedesktop.Secret.Item.Attributes".to_owned(), attrs),
            ]);
            let reply = self
                .call(
                    "/org/freedesktop/secrets/aliases/default",
                    "org.freedesktop.Secret.Collection",
                    "CreateItem",
                    &(properties, secret, true),
                )
                .await?;
            let (_item, prompt): (OwnedObjectPath, OwnedObjectPath) =
                reply.body().deserialize().map_err(|_| Error::Storage)?;
            if prompt.as_str() != "/" {
                return Err(Error::Locked);
            }
            Ok(())
        }
        .await;
        session.close().await;
        result
    }
    fn commit(&self, credentials: &Credentials) -> Result<(), Error> {
        let mut journal = self.journal.lock().map_err(|_| Error::Storage)?;
        if journal.deny_all || journal.revoked.contains(credentials.nonce()) {
            return Err(Error::Cancelled);
        }
        if !journal.pending.contains(credentials.nonce()) {
            return Err(Error::Storage);
        }
        journal
            .storage
            .write(
                &serde_json::to_vec(&Checkpoint {
                    version: 1,
                    nonce: Some(credentials.nonce().into()),
                })
                .map_err(|_| Error::Storage)?,
            )
            .map_err(|_| Error::Storage)?;
        journal.pending.remove(credentials.nonce());
        Ok(())
    }
    fn revoke(&self, nonce: Option<&str>) -> Result<(), Error> {
        let mut journal = self.journal.lock().map_err(|_| Error::Storage)?;
        let current = read_checkpoint(&journal.storage)?;
        let mut revoked: Vec<String> = match nonce {
            Some(nonce) => vec![nonce.into()],
            None => journal
                .pending
                .iter()
                .cloned()
                .chain(current.clone())
                .collect(),
        };
        for grant in revoked.drain(..) {
            journal.pending.remove(&grant);
            if journal.revoked.len() >= 256 {
                journal.deny_all = true;
            } else {
                journal.revoked.insert(grant);
            }
        }
        // Commit and revocation use this same lock. Even a not-yet-current
        // staged grant stays revoked until this store (and its writers) exit.
        if nonce.is_none() || journal.deny_all || current.as_deref() == nonce {
            journal
                .storage
                .write(
                    &serde_json::to_vec(&Checkpoint {
                        version: 1,
                        nonce: None,
                    })
                    .map_err(|_| Error::Storage)?,
                )
                .map_err(|_| Error::Storage)?;
        }
        Ok(())
    }
    async fn clear(&self) -> Result<(), Error> {
        self.revoke(None)?; // Fails closed before any remote mutation or timeout.
        tokio::time::timeout(Duration::from_secs(10), async {
            for item in self.items(None).await? {
                let reply = self
                    .call(item.as_str(), "org.freedesktop.Secret.Item", "Delete", &())
                    .await?;
                let prompt: OwnedObjectPath =
                    reply.body().deserialize().map_err(|_| Error::Storage)?;
                if prompt.as_str() != "/" {
                    return Err(Error::Locked);
                }
            }
            Ok(())
        })
        .await
        .map_err(|_| Error::Storage)?
    }
}

#[cfg(test)]
mod tests;
