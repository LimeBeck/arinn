// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-stable web-shortcut contract (ADR-0043, ADR-0045). Web shortcuts are Arinn state,
//! exported by the daemon at its root path; the daemon backend is a separate task.
use crate::validation::{self, ErrorCode, MAX_BODY_BYTES};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    net::{Ipv4Addr, Ipv6Addr},
};
use zbus::zvariant::{LE, OwnedObjectPath, Type, serialized::Context, to_bytes};

pub const INTERFACE: &str = "org.arinn.WebShortcuts1";
pub const MAX_SHORTCUTS: usize = 64;
pub const MAX_URL_BYTES: usize = 2048;
pub const MAX_TITLE_BYTES: usize = 256;

/// A user-visible web shortcut. `icon` is an opaque packaged icon reference or
/// empty; optional site discovery is separate (ADR-0075). `url` is user configuration and MUST NOT
/// appear in logs or diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct WebShortcut {
    pub id: String,
    pub title: String,
    pub url: String,
    pub icon: String,
}

/// The complete, ordered list. It is small enough for one message, so it has
/// no paging; `revision` guards updates against concurrent changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct WebShortcutList {
    pub revision: u64,
    pub shortcuts: Vec<WebShortcut>,
}

/// An absolute URL without user information, whitespace or control
/// characters, with a non-empty host and a valid optional port. `https` is
/// accepted for any host; `http` only for a local-network host (ADR-0045).
pub fn url(value: &str) -> Result<(), ErrorCode> {
    parse(value, MAX_URL_BYTES, false)
}

/// A URL for generic browser `open` (spec §7.1): the same syntax rules as
/// [`url`], but plain `http` is accepted for any host. Web shortcuts use the
/// stricter [`url`]; this is for the browser contract (ADR-0048).
pub fn open_url(value: &str) -> Result<(), ErrorCode> {
    parse(value, validation::MAX_URL_BYTES, true)
}

fn parse(value: &str, max_bytes: usize, public_http: bool) -> Result<(), ErrorCode> {
    let invalid = Err(ErrorCode::InvalidArgument);
    if value.len() > max_bytes
        || value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
    {
        return invalid;
    }
    let scheme = |prefix: &str| {
        value
            .get(..prefix.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
    };
    let (secure, rest) = if scheme("https://") {
        (true, &value[8..])
    } else if scheme("http://") {
        (false, &value[7..])
    } else {
        return invalid;
    };
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    if authority.contains('@') {
        return invalid;
    }
    let (host, port) = match authority.strip_prefix('[') {
        Some(literal) => match literal.split_once(']') {
            Some((address, tail)) if !address.is_empty() => {
                if tail.is_empty() {
                    (address, None)
                } else if let Some(port) = tail.strip_prefix(':') {
                    (address, Some(port))
                } else {
                    return invalid;
                }
            }
            _ => return invalid,
        },
        None => match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        },
    };
    if host.is_empty() || (!secure && !public_http && !local_host(host)) {
        return invalid;
    }
    if let Some(port) = port
        && (port.is_empty()
            || !port.bytes().all(|b| b.is_ascii_digit())
            || !matches!(port.parse::<u32>(), Ok(1..=65535)))
    {
        return invalid;
    }
    Ok(())
}

/// A host that cannot be a public Internet site, judged from the URL alone
/// (no DNS): private, loopback, link-local and unique-local IP literals,
/// single-label names and the local-use zones `.local`, `.lan`, `.home.arpa`
/// and `.internal`. IPv6 literals arrive without brackets.
pub fn local_host(host: &str) -> bool {
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return ip.is_private() || ip.is_loopback() || ip.is_link_local();
    }
    if let Ok(ip) = host.parse::<Ipv6Addr>() {
        let first = ip.segments()[0];
        return ip.is_loopback() || first & 0xfe00 == 0xfc00 || first & 0xffc0 == 0xfe80;
    }
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    if host.is_empty() || host.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        // Numeric forms that are not dotted-quad IPv4 are never local names.
        return false;
    }
    !host.contains('.')
        || [".local", ".lan", ".home.arpa", ".internal"]
            .iter()
            .any(|zone| host.ends_with(zone))
}

/// A non-empty single-line title.
pub fn title(value: &str) -> Result<(), ErrorCode> {
    if value.trim().is_empty()
        || value.len() > MAX_TITLE_BYTES
        || value.chars().any(char::is_control)
    {
        Err(ErrorCode::InvalidArgument)
    } else {
        Ok(())
    }
}

impl WebShortcut {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        title(&self.title)?;
        url(&self.url)?;
        if !self.icon.is_empty() {
            validation::id(&self.icon)?;
        }
        Ok(())
    }
}

impl WebShortcutList {
    pub fn encoded_len(&self) -> Result<usize, ErrorCode> {
        to_bytes(Context::new_dbus(LE, 0), &(self,))
            .map(|bytes| bytes.len())
            .map_err(|_| ErrorCode::InvalidArgument)
    }

    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.revision == 0 || self.shortcuts.len() > MAX_SHORTCUTS {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut ids = HashSet::new();
        for shortcut in &self.shortcuts {
            shortcut.validate()?;
            if !ids.insert(&shortcut.id) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        if self.encoded_len()? > MAX_BODY_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}

/// `org.arinn.WebShortcuts1` on the daemon service and root path.
///
/// `Add` is deduplicated by caller and `request_id` and returns the new ID.
/// `Update` and `Remove` take the list revision the caller last read and fail
/// with `Conflict` when it is stale. `Open` returns an `org.arinn.Operation1`
/// object for opening the URL in the default browser provider; with no
/// available browser provider the operation fails with `Unavailable`.
#[zbus::proxy(
    interface = "org.arinn.WebShortcuts1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait WebShortcuts {
    fn list(&self) -> zbus::Result<(WebShortcutList,)>;
    fn add(&self, title: &str, url: &str, icon: &str, request_id: &str) -> zbus::Result<String>;
    fn update(
        &self,
        id: &str,
        revision: u64,
        title: &str,
        url: &str,
        icon: &str,
    ) -> zbus::Result<()>;
    fn remove(&self, id: &str, revision: u64) -> zbus::Result<()>;
    fn open(&self, id: &str, request_id: &str) -> zbus::Result<OwnedObjectPath>;
    #[zbus(signal)]
    fn changed(&self, revision: u64) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shortcut(id: &str) -> WebShortcut {
        WebShortcut {
            id: id.into(),
            title: "Video".into(),
            url: "https://video.example/watch?x=1#t".into(),
            icon: String::new(),
        }
    }

    #[test]
    fn urls_are_https_or_local_http_without_credentials() {
        for good in [
            "https://example.org",
            "HTTPS://Example.org/",
            "https://example.org:8443/path?q=1#frag",
            "https://[2001:db8::1]:443/",
            "https://пример.рф/",
            // Plain http only inside the local network.
            "http://192.168.1.10:8096/web/",
            "HTTP://10.0.0.2",
            "http://172.31.255.1/",
            "http://127.0.0.1:8080/",
            "http://169.254.1.1/",
            "http://[fd00::1]:8096/",
            "http://[fe80::1]/",
            "http://[::1]/",
            "http://nas/",
            "http://jellyfin:8096",
            "http://tv.local/",
            "http://media.lan/",
            "http://router.home.arpa/",
            "http://kodi.internal/",
            "http://NAS.LOCAL./",
        ] {
            assert_eq!(url(good), Ok(()), "{good}");
        }
        let long = format!("https://example.org/{}", "x".repeat(MAX_URL_BYTES));
        for bad in [
            "",
            "http://example.org",
            "http://8.8.8.8/",
            "http://172.32.0.1/",
            "http://[2001:db8::1]/",
            "http://local.example.org/",
            "http://example.local.org/",
            "http://10.1/",
            "http://3232235777/",
            "http://user@nas/",
            "http:///path",
            "ftp://nas/",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://",
            "https:///path",
            "https://user:secret@example.org/",
            "https://user@example.org/",
            "https://example.org:0/",
            "https://example.org:99999/",
            "https://example.org:/",
            "https://example.org:8x/",
            "https://[::1/",
            "https://[]/",
            "https://[::1]x/",
            "https://exa mple.org/",
            "https://example.org/\npath",
            "https://example.org\\@evil.example/",
            long.as_str(),
        ] {
            assert_eq!(url(bad), Err(ErrorCode::InvalidArgument), "{bad:?}");
        }
    }

    #[test]
    fn open_urls_allow_public_http_but_keep_the_syntax_rules() {
        for good in ["http://example.org/", "https://example.org", "http://nas/"] {
            assert_eq!(open_url(good), Ok(()), "{good}");
        }
        for bad in [
            "",
            "ftp://example.org/",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "chrome://settings",
            "http://user@example.org/",
            "http:///path",
            "https://exa mple.org/",
        ] {
            assert_eq!(open_url(bad), Err(ErrorCode::InvalidArgument), "{bad:?}");
        }
        let long = format!(
            "https://example.org/{}",
            "x".repeat(validation::MAX_URL_BYTES)
        );
        assert_eq!(open_url(&long), Err(ErrorCode::InvalidArgument));
    }

    #[test]
    fn wire_signatures_and_roundtrip() {
        assert_eq!(WebShortcut::SIGNATURE.to_string(), "(ssss)");
        assert_eq!(WebShortcutList::SIGNATURE.to_string(), "(ta(ssss))");
        let list = WebShortcutList {
            revision: 3,
            shortcuts: vec![shortcut("a"), shortcut("b")],
        };
        let bytes = to_bytes(Context::new_dbus(LE, 0), &(&list,)).unwrap();
        let (decoded,): (WebShortcutList,) = bytes.deserialize().unwrap().0;
        assert_eq!(decoded, list);
    }

    #[test]
    fn titles_are_single_line_and_bounded() {
        assert_eq!(title("Кинопоиск"), Ok(()));
        for bad in ["", "   ", "a\nb", &"x".repeat(MAX_TITLE_BYTES + 1)] {
            assert_eq!(title(bad), Err(ErrorCode::InvalidArgument), "{bad:?}");
        }
    }

    #[test]
    fn lists_are_bounded_unique_and_revisioned() {
        let mut list = WebShortcutList {
            revision: 1,
            shortcuts: (0..MAX_SHORTCUTS)
                .map(|i| shortcut(&i.to_string()))
                .collect(),
        };
        assert_eq!(list.validate(), Ok(()));
        // The largest valid list still fits in one message.
        for s in &mut list.shortcuts {
            s.title = "x".repeat(MAX_TITLE_BYTES);
            s.url = format!("https://e.org/{}", "x".repeat(MAX_URL_BYTES - 14));
            s.icon = "i".repeat(validation::MAX_ID_BYTES);
            s.id = format!("{:0>256}", s.id);
        }
        assert_eq!(list.validate(), Ok(()));
        list.shortcuts.push(shortcut("extra"));
        assert_eq!(list.validate(), Err(ErrorCode::InvalidArgument));
        list.shortcuts = vec![shortcut("same"), shortcut("same")];
        assert_eq!(list.validate(), Err(ErrorCode::InvalidArgument));
        list.shortcuts = vec![shortcut("one")];
        list.revision = 0;
        assert_eq!(list.validate(), Err(ErrorCode::InvalidArgument));
        list.revision = 2;
        list.shortcuts[0].url = "http://insecure.example/".into();
        assert_eq!(list.validate(), Err(ErrorCode::InvalidArgument));
    }
}
