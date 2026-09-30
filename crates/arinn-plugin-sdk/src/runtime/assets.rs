// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Optional `org.arinn.AssetProvider1` for media and library providers
//! (ADR-0062). The handler returns image bytes for an asset id; the SDK checks
//! the id, the caller and the image before anything reaches the daemon.
use arinn_protocol::{
    assets::{self, Asset},
    production,
    validation::ErrorCode,
};
use std::{future::Future, pin::Pin, sync::Arc};

pub trait AssetHandler: Send + Sync + 'static {
    /// The image for `id`: `NotFound` for an unknown id. Never a path or URL.
    fn asset(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Asset, ErrorCode>> + Send + '_>>;
}

/// Validate the id, ask the handler and validate its image. An image the
/// contract does not allow is the provider's own fault: `Internal`.
pub(super) async fn serve(
    handler: &Arc<dyn AssetHandler>,
    id: &str,
) -> production::Result<(Asset,)> {
    assets::asset_id(id)?;
    let asset = handler.asset(id.into()).await?;
    asset.validate().map_err(|_| ErrorCode::Internal)?;
    Ok((asset,))
}
