// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod cli;

use arinn_protocol::{
    browser::{BrowserApiProxy, TabPage},
    production::{ApplicationApiProxy, ProductionDaemonProxy, ProductionOperationProxy},
    web::WebShortcutsProxy,
};
use serde::Serialize;
use std::time::Duration;
fn output(value: &impl Serialize, json: bool) -> Result<(), u8> {
    let value = serde_json::to_value(value).map_err(|_| 1)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&value).map_err(|_| 1)?);
    } else {
        print!("{}", human(&value));
    }
    Ok(())
}
/// The daemon's build id, or `None` for a daemon older than `BuildInfo`
/// (ADR-0097). Any other failure is a runtime failure, not an old daemon.
fn daemon_build(reply: zbus::Result<(String, String)>) -> Result<Option<String>, u8> {
    match reply {
        Ok((_, id)) => Ok(Some(id)),
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "org.freedesktop.DBus.Error.UnknownMethod" =>
        {
            Ok(None)
        }
        Err(zbus::Error::FDO(error)) if matches!(*error, zbus::fdo::Error::UnknownMethod(_)) => {
            Ok(None)
        }
        Err(_) => Err(1),
    }
}
/// `status` with this client's build id and the daemon's (`null` when the
/// daemon predates `BuildInfo`).
fn with_builds(
    status: &arinn_protocol::Status,
    daemon: Option<&str>,
) -> Result<serde_json::Value, u8> {
    let mut value = serde_json::to_value(status).map_err(|_| 1)?;
    let map = value.as_object_mut().ok_or(1)?;
    map.insert("build_id".into(), env!("ARINN_BUILD_ID").into());
    map.insert("daemon_build_id".into(), daemon.into());
    Ok(value)
}
/// `key: value` lines; the objects of any list, nested ones too, are
/// separated by a blank line.
fn human(value: &serde_json::Value) -> String {
    let mut out = String::new();
    human_into(value, "", &mut out);
    out
}
fn human_into(value: &serde_json::Value, prefix: &str, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                human_into(value, &format!("{prefix}{key}: "), out);
            }
        }
        serde_json::Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                if index > 0 && value.is_object() && !out.ends_with("\n\n") {
                    out.push('\n');
                }
                human_into(value, prefix, out);
            }
        }
        serde_json::Value::String(text) => out.push_str(&format!("{prefix}{text}\n")),
        _ => out.push_str(&format!("{prefix}{value}\n")),
    }
}
/// `providers` as a table, one row per provider (`--json` keeps all fields).
fn provider_table(providers: &[arinn_protocol::production::ProviderInfo]) -> String {
    let header = [
        "ID",
        "CAPABILITY",
        "STATE",
        "BACKEND",
        "VERSION",
        "RESTARTS",
        "LAST ERROR",
    ];
    let rows: Vec<[String; 7]> = providers
        .iter()
        .map(|p| {
            let error = if p.last_error.is_empty() {
                "-".to_owned()
            } else if p.recovered_at > 0 {
                format!("{} (recovered)", p.last_error)
            } else {
                p.last_error.clone()
            };
            [
                p.id.clone(),
                p.capabilities.join(","),
                p.runtime_state.clone(),
                p.backend_state.clone(),
                p.version.clone(),
                format!(
                    "{}/{}",
                    p.restarts_since_daemon_start, p.cumulative_restart_count
                ),
                error,
            ]
        })
        .collect();
    let mut widths = header.map(|h| h.chars().count());
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let line = |cells: &[&str]| {
        let mut text = String::new();
        for (index, (cell, width)) in cells.iter().zip(widths).enumerate() {
            if index + 1 == cells.len() {
                text.push_str(cell);
            } else {
                text.push_str(cell);
                text.push_str(&" ".repeat(width - cell.chars().count() + 2));
            }
        }
        text.trim_end().to_owned() + "\n"
    };
    let mut out = line(&header);
    for row in &rows {
        out.push_str(&line(&row.each_ref().map(String::as_str)));
    }
    out
}
async fn operation(c: &zbus::Connection, path: zbus::zvariant::OwnedObjectPath) -> Result<(), u8> {
    let op = ProductionOperationProxy::builder(c)
        .path(path)
        .map_err(|_| 1)?
        .build()
        .await
        .map_err(|_| 1)?;
    // Subscribe before the initial read: completion may precede the method reply.
    let _subscription = op.receive_changed().await.map_err(|_| 1)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(40);
    loop {
        let (state,) = op.get().await.map_err(|_| 1)?;
        match state.state.as_str() {
            "succeeded" => return Ok(()),
            "failed" | "cancelled" => {
                eprintln!("{}", state.error);
                return Err(1);
            }
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            eprintln!("Timeout");
            return Err(1);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
async fn pages(
    app: &ApplicationApiProxy<'_>,
) -> Result<Vec<arinn_protocol::application::ApplicationPage>, u8> {
    let mut token = String::new();
    let mut pages = Vec::new();
    let mut expected = None;
    let mut size = 0;
    let mut tokens = std::collections::HashSet::new();
    loop {
        let (page,) = app.get_applications(&token).await.map_err(|_| 1)?;
        page.validate().map_err(|_| 1)?;
        if expected
            .as_ref()
            .is_some_and(|pair| pair != &(page.generation.clone(), page.revision))
        {
            return Err(1);
        }
        expected = Some((page.generation.clone(), page.revision));
        size += page.encoded_len().map_err(|_| 1)?;
        if size > 16 * 1024 * 1024 {
            return Err(1);
        }
        token = page.next_token.clone();
        pages.push(page);
        if token.is_empty() {
            break;
        }
        if !tokens.insert(token.clone()) {
            return Err(1);
        }
    }
    Ok(pages)
}
async fn tab_pages(browser: &BrowserApiProxy<'_>) -> Result<Vec<TabPage>, u8> {
    let mut token = String::new();
    let mut pages = Vec::new();
    let mut tokens = std::collections::HashSet::new();
    let mut size = 0;
    loop {
        let (page,) = browser.get_tabs(&token).await.map_err(|_| 1)?;
        page.validate().map_err(|_| 1)?;
        if pages.first().is_some_and(|first: &TabPage| {
            (&first.generation, first.revision) != (&page.generation, page.revision)
        }) {
            return Err(1);
        }
        size += page.encoded_len().map_err(|_| 1)?;
        if size > 16 * 1024 * 1024 {
            return Err(1);
        }
        token = page.next_token.clone();
        pages.push(page);
        if token.is_empty() {
            return Ok(pages);
        }
        if !tokens.insert(token.clone()) {
            return Err(1);
        }
    }
}
/// Print the Arinn error name of a failed call (for example `Conflict`) and fail.
fn failed(error: zbus::Error) -> u8 {
    if let zbus::Error::MethodError(name, _, _) = &error
        && let Some(code) = name.as_str().strip_prefix("org.arinn.Error1.")
    {
        eprintln!("{code}");
    }
    1
}
/// `shortcuts …` (ADR-0043): web shortcut management without a keyboard on the
/// TV. URLs appear only in this command's own output, never in logs.
async fn shortcuts(c: &zbus::Connection, args: &[String], json: bool) -> Result<(), u8> {
    let proxy = WebShortcutsProxy::new(c).await.map_err(|_| 3)?;
    let request = uuid::Uuid::new_v4().to_string();
    let revision = || async {
        proxy
            .list()
            .await
            .map(|(list,)| list.revision)
            .map_err(failed)
    };
    let icon = |index: usize| args.get(index).cloned().unwrap_or_default();
    match args.first().map(String::as_str) {
        None | Some("list") => output(&proxy.list().await.map_err(failed)?.0, json),
        Some("add") => {
            let id = proxy
                .add(&args[1], &args[2], &icon(3), &request)
                .await
                .map_err(failed)?;
            output(&serde_json::json!({ "id": id }), json)
        }
        Some("edit") => proxy
            .update(&args[1], revision().await?, &args[2], &args[3], &icon(4))
            .await
            .map_err(failed),
        Some("remove") => proxy
            .remove(&args[1], revision().await?)
            .await
            .map_err(failed),
        Some("open") => operation(c, proxy.open(&args[1], &request).await.map_err(failed)?).await,
        _ => Err(2),
    }
}
/// `browser …` (ADR-0048): tabs of the ready browser providers.
async fn browser(
    c: &zbus::Connection,
    p: &ProductionDaemonProxy<'_>,
    args: &[String],
    json: bool,
) -> Result<(), u8> {
    let providers: Vec<_> = p
        .providers()
        .await
        .map_err(|_| 1)?
        .into_iter()
        .filter(|p| p.runtime_state == "ready" && p.capabilities.iter().any(|c| c == "browser"))
        .collect();
    let proxy = |path: zbus::zvariant::OwnedObjectPath| async move {
        BrowserApiProxy::builder(c)
            .path(path)
            .map_err(|_| 1)?
            .build()
            .await
            .map_err(|_| 1)
    };
    if args.first().map(String::as_str) == Some("tabs") {
        let mut tabs = Vec::new();
        for provider in &providers {
            for page in tab_pages(&proxy(provider.path.clone()).await?).await? {
                for tab in page.tabs {
                    tabs.push(serde_json::json!({"provider_id":provider.id,"generation":page.generation,"tab":tab}));
                }
            }
        }
        return output(&tabs, json);
    }
    let provider = providers.iter().find(|p| p.id == args[1]).ok_or(1)?;
    let browser = proxy(provider.path.clone()).await?;
    let request = uuid::Uuid::new_v4().to_string();
    let path = if args[0] == "activate" {
        browser
            .activate_tab(&provider.generation, &args[2], &request)
            .await
    } else {
        browser
            .close_tab(&provider.generation, &args[2], &request)
            .await
    }
    .map_err(failed)?;
    operation(c, path).await
}
async fn run() -> Result<(), u8> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    args.retain(|a| a != "--json");
    if let Some(help) = cli::help(&args) {
        print!("{help}");
        return Ok(());
    }
    if args.len() == 1 && (args[0] == "--version" || args[0] == "-V") {
        println!("arinnctl {}", env!("ARINN_BUILD_ID"));
        return Ok(());
    }
    if args.is_empty() {
        args.push("status".into());
    }
    if !cli::valid(&args) {
        eprintln!("{}", cli::usage_error(&args));
        return Err(2);
    }
    let c = zbus::connection::Builder::session()
        .map_err(|_| 3)?
        .method_timeout(Duration::from_secs(3))
        .build()
        .await
        .map_err(|_| 3)?;
    let bus = zbus::fdo::DBusProxy::new(&c).await.map_err(|_| 3)?;
    if !bus
        .name_has_owner(arinn_protocol::production::SERVICE.try_into().unwrap())
        .await
        .map_err(|_| 3)?
    {
        return Err(3);
    }
    let p = ProductionDaemonProxy::new(&c).await.map_err(|_| 3)?;
    let request = uuid::Uuid::new_v4().to_string();
    match args[0].as_str() {
        "status" => {
            let status = p.status().await.map_err(|_| 1)?.0;
            let daemon = daemon_build(p.build_info().await)?;
            output(&with_builds(&status, daemon.as_deref())?, json)
        }
        "providers" if args.len() == 1 => {
            let providers = p.providers().await.map_err(|_| 1)?;
            if json {
                output(&providers, json)
            } else {
                print!("{}", provider_table(&providers));
                Ok(())
            }
        }
        "capabilities" => output(&p.capabilities().await.map_err(|_| 1)?, json),
        "activities" => {
            let mut activities = Vec::new();
            for provider in p
                .providers()
                .await
                .map_err(|_| 1)?
                .into_iter()
                .filter(|p| p.runtime_state == "ready")
            {
                let app = ApplicationApiProxy::builder(&c)
                    .path(provider.path.clone())
                    .map_err(|_| 1)?
                    .build()
                    .await
                    .map_err(|_| 1)?;
                for page in pages(&app).await? {
                    for running in page.running {
                        activities.push(serde_json::json!({"provider_id":provider.id,"generation":page.generation,"instance":running}));
                    }
                }
            }
            output(&activities, json)
        }
        "diagnostics" => output(
            &serde_json::json!({"error":p.diagnostics().await.map_err(|_|1)?,"providers":p.providers().await.map_err(|_|1)?}),
            json,
        ),
        "home" => p.home().await.map_err(|_| 1),
        "shortcuts" => shortcuts(&c, &args[1..], json).await,
        "browser" => browser(&c, &p, &args[1..], json).await,
        "back" => p.back().await.map_err(|_| 1),
        "provider" => operation(&c, p.restart(&args[2], &request).await.map_err(|_| 1)?).await,
        "providers" => operation(&c, p.reload(&request).await.map_err(|_| 1)?).await,
        action => {
            let providers = p.providers().await.map_err(|_| 1)?;
            let provider = providers.iter().find(|p| p.id == args[1]).ok_or(1)?;
            let app = ApplicationApiProxy::builder(&c)
                .path(provider.path.clone())
                .map_err(|_| 1)?
                .build()
                .await
                .map_err(|_| 1)?;
            if action == "snapshot" {
                output(&pages(&app).await?, json)
            } else {
                let path = match action {
                    "launch" => app.launch(&provider.generation, &args[2], &request).await,
                    "activate" => app.activate(&provider.generation, &args[2], &request).await,
                    _ => {
                        app.terminate(&provider.generation, &args[2], &request)
                            .await
                    }
                }
                .map_err(|_| 1)?;
                operation(&c, path).await
            }
        }
    }
}
#[tokio::main]
async fn main() {
    if let Err(code) = run().await {
        if code != 2 {
            eprintln!(
                "{}",
                if code == 3 {
                    "Daemon unavailable"
                } else {
                    "Operation failed"
                }
            );
        }
        std::process::exit(i32::from(code));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arinn_protocol::production::ProviderInfo;

    #[test]
    fn status_names_both_builds() {
        let status = arinn_protocol::Status {
            epoch: "e".into(),
            provider_available: true,
            generation: "g".into(),
            revision: 1,
            home_visible: true,
            return_window: String::new(),
            active_window: String::new(),
            active_class: String::new(),
            active_fullscreen: false,
            platform_connected: true,
            home_output: String::new(),
            active_output: String::new(),
        };
        let value = with_builds(&status, Some("0.1.0+g1bd20d647afe")).unwrap();
        assert_eq!(value["build_id"], env!("ARINN_BUILD_ID"));
        assert_eq!(value["daemon_build_id"], "0.1.0+g1bd20d647afe");
        assert!(human(&value).contains("daemon_build_id: 0.1.0+g1bd20d647afe\n"));
        // A daemon without BuildInfo: unknown, not invented.
        let value = with_builds(&status, None).unwrap();
        assert!(value["daemon_build_id"].is_null());
        assert!(human(&value).contains("daemon_build_id: null\n"));
    }

    #[test]
    fn only_a_missing_build_info_is_an_old_daemon() {
        let unknown =
            zbus::names::OwnedErrorName::try_from("org.freedesktop.DBus.Error.UnknownMethod")
                .unwrap();
        let message = zbus::message::Message::method_call("/", "BuildInfo")
            .unwrap()
            .build(&())
            .unwrap();
        assert_eq!(
            daemon_build(Ok(("0.1.0".into(), "0.1.0+g1bd20d647afe".into()))),
            Ok(Some("0.1.0+g1bd20d647afe".into()))
        );
        assert_eq!(
            daemon_build(Err(zbus::Error::MethodError(unknown, None, message))),
            Ok(None)
        );
        assert_eq!(
            daemon_build(Err(zbus::Error::FDO(Box::new(
                zbus::fdo::Error::UnknownMethod("BuildInfo".into())
            )))),
            Ok(None)
        );
        // A timeout or another error is a failure, not "no BuildInfo".
        assert_eq!(
            daemon_build(Err(zbus::Error::InputOutput(std::sync::Arc::new(
                std::io::Error::from(std::io::ErrorKind::TimedOut)
            )))),
            Err(1)
        );
        assert_eq!(
            daemon_build(Err(zbus::Error::FDO(Box::new(
                zbus::fdo::Error::AccessDenied("no".into())
            )))),
            Err(1)
        );
    }

    fn provider(id: &str, error: &str, error_at: u64, recovered_at: u64) -> ProviderInfo {
        ProviderInfo {
            id: id.into(),
            plugin_id: id.into(),
            source_path: String::new(),
            name: String::new(),
            version: "0.0.1".into(),
            path: "/org/arinn/Daemon1/providers/p_x".try_into().unwrap(),
            capabilities: vec!["media".into()],
            runtime_state: "ready".into(),
            backend_state: "available".into(),
            generation: String::new(),
            revision: 1,
            last_error: error.into(),
            error_at,
            recovered_at,
            restarts_since_daemon_start: 1,
            cumulative_restart_count: 3,
        }
    }

    #[test]
    fn records_in_nested_lists_are_separated() {
        let value = serde_json::json!({
            "revision": 1,
            "shortcuts": [
                {"id": "one", "title": "One"},
                {"id": "two", "title": "Two"}
            ]
        });
        assert_eq!(
            human(&value),
            "revision: 1\nshortcuts: id: one\nshortcuts: title: One\n\nshortcuts: id: two\nshortcuts: title: Two\n"
        );
        // Top-level lists too; scalar lists stay one line per value.
        let value = serde_json::json!([{"id": "a"}, {"id": "b"}]);
        assert_eq!(human(&value), "id: a\n\nid: b\n");
        assert_eq!(human(&serde_json::json!(["x", "y"])), "x\ny\n");
    }

    #[test]
    fn providers_are_one_aligned_row_each() {
        let table = provider_table(&[
            provider("mpris", "", 0, 0),
            provider("library", "Unavailable", 10, 20),
            provider("chrome-long-id", "Timeout", 30, 0),
        ]);
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("ID              CAPABILITY  STATE"));
        assert!(lines[1].starts_with("mpris           media       ready"));
        assert!(lines[1].ends_with("1/3       -"));
        assert!(lines[2].ends_with("Unavailable (recovered)"));
        assert!(lines[3].ends_with("Timeout"));
        // Recovered is any non-zero recovered_at, as in the daemon, even
        // after a backward clock step.
        let table = provider_table(&[provider("clock", "Timeout", 50, 40)]);
        assert!(
            table
                .lines()
                .nth(1)
                .unwrap()
                .ends_with("Timeout (recovered)")
        );
        // Columns line up: every row puts STATE at the same offset.
        let state = lines[0].find("STATE").unwrap();
        assert!(
            lines
                .iter()
                .skip(1)
                .all(|l| &l[state..state + 5] == "ready")
        );
    }
}
