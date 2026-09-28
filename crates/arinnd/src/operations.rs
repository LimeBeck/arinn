// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_protocol::{
    production::{self, EntityRef, OperationInfo},
    validation::ErrorCode,
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use zbus::{Connection, object_server::SignalEmitter, zvariant::OwnedObjectPath};
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
#[derive(Clone)]
pub struct Operation {
    pub value: Arc<Mutex<OperationInfo>>,
    pub caller: String,
    provider_id: String,
    request_id: String,
    admitted_at: Instant,
}
impl Operation {
    pub async fn finish(
        &self,
        c: &Connection,
        path: &OwnedObjectPath,
        outcome: Result<Vec<EntityRef>, ErrorCode>,
    ) {
        let mut value = self.value.lock().await;
        if !["queued", "running"].contains(&value.state.as_str()) {
            return;
        }
        value.ended_at = now();
        if value.started_at == 0 {
            value.started_at = value.ended_at;
        }

        match outcome {
            Ok(results) => {
                value.state = "succeeded".into();
                value.results = results;
            }
            Err(error) => {
                value.state = "failed".into();
                value.error = error.to_string();
                value.retryable = error == ErrorCode::Unavailable;
            }
        }
        eprintln!(
            "{}",
            serde_json::json!({"level":"info","component":"arinnd","provider_id":if self.provider_id.is_empty(){None}else{Some(&self.provider_id)},"request_id":self.request_id,"operation":"complete","outcome":value.state,"error":if value.error.is_empty(){None}else{Some(&value.error)},"duration_ms":self.admitted_at.elapsed().as_millis()})
        );
        drop(value);
        if let Ok(emitter) = SignalEmitter::new(c, path.clone()) {
            let _ = Self::changed(&emitter).await;
        }
    }
}
#[zbus::interface(name = "org.arinn.Operation1")]
impl Operation {
    async fn get(&self) -> (OperationInfo,) {
        (self.value.lock().await.clone(),)
    }
    async fn cancel(&self) -> production::Result<()> {
        Err(ErrorCode::Unsupported.into())
    }
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}
struct Entry {
    fingerprint: String,
    path: OwnedObjectPath,
    operation: Operation,
    provider: String,
}
#[derive(Clone, Default)]
pub struct Operations {
    entries: Arc<Mutex<HashMap<(String, String), Entry>>>,
}
impl Operations {
    pub async fn existing(
        &self,
        caller: &str,
        request_id: &str,
        fingerprint: &str,
    ) -> Result<Option<OwnedObjectPath>, ErrorCode> {
        uuid::Uuid::parse_str(request_id).map_err(|_| ErrorCode::InvalidArgument)?;
        let entries = self.entries.lock().await;
        if let Some(entry) = entries.get(&(caller.into(), request_id.into())) {
            if entry.fingerprint != fingerprint {
                return Err(ErrorCode::Conflict);
            }
            return Ok(Some(entry.path.clone()));
        }
        Ok(None)
    }

    pub async fn admit(
        &self,
        c: &Connection,
        caller: &str,
        request_id: &str,
        fingerprint: &str,
        provider: &str,
    ) -> Result<(OwnedObjectPath, Operation, bool), ErrorCode> {
        uuid::Uuid::parse_str(request_id).map_err(|_| ErrorCode::InvalidArgument)?;
        let key = (caller.to_owned(), request_id.to_owned());
        let mut entries = self.entries.lock().await;
        if let Some(entry) = entries.get(&key) {
            if entry.fingerprint != fingerprint {
                return Err(ErrorCode::Conflict);
            }
            return Ok((entry.path.clone(), entry.operation.clone(), false));
        }
        if entries.len() >= 1024
            || entries.keys().filter(|(owner, _)| owner == caller).count() >= 128
        {
            return Err(ErrorCode::Unavailable);
        }
        let mut active = 0;
        for entry in entries.values().filter(|entry| entry.provider == provider) {
            if ["queued", "running"].contains(&entry.operation.value.lock().await.state.as_str()) {
                active += 1;
            }
        }
        if active >= 32 {
            return Err(ErrorCode::Unavailable);
        }
        let path: OwnedObjectPath = format!(
            "/org/arinn/Daemon1/operations/o_{}",
            uuid::Uuid::new_v4().simple()
        )
        .try_into()
        .unwrap();
        let operation = Operation {
            caller: caller.into(),
            provider_id: provider.into(),
            request_id: request_id.into(),
            admitted_at: Instant::now(),
            value: Arc::new(Mutex::new(OperationInfo {
                state: "queued".into(),
                error: String::new(),
                retryable: false,
                started_at: 0,
                ended_at: 0,
                results: Vec::new(),
                cancellable: false,
            })),
        };
        c.object_server()
            .at(path.clone(), operation.clone())
            .await
            .map_err(|_| ErrorCode::Internal)?;
        entries.insert(
            key.clone(),
            Entry {
                fingerprint: fingerprint.into(),
                path: path.clone(),
                operation: operation.clone(),
                provider: provider.into(),
            },
        );
        let book = self.clone();
        let connection = c.clone();
        let object = path.clone();
        let op = operation.clone();
        tokio::spawn(async move {
            // Retention starts at completion, not admission. Polling here only
            // reaps objects; it does not retry or drive application commands.
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if op.value.lock().await.ended_at > 0 {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_secs(300)).await;
            book.entries.lock().await.remove(&key);
            let _ = connection
                .object_server()
                .remove::<Operation, _>(object)
                .await;
        });
        Ok((path, operation, true))
    }
    pub async fn invalidate(&self, c: &Connection, provider: Option<&str>, caller: Option<&str>) {
        let entries = self.entries.lock().await;
        for entry in entries.values() {
            if provider.is_some_and(|id| id == entry.provider)
                || caller.is_some_and(|id| id == entry.operation.caller)
            {
                entry
                    .operation
                    .finish(c, &entry.path, Err(ErrorCode::Unavailable))
                    .await;
            }
        }
    }
}
