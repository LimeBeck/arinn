// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_protocol::{
    production::{PATH, SERVICE},
    validation::ErrorCode,
};
use arinnd::{
    runtime::{Core, Daemon},
    storage::Paths,
};
use std::path::PathBuf;

async fn run() -> Result<(), ErrorCode> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args == ["--check-plugins"] {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or(ErrorCode::InvalidArgument)?;
        let roots = arinnd::registry::discovery_roots(
            &home,
            std::env::var_os("XDG_DATA_HOME").as_deref(),
            std::env::var_os("XDG_DATA_DIRS").as_deref(),
        );
        let registry =
            tokio::task::spawn_blocking(move || arinnd::registry::Registry::discover(&roots))
                .await
                .map_err(|_| ErrorCode::Internal)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&registry).map_err(|_| ErrorCode::Internal)?
        );
        return if registry.diagnostics.is_empty()
            && registry.providers.values().all(|r| r.error.is_none())
        {
            Ok(())
        } else {
            Err(ErrorCode::InvalidArgument)
        };
    }
    if !args.is_empty() {
        eprintln!("usage: arinnd [--check-plugins]");
        std::process::exit(2);
    }
    let paths = Paths::from_env()?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or(ErrorCode::Unavailable)?
        .join("arinn");
    let connection = zbus::connection::Builder::session()
        .map_err(|_| ErrorCode::Unavailable)?
        .max_queued(32)
        .name(SERVICE)
        .map_err(|_| ErrorCode::Unavailable)?
        .build()
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    let core = Core::new(&connection, paths, runtime);
    connection
        .object_server()
        .at(PATH, Daemon { core: core.clone() })
        .await
        .map_err(|_| ErrorCode::Internal)?;
    core.start().await;
    eprintln!(
        "{}",
        serde_json::json!({"level":"info","component":"arinnd","provider_id":null,"request_id":null,"operation":"ready","error":null,"duration_ms":0})
    );
    tokio::signal::ctrl_c()
        .await
        .map_err(|_| ErrorCode::Internal)?;
    Ok(())
}
#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        if std::env::args_os().nth(1).as_deref() != Some(std::ffi::OsStr::new("--check-plugins")) {
            eprintln!(
                "{}",
                serde_json::json!({"level":"error","component":"arinnd","provider_id":null,"request_id":null,"operation":"startup","error":error.to_string(),"duration_ms":0})
            );
        }
        std::process::exit(1);
    }
}
