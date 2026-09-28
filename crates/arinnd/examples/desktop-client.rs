// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! End-to-end client for the temporary desktop-provider private-bus suite.
use arinn_protocol::{
    application::ApplicationPage,
    kwin::KWinBridgeProxy,
    production::{ApplicationApiProxy, ProductionDaemonProxy, ProductionOperationProxy},
};
use std::time::Duration;
use uuid::Uuid;
use zbus::{Connection, zvariant::OwnedObjectPath};
async fn page(
    app: &ApplicationApiProxy<'_>,
    predicate: impl Fn(&ApplicationPage) -> bool,
) -> ApplicationPage {
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            if let Ok((page,)) = app.get_applications("").await
                && predicate(&page)
            {
                return page;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap()
}
async fn operation(c: &Connection, path: OwnedObjectPath, error: &str) {
    let proxy = ProductionOperationProxy::builder(c)
        .path(path)
        .unwrap()
        .build()
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            let (info,) = proxy.get().await.unwrap();
            if ["succeeded", "failed"].contains(&info.state.as_str()) {
                assert_eq!(info.error, error);
                assert_eq!(
                    info.state,
                    if error.is_empty() {
                        "succeeded"
                    } else {
                        "failed"
                    }
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
}
fn error(result: zbus::Result<impl std::fmt::Debug>, expected: &str) {
    assert!(
        matches!(result,Err(zbus::Error::MethodError(ref name,_,_)) if name.as_str()==format!("org.arinn.Error1.{expected}")),
        "{result:?}"
    );
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    anyhow::ensure!(std::env::var("ARINN_PRIVATE_TEST_BUS").as_deref() == Ok("1"));
    let c = Connection::session().await?;
    c.request_name("org.kde.KWin").await?;
    let daemon = ProductionDaemonProxy::new(&c).await?;
    let info = daemon
        .providers()
        .await?
        .into_iter()
        .find(|p| p.id == "desktop")
        .unwrap();
    let app = ApplicationApiProxy::builder(&c)
        .path(info.path)?
        .build()
        .await?;
    let bridge = KWinBridgeProxy::new(&c).await?;
    let generation = bridge.connect(&Uuid::new_v4().to_string()).await?;
    bridge.begin_inventory(&generation, 1, 3).await?;
    for (i, token, desktop, flags) in [
        (0, "window-a", "fixture.desktop", 13),
        (1, "window-b", "fixture.desktop", 4),
        (2, "unknown", "unproven", 0),
    ] {
        bridge
            .put_window(&generation, 1, i, token, desktop, "", flags)
            .await?;
    }
    bridge.commit_inventory(&generation, 1).await?;
    let controller = bridge.clone();
    let control_generation = generation.clone();
    let poll = tokio::spawn(async move {
        loop {
            let command = controller.next_control(&control_generation).await?;
            if command.len() == 3 {
                controller
                    .complete_control(
                        &control_generation,
                        &command[0],
                        if command[1] == "close" {
                            "PermissionDenied"
                        } else {
                            ""
                        },
                    )
                    .await?;
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), zbus::Error>(())
    });
    let current = page(&app, |p| p.applications.len() == 2 && p.running.len() == 2).await;
    assert_eq!(current.running[0].id, "window-a");
    let request = Uuid::new_v4().to_string();
    let launch = app
        .launch(&current.generation, "fixture.desktop", &request)
        .await?;
    assert_eq!(
        app.launch(&current.generation, "fixture.desktop", &request)
            .await?,
        launch
    );
    operation(&c, launch, "").await;
    operation(
        &c,
        app.launch(
            &current.generation,
            "broken.desktop",
            &Uuid::new_v4().to_string(),
        )
        .await?,
        "Unavailable",
    )
    .await;
    operation(
        &c,
        app.activate(&current.generation, "window-a", &Uuid::new_v4().to_string())
            .await?,
        "",
    )
    .await;
    operation(
        &c,
        app.terminate(&current.generation, "window-a", &Uuid::new_v4().to_string())
            .await?,
        "PermissionDenied",
    )
    .await;
    error(
        app.terminate(&current.generation, "window-b", &Uuid::new_v4().to_string())
            .await,
        "Unsupported",
    );
    error(
        app.activate(
            &Uuid::new_v4().to_string(),
            "window-a",
            &Uuid::new_v4().to_string(),
        )
        .await,
        "Conflict",
    );
    bridge.begin_inventory(&generation, 2, 1).await?;
    bridge
        .put_window(&generation, 2, 0, "remapped", "fixture.desktop", "", 13)
        .await?;
    bridge.commit_inventory(&generation, 2).await?;
    page(&app, |p| {
        p.running.len() == 1 && p.running[0].id == "remapped"
    })
    .await;
    error(
        app.activate(&current.generation, "window-a", &Uuid::new_v4().to_string())
            .await,
        "NotFound",
    );
    // Lost authoritative inventory clears running instances, not the launch catalogue.
    c.release_name("org.kde.KWin").await?;
    page(&app, |p| p.running.is_empty() && p.applications.len() == 2).await;
    poll.abort();
    let _ = poll.await;
    println!(
        "PASS: desktop catalogue, system launch, controls, failures, remap and inventory loss"
    );
    Ok(())
}
