// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
        human(&value, "");
    }
    Ok(())
}
fn human(value: &serde_json::Value, prefix: &str) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                human(value, &format!("{prefix}{key}: "));
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                human(value, prefix);
            }
        }
        serde_json::Value::String(text) => println!("{prefix}{text}"),
        _ => println!("{prefix}{value}"),
    }
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
    if args.is_empty() {
        args.push("status".into());
    }
    let valid = matches!(args.as_slice(),[cmd] if ["status","providers","capabilities","activities","diagnostics","home","back"].contains(&cmd.as_str()))
        || matches!(args.as_slice(),[cmd,sub] if cmd=="providers"&&sub=="reload")
        || matches!(args.as_slice(),[cmd,sub,_] if cmd=="provider"&&sub=="restart")
        || matches!(args.as_slice(),[cmd,_] if cmd=="snapshot")
        || matches!(args.as_slice(),[cmd,_,_] if ["launch","activate","terminate"].contains(&cmd.as_str()))
        || matches!(args.as_slice(),[cmd] if cmd=="shortcuts")
        || matches!(args.as_slice(),[cmd,sub] if cmd=="shortcuts"&&sub=="list")
        || matches!(args.as_slice(),[cmd,sub,..] if cmd=="shortcuts"&&sub=="add"&&(4..=5).contains(&args.len()))
        || matches!(args.as_slice(),[cmd,sub,..] if cmd=="shortcuts"&&sub=="edit"&&(5..=6).contains(&args.len()))
        || matches!(args.as_slice(),[cmd,sub,_] if cmd=="shortcuts"&&["remove","open"].contains(&sub.as_str()))
        || matches!(args.as_slice(),[cmd,sub] if cmd=="browser"&&sub=="tabs")
        || matches!(args.as_slice(),[cmd,sub,_,_] if cmd=="browser"&&["activate","close"].contains(&sub.as_str()));
    if !valid {
        eprintln!(
            "usage: arinnctl [--json] status|providers|capabilities|diagnostics|snapshot <provider>|provider restart <id>|providers reload|launch <provider> <id>|activate <provider> <id>|terminate <provider> <id>|home|back|shortcuts [list]|shortcuts add <title> <url> [icon]|shortcuts edit <id> <title> <url> [icon]|shortcuts remove <id>|shortcuts open <id>|browser tabs|browser activate <provider> <tab>|browser close <provider> <tab>"
        );
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
        "status" => output(&p.status().await.map_err(|_| 1)?.0, json),
        "providers" if args.len() == 1 => output(&p.providers().await.map_err(|_| 1)?, json),
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
        eprintln!(
            "{}",
            if code == 3 {
                "Daemon unavailable"
            } else if code == 2 {
                "Invalid usage"
            } else {
                "Operation failed"
            }
        );
        std::process::exit(i32::from(code));
    }
}
