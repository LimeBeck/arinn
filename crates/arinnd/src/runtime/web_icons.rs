// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Optional site-icon routing (ADR-0075). Selection deliberately ignores browser
//! backend connectivity and never launches or probes a second provider.
use super::*;
use crate::shortcuts::WebShortcuts;
use arinn_protocol::{
    assets::Asset,
    web_icons::{self, WebIconProviderProxy},
};
use tokio::sync::Semaphore;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Selection {
    id: String,
    owner: String,
    generation: String,
}
impl Core {
    async fn icon_provider(&self) -> Result<Selection, ErrorCode> {
        let state = self.state.lock().await;
        let (id, p) = state
            .providers
            .iter()
            .find(|(_, p)| {
                p.capability() == "browser"
                    && p.info.runtime_state == "ready"
                    && !p.owner.is_empty()
            })
            .ok_or(ErrorCode::Unavailable)?;
        Ok(Selection {
            id: id.clone(),
            owner: p.owner.clone(),
            generation: p.info.generation.clone(),
        })
    }
    pub async fn export_web_icons(&self, shortcuts: WebShortcuts) -> Result<(), ErrorCode> {
        self.connection
            .object_server()
            .at(
                production::PATH,
                WebIcons {
                    core: self.clone(),
                    shortcuts,
                    slots: Semaphore::new(4),
                    support: Mutex::new(None),
                },
            )
            .await
            .map_err(|_| ErrorCode::Internal)?;
        Ok(())
    }
}
struct WebIcons {
    core: Core,
    shortcuts: WebShortcuts,
    slots: Semaphore,
    support: Mutex<Option<(String, bool)>>,
}
fn origin(value: &str) -> Result<String, ErrorCode> {
    let mut url = url::Url::parse(value).map_err(|_| ErrorCode::Unsupported)?;
    if url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ErrorCode::Unsupported);
    }
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    url.set_port(None).map_err(|_| ErrorCode::Unsupported)?;
    let origin = url.to_string();
    web_icons::origin(&origin).map_err(|_| ErrorCode::Unsupported)?;
    Ok(origin)
}
fn introspection(xml: &str) -> Result<roxmltree::Document<'_>, ErrorCode> {
    // Standard D-Bus introspection includes a DOCTYPE. Allow that declaration,
    // but never entity definitions, external resolution or unbounded trees.
    if xml.len() > 64 * 1024 || xml.contains("<!ENTITY") {
        return Err(ErrorCode::Unavailable);
    }
    roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 4096,
            entity_resolver: None,
        },
    )
    .map_err(|_| ErrorCode::Unavailable)
}
impl WebIcons {
    async fn supports(&self, owner: &str) -> Result<(), ErrorCode> {
        if let Some((known, supported)) = self.support.lock().await.as_ref()
            && known == owner
        {
            return if *supported {
                Ok(())
            } else {
                Err(ErrorCode::Unsupported)
            };
        }
        let proxy = zbus::fdo::IntrospectableProxy::builder(&self.core.connection)
            .destination(owner)
            .map_err(|_| ErrorCode::Internal)?
            .path(production::PROVIDER_PATH)
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let xml = proxy
            .introspect()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let document = introspection(&xml)?;
        let supported = document.root_element().children().any(|node| {
            node.has_tag_name("interface")
                && node.attribute("name") == Some(web_icons::PROVIDER_INTERFACE)
        });
        *self.support.lock().await = Some((owner.to_owned(), supported));
        if supported {
            Ok(())
        } else {
            Err(ErrorCode::Unsupported)
        }
    }
    async fn route(&self, revision: u64, id: &str) -> Result<Asset, ErrorCode> {
        let shortcut = self.shortcuts.icon_shortcut(revision, id).await?;
        if !shortcut.icon.is_empty() {
            return Err(ErrorCode::Unsupported);
        }
        let origin = origin(&shortcut.url)?;
        // The root endpoint can be reached before supervised providers finish
        // registration. Wait within Get's existing deadline, without launching
        // anything or retrying a provider that already rejected a request.
        let before = loop {
            if let Ok(provider) = self.core.icon_provider().await {
                break provider;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        self.supports(&before.owner).await?;
        self.shortcuts.icon_shortcut(revision, id).await?;
        if self
            .core
            .icon_provider()
            .await
            .map_err(|_| ErrorCode::Conflict)?
            != before
        {
            return Err(ErrorCode::Conflict);
        }
        let proxy = WebIconProviderProxy::builder(&self.core.connection)
            .destination(before.owner.as_str())
            .map_err(|_| ErrorCode::Internal)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let result = proxy.get_icon(&before.generation, &origin).await;
        self.shortcuts.icon_shortcut(revision, id).await?;
        if self
            .core
            .icon_provider()
            .await
            .map_err(|_| ErrorCode::Conflict)?
            != before
        {
            return Err(ErrorCode::Conflict);
        }
        let (asset,) = result.map_err(|error| match &error {
            zbus::Error::MethodError(name, _, _)
                if matches!(
                    name.as_str(),
                    "org.freedesktop.DBus.Error.UnknownInterface"
                        | "org.freedesktop.DBus.Error.UnknownMethod"
                ) =>
            {
                ErrorCode::Unsupported
            }
            _ => normalize_error(error),
        })?;
        web_icons::image(&asset).map_err(|_| ErrorCode::Internal)?;
        Ok(asset)
    }
}
#[zbus::interface(name = "org.arinn.WebShortcutIcons1")]
impl WebIcons {
    async fn get(&self, epoch: &str, revision: u64, id: &str) -> production::Result<(Asset,)> {
        web_icons::request(epoch, revision, id)?;
        if epoch != self.core.epoch {
            return Err(ErrorCode::Conflict.into());
        }
        let _permit = self
            .slots
            .try_acquire()
            .map_err(|_| ErrorCode::Unavailable)?;
        Ok((
            tokio::time::timeout(Duration::from_secs(6), self.route(revision, id))
                .await
                .map_err(|_| ErrorCode::Unavailable)??,
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_canonical_https_origins_leave_the_daemon() {
        assert_eq!(
            origin("https://Example.COM:443/private/path?secret=1#fragment").unwrap(),
            "https://example.com/"
        );
        for value in [
            "http://localhost/private",
            "https://user:password@example.com/",
            "https://example.com:8443/",
        ] {
            assert_eq!(origin(value).unwrap_err(), ErrorCode::Unsupported);
        }
    }
}

#[cfg(test)]
#[path = "web_icons_tests.rs"]
mod bus_tests;
