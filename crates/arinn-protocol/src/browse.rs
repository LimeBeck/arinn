// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-stable address and search contract (ADR-0051, ADR-0066). The shell
//! sends the text the user typed; the daemon resolves it with
//! [`resolve_input`], keeps up to [`MAX_RECENT`] recent entries and opens the
//! result in the default BrowserProvider. Typed text and URLs are never
//! logged. Search engines are packaging data, never core code (R-01).
use crate::{
    validation::ErrorCode,
    web::{self},
};
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use zbus::zvariant::{OwnedObjectPath, Type};

pub const INTERFACE: &str = "org.arinn.Browse1";
pub const MAX_INPUT_BYTES: usize = 2048;
pub const MAX_RECENT: usize = 5;
pub const KIND_ADDRESS: &str = "address";
pub const KIND_SEARCH: &str = "search";
pub const KIND_INVALID: &str = "invalid";
/// The placeholder a search template carries exactly once.
pub const QUERY: &str = "{query}";

/// What Enter would do with the typed text. `url` is set for an address or a
/// search; `engine` is the search engine's title for a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Target {
    pub kind: String,
    pub url: String,
    pub engine: String,
}
impl Target {
    fn invalid() -> Self {
        Self {
            kind: KIND_INVALID.into(),
            url: String::new(),
            engine: String::new(),
        }
    }
}

/// A search URL template: an `https` URL with [`QUERY`] exactly once.
pub fn search_template(template: &str) -> Result<(), ErrorCode> {
    if template.matches(QUERY).count() != 1
        || !template.to_ascii_lowercase().starts_with("https://")
    {
        return Err(ErrorCode::InvalidArgument);
    }
    web::open_url(&template.replace(QUERY, "q"))
}

fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn tld_letters(host: &str) -> bool {
    host.rsplit_once('.').is_some_and(|(name, tld)| {
        !name.is_empty() && tld.len() >= 2 && tld.bytes().all(|b| b.is_ascii_alphabetic())
    })
}

/// Resolve typed text by the ADR-0051 rules:
/// 1. `http://` and `https://` are used as typed;
/// 2. an IPv4 address, `localhost`, a `*.local` host or any host with a port
///    gets `http://`;
/// 3. a dotted host whose last label has two or more letters, with no
///    spaces, gets `https://`;
/// 4. any other scheme is invalid;
/// 5. everything else is a search with `template` (`engine` is its title).
pub fn resolve_input(text: &str, template: &str, engine: &str) -> Target {
    let text = text.trim();
    if text.is_empty() || text.len() > MAX_INPUT_BYTES || text.chars().any(char::is_control) {
        return Target::invalid();
    }
    let address = |url: String| {
        if web::open_url(&url).is_ok() {
            Target {
                kind: KIND_ADDRESS.into(),
                url,
                engine: String::new(),
            }
        } else {
            Target::invalid()
        }
    };
    if let Some((scheme, _)) = text.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        return if scheme == "http" || scheme == "https" {
            address(text.to_owned())
        } else {
            Target::invalid()
        };
    }
    if !text.chars().any(char::is_whitespace) {
        let authority = &text[..text.find(['/', '?', '#']).unwrap_or(text.len())];
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
                (host, true)
            }
            _ => (authority, false),
        };
        let host_lower = host.to_ascii_lowercase();
        let local = host_lower == "localhost"
            || host_lower.ends_with(".local")
            || host.parse::<Ipv4Addr>().is_ok();
        if !host.is_empty() && (port || local) {
            return address(format!("http://{text}"));
        }
        if tld_letters(host) {
            return address(format!("https://{text}"));
        }
    }
    if search_template(template).is_err() {
        return Target::invalid();
    }
    Target {
        kind: KIND_SEARCH.into(),
        url: template.replace(QUERY, &encode(text)),
        engine: engine.into(),
    }
}

/// `org.arinn.Browse1` on `/org/arinn/Daemon1`.
#[zbus::proxy(
    interface = "org.arinn.Browse1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait BrowseApi {
    /// What Enter would do; never opens anything or records the text.
    fn resolve(&self, text: &str) -> zbus::Result<(Target,)>;
    /// Resolve, record as a recent entry and open in the default
    /// BrowserProvider (ADR-0050: an open origin is focused). An
    /// `Operation1`; an invalid text is `InvalidArgument` at once.
    fn open(&self, text: &str, request_id: &str) -> zbus::Result<OwnedObjectPath>;
    /// Up to five recent entries, newest first.
    fn get_recent(&self) -> zbus::Result<Vec<String>>;
    fn clear_recent(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn recent_changed(&self) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "https://search.example/?q={query}";
    fn kind(text: &str) -> (String, String) {
        let t = resolve_input(text, T, "Example");
        (t.kind, t.url)
    }

    #[test]
    fn addresses_follow_the_adr_0051_rules() {
        let cases = [
            ("https://site.example/a", "https://site.example/a"),
            ("HTTP://nas.local:8096", "HTTP://nas.local:8096"),
            ("192.168.1.10:8096", "http://192.168.1.10:8096"),
            ("192.168.1.10", "http://192.168.1.10"),
            ("localhost:8080/x", "http://localhost:8080/x"),
            ("nas.local", "http://nas.local"),
            ("media:8096", "http://media:8096"),
            ("kinopoisk.ru", "https://kinopoisk.ru"),
            ("hd.kinopoisk.ru/film/1", "https://hd.kinopoisk.ru/film/1"),
        ];
        for (text, url) in cases {
            assert_eq!(kind(text), (KIND_ADDRESS.into(), url.into()), "{text}");
        }
    }

    #[test]
    fn other_schemes_and_bad_input_are_invalid() {
        for text in [
            "",
            "   ",
            "ftp://x.example",
            "file:///etc/passwd",
            "javascript://x",
            "a\u{7}b",
        ] {
            assert_eq!(kind(text).0, KIND_INVALID, "{text:?}");
        }
        assert_eq!(kind(&"a".repeat(MAX_INPUT_BYTES + 1)).0, KIND_INVALID);
    }

    #[test]
    fn everything_else_is_a_search() {
        let t = resolve_input("погода москва", T, "Example");
        assert_eq!(t.kind, KIND_SEARCH);
        assert_eq!(t.engine, "Example");
        assert_eq!(
            t.url,
            "https://search.example/?q=%D0%BF%D0%BE%D0%B3%D0%BE%D0%B4%D0%B0%20%D0%BC%D0%BE%D1%81%D0%BA%D0%B2%D0%B0"
        );
        for text in ["film 2024", "version1.2", "a.b", "hello"] {
            assert_eq!(kind(text).0, KIND_SEARCH, "{text}");
        }
        // Query characters that would break the URL are encoded.
        assert_eq!(kind("a&b=c#d").1, "https://search.example/?q=a%26b%3Dc%23d");
    }

    #[test]
    fn templates_must_be_https_with_one_query() {
        assert!(search_template(T).is_ok());
        for bad in [
            "http://search.example/?q={query}",
            "https://search.example/",
            "https://search.example/?q={query}&r={query}",
            "https://user@search.example/?q={query}",
        ] {
            assert!(search_template(bad).is_err(), "{bad}");
            assert_eq!(resolve_input("film", bad, "X").kind, KIND_INVALID, "{bad}");
        }
    }

    #[test]
    fn wire_signature() {
        assert_eq!(Target::SIGNATURE.to_string(), "(sss)");
    }
}
