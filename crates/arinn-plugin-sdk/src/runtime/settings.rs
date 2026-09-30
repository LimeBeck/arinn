// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Optional `org.arinn.ProviderSettings1` for any provider (ADR-0065). The
//! provider declares a typed schema with defaults; the daemon stores the
//! values and pushes changes with `Apply`. At start the provider reads its
//! values from `ARINN_PROVIDER_SETTINGS` (ADR-0064) with [`startup_settings`].
use super::RegistrationContext;
use arinn_protocol::{
    production,
    settings::{self, Setting},
    validation::ErrorCode,
};
use serde_json::{Map, Value};
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::sync::watch;
use zbus::message::Header;

pub trait SettingsHandler: Send + Sync + 'static {
    /// The schema with defaults as values. It must not change while the
    /// provider runs.
    fn schema(&self) -> Vec<Setting>;
    /// Apply the complete effective values, already checked against the
    /// schema. Answer `Unsupported` when they can only take effect at the
    /// next start.
    fn apply(
        &self,
        values: Map<String, Value>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>>;
}

/// The effective values for `schema` from a stored JSON object: valid stored
/// values, defaults for the rest. Malformed input yields the defaults.
pub fn settings_from_json(schema: &[Setting], stored: &str) -> Map<String, Value> {
    let stored = serde_json::from_str::<Value>(stored)
        .ok()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    settings::effective_values(schema, &stored)
}

/// [`settings_from_json`] over `ARINN_PROVIDER_SETTINGS`.
pub fn startup_settings(schema: &[Setting]) -> Map<String, Value> {
    settings_from_json(
        schema,
        &std::env::var("ARINN_PROVIDER_SETTINGS").unwrap_or_default(),
    )
}

/// Export `ProviderSettings1` on `connection`. `registrations` is the
/// service's registration channel: only the registered daemon may call, and
/// only in the current generation.
pub async fn serve_settings(
    connection: &zbus::Connection,
    handler: impl SettingsHandler,
    registrations: watch::Receiver<Option<RegistrationContext>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let schema = handler.schema();
    settings::validate_schema(&schema).map_err(|e| format!("invalid settings schema: {e}"))?;
    connection
        .object_server()
        .at(
            production::PROVIDER_PATH,
            SettingsIface {
                handler: Arc::new(handler),
                schema,
                registrations,
            },
        )
        .await?;
    Ok(())
}

struct SettingsIface {
    handler: Arc<dyn SettingsHandler>,
    schema: Vec<Setting>,
    registrations: watch::Receiver<Option<RegistrationContext>>,
}
impl SettingsIface {
    fn authorize(&self, generation: &str, header: &Header<'_>) -> Result<(), ErrorCode> {
        let context = self
            .registrations
            .borrow()
            .clone()
            .ok_or(ErrorCode::Unavailable)?;
        if header.sender().map(|s| s.as_str()) != Some(context.daemon_owner.as_str()) {
            return Err(ErrorCode::PermissionDenied);
        }
        if context.generation != generation {
            return Err(ErrorCode::Conflict);
        }
        Ok(())
    }
}
#[zbus::interface(name = "org.arinn.ProviderSettings1")]
impl SettingsIface {
    async fn get_schema(
        &self,
        generation: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<Vec<Setting>> {
        self.authorize(generation, &header)?;
        Ok(self.schema.clone())
    }

    async fn apply(
        &self,
        generation: &str,
        values: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<()> {
        self.authorize(generation, &header)?;
        if values.len() > settings::MAX_JSON_BYTES * 4 {
            return Err(ErrorCode::InvalidArgument.into());
        }
        let Ok(Value::Object(values)) = serde_json::from_str::<Value>(values) else {
            return Err(ErrorCode::InvalidArgument.into());
        };
        // Every id must be known and every value valid; nothing is applied
        // otherwise.
        for (id, value) in &values {
            let setting = self
                .schema
                .iter()
                .find(|s| &s.id == id)
                .ok_or(ErrorCode::InvalidArgument)?;
            setting.check_value(&value.to_string())?;
        }
        let complete = settings::effective_values(&self.schema, &values);
        Ok(self.handler.apply(complete).await?)
    }
}
