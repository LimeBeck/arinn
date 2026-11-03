// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Private signed audio resolution. The anonymous CDN client never carries OAuth.
use crate::{Api, Error, parse};
use arinn_music_common::player::Source;
use md5::{Digest, Md5};
use reqwest::{Method, Url};
const XML_LIMIT: usize = 64 * 1024;
fn cdn_url(value: &str) -> Result<Url, Error> {
    let url = Url::parse(value).map_err(|_| Error::InvalidResponse)?;
    let host = url.host_str().ok_or(Error::InvalidResponse)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|p| p != 443)
        || !(host.ends_with(".yandex.net") || host.ends_with(".yandex.ru"))
        || value.len() > 16 * 1024
    {
        return Err(Error::InvalidResponse);
    }
    Ok(url)
}
fn signed(xml: &str) -> Result<String, Error> {
    if xml.len() > XML_LIMIT {
        return Err(Error::TooLarge);
    }
    let doc = roxmltree::Document::parse(xml).map_err(|_| Error::InvalidResponse)?;
    let field = |name| {
        let mut nodes = doc.descendants().filter(|n| n.has_tag_name(name));
        let value = nodes
            .next()
            .and_then(|n| n.text())
            .ok_or(Error::InvalidResponse)?;
        if nodes.next().is_some() || value.is_empty() || value.chars().any(char::is_control) {
            return Err(Error::InvalidResponse);
        }
        Ok(value)
    };
    let host = field("host")?;
    let path = field("path")?;
    let timestamp = field("ts")?;
    let salt = field("s")?;
    if !path.starts_with('/')
        || path.starts_with("//")
        || path.contains(['?', '#', '\\'])
        || !timestamp.bytes().all(|b| b.is_ascii_digit())
        || host.contains(['/', ':', '@', '?', '#'])
    {
        return Err(Error::InvalidResponse);
    }
    // Public Yandex download-info protocol signature, not a credential or password hash.
    let hash = Md5::digest(format!("XGRlBW9FXlekgbPrRHuSiA{}{}", &path[1..], salt).as_bytes());
    let url = format!("https://{host}/get-mp3/{hash:x}/{timestamp}{path}");
    cdn_url(&url)?;
    Ok(url)
}
impl Api {
    pub async fn artwork(&self, id: &str) -> Result<arinn_protocol::assets::Asset, Error> {
        self.guard(self.artwork_inner(id)).await
    }
    async fn artwork_inner(&self, id: &str) -> Result<arinn_protocol::assets::Asset, Error> {
        if !parse::track_id(id) {
            return Err(Error::InvalidInput);
        }
        let metadata = self
            .request(Method::POST, &["tracks"], &[("track-ids", id.into())])
            .await?;
        let track = metadata
            .as_array()
            .and_then(|v| v.first())
            .ok_or(Error::InvalidResponse)?;
        let reference = track["coverUri"].as_str().ok_or(Error::Unavailable)?;
        let url = cdn_url(&format!(
            "https://{}",
            reference
                .trim_start_matches("https://")
                .replace("%%", "600x600")
        ))?;
        if !["avatars.yandex.net", "avatars.mds.yandex.net"].contains(&url.host_str().unwrap_or(""))
        {
            return Err(Error::InvalidResponse);
        }
        let mut response = self
            .downloads
            .get(url)
            .send()
            .await
            .map_err(|_| Error::Transport)?;
        if !response.status().is_success() {
            return Err(Error::Unavailable);
        }
        let mut data = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if data.len() + chunk.len() > arinn_protocol::assets::MAX_ASSET_BYTES {
                return Err(Error::TooLarge);
            }
            data.extend_from_slice(&chunk);
        }
        let mime = arinn_protocol::assets::sniff(&data)
            .ok_or(Error::InvalidResponse)?
            .into();
        Ok(arinn_protocol::assets::Asset { mime, data })
    }
    pub async fn audio(&self, id: &str) -> Result<Source, Error> {
        self.guard(self.audio_inner(id)).await
    }
    async fn audio_inner(&self, id: &str) -> Result<Source, Error> {
        if !parse::track_id(id) {
            return Err(Error::InvalidInput);
        }
        let result = self
            .request(Method::GET, &["tracks", id, "download-info"], &[])
            .await?;
        let choices = result
            .as_array()
            .filter(|v| v.len() <= 32)
            .ok_or(Error::InvalidResponse)?;
        let choice = choices
            .iter()
            .filter(|v| {
                v["codec"] == "mp3" && v["transport"] == "raw" && v["gain"].as_bool() != Some(true)
            })
            .max_by_key(|v| v["bitrateInKbps"].as_u64().unwrap_or(0))
            .ok_or(Error::Forbidden)?;
        let url = cdn_url(
            choice["downloadInfoUrl"]
                .as_str()
                .ok_or(Error::InvalidResponse)?,
        )?;
        let mut response = self
            .downloads
            .get(url)
            .send()
            .await
            .map_err(|_| Error::Transport)?;
        if !response.status().is_success() {
            return Err(Error::Unavailable);
        }
        if response
            .content_length()
            .is_some_and(|n| n > XML_LIMIT as u64)
        {
            return Err(Error::TooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if bytes.len() + chunk.len() > XML_LIMIT {
                return Err(Error::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        let xml = std::str::from_utf8(&bytes).map_err(|_| Error::InvalidResponse)?;
        Ok(Source::Stream {
            url: signed(xml)?,
            headers: vec![],
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_source_is_bounded_https_and_cannot_redirect_credentials() {
        let xml = "<download-info><host>storage.mds.yandex.net</host><path>/fixture.mp3</path><ts>123</ts><s>fixture</s></download-info>";
        assert_eq!(
            signed(xml).unwrap(),
            "https://storage.mds.yandex.net/get-mp3/1a75d41ff3e78df57c0e4c9e2dd12784/123/fixture.mp3"
        );
        for host in [
            "example.org",
            "yandex.net.example.org",
            "storage.mds.yandex.net@other.yandex.net",
            "127.0.0.1",
        ] {
            assert!(signed(&xml.replace("storage.mds.yandex.net", host)).is_err());
        }
        assert!(cdn_url("http://storage.mds.yandex.net/fixture").is_err());
        assert!(cdn_url("https://oauth:secret@storage.mds.yandex.net/fixture").is_err());
        assert!(signed(&xml.replace("/fixture.mp3", "//fixture")).is_err());
        assert!(signed(&format!("{xml}{xml}")).is_err());
    }
}
