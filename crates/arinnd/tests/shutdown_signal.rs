// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration test: SIGTERM must unblock the daemon shutdown wait (issue #604).

use std::{fs, time::Duration};

#[tokio::test]
async fn sigterm_unblocks_shutdown_wait() {
    let marker =
        std::env::temp_dir().join(format!("arinn-shutdown-signal-{}", uuid::Uuid::new_v4()));
    let pid = std::process::id() as libc::pid_t;
    let marker_path = marker.clone();
    let waiter = tokio::spawn(async move {
        arinnd::signal::wait_for_stop_signal().await.unwrap();
        fs::write(marker_path, b"ok").unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    // SAFETY: this integration test binary is isolated from other tests.
    let sent = unsafe { libc::kill(pid, libc::SIGTERM) };
    assert_eq!(sent, 0);
    tokio::time::timeout(Duration::from_secs(2), waiter)
        .await
        .expect("SIGTERM should unblock shutdown wait")
        .expect("wait task should succeed");
    assert_eq!(fs::read(&marker).unwrap(), b"ok");
    let _ = fs::remove_file(marker);
}

#[test]
fn shutdown_log_json_matches_ready_format() {
    let line = arinnd::signal::shutdown_log_json();
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["level"], "info");
    assert_eq!(value["component"], "arinnd");
    assert_eq!(value["operation"], "shutdown");
    assert!(value["provider_id"].is_null());
    assert!(value["request_id"].is_null());
    assert!(value["error"].is_null());
    assert_eq!(value["duration_ms"], 0);
}
