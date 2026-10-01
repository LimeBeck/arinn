// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{E, Lease};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use url::Url;

pub(super) fn eligible(url: &Url) -> Result<(), E> {
    if url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err(E::Unsupported);
    }
    Ok(())
}

/// Conservative public-unicast allowlist. Special-purpose ranges are never
/// contacted, including IPv4 embedding/transition mechanisms in IPv6.
pub(super) fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let n = u32::from(ip);
            ![
                (0x00000000, 8),
                (0x0a000000, 8),
                (0x64400000, 10),
                (0x7f000000, 8),
                (0xa9fe0000, 16),
                (0xac100000, 12),
                (0xc0000000, 24),
                (0xc0000200, 24),
                (0xc0586300, 24),
                (0xc0a80000, 16),
                (0xc6120000, 15),
                (0xc6336400, 24),
                (0xcb007100, 24),
                (0xe0000000, 4),
                (0xf0000000, 4),
            ]
            .iter()
            .any(|&(base, bits)| n >> (32 - bits) == base >> (32 - bits))
        }
        IpAddr::V6(ip) => {
            let n = u128::from(ip);
            n >> 125 == 1
                && n >> 105 != 0x200100_u128 >> 1
                && n >> 96 != 0x20010db8
                && n >> 112 != 0x2002
                && n >> 108 != 0x3fff0
        }
    }
}

fn approved(addresses: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, E> {
    if addresses.is_empty() || addresses.len() > 64 || addresses.iter().any(|a| !public(a.ip())) {
        return Err(E::Unsupported);
    }
    Ok(addresses)
}
fn redirect(current: &Url, location: &str, redirects: usize) -> Result<Url, E> {
    if redirects >= 3 {
        return Err(E::Unavailable);
    }
    let url = current.join(location).map_err(|_| E::Unsupported)?;
    eligible(&url)?;
    Ok(url)
}
fn append(bytes: &mut Vec<u8>, chunk: &[u8], limit: usize) -> Result<(), E> {
    if chunk.len() > limit.saturating_sub(bytes.len()) {
        return Err(E::NotFound);
    }
    bytes.extend_from_slice(chunk);
    Ok(())
}
pub(super) struct Fetcher {
    requests: usize,
    pub lease: Lease,
}
impl Fetcher {
    pub fn new(lease: Lease) -> Self {
        Self { requests: 0, lease }
    }
    pub async fn get(&mut self, mut url: Url, limit: usize) -> Result<(Url, Vec<u8>), E> {
        for redirects in 0..=3 {
            eligible(&url)?;
            if self.requests >= 8 {
                return Err(E::Unavailable);
            }
            self.requests += 1;
            let host = url
                .host_str()
                .ok_or(E::Unsupported)?
                .trim_matches(['[', ']'])
                .to_owned();
            let dns_host = host.clone();
            let lease = self.lease.clone();
            let addresses: Vec<SocketAddr> = tokio::task::spawn_blocking(move || {
                let _lease = lease;
                let addresses: Vec<_> = (dns_host.as_str(), 443)
                    .to_socket_addrs()
                    .map_err(|_| E::Unavailable)?
                    .take(65)
                    .collect();
                approved(addresses)
            })
            .await
            .map_err(|_| E::Internal)??;
            // No system proxy and no second DNS lookup. The URL hostname remains
            // intact for certificate validation and TLS SNI.
            let client = reqwest::Client::builder()
                .no_proxy()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .referer(false)
                .timeout(std::time::Duration::from_secs(5))
                .http1_only()
                .resolve_to_addrs(&host, &addresses)
                .build()
                .map_err(|_| E::Unavailable)?;
            let mut response = client
                .get(url.clone())
                .send()
                .await
                .map_err(|_| E::Unavailable)?;
            if response.status().is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or(E::NotFound)?;
                url = redirect(&url, location, redirects)?;
                continue;
            }
            if !response.status().is_success() {
                return Err(E::NotFound);
            }
            if response
                .headers()
                .contains_key(reqwest::header::CONTENT_ENCODING)
                || response
                    .content_length()
                    .is_some_and(|len| len > limit as u64)
            {
                return Err(E::NotFound);
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| E::Unavailable)? {
                append(&mut bytes, &chunk, limit)?;
            }
            return Ok((url, bytes));
        }
        Err(E::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excludes_special_addresses_and_credentials() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "100.64.0.1",
            "169.254.169.254",
            "172.31.2.3",
            "192.168.1.1",
            "192.0.0.9",
            "198.19.0.1",
            "203.0.113.1",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
            "2001:db8::1",
            "2001::1",
            "2002:808:808::1",
            "3fff::1",
            "fc00::1",
            "fe80::1",
        ] {
            assert!(!public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
            assert!(public(ip.parse().unwrap()), "{ip}");
        }
        for u in [
            "http://example.com/",
            "https://a:b@example.com/",
            "https://example.com:8443/",
        ] {
            assert!(eligible(&Url::parse(u).unwrap()).is_err());
        }
    }
    #[test]
    fn mixed_dns_redirects_and_streaming_budgets_fail_closed() {
        let public: SocketAddr = "8.8.8.8:443".parse().unwrap();
        let private: SocketAddr = "127.0.0.1:443".parse().unwrap();
        assert_eq!(approved(vec![public, private]).unwrap_err(), E::Unsupported);
        assert!(approved(vec![]).is_err());
        assert!(approved(vec![public; 65]).is_err());
        assert_eq!(approved(vec![public]).unwrap(), vec![public]);
        let url = Url::parse("https://example.invalid/").unwrap();
        assert!(redirect(&url, "http://example.invalid/", 0).is_err());
        assert!(redirect(&url, "https://a:b@example.invalid/", 0).is_err());
        assert!(redirect(&url, "/next", 2).is_ok());
        assert_eq!(redirect(&url, "/next", 3).unwrap_err(), E::Unavailable);
        for limit in [128 * 1024, 512 * 1024, 1024 * 1024] {
            let mut bytes = Vec::new();
            append(&mut bytes, &vec![0; limit - 1], limit).unwrap();
            append(&mut bytes, &[0], limit).unwrap();
            assert!(append(&mut bytes, &[0], limit).is_err());
            assert_eq!(bytes.len(), limit);
        }
    }
    #[tokio::test]
    async fn exhausted_request_budget_never_resolves_or_connects() {
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(1));
        let lease = std::sync::Arc::new(semaphore.acquire_owned().await.unwrap());
        let mut fetch = Fetcher { requests: 8, lease };
        assert_eq!(
            fetch
                .get(Url::parse("https://example.invalid/").unwrap(), 512 * 1024)
                .await
                .unwrap_err(),
            E::Unavailable
        );
        assert_eq!(fetch.requests, 8);
    }
}
