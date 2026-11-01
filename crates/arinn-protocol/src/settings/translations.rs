// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Translations of a provider's setting texts (ADR-0129). The schema keeps
//! the English source texts; a provider may add one table with the title,
//! description, enum option titles and integer unit per locale and setting.
//! The daemon passes the table through, and each client picks its language
//! with [`Translations::lookup`], falling back to the source text.
use super::{KIND_ENUM, KIND_INTEGER, Setting, unit};
use crate::validation::{self, ErrorCode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most locales in one table.
pub const MAX_LOCALES: usize = 64;
/// Largest table, as JSON.
pub const MAX_TRANSLATION_BYTES: usize = 256 * 1024;

/// The translated texts of one setting. Every field is optional; an absent
/// field keeps the source text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Text {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Integer settings with a `unit` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Enum value → option title.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
}

/// Locale → setting id → texts. Serialized as one JSON object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Translations(pub BTreeMap<String, BTreeMap<String, Text>>);

impl Translations {
    /// Adds the texts of `id` in `locale`, replacing earlier ones.
    pub fn with(mut self, locale: &str, id: &str, text: Text) -> Self {
        self.0
            .entry(locale.to_owned())
            .or_default()
            .insert(id.to_owned(), text);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.0.values().all(BTreeMap::is_empty)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }

    /// Parses a table; it still has to be [validated](Self::validate)
    /// against its schema.
    pub fn from_json(text: &str) -> Result<Self, ErrorCode> {
        if text.len() > MAX_TRANSLATION_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        serde_json::from_str(text).map_err(|_| ErrorCode::InvalidArgument)
    }

    /// Checks the table against the schema it translates: known locales
    /// keys, setting ids and option values, a unit only where the setting
    /// has one, and the size limits.
    pub fn validate(&self, schema: &[Setting]) -> Result<(), ErrorCode> {
        if self.0.len() > MAX_LOCALES || self.to_json().len() > MAX_TRANSLATION_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        for (key, texts) in &self.0 {
            if !locale(key) {
                return Err(ErrorCode::InvalidArgument);
            }
            for (id, text) in texts {
                let setting = schema
                    .iter()
                    .find(|s| &s.id == id)
                    .ok_or(ErrorCode::InvalidArgument)?;
                check_text(setting, text)?;
            }
        }
        Ok(())
    }

    /// The texts of `id` for `locale` (`ru_RU`): each field from the full
    /// locale, else from its language (`ru`). Fields that neither has stay
    /// absent, and the caller shows the source text.
    pub fn lookup(&self, locale: &str, id: &str) -> Text {
        let language = locale
            .split_once('_')
            .map_or(locale, |(language, _)| language);
        let find = |key: &str| self.0.get(key).and_then(|texts| texts.get(id));
        let (full, base) = (
            (language != locale).then(|| find(locale)).flatten(),
            find(language),
        );
        let pick = |field: fn(&Text) -> &Option<String>| {
            full.and_then(|t| field(t).clone())
                .or_else(|| base.and_then(|t| field(t).clone()))
        };
        let mut options = base.map(|t| t.options.clone()).unwrap_or_default();
        options.extend(full.map(|t| t.options.clone()).unwrap_or_default());
        Text {
            title: pick(|t| &t.title),
            description: pick(|t| &t.description),
            unit: pick(|t| &t.unit),
            options,
        }
    }
}

fn check_text(setting: &Setting, text: &Text) -> Result<(), ErrorCode> {
    let constraints = setting.constraints()?;
    let non_empty = |value: &str| {
        validation::text(value)?;
        (!value.is_empty())
            .then_some(())
            .ok_or(ErrorCode::InvalidArgument)
    };
    if let Some(title) = &text.title {
        non_empty(title)?;
    }
    if let Some(description) = &text.description {
        validation::text(description)?;
    }
    if let Some(value) = &text.unit
        && !(setting.kind == KIND_INTEGER && constraints.unit.is_some() && unit(value))
    {
        return Err(ErrorCode::InvalidArgument);
    }
    if !text.options.is_empty() && setting.kind != KIND_ENUM {
        return Err(ErrorCode::InvalidArgument);
    }
    for (value, title) in &text.options {
        if !constraints.options.iter().any(|o| &o.value == value) {
            return Err(ErrorCode::InvalidArgument);
        }
        non_empty(title)?;
    }
    Ok(())
}

/// A locale key: a language of two or three lowercase letters, optionally
/// followed by `_` and a territory of two uppercase letters (`ru`, `pt_BR`),
/// the form of Qt's `QLocale::name()`.
pub fn locale(value: &str) -> bool {
    let (language, territory) = match value.split_once('_') {
        Some((language, territory)) => (language, Some(territory)),
        None => (value, None),
    };
    (2..=3).contains(&language.len())
        && language.bytes().all(|b| b.is_ascii_lowercase())
        && territory.is_none_or(|t| t.len() == 2 && t.bytes().all(|b| b.is_ascii_uppercase()))
}
