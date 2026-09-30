// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.Assets1` on the daemon path of `media` and `library` providers
//! (ADR-0062). The daemon forwards to the provider's `AssetProvider1` and
//! validates the image again. Ids and bytes are never logged.
use super::*;
use arinn_protocol::assets::{self, Asset, AssetProviderProxy, FEATURE_ASSETS};

const ASSET_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) struct AssetsEndpoint {
    pub(super) core: Core,
    pub(super) id: String,
}
#[zbus::interface(name = "org.arinn.Assets1")]
impl AssetsEndpoint {
    async fn get_asset(&self, generation: &str, id: &str) -> production::Result<(Asset,)> {
        Ok((self.core.asset(&self.id, generation, id).await?,))
    }
}

impl Core {
    async fn asset(&self, provider: &str, generation: &str, id: &str) -> Result<Asset, ErrorCode> {
        assets::asset_id(id)?;
        let owner = {
            let state = self.state.lock().await;
            let p = state.providers.get(provider).ok_or(ErrorCode::NotFound)?;
            if !["media", "library"].contains(&p.capability()) {
                return Err(ErrorCode::Unsupported);
            }
            if p.info.generation != generation {
                return Err(ErrorCode::Conflict);
            }
            if p.info.runtime_state != "ready" || p.info.backend_state == "unavailable" {
                return Err(ErrorCode::Unavailable);
            }
            if !p.features.iter().any(|f| f == FEATURE_ASSETS) {
                return Err(ErrorCode::Unsupported);
            }
            p.owner.clone()
        };
        let proxy = AssetProviderProxy::builder(&self.connection)
            .destination(owner.as_str())
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let (asset,) = tokio::time::timeout(ASSET_TIMEOUT, proxy.get_asset(generation, id))
            .await
            .map_err(|_| ErrorCode::Timeout)?
            .map_err(normalize_error)?;
        // The SDK already checks this; a provider not built on it may not.
        asset.validate().map_err(|_| ErrorCode::Internal)?;
        Ok(asset)
    }
}
