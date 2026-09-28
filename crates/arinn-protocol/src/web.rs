// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-stable web-shortcut contract (ADR-0043). Web shortcuts are Arinn state,
//! exported by the daemon at its root path; the daemon backend is a separate task.
use crate::validation::{self, ErrorCode, MAX_BODY_BYTES};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zbus::zvariant::{LE, OwnedObjectPath, Type, serialized::Context, to_bytes};

pub const INTERFACE: &str = "org.arinn.WebShortcuts1";
pub const MAX_SHORTCUTS: usize = 64;
pub const MAX_URL_BYTES: usize = 2048;
pub const MAX_TITLE_BYTES: usize = 256;

/// A user-visible web shortcut. `icon` is an opaque packaged icon reference or
/// empty; Arinn never fetches icons. `url` is user configuration and MUST NOT
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

/// An absolute `https` URL without user information, whitespace or control
/// characters, with a non-empty host and a valid optional port.
pub fn url(value: &str) -> Result<(), ErrorCode> {
    let invalid = Err(ErrorCode::InvalidArgument);
    if value.len() > MAX_URL_BYTES
        || value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
    {
        return invalid;
    }
    let Some(scheme) = value.get(..8) else {
        return invalid;
    };
    if !scheme.eq_ignore_ascii_case("https://") {
        return invalid;
    }
    let rest = &value[8..];
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
    if host.is_empty() {
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
    fn urls_are_https_without_credentials() {
        for good in [
            "https://example.org",
            "HTTPS://Example.org/",
            "https://example.org:8443/path?q=1#frag",
            "https://[2001:db8::1]:443/",
            "https://пример.рф/",
        ] {
            assert_eq!(url(good), Ok(()), "{good}");
        }
        let long = format!("https://example.org/{}", "x".repeat(MAX_URL_BYTES));
        for bad in [
            "",
            "http://example.org",
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
