// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_protocol::{DaemonProxy, OperationProxy, SERVICE};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let connection = zbus::Connection::session().await?;
    let p = DaemonProxy::new(&connection).await?;
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str).unwrap_or("status") {
        "status" => println!("{}", serde_json::to_string_pretty(&p.status().await?.0)?),
        "snapshot" => println!("{}", serde_json::to_string_pretty(&p.snapshot().await?.0)?),
        "home" => p.home().await?,
        "back" => p.back().await?,
        "activate" => {
            let snapshot = p.snapshot().await?.0;
            let id = args.get(1).map(String::as_str).unwrap_or("demo.activity");
            let path = p
                .activate(&snapshot.generation, id, &uuid::Uuid::new_v4().to_string())
                .await?;
            let op = OperationProxy::builder(&connection)
                .destination(SERVICE)?
                .path(path)?
                .build()
                .await?;
            loop {
                let state = op.state().await?;
                if state == "succeeded" {
                    println!("succeeded");
                    break;
                }
                if state == "failed" {
                    anyhow::bail!("{}", op.error().await?);
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
        _ => anyhow::bail!("usage: arinnctl [status|snapshot|home|back|activate [id]]"),
    }
    Ok(())
}
