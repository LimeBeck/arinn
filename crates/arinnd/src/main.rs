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

async fn run(started: std::time::Instant) -> Result<(), ErrorCode> {
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
    let paths_state = paths.state.clone();
    let engines = tokio::task::spawn_blocking(|| {
        arinnd::browse::load_engines(&arinnd::shortcuts::data_roots())
    })
    .await
    .map_err(|_| ErrorCode::Internal)?;
    let core = Core::new(&connection, paths, runtime).with_search_engines(engines);
    connection
        .object_server()
        .at(PATH, Daemon { core: core.clone() })
        .await
        .map_err(|_| ErrorCode::Internal)?;
    core.export_platform().await?;
    core.export_settings().await?;
    core.export_folders().await?;
    core.export_browse().await?;
    connection
        .object_server()
        .at(
            PATH,
            arinnd::favorites::Favorites::new(&paths_state).with_core(core.clone()),
        )
        .await
        .map_err(|_| ErrorCode::Internal)?;
    let shortcuts = arinnd::shortcuts::WebShortcuts::new(&paths_state, core.operations.clone())
        .with_browser(core.clone());
    let migrations = tokio::task::spawn_blocking(|| {
        arinnd::shortcuts::load_migrations(&arinnd::shortcuts::data_roots())
    })
    .await
    .map_err(|_| ErrorCode::Internal)?;
    shortcuts.migrate_presets(migrations).await?;
    let defaults = tokio::task::spawn_blocking(|| {
        arinnd::shortcuts::load_defaults(&arinnd::shortcuts::data_roots())
    })
    .await
    .map_err(|_| ErrorCode::Internal)?;
    if shortcuts.seed(defaults).await.is_err() {
        eprintln!(
            "{}",
            serde_json::json!({"level":"warn","component":"arinnd","provider_id":null,"request_id":null,"operation":"seed-web-shortcuts","error":"Internal","duration_ms":0})
        );
    }
    core.export_web_icons(shortcuts.clone()).await?;
    core.export_media_association().await?;
    core.export_system_status(arinnd::status_sources::Sources::system())
        .await?;
    core.export_system_control(arinnd::system_control::Control::system())
        .await?;
    connection
        .object_server()
        .at(PATH, shortcuts)
        .await
        .map_err(|_| ErrorCode::Internal)?;
    core.start().await;
    arinnd::telemetry::record("ready", None, started.elapsed().as_millis() as u64);
    eprintln!(
        "{}",
        serde_json::json!({"level":"info","component":"arinnd","provider_id":null,"request_id":null,"operation":"ready","error":null,"duration_ms":0})
    );
    tokio::signal::ctrl_c()
        .await
        .map_err(|_| ErrorCode::Internal)?;
    core.shutdown_supervision().await;
    arinnd::telemetry::record("shutdown", None, 0);
    Ok(())
}
#[tokio::main]
async fn main() {
    let started = std::time::Instant::now();
    let telemetry = if std::env::args_os().nth(1).is_none() {
        arinnd::telemetry::init()
    } else {
        None
    };
    if let Err(error) = run(started).await {
        if std::env::args_os().nth(1).as_deref() != Some(std::ffi::OsStr::new("--check-plugins")) {
            eprintln!(
                "{}",
                serde_json::json!({"level":"error","component":"arinnd","provider_id":null,"request_id":null,"operation":"startup","error":error.to_string(),"duration_ms":0})
            );
        }
        arinnd::telemetry::record("startup", Some(error), started.elapsed().as_millis() as u64);
        drop(telemetry);
        std::process::exit(1);
    }
}
