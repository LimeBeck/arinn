// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! mpv JSON IPC over a Unix socket. A dropped peer fails in-flight requests.
//! Those requests are not written again.

use arinn_protocol::validation::ErrorCode;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{
        UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{Mutex, mpsc::UnboundedSender, oneshot},
    task::JoinHandle,
};

pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Clone, Debug)]
pub(crate) struct Reply {
    pub error: String,
    pub data: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct Event {
    pub name: String,
    pub property: Option<String>,
    pub data: Value,
    pub reason: Option<String>,
    pub file_error: Option<String>,
}

pub(crate) enum Incoming {
    Event { generation: u64, event: Event },
    Dead { generation: u64 },
}

struct Inner {
    writer: Mutex<OwnedWriteHalf>,
    pending: Mutex<HashMap<i64, oneshot::Sender<Reply>>>,
    alive: AtomicBool,
    next_id: AtomicI64,
}

pub(crate) struct Client {
    inner: Arc<Inner>,
    reader: Option<JoinHandle<()>>,
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            reader: None,
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(reader) = self.reader.take() {
            reader.abort();
        }
    }
}

impl Client {
    pub(crate) async fn connect(
        path: &Path,
        incoming: UnboundedSender<Incoming>,
        generation: u64,
    ) -> Result<Self, ErrorCode> {
        let stream = UnixStream::connect(path)
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        Ok(Self::from_stream(stream, incoming, generation))
    }

    fn from_stream(
        stream: UnixStream,
        incoming: UnboundedSender<Incoming>,
        generation: u64,
    ) -> Self {
        let (read, write) = stream.into_split();
        let inner = Arc::new(Inner {
            writer: Mutex::new(write),
            pending: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
            next_id: AtomicI64::new(1),
        });
        let reader = tokio::spawn(read_loop(read, Arc::clone(&inner), incoming, generation));
        Self {
            inner,
            reader: Some(reader),
        }
    }

    pub(crate) fn alive(&self) -> bool {
        self.inner.alive.load(Ordering::Acquire)
    }

    /// Send one command. A failure does not send it again.
    pub(crate) async fn request(&self, command: Value) -> Result<Reply, ErrorCode> {
        if !self.alive() {
            return Err(ErrorCode::Unavailable);
        }
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let line = serde_json::to_string(&serde_json::json!({
            "command": command,
            "request_id": id,
        }))
        .map_err(|_| ErrorCode::Internal)?;
        let (tx, rx) = oneshot::channel();
        self.inner.pending.lock().await.insert(id, tx);
        if let Err(error) = write_line(&self.inner, &line).await {
            self.inner.pending.lock().await.remove(&id);
            self.fail();
            return Err(error);
        }
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(reply)) => Ok(reply),
            _ => {
                self.inner.pending.lock().await.remove(&id);
                Err(ErrorCode::Unavailable)
            }
        }
    }

    fn fail(&self) {
        self.inner.alive.store(false, Ordering::Release);
    }
}

async fn write_line(inner: &Inner, line: &str) -> Result<(), ErrorCode> {
    let mut writer = inner.writer.lock().await;
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    writer
        .write_all(b"\n")
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    writer.flush().await.map_err(|_| ErrorCode::Unavailable)?;
    Ok(())
}

async fn read_loop(
    read: OwnedReadHalf,
    inner: Arc<Inner>,
    incoming: UnboundedSender<Incoming>,
    generation: u64,
) {
    let mut lines = BufReader::new(read).lines();
    loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            _ => break,
        };
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(name) = value.get("event").and_then(|v| v.as_str()) {
            let event = Event {
                name: name.to_string(),
                property: value
                    .get("name")
                    .and_then(|item| item.as_str())
                    .map(str::to_string),
                data: value.get("data").cloned().unwrap_or(Value::Null),
                reason: value
                    .get("reason")
                    .and_then(|item| item.as_str())
                    .map(str::to_string),
                file_error: value
                    .get("file_error")
                    .and_then(|item| item.as_str())
                    .map(str::to_string),
            };
            if incoming
                .send(Incoming::Event { generation, event })
                .is_err()
            {
                break;
            }
            continue;
        }
        let id = value
            .get("request_id")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let reply = Reply {
            data: value.get("data").cloned().unwrap_or(Value::Null),
            error: value
                .get("error")
                .and_then(|item| item.as_str())
                .unwrap_or("")
                .to_string(),
        };
        if let Some(tx) = inner.pending.lock().await.remove(&id) {
            let _ = tx.send(reply);
        }
    }
    inner.alive.store(false, Ordering::Release);
    let pending = std::mem::take(&mut *inner.pending.lock().await);
    drop(pending);
    let _ = incoming.send(Incoming::Dead { generation });
}

pub(crate) fn map_error(error: &str) -> Result<(), ErrorCode> {
    match error {
        "success" => Ok(()),
        "invalid parameter" => Err(ErrorCode::InvalidArgument),
        _ => Err(ErrorCode::Unavailable),
    }
}
