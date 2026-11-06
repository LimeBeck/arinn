// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Optional, generation-bound fixed provider text (ADR-0135).
use super::RegistrationContext;
use arinn_protocol::{
    localization::{self, Catalog, ResolvedText, TextRequest},
    production,
    validation::ErrorCode,
};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};
use tokio::sync::watch;
use zbus::message::Header;

/// Authoritative current publication for a fixed-text handler. The service
/// captures identity, assigned revision and records under the same lock.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSnapshot<T> {
    pub generation: String,
    pub revision: u64,
    pub entries: Vec<T>,
}

pub trait TextsHandler: Send + Sync + 'static {
    /// Immutable for this export. The daemon checks plugin_id against its
    /// registered manifest, never against a client-supplied identity.
    fn catalog(&self) -> Catalog;
    /// Exact locale -> opaque QM bytes. No Qt dependency in the provider.
    /// This must exactly match Catalog.locales and stays immutable.
    fn translations(&self) -> BTreeMap<String, Vec<u8>> {
        BTreeMap::new()
    }
    /// Resolve only explicitly fixed fields in the indicated original
    /// snapshot. Omit stale/unknown requests and every content field; never
    /// search user content for a source-string match.
    fn resolve(
        &self,
        generation: String,
        requests: Vec<TextRequest>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ResolvedText>, ErrorCode>> + Send + '_>>;
}

pub async fn serve_texts(
    connection: &zbus::Connection,
    handler: impl TextsHandler,
    registrations: watch::Receiver<Option<RegistrationContext>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let catalog = handler.catalog();
    let json = catalog
        .to_json()
        .map_err(|_| "invalid localization catalog")?;
    let translations = handler.translations();
    if translations.len() != catalog.locales.len()
        || translations.values().map(Vec::len).sum::<usize>() > localization::MAX_TOTAL_QM_BYTES
        || translations.iter().any(|(locale, bytes)| {
            !catalog.locales.contains(locale)
                || bytes.is_empty()
                || bytes.len() > localization::MAX_QM_BYTES
        })
    {
        return Err("invalid localization translations".into());
    }
    let exported = connection
        .object_server()
        .at(
            production::PROVIDER_PATH,
            TextsIface {
                handler: Arc::new(handler),
                catalog,
                json,
                translations,
                registrations,
            },
        )
        .await?;
    if !exported {
        return Err("provider texts endpoint already exported".into());
    }
    Ok(())
}

struct TextsIface {
    handler: Arc<dyn TextsHandler>,
    catalog: Catalog,
    json: String,
    translations: BTreeMap<String, Vec<u8>>,
    registrations: watch::Receiver<Option<RegistrationContext>>,
}
impl TextsIface {
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
#[zbus::interface(name = "org.arinn.ProviderTexts1")]
impl TextsIface {
    async fn get_catalog(
        &self,
        generation: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<String> {
        self.authorize(generation, &header)?;
        Ok(self.json.clone())
    }
    async fn get_translation(
        &self,
        generation: &str,
        locale: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<Vec<u8>> {
        self.authorize(generation, &header)?;
        localization::validate_locale(locale)?;
        Ok(self
            .translations
            .get(locale)
            .cloned()
            .ok_or(ErrorCode::NotFound)?)
    }
    async fn resolve(
        &self,
        generation: &str,
        requests: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> production::Result<String> {
        self.authorize(generation, &header)?;
        let requests = localization::requests_from_json(requests)?;
        let result = self
            .handler
            .resolve(generation.into(), requests.clone())
            .await;
        // No response from an old registration may escape after an await.
        self.authorize(generation, &header)?;
        Ok(localization::resolved_to_json(
            &self.catalog,
            &requests,
            &result?,
        )?)
    }
}
