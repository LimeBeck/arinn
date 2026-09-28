// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_protocol::production::{
    ApplicationApiProxy, ProductionDaemonProxy, ProductionOperationProxy,
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
        || matches!(args.as_slice(),[cmd,_,_] if ["launch","activate","terminate"].contains(&cmd.as_str()));
    if !valid {
        eprintln!(
            "usage: arinnctl [--json] status|providers|capabilities|diagnostics|snapshot <provider>|provider restart <id>|providers reload|launch <provider> <id>|activate <provider> <id>|terminate <provider> <id>|home|back"
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
