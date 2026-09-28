// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reusable application-capability contract client for the private-bus harness.
use arinn_protocol::production::{
    ApplicationApiProxy, PROVIDER_PATH, ProductionDaemonProxy, ProductionOperationProxy,
    Registration,
};
use std::time::Duration;
use uuid::Uuid;
fn expect(error: zbus::Error, suffix: &str) {
    match error {
        zbus::Error::MethodError(name, _, _) => {
            assert_eq!(name.as_str(), format!("org.arinn.Error1.{suffix}"))
        }
        other => panic!("unexpected transport error: {other}"),
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let connection = zbus::Connection::session().await?;
    let daemon = ProductionDaemonProxy::new(&connection).await?;
    let provider_id = std::env::args().nth(1).unwrap_or("test".into());
    let mode = std::env::args().nth(2).unwrap_or("basic".into());
    let info = daemon
        .providers()
        .await?
        .into_iter()
        .find(|p| p.id == provider_id)
        .unwrap();
    // Run only declared capability contracts; no product-specific routing.
    assert_eq!(info.capabilities, vec!["application"]);
    let app = ApplicationApiProxy::builder(&connection)
        .path(info.path.clone())?
        .build()
        .await?;
    let (first,) = app.get_applications("").await?;
    first.validate()?;
    let request = Uuid::new_v4().to_string();
    expect(
        app.activate(&info.generation, "unknown", &request)
            .await
            .unwrap_err(),
        "NotFound",
    );
    expect(
        app.activate(&Uuid::new_v4().to_string(), "demo.activity", &request)
            .await
            .unwrap_err(),
        "Conflict",
    );
    expect(
        app.activate(&info.generation, "demo.activity", "not-a-uuid")
            .await
            .unwrap_err(),
        "InvalidArgument",
    );
    expect(
        app.activate(&info.generation, &"x".repeat(257), &request)
            .await
            .unwrap_err(),
        "InvalidArgument",
    );
    let path = app
        .activate(&info.generation, "demo.activity", &request)
        .await?;
    assert_eq!(
        app.activate(&info.generation, "demo.activity", &request)
            .await?,
        path
    );
    expect(
        app.terminate(&info.generation, "demo.activity", &request)
            .await
            .unwrap_err(),
        "Conflict",
    );
    let operation = ProductionOperationProxy::builder(&connection)
        .path(path.clone())?
        .build()
        .await?;
    let _subscription = operation.receive_changed().await?;
    expect(operation.cancel().await.unwrap_err(), "Unsupported");
    if !first.next_token.is_empty() {
        let other = zbus::Connection::session().await?;
        let other_app = ApplicationApiProxy::builder(&other)
            .path(info.path.clone())?
            .build()
            .await?;
        expect(
            other_app
                .get_applications(&first.next_token)
                .await
                .unwrap_err(),
            "PermissionDenied",
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
        let (second,) = app.get_applications(&first.next_token).await?;
        assert_eq!(second.generation, first.generation);
        assert_eq!(second.revision, first.revision);
    }
    if mode == "limits" {
        for _ in 1..32 {
            app.activate(
                &info.generation,
                "demo.activity",
                &Uuid::new_v4().to_string(),
            )
            .await?;
        }
        expect(
            app.activate(
                &info.generation,
                "demo.activity",
                &Uuid::new_v4().to_string(),
            )
            .await
            .unwrap_err(),
            "Unavailable",
        );
        let object = daemon
            .restart(&provider_id, &Uuid::new_v4().to_string())
            .await?;
        let restart = ProductionOperationProxy::builder(&connection)
            .path(object)?
            .build()
            .await?;
        for _ in 0..100 {
            if restart.get().await?.0.state == "succeeded" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let state = operation.get().await?.0;
        assert_eq!(state.state, "failed");
        assert_eq!(state.error, "Unavailable");
        tokio::time::sleep(Duration::from_secs(7)).await;
        assert_eq!(operation.get().await?.0.error, "Unavailable");
    } else {
        for _ in 0..100 {
            if operation.get().await?.0.state == "succeeded" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let result = operation.get().await?.0;
        assert_eq!(result.state, "succeeded");
        assert!(result.started_at > 0 && result.ended_at >= result.started_at);
    }
    let invalid = Registration {
        plugin_id: provider_id.clone(),
        provider_id,
        api_major: 99,
        api_minor: 0,
        version: "0.0.1".into(),
        capabilities: vec!["application".into()],
        object_path: PROVIDER_PATH.try_into()?,
        token: "invalid".into(),
    };
    expect(daemon.register(invalid).await.unwrap_err(), "Unsupported");
    println!(
        "PASS: declared application contract, safe errors, generation validation, request deduplication, immutable/caller-bound pages and operation lifecycle"
    );
    Ok(())
}
