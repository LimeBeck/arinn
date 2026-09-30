// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.Settings1` (ADR-0065): provider settings schemas, current
//! values from `config.toml` (ADR-0064) and live apply. The provider id
//! `arinn` carries the daemon's own settings. Values are never logged.
use super::*;
use crate::storage::InstanceConfig;
use arinn_protocol::settings::{
    self, DAEMON_PROVIDER_ID, ProviderSettingsProxy, Setting, effective_values,
};
use serde_json::{Map, Value};

const SCHEMA_TIMEOUT: Duration = Duration::from_secs(2);
const APPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// The daemon's own settings. `search_engine` is listed only when engines
/// are packaged (ADR-0066); its default is the first one.
fn daemon_schema(engines: &[crate::browse::Engine]) -> Vec<Setting> {
    let mut schema = vec![Setting {
        id: "browser_application".into(),
        category: "applications".into(),
        title: "Browser application".into(),
        description: "Application ID launched when a web shortcut opens with no browser running; empty uses the packaged choice".into(),
        kind: settings::KIND_STRING.into(),
        value: "\"\"".into(),
        constraints: r#"{"max_length":255}"#.into(),
        enabled: true,
    }];
    if let Some(first) = engines.first() {
        let options: Vec<Value> = engines
            .iter()
            .map(|e| serde_json::json!({"value": e.id, "title": e.title}))
            .collect();
        schema.push(Setting {
            id: "search_engine".into(),
            category: "browser".into(),
            title: "Search engine".into(),
            description: "Used when typed text is not an address".into(),
            kind: settings::KIND_ENUM.into(),
            value: Value::String(first.id.clone()).to_string(),
            constraints: serde_json::json!({ "options": options }).to_string(),
            enabled: true,
        });
    }
    schema
}

/// Stored values of one provider as a JSON object.
fn stored_values(config: &Config, provider: &str) -> Map<String, Value> {
    if provider == DAEMON_PROVIDER_ID {
        let mut map = Map::new();
        map.insert(
            "browser_application".into(),
            Value::String(config.browser_application.clone()),
        );
        if !config.search_engine.is_empty() {
            map.insert(
                "search_engine".into(),
                Value::String(config.search_engine.clone()),
            );
        }
        return map;
    }
    config
        .providers
        .get(provider)
        .and_then(|i| crate::storage::settings_json(&i.settings).ok())
        .and_then(|j| serde_json::from_str::<Value>(&j).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn missing_interface(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::MethodError(name, _, _) => [
            "org.freedesktop.DBus.Error.UnknownInterface",
            "org.freedesktop.DBus.Error.UnknownMethod",
            "org.freedesktop.DBus.Error.UnknownObject",
        ]
        .contains(&name.as_str()),
        zbus::Error::FDO(e) => matches!(
            **e,
            zbus::fdo::Error::UnknownInterface(_)
                | zbus::fdo::Error::UnknownMethod(_)
                | zbus::fdo::Error::UnknownObject(_)
        ),
        _ => false,
    }
}

impl Core {
    pub async fn export_settings(&self) -> Result<(), ErrorCode> {
        self.connection
            .object_server()
            .at(production::PATH, SettingsEndpoint { core: self.clone() })
            .await
            .map(|_| ())
            .map_err(|_| ErrorCode::Internal)
    }

    /// The provider's generation and schema; an empty schema when the
    /// provider has no settings. Cached per registration generation.
    async fn settings_schema(&self, id: &str) -> Result<(String, Vec<Setting>), ErrorCode> {
        if id == DAEMON_PROVIDER_ID {
            return Ok((self.epoch.clone(), daemon_schema(&self.search_engines)));
        }
        let (owner, generation) = {
            let state = self.state.lock().await;
            let p = state.providers.get(id).ok_or(ErrorCode::NotFound)?;
            if p.info.runtime_state != "ready" || p.owner.is_empty() {
                return Err(ErrorCode::Unavailable);
            }
            if let Some((cached, schema)) = &p.settings_schema
                && *cached == p.info.generation
            {
                return Ok((cached.clone(), schema.clone()));
            }
            (p.owner.clone(), p.info.generation.clone())
        };
        let proxy = ProviderSettingsProxy::builder(&self.connection)
            .destination(owner.as_str())
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let schema = match tokio::time::timeout(SCHEMA_TIMEOUT, proxy.get_schema(&generation)).await
        {
            Err(_) => return Err(ErrorCode::Timeout),
            Ok(Err(error)) if missing_interface(&error) => Vec::new(),
            Ok(Err(error)) => return Err(normalize_error(error)),
            // A schema the contract rejects counts as no settings.
            Ok(Ok(schema)) => {
                if settings::validate_schema(&schema).is_ok() {
                    schema
                } else {
                    Vec::new()
                }
            }
        };
        let mut state = self.state.lock().await;
        if let Some(p) = state.providers.get_mut(id)
            && p.info.generation == generation
        {
            p.settings_schema = Some((generation.clone(), schema.clone()));
        }
        Ok((generation, schema))
    }

    async fn settings_providers(&self) -> Vec<(String, String)> {
        let candidates: Vec<(String, String)> = {
            let state = self.state.lock().await;
            state
                .providers
                .iter()
                .filter(|(_, p)| p.info.runtime_state == "ready" && !p.owner.is_empty())
                .map(|(id, p)| (id.clone(), p.info.name.clone()))
                .collect()
        };
        let mut result = vec![(DAEMON_PROVIDER_ID.to_owned(), "Arinn".to_owned())];
        let checks = candidates.into_iter().map(|(id, name)| async move {
            let has = self
                .settings_schema(&id)
                .await
                .is_ok_and(|(_, schema)| !schema.is_empty());
            has.then_some((id, name))
        });
        result.extend(
            futures_util::future::join_all(checks)
                .await
                .into_iter()
                .flatten(),
        );
        result
    }

    async fn current_settings(&self, id: &str) -> Result<(String, Vec<Setting>), ErrorCode> {
        let (generation, schema) = self.settings_schema(id).await?;
        let config = Config::load(&self.paths.config)?;
        let values = effective_values(&schema, &stored_values(&config, id));
        let settings = schema
            .into_iter()
            .map(|mut s| {
                if let Some(v) = values.get(&s.id) {
                    s.value = v.to_string();
                }
                s
            })
            .collect();
        Ok((generation, settings))
    }

    async fn set_setting(
        &self,
        id: &str,
        generation: &str,
        setting: &str,
        value: &str,
        request_id: &str,
        caller: &str,
    ) -> Result<OwnedObjectPath, ErrorCode> {
        validation::id(id)?;
        validation::id(setting)?;
        let fingerprint = serde_json::to_string(&("settings", id, generation, setting, value))
            .map_err(|_| ErrorCode::Internal)?;
        if let Some(path) = self
            .operations
            .existing(caller, request_id, &fingerprint)
            .await?
        {
            return Ok(path);
        }
        let (current, schema) = self.settings_schema(id).await?;
        if current != generation {
            return Err(ErrorCode::Conflict);
        }
        let parsed = schema
            .iter()
            .find(|s| s.id == setting)
            .ok_or(ErrorCode::NotFound)?
            .check_value(value)?;
        let (owner, plugin_id) = if id == DAEMON_PROVIDER_ID {
            (String::new(), String::new())
        } else {
            let state = self.state.lock().await;
            let p = state.providers.get(id).ok_or(ErrorCode::NotFound)?;
            (p.owner.clone(), p.info.plugin_id.clone())
        };
        let (path, op, fresh) = self
            .operations
            .admit(&self.connection, caller, request_id, &fingerprint, id)
            .await?;
        if !fresh {
            return Ok(path);
        }
        let core = self.clone();
        let (id, generation, setting) = (id.to_owned(), generation.to_owned(), setting.to_owned());
        let path_copy = path.clone();
        tokio::spawn(async move {
            {
                let mut value = op.value.lock().await;
                if value.state != "queued" {
                    return;
                }
                value.state = "running".into();
                value.started_at = now();
            }
            let result = core
                .store_and_apply(
                    &id,
                    &generation,
                    &setting,
                    parsed,
                    &schema,
                    &owner,
                    &plugin_id,
                )
                .await;
            if result.is_ok()
                && let Ok(emitter) = SignalEmitter::new(&core.connection, production::PATH)
            {
                let _ = SettingsEndpoint::settings_changed(&emitter, &id).await;
            }
            op.finish(&core.connection, &path_copy, result.map(|_| Vec::new()))
                .await;
        });
        Ok(path)
    }

    /// Write the value to `config.toml`, then apply the complete values to
    /// the running provider. A provider that rejects them restores the
    /// previous file; `Unsupported` keeps the value for its next start.
    #[allow(clippy::too_many_arguments)]
    async fn store_and_apply(
        &self,
        id: &str,
        generation: &str,
        setting: &str,
        value: Value,
        schema: &[Setting],
        owner: &str,
        plugin_id: &str,
    ) -> Result<(), ErrorCode> {
        let _serial = self.settings_lock.lock().await;
        let directory = self.paths.config.clone();
        let mut config = Config::load(&directory)?;
        let previous = config.clone();
        if id == DAEMON_PROVIDER_ID {
            let value = value.as_str().unwrap_or_default().to_owned();
            match setting {
                "search_engine" => config.search_engine = value,
                _ => config.browser_application = value,
            }
            return config.save(&directory);
        }
        let entry = config
            .providers
            .entry(id.to_owned())
            .or_insert_with(|| InstanceConfig {
                plugin_id: plugin_id.to_owned(),
                enabled: true,
                settings: toml::Table::new(),
            });
        entry.settings.insert(
            setting.to_owned(),
            toml::Value::try_from(&value).map_err(|_| ErrorCode::InvalidArgument)?,
        );
        config.save(&directory)?;
        let values = Value::Object(effective_values(schema, &stored_values(&config, id)));
        let applied = async {
            let proxy = ProviderSettingsProxy::builder(&self.connection)
                .destination(owner)
                .map_err(|_| ErrorCode::Internal)?
                .build()
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            tokio::time::timeout(APPLY_TIMEOUT, proxy.apply(generation, &values.to_string()))
                .await
                .map_err(|_| ErrorCode::Timeout)?
                .map_err(normalize_error)
        }
        .await;
        match applied {
            Ok(()) | Err(ErrorCode::Unsupported) => Ok(()),
            Err(error) => {
                previous.save(&directory)?;
                Err(error)
            }
        }
    }
}

pub(super) struct SettingsEndpoint {
    core: Core,
}
#[zbus::interface(name = "org.arinn.Settings1")]
impl SettingsEndpoint {
    async fn list_providers(&self) -> Vec<(String, String)> {
        self.core.settings_providers().await
    }
    async fn get_settings(&self, provider_id: &str) -> production::Result<(String, Vec<Setting>)> {
        Ok(self.core.current_settings(provider_id).await?)
    }
    async fn set_value(
        &self,
        provider_id: &str,
        generation: &str,
        setting_id: &str,
        value: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .set_setting(
                provider_id,
                generation,
                setting_id,
                value,
                request_id,
                &sender(&h)?,
            )
            .await?)
    }
    #[zbus(signal)]
    async fn settings_changed(emitter: &SignalEmitter<'_>, provider_id: &str) -> zbus::Result<()>;
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
