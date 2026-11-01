// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_protocol::validation::ErrorCode;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let Ok(args) = std::env::args_os()
        .skip(1)
        .map(|arg| arg.into_string())
        .collect::<Result<Vec<String>, _>>()
    else {
        std::process::exit(2);
    };
    // The recognition dry run (ADR-0128) runs offline and exits.
    if args.first().map(String::as_str) == Some("--check-recognition") {
        let code = arinn_plugin_library::check_recognition(&args[1..], &mut std::io::stdout());
        std::process::exit(code);
    }
    if !args.is_empty() {
        std::process::exit(2);
    }
    if let Err(error) = run().await {
        eprintln!(
            "{{\"level\":\"error\",\"component\":\"arinn-plugin-library\",\"operation\":\"runtime\",\"error\":\"{error}\"}}"
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
    arinn_plugin_library::serve(connection).await
}
