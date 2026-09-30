// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ProviderSettings1` served by the SDK (ADR-0065) on a private bus.
mod common;
use arinn_plugin_sdk::runtime::{
    RegistrationContext, SettingsHandler, serve_settings, settings_from_json,
};
use arinn_protocol::{
    settings::{ProviderSettingsProxy, Setting},
    validation::ErrorCode,
};
use serde_json::{Map, Value, json};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

fn schema() -> Vec<Setting> {
    vec![
        Setting {
            id: "roots".into(),
            category: "media".into(),
            title: "Library folders".into(),
            description: String::new(),
            kind: "path".into(),
            value: r#"["/srv/films"]"#.into(),
            constraints: r#"{"directory":true,"multiple":true}"#.into(),
            enabled: true,
        },
        Setting {
            id: "scan".into(),
            category: "media".into(),
            title: "Watch for changes".into(),
            description: String::new(),
            kind: "boolean".into(),
            value: "true".into(),
            constraints: String::new(),
            enabled: true,
        },
    ]
}

#[derive(Clone, Default)]
struct Handler(Arc<Mutex<Vec<Map<String, Value>>>>);
impl SettingsHandler for Handler {
    fn schema(&self) -> Vec<Setting> {
        schema()
    }
    fn apply(
        &self,
        values: Map<String, Value>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        self.0.lock().unwrap().push(values);
        Box::pin(async { Ok(()) })
    }
}

fn error_name(result: zbus::Result<impl std::fmt::Debug>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => format!("{other:?}"),
    }
}

#[tokio::test]
async fn settings_are_served_only_to_the_daemon_and_checked() {
    let bus = common::Bus::start();
    let daemon = bus.connection().await;
    let provider = bus.connection().await;
    let stranger = bus.connection().await;
    let generation = uuid::Uuid::new_v4().to_string();
    let (_registrations, receiver) = tokio::sync::watch::channel(Some(RegistrationContext {
        connection: provider.clone(),
        daemon_owner: daemon.unique_name().unwrap().to_string(),
        daemon_epoch: "epoch".into(),
        generation: generation.clone(),
    }));
    let handler = Handler::default();
    serve_settings(&provider, handler.clone(), receiver)
        .await
        .unwrap();
    let proxy = |connection: &zbus::Connection| {
        ProviderSettingsProxy::builder(connection)
            .destination(provider.unique_name().unwrap().clone())
            .unwrap()
            .build()
    };
    let settings = proxy(&daemon).await.unwrap();
    assert_eq!(settings.get_schema(&generation).await.unwrap(), schema());

    // A partial update is completed with defaults before it is applied.
    settings
        .apply(&generation, r#"{"roots":["/mnt/nas/films"]}"#)
        .await
        .unwrap();
    assert_eq!(
        Value::Object(handler.0.lock().unwrap()[0].clone()),
        json!({"roots":["/mnt/nas/films"],"scan":true})
    );
    for bad in [
        r#"{"roots":"/not/a/list"}"#,
        r#"{"unknown":true}"#,
        r#"["not","an","object"]"#,
        "not json",
    ] {
        assert_eq!(
            error_name(settings.apply(&generation, bad).await),
            "org.arinn.Error1.InvalidArgument",
            "{bad}"
        );
    }
    assert_eq!(
        error_name(settings.get_schema("stale").await),
        "org.arinn.Error1.Conflict"
    );
    // Nobody but the registered daemon.
    let other = proxy(&stranger).await.unwrap();
    assert_eq!(
        error_name(other.get_schema(&generation).await),
        "org.arinn.Error1.PermissionDenied"
    );
    assert_eq!(handler.0.lock().unwrap().len(), 1);

    // The live export matches the published XML.
    let live = zbus::fdo::IntrospectableProxy::builder(&daemon)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .path("/org/arinn/Provider1")
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    let file = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/provider-settings1.xml"
    ))
    .unwrap();
    for needle in [
        "<method name=\"GetSchema\">",
        "<method name=\"Apply\">",
        "a(sssssssb)",
    ] {
        assert!(live.contains(needle) && file.contains(needle), "{needle}");
    }
}

#[test]
fn startup_values_come_from_the_stored_json() {
    let values = settings_from_json(&schema(), r#"{"roots":["/data"],"scan":"yes"}"#);
    // The invalid `scan` falls back to its default.
    assert_eq!(
        Value::Object(values),
        json!({"roots":["/data"],"scan":true})
    );
    assert_eq!(
        Value::Object(settings_from_json(&schema(), "")),
        json!({"roots":["/srv/films"],"scan":true})
    );
}
