// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_protocol::validation::ErrorCode;
use std::time::Duration;

#[tokio::main]
async fn main() {
    if std::env::args_os().len() != 1 {
        std::process::exit(2);
    }
    if let Err(error) = run().await {
        eprintln!(
            "{{\"level\":\"error\",\"component\":\"arinn-plugin-mpris\",\"operation\":\"runtime\",\"error\":\"{error}\"}}"
        );
        std::process::exit(1);
    }
}

async fn run() -> Result<(), ErrorCode> {
    let connection = zbus::connection::Builder::session()
        .map_err(|_| ErrorCode::Unavailable)?
        .method_timeout(Duration::from_secs(2))
        .build()
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    arinn_plugin_mpris::serve(connection).await
}
