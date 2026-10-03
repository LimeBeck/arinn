// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-stable provider settings contract (spec §5.6, ADR-0064, ADR-0065).
//!
//! A provider declares a typed schema with defaults through the optional
//! `org.arinn.ProviderSettings1`. The daemon stores the current values in
//! `config.toml` (ADR-0064), serves them to the shell through
//! `org.arinn.Settings1` and pushes changes to the running provider with
//! `Apply`. Values and constraints travel as JSON so that the schema, the
//! stored values and `ARINN_PROVIDER_SETTINGS` share one representation.
//! Providers never supply UI (R-04): the shell renders the typed controls.
use crate::validation::{self, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;
use zbus::zvariant::{OwnedObjectPath, Type};

pub const PROVIDER_INTERFACE: &str = "org.arinn.ProviderSettings1";
pub const INTERFACE: &str = "org.arinn.Settings1";

/// Setting types (spec §5.6).
pub const KIND_BOOLEAN: &str = "boolean";
pub const KIND_INTEGER: &str = "integer";
pub const KIND_STRING: &str = "string";
pub const KIND_ENUM: &str = "enum";
pub const KIND_PATH: &str = "path";
pub const KINDS: &[&str] = &[
    KIND_BOOLEAN,
    KIND_INTEGER,
    KIND_STRING,
    KIND_ENUM,
    KIND_PATH,
];

/// Settings screen categories (spec §5.6, design handoff).
pub const CATEGORIES: &[&str] = &[
    "media",
    "playback",
    "sound",
    "network",
    "applications",
    "appearance",
    "system",
];

pub const MAX_SETTINGS: usize = 128;
pub const MAX_JSON_BYTES: usize = 16 * 1024;
pub const MAX_STRING_LENGTH: u64 = 4096;
pub const MAX_OPTIONS: usize = 64;
pub const MAX_PATHS: u64 = 64;
/// Longest integer `unit`, in bytes (ADR-0071).
pub const MAX_UNIT_BYTES: usize = 16;
const DEFAULT_STRING_LENGTH: u64 = 1024;

/// One setting. In a provider's schema `value` is the default; from the
/// daemon it is the current value. `value` and `constraints` are JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Setting {
    pub id: String,
    pub category: String,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub value: String,
    pub constraints: String,
    pub enabled: bool,
}

/// Parsed constraints of one kind. Unknown keys are rejected.
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Constraints {
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub max_length: Option<u64>,
    #[serde(default)]
    pub options: Vec<EnumOption>,
    #[serde(default)]
    pub directory: bool,
    #[serde(default)]
    pub multiple: bool,
    pub max_items: Option<u64>,
    /// Integer only: short display text after the value, such as `min`
    /// (ADR-0071). The shell shows it; values stay plain integers.
    pub unit: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumOption {
    pub value: String,
    pub title: String,
}

fn json(text: &str) -> Result<Value, ErrorCode> {
    if text.len() > MAX_JSON_BYTES {
        return Err(ErrorCode::InvalidArgument);
    }
    serde_json::from_str(text).map_err(|_| ErrorCode::InvalidArgument)
}

impl Setting {
    /// The constraints, checked against the kind.
    pub fn constraints(&self) -> Result<Constraints, ErrorCode> {
        let c: Constraints = if self.constraints.is_empty() {
            Constraints::default()
        } else {
            serde_json::from_value(json(&self.constraints)?)
                .map_err(|_| ErrorCode::InvalidArgument)?
        };
        let integer_keys = c.min.is_some() || c.max.is_some();
        let string_keys = c.max_length.is_some();
        let path_keys = c.directory || c.multiple || c.max_items.is_some();
        let unit_ok = c.unit.as_deref().is_none_or(unit);
        let ok = match self.kind.as_str() {
            KIND_BOOLEAN => {
                !integer_keys
                    && !string_keys
                    && !path_keys
                    && c.options.is_empty()
                    && c.unit.is_none()
            }
            KIND_INTEGER => {
                !string_keys
                    && !path_keys
                    && c.options.is_empty()
                    && unit_ok
                    && c.min.zip(c.max).is_none_or(|(min, max)| min <= max)
            }
            _ if c.unit.is_some() => false,
            KIND_STRING => {
                !integer_keys
                    && !path_keys
                    && c.options.is_empty()
                    && c.max_length
                        .is_none_or(|l| (1..=MAX_STRING_LENGTH).contains(&l))
            }
            KIND_ENUM => {
                let mut seen = HashSet::new();
                !integer_keys
                    && !string_keys
                    && !path_keys
                    && (1..=MAX_OPTIONS).contains(&c.options.len())
                    && c.options.iter().all(|o| {
                        validation::id(&o.value).is_ok()
                            && validation::text(&o.title).is_ok()
                            && seen.insert(o.value.as_str())
                    })
            }
            KIND_PATH => {
                !integer_keys
                    && !string_keys
                    && c.options.is_empty()
                    && (c.multiple || c.max_items.is_none())
                    && c.max_items.is_none_or(|n| (1..=MAX_PATHS).contains(&n))
            }
            _ => false,
        };
        ok.then_some(c).ok_or(ErrorCode::InvalidArgument)
    }

    /// Checks a candidate value (JSON text) against this setting.
    pub fn check_value(&self, value: &str) -> Result<Value, ErrorCode> {
        let parsed = json(value)?;
        self.check(&parsed)?;
        Ok(parsed)
    }

    fn check(&self, value: &Value) -> Result<(), ErrorCode> {
        let c = self.constraints()?;
        let ok = match self.kind.as_str() {
            KIND_BOOLEAN => value.is_boolean(),
            KIND_INTEGER => value.as_i64().is_some_and(|n| {
                c.min.is_none_or(|min| n >= min) && c.max.is_none_or(|max| n <= max)
            }),
            KIND_STRING => value.as_str().is_some_and(|s| {
                s.chars().count() as u64 <= c.max_length.unwrap_or(DEFAULT_STRING_LENGTH)
                    && !s.chars().any(char::is_control)
            }),
            KIND_ENUM => value
                .as_str()
                .is_some_and(|s| c.options.iter().any(|o| o.value == s)),
            KIND_PATH if c.multiple => value.as_array().is_some_and(|items| {
                items.len() as u64 <= c.max_items.unwrap_or(MAX_PATHS)
                    && items.iter().all(|i| i.as_str().is_some_and(path))
            }),
            KIND_PATH => value.as_str().is_some_and(path),
            _ => false,
        };
        ok.then_some(()).ok_or(ErrorCode::InvalidArgument)
    }

    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        if !CATEGORIES.contains(&self.category.as_str()) || !KINDS.contains(&self.kind.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
        validation::text(&self.title)?;
        validation::text(&self.description)?;
        self.check_value(&self.value).map(|_| ())
    }
}

/// An integer unit: 1 to [`MAX_UNIT_BYTES`] bytes of display text with no
/// control characters and no surrounding spaces.
pub fn unit(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_UNIT_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

/// An absolute local path: no empty, `.` or `..` segment, no control
/// characters, no network (`//`) prefix, at most 4096 bytes.
pub fn path(value: &str) -> bool {
    value.len() <= 4096
        && value.starts_with('/')
        && !value.starts_with("//")
        && !value.chars().any(char::is_control)
        && (value == "/"
            || value[1..]
                .trim_end_matches('/')
                .split('/')
                .all(|s| !s.is_empty() && s != "." && s != ".."))
}

pub fn validate_schema(settings: &[Setting]) -> Result<(), ErrorCode> {
    if settings.len() > MAX_SETTINGS {
        return Err(ErrorCode::InvalidArgument);
    }
    let mut ids = HashSet::new();
    for setting in settings {
        setting.validate()?;
        if !ids.insert(setting.id.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    Ok(())
}

/// The effective values for a schema: each stored value that is still valid
/// for its setting, otherwise the default. Unknown ids are dropped. This is
/// the JSON object a provider receives (`Apply`, `ARINN_PROVIDER_SETTINGS`).
pub fn effective_values(schema: &[Setting], stored: &Map<String, Value>) -> Map<String, Value> {
    schema
        .iter()
        .filter_map(|setting| {
            let value = stored
                .get(&setting.id)
                .filter(|v| setting.check(v).is_ok())
                .cloned()
                .or_else(|| json(&setting.value).ok())?;
            Some((setting.id.clone(), value))
        })
        .collect()
}

pub mod provider {
    use super::Setting;
    #[zbus::proxy(
        interface = "org.arinn.ProviderSettings1",
        default_path = "/org/arinn/Provider1"
    )]
    pub trait ProviderSettings {
        /// The schema, with defaults as values.
        fn get_schema(&self, generation: &str) -> zbus::Result<Vec<Setting>>;
        /// Apply the complete set of effective values (a JSON object). A
        /// provider that can only apply them at its next start answers
        /// `Unsupported`; invalid values are `InvalidArgument`.
        fn apply(&self, generation: &str, values: &str) -> zbus::Result<()>;
    }
}
pub use provider::ProviderSettingsProxy;

/// `org.arinn.Settings1` on `/org/arinn/Daemon1`. The provider id `arinn` is
/// reserved for the daemon's own settings.
#[zbus::proxy(
    interface = "org.arinn.Settings1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait SettingsApi {
    /// Providers with settings: (provider id, name).
    fn list_providers(&self) -> zbus::Result<Vec<(String, String)>>;
    /// The provider's generation and its settings with current values.
    fn get_settings(&self, provider_id: &str) -> zbus::Result<(String, Vec<Setting>)>;
    /// Store a value and push it to the running provider; returns an
    /// `Operation1`.
    fn set_value(
        &self,
        provider_id: &str,
        generation: &str,
        setting_id: &str,
        value: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
    #[zbus(signal)]
    fn settings_changed(&self, provider_id: &str) -> zbus::Result<()>;
}

/// The daemon's own settings appear under this provider id.
pub const DAEMON_PROVIDER_ID: &str = "arinn";

#[cfg(test)]
mod tests;
