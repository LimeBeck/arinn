// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Private-bus lock provider for `tests/package/test_xfce_session.py` only.
//! Not installed by the package.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use zbus::Connection;

#[path = "../../../lock_endpoints.rs"]
mod lock_endpoints;

#[derive(Clone, Default)]
struct Lock(Arc<AtomicBool>);
#[zbus::interface(name = "org.xfce.ScreenSaver")]
impl Lock {
    fn get_active(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[tokio::main]
async fn main() -> zbus::Result<()> {
    let connection = Connection::session().await?;
    connection
        .object_server()
        .at(lock_endpoints::LOCK_ENDPOINTS[2].1, Lock::default())
        .await?;
    connection
        .request_name(lock_endpoints::LOCK_ENDPOINTS[2].0)
        .await?;
    std::future::pending::<()>().await;
    Ok(())
}
