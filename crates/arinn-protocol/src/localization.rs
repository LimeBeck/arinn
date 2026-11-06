// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Additive, client-rendered fixed provider text (ADR-0135). Existing
//! snapshot strings remain the fallback; content is never matched by text.
use crate::validation::{self, ErrorCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const PROVIDER_INTERFACE: &str = "org.arinn.ProviderTexts1";
pub const INTERFACE: &str = "org.arinn.Texts1";
// Leave room for D-Bus strings, identities and framing under the 256 KiB
// message-body limit; these constants bound payloads, not whole messages.
pub const MAX_JSON_BYTES: usize = 240 * 1024;
pub const MAX_QM_BYTES: usize = 240 * 1024;
pub const MAX_TOTAL_QM_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_MESSAGES: usize = 1024;
pub const MAX_ARGUMENTS: usize = 16;
pub const MAX_LOCALES: usize = 64;
pub const MAX_REQUESTS: usize = 128;

fn invalid<T>() -> Result<T, ErrorCode> {
    Err(ErrorCode::InvalidArgument)
}
fn token(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return invalid();
    }
    Ok(())
}
fn text(value: &str) -> Result<(), ErrorCode> {
    validation::text(value)?;
    if value
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return invalid();
    }
    Ok(())
}

/// Canonical language[-Script][-REGION], with a two/three-letter language
/// and an alphabetic or three-digit region. No path or process-locale suffix.
pub fn validate_locale(locale: &str) -> Result<(), ErrorCode> {
    let mut parts = locale.split('-').peekable();
    let language = parts.next().unwrap_or_default();
    if !(2..=3).contains(&language.len()) || !language.bytes().all(|b| b.is_ascii_lowercase()) {
        return invalid();
    }
    if let Some(script) = parts.peek()
        && script.len() == 4
    {
        if !script.as_bytes()[0].is_ascii_uppercase()
            || !script.as_bytes()[1..].iter().all(u8::is_ascii_lowercase)
        {
            return invalid();
        }
        parts.next();
    }
    if let Some(region) = parts.next()
        && !((region.len() == 2 && region.bytes().all(|b| b.is_ascii_uppercase()))
            || (region.len() == 3 && region.bytes().all(|b| b.is_ascii_digit())))
    {
        return invalid();
    }
    if parts.next().is_some() {
        return invalid();
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub format_version: u32,
    pub plugin_id: String,
    pub source_locale: String,
    pub locales: Vec<String>,
    #[serde(deserialize_with = "unique_map")]
    pub messages: BTreeMap<String, Message>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Message {
    pub revision: u32,
    pub source: String,
    #[serde(default, deserialize_with = "unique_map")]
    pub arguments: BTreeMap<String, Argument>,
    #[serde(default)]
    pub numerus: bool,
    #[serde(default)]
    pub comment: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Argument {
    pub position: u8,
    #[serde(rename = "type")]
    pub kind: ArgumentKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArgumentKind {
    Text,
    Integer,
    Number,
}

impl Catalog {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.plugin_id)?;
        if self.format_version != 1
            || self.source_locale != "en"
            || self.messages.len() > MAX_MESSAGES
            || self.locales.len() > MAX_LOCALES
        {
            return invalid();
        }
        let mut locales = BTreeSet::new();
        for locale in &self.locales {
            validate_locale(locale)?;
            if !locales.insert(locale) {
                return invalid();
            }
        }
        for (key, message) in &self.messages {
            token(key)?;
            message.validate()?;
        }
        encoded(self).map(|_| ())
    }
    pub fn from_json(json: &str) -> Result<Self, ErrorCode> {
        let value: Self = decoded(json)?;
        value.validate()?;
        Ok(value)
    }
    pub fn to_json(&self) -> Result<String, ErrorCode> {
        self.validate()?;
        encoded(self)
    }
    /// Stable id for `lrelease -idbased`, scoped to this plugin's translator.
    pub fn translation_id(&self, key: &str) -> Result<String, ErrorCode> {
        let message = self.messages.get(key).ok_or(ErrorCode::NotFound)?;
        Ok(format!("{key}@{}", message.revision))
    }
}

impl Message {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.revision == 0 || self.source.is_empty() || self.arguments.len() > MAX_ARGUMENTS {
            return invalid();
        }
        text(&self.source)?;
        text(&self.comment)?;
        let mut declared = BTreeMap::new();
        for (name, argument) in &self.arguments {
            token(name)?;
            if argument.position == 0 || declared.insert(argument.position, argument.kind).is_some()
            {
                return invalid();
            }
        }
        if declared.keys().copied().ne(1..=self.arguments.len() as u8) {
            return invalid();
        }
        let (positions, localized, numerus) = placeholders(&self.source)?;
        if positions != declared.keys().copied().collect()
            || numerus != self.numerus
            || localized
                .iter()
                .any(|p| declared.get(p) == Some(&ArgumentKind::Text))
        {
            return invalid();
        }
        Ok(())
    }
}

fn placeholders(source: &str) -> Result<(BTreeSet<u8>, BTreeSet<u8>, bool), ErrorCode> {
    let bytes = source.as_bytes();
    let mut positions = BTreeSet::new();
    let mut localized = BTreeSet::new();
    let mut numerus = false;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        let local = bytes.get(i) == Some(&b'L');
        if local {
            i += 1;
        }
        if bytes.get(i) == Some(&b'n') {
            numerus = true;
            i += 1;
        } else if bytes.get(i).is_some_and(u8::is_ascii_digit) {
            let start = i;
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            let value = source[start..i]
                .parse::<u8>()
                .map_err(|_| ErrorCode::InvalidArgument)?;
            if value == 0 || value > 99 || bytes[start] == b'0' {
                return invalid();
            }
            positions.insert(value);
            if local {
                localized.insert(value);
            }
        } else if local {
            return invalid();
        }
    }
    Ok((positions, localized, numerus))
}

/// Standard snapshot owners; content fields not listed in ADR-0135 are
/// never candidates. `Provider` metadata is generation-scoped (revision 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Scope {
    #[serde(rename = "provider")]
    Provider,
    #[serde(rename = "music.entity")]
    MusicEntity,
    #[serde(rename = "music.station")]
    MusicStation,
    #[serde(rename = "download.job")]
    DownloadJob,
    #[serde(rename = "media.session")]
    MediaSession,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRequest {
    pub scope: Scope,
    pub object_id: String,
    pub revision: u64,
    pub field: String,
    pub fallback_text: String,
}
impl TextRequest {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.object_id)?;
        text(&self.fallback_text)?;
        let valid = match self.scope {
            Scope::Provider => {
                self.object_id == "self"
                    && self.revision == 0
                    && ["name", "description"].contains(&self.field.as_str())
            }
            Scope::MusicEntity | Scope::MusicStation | Scope::MediaSession => {
                self.revision > 0 && self.field == "title"
            }
            Scope::DownloadJob => self.revision > 0 && self.field == "error_message",
        };
        if !valid {
            return invalid();
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reference {
    pub message_key: String,
    pub message_revision: u32,
    #[serde(default, deserialize_with = "unique_map")]
    pub arguments: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}
impl Reference {
    pub fn validate(&self, catalog: &Catalog) -> Result<(), ErrorCode> {
        let message = catalog
            .messages
            .get(&self.message_key)
            .ok_or(ErrorCode::InvalidArgument)?;
        if self.message_revision != message.revision
            || self.arguments.len() != message.arguments.len()
            || self.count.is_some() != message.numerus
            || self.count.is_some_and(|n| n > i32::MAX as u32)
        {
            return invalid();
        }
        for (name, argument) in &message.arguments {
            let value = self.arguments.get(name).ok_or(ErrorCode::InvalidArgument)?;
            match argument.kind {
                ArgumentKind::Text => text(value.as_str().ok_or(ErrorCode::InvalidArgument)?)?,
                ArgumentKind::Integer if value.as_i64().is_some() => {}
                ArgumentKind::Number if value.as_f64().is_some_and(f64::is_finite) => {}
                _ => return invalid(),
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedText {
    pub request_index: u32,
    pub fallback_text: String,
    pub message: Reference,
}

pub fn validate_requests(requests: &[TextRequest]) -> Result<(), ErrorCode> {
    if requests.len() > MAX_REQUESTS {
        return invalid();
    }
    let mut seen = BTreeSet::new();
    for request in requests {
        request.validate()?;
        if !seen.insert((
            request.scope,
            &request.object_id,
            request.revision,
            &request.field,
        )) {
            return invalid();
        }
    }
    encoded(requests).map(|_| ())
}
pub fn requests_from_json(json: &str) -> Result<Vec<TextRequest>, ErrorCode> {
    let requests: Vec<TextRequest> = decoded(json)?;
    validate_requests(&requests)?;
    Ok(requests)
}
pub fn requests_to_json(requests: &[TextRequest]) -> Result<String, ErrorCode> {
    validate_requests(requests)?;
    encoded(requests)
}
pub fn validate_resolved(
    catalog: &Catalog,
    requests: &[TextRequest],
    resolved: &[ResolvedText],
) -> Result<(), ErrorCode> {
    catalog.validate()?;
    validate_requests(requests)?;
    if resolved.len() > requests.len() {
        return invalid();
    }
    let mut seen = BTreeSet::new();
    for result in resolved {
        let request = requests
            .get(result.request_index as usize)
            .ok_or(ErrorCode::InvalidArgument)?;
        if !seen.insert(result.request_index) || result.fallback_text != request.fallback_text {
            return invalid();
        }
        result.message.validate(catalog)?;
    }
    encoded(resolved).map(|_| ())
}
pub fn resolved_to_json(
    catalog: &Catalog,
    requests: &[TextRequest],
    resolved: &[ResolvedText],
) -> Result<String, ErrorCode> {
    validate_resolved(catalog, requests, resolved)?;
    encoded(resolved)
}
pub fn resolved_from_json(
    catalog: &Catalog,
    requests: &[TextRequest],
    json: &str,
) -> Result<Vec<ResolvedText>, ErrorCode> {
    let resolved: Vec<ResolvedText> = decoded(json)?;
    validate_resolved(catalog, requests, &resolved)?;
    Ok(resolved)
}
fn encoded<T: Serialize + ?Sized>(value: &T) -> Result<String, ErrorCode> {
    let json = serde_json::to_string(value).map_err(|_| ErrorCode::InvalidArgument)?;
    if json.len() > MAX_JSON_BYTES {
        return invalid();
    }
    Ok(json)
}
fn decoded<T: DeserializeOwned>(json: &str) -> Result<T, ErrorCode> {
    if json.len() > MAX_JSON_BYTES {
        return invalid();
    }
    serde_json::from_str(json).map_err(|_| ErrorCode::InvalidArgument)
}
fn unique_map<'de, D, T>(deserializer: D) -> Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Unique<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Unique<T> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("an object with unique keys")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, T>()? {
                if values.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate key"));
                }
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Unique(std::marker::PhantomData))
}

#[zbus::proxy(
    interface = "org.arinn.ProviderTexts1",
    default_path = "/org/arinn/Provider1"
)]
pub trait ProviderTexts {
    fn get_catalog(&self, generation: &str) -> zbus::Result<String>;
    fn get_translation(&self, generation: &str, locale: &str) -> zbus::Result<Vec<u8>>;
    fn resolve(&self, generation: &str, requests: &str) -> zbus::Result<String>;
}

/// Daemon routing contract; the runtime implementation follows separately.
#[zbus::proxy(
    interface = "org.arinn.Texts1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait Texts {
    fn get_catalog(&self, provider_id: &str) -> zbus::Result<(String, String)>;
    fn get_translation(
        &self,
        provider_id: &str,
        generation: &str,
        locale: &str,
    ) -> zbus::Result<Vec<u8>>;
    fn resolve(&self, provider_id: &str, generation: &str, requests: &str) -> zbus::Result<String>;
}

#[cfg(test)]
mod tests;
