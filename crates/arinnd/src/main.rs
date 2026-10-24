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
use zbus::fdo::{RequestNameFlags, RequestNameReply};

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
    if args == ["--release-power-key"] {
        // ExecStopPost of arinnd.service and `arinn-kwin-adapter stop`: the
        // desktop handles the power key again once Arinn stops (ADR-0111).
        // A session without a bus or KGlobalAccel has nothing to give back.
        if let Ok(connection) = zbus::Connection::session().await {
            arinnd::power_key::release(&connection).await;
        }
        return Ok(());
    }
    if !args.is_empty() {
        eprintln!("usage: arinnd [--check-plugins | --release-power-key]");
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
    // Before Daemon1 is reachable, so no explicit Home request can arrive
    // while the background handoff is missing. Without a system bus, sleep is
    // seen only through the lock taken first.
    let system = zbus::Connection::system().await.ok();
    core.export_background(arinnd::session_lock::watch(connection.clone(), system))
        .await?;
    connection
        .object_server()
        .at(PATH, Daemon { core: core.clone() })
        .await
        .map_err(|_| ErrorCode::Internal)?;
    core.export_platform().await?;
    core.export_keyboard().await?;
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
    // Shortcut store access through WebShortcuts1 and WebShortcutIcons1
    // waits until the migrations and the seed below are done.
    let startup = shortcuts.startup().await;
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
    // The name comes after every export, so a client that sees it always
    // reaches served objects; until then only the unique name, which no
    // client looks for, leads here. It comes before the state changes below,
    // so a second instance fails here without touching them: it neither
    // queues for the name nor replaces the owner (#463).
    let reply = connection
        .request_name_with_flags(SERVICE, RequestNameFlags::DoNotQueue.into())
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    if reply != RequestNameReply::PrimaryOwner {
        return Err(ErrorCode::Unavailable);
    }
    let migrations = tokio::task::spawn_blocking(|| {
        arinnd::shortcuts::load_migrations(&arinnd::shortcuts::data_roots())
    })
    .await
    .map_err(|_| ErrorCode::Internal)?;
    startup.migrate_presets(migrations).await?;
    let defaults = tokio::task::spawn_blocking(|| {
        arinnd::shortcuts::load_defaults(&arinnd::shortcuts::data_roots())
    })
    .await
    .map_err(|_| ErrorCode::Internal)?;
    if startup.seed(defaults).await.is_err() {
        eprintln!(
            "{}",
            serde_json::json!({"level":"warn","component":"arinnd","provider_id":null,"request_id":null,"operation":"seed-web-shortcuts","error":"Internal","duration_ms":0})
        );
    }
    drop(startup);
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
