// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Session shutdown signals for the daemon main loop.

use arinn_protocol::validation::ErrorCode;

/// JSON log line for orderly shutdown, matching the `ready` line format.
pub fn shutdown_log_json() -> String {
    serde_json::json!({
        "level": "info",
        "component": "arinnd",
        "provider_id": null,
        "request_id": null,
        "operation": "shutdown",
        "error": null,
        "duration_ms": 0,
    })
    .to_string()
}

/// Wait until the process receives SIGINT or SIGTERM.
pub async fn wait_for_stop_signal() -> Result<(), ErrorCode> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| ErrorCode::Internal)?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result.map_err(|_| ErrorCode::Internal),
        _ = terminate.recv() => Ok(()),
    }
}
