// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native Messaging host. The browser starts it with the caller's origin as
//! the first argument; anything but the packaged extension is refused.
use arinn_plugin_chrome::{EXTENSION_ID, host};

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let origin = std::env::args().nth(1).unwrap_or_default();
    if origin != format!("chrome-extension://{EXTENSION_ID}/") {
        eprintln!("arinn-chrome-host: refused an unknown extension origin");
        return std::process::ExitCode::from(2);
    }
    let connection = match zbus::connection::Builder::session() {
        Ok(builder) => builder.build().await,
        Err(e) => Err(e),
    };
    let result = match connection {
        Ok(connection) => host::run(connection, tokio::io::stdin(), tokio::io::stdout()).await,
        Err(e) => Err(e.into()),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            // Never print message contents: they may hold titles or URLs.
            eprintln!(
                "arinn-chrome-host: {}",
                e.root_cause().to_string().lines().next().unwrap_or("error")
            );
            std::process::ExitCode::FAILURE
        }
    }
}
