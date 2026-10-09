// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `arinn-chrome-host`: started by the browser for `connectNative`. It only
//! validates, frames and forwards: extension snapshots go to the provider's
//! bridge, provider commands go to the extension. It exits when the browser
//! closes the port or the provider goes away; the extension reconnects.
use crate::{
    BRIDGE_NAME, HOST_PATH,
    bridge::BridgeClientProxy,
    framing,
    messages::{self, ACTIONS, FromExtension, ToExtension},
};
use futures_util::StreamExt;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{mpsc, oneshot},
};
use zbus::{fdo, message::Header};

pub const RESULT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PENDING: usize = 32;

struct Host {
    bridge_owner: String,
    serial: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<String>>>>,
    outgoing: mpsc::Sender<Vec<u8>>,
}

#[zbus::interface(name = "org.arinn.ChromeHost1")]
impl Host {
    /// Forward a command to the extension and wait for its result. Returns ""
    /// on success or an error code; only the bridge owner may call it.
    async fn command(
        &self,
        action: &str,
        tab: i64,
        url: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        if header.sender().map(|s| s.as_str()) != Some(self.bridge_owner.as_str()) {
            return Err(fdo::Error::AccessDenied("only the provider".into()));
        }
        let valid = match action {
            "open" => arinn_protocol::web::open_url(url).is_ok(),
            _ => ACTIONS.contains(&action) && url.is_empty(),
        };
        if !valid {
            return Ok("InvalidArgument".into());
        }
        let serial = self.serial.fetch_add(1, Ordering::Relaxed) + 1;
        let (reply, result) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            if pending.len() >= MAX_PENDING {
                return Ok("Unavailable".into());
            }
            pending.insert(serial, reply);
        }
        let body = serde_json::to_vec(&ToExtension::Command {
            serial,
            action: action.into(),
            tab_id: tab,
            url: url.into(),
        })
        .map_err(|_| fdo::Error::Failed("encode".into()))?;
        if self.outgoing.send(body).await.is_err() {
            self.pending.lock().unwrap().remove(&serial);
            return Ok("Unavailable".into());
        }
        match tokio::time::timeout(RESULT_TIMEOUT, result).await {
            Ok(Ok(error)) => Ok(error),
            Ok(Err(_)) => Ok("Unavailable".into()),
            Err(_) => {
                self.pending.lock().unwrap().remove(&serial);
                Ok("Timeout".into())
            }
        }
    }
}

/// Map an extension error to the codes the provider understands.
fn result_code(error: &str) -> String {
    match error {
        "" | "NotFound" | "InvalidArgument" | "Timeout" | "PermissionDenied" => error.into(),
        _ => "Unavailable".into(),
    }
}

/// Run the host until the browser closes the port or the provider disappears.
pub async fn run<R, W>(
    connection: zbus::Connection,
    mut input: R,
    mut output: W,
) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let bus = fdo::DBusProxy::new(&connection).await?;
    // Subscribe before reading the owner, so no change between the two is lost.
    let mut owner_changes = bus
        .receive_name_owner_changed_with_args(&[(0, BRIDGE_NAME)])
        .await?;
    let bridge_owner = bus
        .get_name_owner(BRIDGE_NAME.try_into()?)
        .await?
        .to_string();
    let bridge = BridgeClientProxy::builder(&connection)
        .destination(bridge_owner.clone())?
        .build()
        .await?;
    let (outgoing, mut frames) = mpsc::channel::<Vec<u8>>(16);
    let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<String>>>> = Arc::default();
    connection
        .object_server()
        .at(
            HOST_PATH,
            Host {
                bridge_owner: bridge_owner.clone(),
                serial: AtomicU64::new(0),
                pending: pending.clone(),
                outgoing,
            },
        )
        .await?;
    let id = bridge.attach().await?;
    let writer = tokio::spawn(async move {
        while let Some(frame) = frames.recv().await {
            if framing::write(&mut output, &frame).await.is_err() {
                break;
            }
        }
    });
    let result = loop {
        tokio::select! {
            frame = framing::read(&mut input) => {
                let frame = match frame {
                    Ok(Some(frame)) => frame,
                    Ok(None) => break Ok(()),
                    Err(e) => break Err(e.into()),
                };
                match messages::parse(&frame) {
                    Ok(FromExtension::Snapshot { windows, tabs }) => {
                        let (windows, tabs) = messages::wire(windows, tabs);
                        if let Err(e) = bridge.publish(&id, windows, tabs).await {
                            break Err(e.into());
                        }
                    }
                    Ok(FromExtension::Result { serial, error }) => {
                        if let Some(reply) = pending.lock().unwrap().remove(&serial) {
                            let _ = reply.send(result_code(&error));
                        }
                    }
                    // A malformed message ends the connection; the extension reconnects.
                    Err(reason) => break Err(anyhow::anyhow!(reason)),
                }
            }
            change = owner_changes.next() => {
                // The provider is gone when the name loses its owner or moves
                // to a new one (a restarted provider queued for it, #356).
                let owner = change.map(|c| {
                    c.args()
                        .ok()
                        .and_then(|a| a.new_owner.as_ref().map(|o| o.to_string()))
                });
                if owner.is_none_or(|owner| owner.as_deref() != Some(bridge_owner.as_str())) {
                    break Ok(());
                }
            }
        }
    };
    writer.abort();
    result
}
