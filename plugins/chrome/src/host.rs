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

/// Plugin-private command result: normalized error, selected window, newly created.
pub type CommandResult = (String, i64, bool);
fn failed(code: &str) -> CommandResult {
    (code.into(), -1, false)
}

struct Host {
    bridge_owner: String,
    serial: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<CommandResult>>>>,
    outgoing: mpsc::Sender<Vec<u8>>,
}

#[zbus::interface(name = "org.arinn.ChromeHost1")]
impl Host {
    /// Forward a command and return its error and exact open-window result.
    /// Only the bridge owner may call this plugin-private method.
    async fn command(
        &self,
        action: &str,
        tab: i64,
        url: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<CommandResult> {
        if header.sender().map(|s| s.as_str()) != Some(self.bridge_owner.as_str()) {
            return Err(fdo::Error::AccessDenied("only the provider".into()));
        }
        let valid = match action {
            "open" => arinn_protocol::web::open_url(url).is_ok(),
            _ => ACTIONS.contains(&action) && url.is_empty(),
        };
        if !valid {
            return Ok(failed("InvalidArgument"));
        }
        let serial = self.serial.fetch_add(1, Ordering::Relaxed) + 1;
        let (reply, result) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            if pending.len() >= MAX_PENDING {
                return Ok(failed("Unavailable"));
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
            return Ok(failed("Unavailable"));
        }
        match tokio::time::timeout(RESULT_TIMEOUT, result).await {
            Ok(Ok(error)) => Ok(error),
            Ok(Err(_)) => Ok(failed("Unavailable")),
            Err(_) => {
                self.pending.lock().unwrap().remove(&serial);
                Ok(failed("Timeout"))
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

/// Whether a bridge-name change ends this host's provider: the name left the
/// owner it attached to, whether it is now unowned or moved to a restarted
/// provider that queued for it (#356). Changes that predate the owner the host
/// read, which the stream can still deliver, do not end it.
fn provider_left(old_owner: Option<&str>, bridge_owner: &str) -> bool {
    old_owner == Some(bridge_owner)
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
    let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<CommandResult>>>> = Arc::default();
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
                    Ok(FromExtension::Result { serial, error, window_id, created }) => {
                        if let Some(reply) = pending.lock().unwrap().remove(&serial) {
                            let _ = reply.send((result_code(&error), window_id, created));
                        }
                    }
                    // A malformed message ends the connection; the extension reconnects.
                    Err(reason) => break Err(anyhow::anyhow!(reason)),
                }
            }
            change = owner_changes.next() => {
                let Some(change) = change else { break Ok(()) };
                let old_owner = change
                    .args()
                    .ok()
                    .and_then(|a| a.old_owner.as_ref().map(|o| o.to_string()));
                if provider_left(old_owner.as_deref(), &bridge_owner) {
                    break Ok(());
                }
            }
        }
    };
    writer.abort();
    result
}

#[cfg(test)]
mod tests {
    use super::provider_left;

    #[test]
    fn only_a_change_away_from_the_attached_owner_ends_the_host() {
        // The attached provider went away, or the name moved on from it.
        assert!(provider_left(Some(":1.7"), ":1.7"));
        // A change that predates the owner snapshot: the old provider left
        // before the host attached to its replacement.
        assert!(!provider_left(Some(":1.3"), ":1.7"));
        // The replacement acquiring the name it now holds.
        assert!(!provider_left(None, ":1.7"));
    }
}
