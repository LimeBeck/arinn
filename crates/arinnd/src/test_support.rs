// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Helpers for tests that call objects on a private bus (#524, #528).
use std::{future::Future, panic::Location, time::Duration};

/// Far above any answer in these tests, the 10 s backstops of
/// `crate::system_control` included, so only a call that never gets a reply
/// runs into it.
pub(crate) const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// A call or setup step that fails at its own line instead of hanging the
/// test binary when no reply ever comes.
pub(crate) trait Bounded: Future + Sized {
    #[track_caller]
    fn bounded(self) -> impl Future<Output = Self::Output> {
        let at = Location::caller();
        async move {
            tokio::time::timeout(CALL_TIMEOUT, self)
                .await
                .unwrap_or_else(|_| panic!("no reply within {CALL_TIMEOUT:?} at {at}"))
        }
    }
}
impl<F: Future> Bounded for F {}

/// Returns once `server`'s object server answers `client`.
///
/// zbus subscribes a connection's object server to incoming method calls in
/// a task that the first `object_server()` call spawns, and `at` does not
/// wait for it. A call that the connection reads before that task has run is
/// dropped without a reply, so its caller waits forever (#524). The daemon
/// and the providers are safe: the daemon is found by its well-known name,
/// requested after every export, and a provider is called only after it
/// registers. Tests that call a connection right after its first export
/// wait here until a ping is answered; a dropped ping is sent again. Every
/// object path answers `Peer.Ping`, so the root is pinged.
pub(crate) async fn served(client: &zbus::Connection, server: &zbus::Connection) {
    let peer = zbus::fdo::PeerProxy::builder(client)
        .destination(server.unique_name().unwrap().to_string())
        .unwrap()
        .path("/")
        .unwrap()
        .build()
        .await
        .unwrap();
    async {
        loop {
            match tokio::time::timeout(Duration::from_millis(100), peer.ping()).await {
                Ok(answer) => break answer.unwrap(),
                Err(_) => continue,
            }
        }
    }
    .bounded()
    .await
}
