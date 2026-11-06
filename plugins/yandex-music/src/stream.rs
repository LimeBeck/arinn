// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Private signed audio resolution. The anonymous CDN client never carries OAuth.
use crate::{Api, Error, parse};
use arinn_music_common::player::Source;
use md5::{Digest, Md5};
use reqwest::{Method, Url};
const XML_LIMIT: usize = 64 * 1024;
fn download_info_url(result: &serde_json::Value) -> Result<Url, Error> {
    let choices = result
        .as_array()
        .filter(|v| v.len() <= 32)
        .ok_or(Error::InvalidResponse)?;
    let choice = choices
        .iter()
        .filter(|v| {
            // Download-info responses may omit transport for direct MP3 files.
            // Explicit null, malformed values and other transports stay unsupported.
            v["codec"] == "mp3"
                && v.get("transport")
                    .is_none_or(|transport| transport == "raw")
                && v["gain"].as_bool() != Some(true)
        })
        .max_by_key(|v| v["bitrateInKbps"].as_u64().unwrap_or(0))
        .ok_or(Error::Forbidden)?;
    cdn_url(
        choice["downloadInfoUrl"]
            .as_str()
            .ok_or(Error::InvalidResponse)?,
    )
}
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
/// A cover reference ("avatars.yandex.net/get-music-content/…/%%") as the
/// 600 × 600 image on the avatars CDN; any other host is refused (#825).
pub(crate) fn cover_url(reference: &str) -> Result<Url, Error> {
    let url = cdn_url(&format!(
        "https://{}",
        reference
            .trim_start_matches("https://")
            .replace("%%", "600x600")
    ))?;
    if !["avatars.yandex.net", "avatars.mds.yandex.net"].contains(&url.host_str().unwrap_or("")) {
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
        || timestamp.len() > 64
        || !timestamp.bytes().all(|b| b.is_ascii_hexdigit())
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
async fn buffer_audio(mut response: reqwest::Response) -> Result<Source, Error> {
    use std::io::{Seek, SeekFrom, Write};
    const LIMIT: usize = 32 * 1024 * 1024;
    if response.content_length().is_some_and(|n| n > LIMIT as u64) {
        return Err(Error::TooLarge);
    }
    let mut file = arinn_music_common::player::audio_buffer().map_err(|_| Error::Unavailable)?;
    let mut size = 0;
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
        size += chunk.len();
        if size > LIMIT {
            return Err(Error::TooLarge);
        }
        file.write_all(&chunk).map_err(|_| Error::Unavailable)?;
    }
    if size == 0 {
        return Err(Error::Unavailable);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| Error::Unavailable)?;
    Ok(Source::File(file))
}
impl Api {
    pub(crate) async fn artwork(
        &self,
        cover: &parse::Cover,
    ) -> Result<arinn_protocol::assets::Asset, Error> {
        self.guard(self.artwork_inner(cover)).await
    }
    /// The image the cover's API object names (#825): a track's or an
    /// album's `coverUri`, an artist's or a playlist's `cover.uri`, or else
    /// its `ogImage`. A playlist's comes from the account's playlist list,
    /// which carries no tracks.
    pub(crate) async fn cover_reference(&self, cover: &parse::Cover) -> Result<String, Error> {
        let reference = |value: &serde_json::Value| {
            parse::cover_reference(value)
                .map(str::to_owned)
                .ok_or(Error::Unavailable)
        };
        match cover {
            parse::Cover::Track(id) => {
                if !parse::track_id(id) {
                    return Err(Error::InvalidInput);
                }
                let metadata = self
                    .request(Method::POST, &["tracks"], &[("track-ids", id.clone())])
                    .await?;
                reference(
                    metadata
                        .as_array()
                        .and_then(|v| v.first())
                        .ok_or(Error::InvalidResponse)?,
                )
            }
            parse::Cover::Album(id) => {
                if !parse::digits(id) {
                    return Err(Error::InvalidInput);
                }
                reference(&self.request(Method::GET, &["albums", id], &[]).await?)
            }
            parse::Cover::Artist(id) => {
                if !parse::digits(id) {
                    return Err(Error::InvalidInput);
                }
                let brief = self
                    .request(Method::GET, &["artists", id, "brief-info"], &[])
                    .await?;
                reference(&brief["artist"])
            }
            parse::Cover::Playlist { uid, kind } => {
                if !parse::digits(uid) || !parse::digits(kind) {
                    return Err(Error::InvalidInput);
                }
                let list = self
                    .request(Method::GET, &["users", uid, "playlists", "list"], &[])
                    .await?;
                let playlists = list
                    .as_array()
                    .filter(|v| v.len() <= 200)
                    .ok_or(Error::InvalidResponse)?;
                let playlist = playlists
                    .iter()
                    .find(|p| {
                        p["kind"].as_u64().map(|n| n.to_string()).as_deref() == Some(kind.as_str())
                            || p["kind"].as_str() == Some(kind.as_str())
                    })
                    .ok_or(Error::NotFound)?;
                reference(playlist)
            }
        }
    }
    async fn artwork_inner(
        &self,
        cover: &parse::Cover,
    ) -> Result<arinn_protocol::assets::Asset, Error> {
        let url = cover_url(&self.cover_reference(cover).await?)?;
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
    /// Prepare a single next track in RAM, without forwarding OAuth to its CDN.
    pub async fn prebuffer(&self, id: &str) -> Result<Source, Error> {
        self.guard(async {
            let Source::Stream { url, headers } = self.audio(id).await? else {
                return Err(Error::InvalidResponse);
            };
            if !headers.is_empty() {
                return Err(Error::InvalidResponse);
            }
            let response = self
                .downloads
                .get(cdn_url(&url)?)
                .send()
                .await
                .map_err(|_| Error::Transport)?;
            if !response.status().is_success() {
                return Err(Error::Unavailable);
            }
            buffer_audio(response).await
        })
        .await
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
        let url = download_info_url(&result)?;
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
    async fn buffer_response(size: usize, body: &'static [u8]) -> Result<Source, Error> {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/fixture", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut b = [0];
            while !request.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut b).unwrap();
                request.push(b[0]);
            }
            assert!(
                !String::from_utf8_lossy(&request)
                    .to_lowercase()
                    .contains("authorization:")
            );
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            let _ = socket.write_all(body);
        });
        let response = reqwest::Client::new().get(url).send().await.unwrap();
        let result = buffer_audio(response).await;
        worker.join().unwrap();
        result
    }
    #[tokio::test]
    async fn preload_is_anonymous_ram_file_and_rejects_empty_oversize_or_truncated_audio() {
        use std::io::Read;
        let Source::File(mut file) = buffer_response(7, b"ID3test").await.unwrap() else {
            panic!("expected RAM buffer");
        };
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"ID3test");
        use std::os::fd::AsRawFd;
        assert!(
            std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))
                .unwrap()
                .to_string_lossy()
                .contains("memfd:arinn-audio")
        );
        assert!(matches!(
            buffer_response(33 * 1024 * 1024, b"").await,
            Err(Error::TooLarge)
        ));
        assert!(buffer_response(0, b"").await.is_err());
        assert!(buffer_response(100, b"short").await.is_err());
    }
    #[test]
    fn hexadecimal_timestamp_is_preserved_and_delimiters_are_rejected() {
        let xml = "<download-info><host>storage.mds.yandex.net</host><path>/fixture.mp3</path><ts>6aBc123F</ts><s>fixture</s></download-info>";
        assert!(signed(xml).unwrap().contains("/6aBc123F/fixture.mp3"));
        for invalid in [
            "0x123",
            "xyz",
            "../123",
            "123?x",
            "123#x",
            "123%2f",
            "123 456",
            "123\n456",
            &"a".repeat(65),
        ] {
            assert!(signed(&xml.replace("6aBc123F", invalid)).is_err());
        }
    }
    #[test]
    fn mp3_download_info_without_transport_resolves() {
        let result = serde_json::json!([{
            "codec": "mp3", "gain": false, "bitrateInKbps": 192,
            "downloadInfoUrl": "https://storage.mds.yandex.net/download-info/fixture"
        }]);
        assert_eq!(
            download_info_url(&result).unwrap().as_str(),
            "https://storage.mds.yandex.net/download-info/fixture"
        );
    }
    #[test]
    fn download_info_keeps_transport_codec_gain_and_bitrate_selection() {
        let choice = serde_json::json!({
            "codec": "mp3", "transport": "raw", "gain": false, "bitrateInKbps": 192,
            "downloadInfoUrl": "https://storage.mds.yandex.net/download-info/fixture"
        });
        assert!(download_info_url(&serde_json::json!([choice.clone()])).is_ok());
        for transport in serde_json::json!(["hls", "dash", "", null, false, 1, {}, []])
            .as_array()
            .unwrap()
        {
            let mut unsupported = choice.clone();
            unsupported["transport"] = transport.clone();
            assert!(matches!(
                download_info_url(&serde_json::json!([unsupported])),
                Err(Error::Forbidden)
            ));
        }
        for (field, value) in [
            ("codec", serde_json::json!("aac")),
            ("gain", serde_json::json!(true)),
        ] {
            let mut unsupported = choice.clone();
            unsupported[field] = value;
            assert!(matches!(
                download_info_url(&serde_json::json!([unsupported])),
                Err(Error::Forbidden)
            ));
        }
        let mut higher = choice.clone();
        higher.as_object_mut().unwrap().remove("transport");
        higher["bitrateInKbps"] = serde_json::json!(320);
        higher["downloadInfoUrl"] =
            serde_json::json!("https://storage.mds.yandex.net/download-info/higher");
        let mut unsupported = choice.clone();
        unsupported["transport"] = serde_json::json!("hls");
        unsupported["bitrateInKbps"] = serde_json::json!(512);
        assert_eq!(
            download_info_url(&serde_json::json!([choice, higher, unsupported]))
                .unwrap()
                .path(),
            "/download-info/higher"
        );
    }
    #[test]
    fn download_info_keeps_response_and_url_bounds() {
        assert!(matches!(
            download_info_url(&serde_json::json!({})),
            Err(Error::InvalidResponse)
        ));
        assert!(matches!(
            download_info_url(&serde_json::json!([])),
            Err(Error::Forbidden)
        ));
        for value in [
            serde_json::Value::Null,
            serde_json::json!("http://storage.mds.yandex.net/fixture"),
            serde_json::json!("https://example.org/fixture"),
            serde_json::json!("https://user:password@storage.mds.yandex.net/fixture"),
            serde_json::json!("https://storage.mds.yandex.net:444/fixture"),
            serde_json::json!(format!(
                "https://storage.mds.yandex.net/{}",
                "x".repeat(16 * 1024)
            )),
        ] {
            let choice = serde_json::json!({"codec":"mp3", "gain":false, "downloadInfoUrl":value});
            assert!(matches!(
                download_info_url(&serde_json::json!([choice])),
                Err(Error::InvalidResponse)
            ));
        }
        let choice = serde_json::json!({"codec":"mp3", "gain":false,
            "downloadInfoUrl":"https://storage.mds.yandex.net/fixture"});
        assert!(download_info_url(&serde_json::json!(vec![choice.clone(); 32])).is_ok());
        assert!(matches!(
            download_info_url(&serde_json::json!(vec![choice; 33])),
            Err(Error::InvalidResponse)
        ));
    }
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
