// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-stable browser contract (ADR-0048). Tab titles and URLs are private
//! browsing data: they travel to the shell but MUST NOT appear in logs or
//! diagnostics (spec §13).
use crate::validation::{self, ErrorCode, MAX_BODY_BYTES, MAX_PAGE_ENTRIES};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zbus::zvariant::{LE, OwnedObjectPath, Type, serialized::Context, to_bytes};

pub const PROVIDER_INTERFACE: &str = "org.arinn.BrowserProvider1";
pub const INTERFACE: &str = "org.arinn.Browser1";

/// A browser window. `window_token` is the PlatformWindows1 token of the
/// matching compositor window when the provider has validated the link, or
/// empty (ADR-0038, ADR-0040). `focused` marks the browser's focused window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct BrowserWindow {
    pub id: String,
    pub window_token: String,
    pub focused: bool,
}

/// A tab. `id` and `window_id` are opaque provider IDs. `url` may be any
/// scheme the browser shows (including internal pages) or empty; it is not an
/// input for `open`. There is no favicon in version 1 (ADR-0048).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Tab {
    pub id: String,
    pub window_id: String,
    pub title: String,
    pub url: String,
    pub active: bool,
    pub audible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TabPage {
    pub generation: String,
    pub revision: u64,
    pub windows: Vec<BrowserWindow>,
    pub tabs: Vec<Tab>,
    pub next_token: String,
}

/// A tab URL as reported by the browser: bounded, single-line, possibly empty.
pub fn tab_url(value: &str) -> Result<(), ErrorCode> {
    if value.len() > validation::MAX_URL_BYTES
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        Err(ErrorCode::InvalidArgument)
    } else {
        Ok(())
    }
}

impl BrowserWindow {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        if !self.window_token.is_empty() {
            validation::id(&self.window_token)?;
        }
        Ok(())
    }
}

impl Tab {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        validation::id(&self.window_id)?;
        validation::text(&self.title)?;
        tab_url(&self.url)
    }
}

impl TabPage {
    /// Size of the actual D-Bus body, as for `ApplicationPage`.
    pub fn encoded_len(&self) -> Result<usize, ErrorCode> {
        to_bytes(Context::new_dbus(LE, 0), &(self,))
            .map(|bytes| bytes.len())
            .map_err(|_| ErrorCode::InvalidArgument)
    }

    /// A single page. Cross-entry rules that span pages are checked on the
    /// complete snapshot by [`validate_entries`].
    pub fn validate(&self) -> Result<(), ErrorCode> {
        uuid::Uuid::parse_str(&self.generation).map_err(|_| ErrorCode::InvalidArgument)?;
        if !self.next_token.is_empty() {
            validation::id(&self.next_token)?;
        }
        if self.revision == 0 || self.windows.len() + self.tabs.len() > MAX_PAGE_ENTRIES {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut windows = HashSet::new();
        for window in &self.windows {
            window.validate()?;
            if !windows.insert(&window.id) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        let mut tabs = HashSet::new();
        for tab in &self.tabs {
            tab.validate()?;
            if !tabs.insert(&tab.id) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        if self.encoded_len()? > MAX_BODY_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}

/// Validate a complete snapshot before making any part visible: unique window
/// and tab IDs, every tab in a listed window, at most one active tab per window
/// and at most one focused window.
pub fn validate_entries(windows: &[BrowserWindow], tabs: &[Tab]) -> Result<(), ErrorCode> {
    let mut ids = HashSet::new();
    let mut focused = 0;
    for window in windows {
        window.validate()?;
        if !ids.insert(window.id.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
        focused += usize::from(window.focused);
    }
    if focused > 1 {
        return Err(ErrorCode::InvalidArgument);
    }
    let mut tab_ids = HashSet::new();
    let mut active = HashSet::new();
    for tab in tabs {
        tab.validate()?;
        if !ids.contains(tab.window_id.as_str())
            || !tab_ids.insert(tab.id.as_str())
            || (tab.active && !active.insert(tab.window_id.as_str()))
        {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    Ok(())
}

/// `org.arinn.BrowserProvider1`, exported by a `browser` provider at
/// `/org/arinn/Provider1` next to its registration. Only the daemon calls it.
///
/// `OpenUrl` opens an `http`/`https` URL ([`crate::web::open_url`]) in a new
/// tab of the provider's default connection; `ActivateTab` selects the tab and
/// focuses its browser window; `CloseTab` closes it. All return when the
/// browser acknowledged the command, or fail with `Unavailable` (no browser
/// connection), `NotFound`, `Conflict` (stale generation) or `Timeout`.
/// Compositor activation of the window is a separate PlatformWindows1 action.
pub mod provider {
    use super::TabPage;
    #[zbus::proxy(
        interface = "org.arinn.BrowserProvider1",
        default_path = "/org/arinn/Provider1"
    )]
    pub trait BrowserProvider {
        fn get_tabs(&self, token: &str) -> zbus::Result<(TabPage,)>;
        fn backend_state(&self) -> zbus::Result<String>;
        fn open_url(&self, generation: &str, url: &str) -> zbus::Result<()>;
        fn activate_tab(&self, generation: &str, id: &str) -> zbus::Result<()>;
        fn close_tab(&self, generation: &str, id: &str) -> zbus::Result<()>;
        #[zbus(signal, name = "StateChanged")]
        fn provider_state_changed(&self, generation: &str, revision: u64) -> zbus::Result<()>;
    }
}
pub use provider::BrowserProviderProxy;

/// `org.arinn.Browser1`, exported by the daemon for each registered browser
/// provider at the provider's `/org/arinn/Daemon1/providers/p_<uuid_hex>` path,
/// like `org.arinn.Application1`. Commands return `org.arinn.Operation1`
/// objects; `ActivateTab` also activates the tab's compositor window and hides
/// Home when it succeeds.
#[zbus::proxy(
    interface = "org.arinn.Browser1",
    default_service = "org.arinn.Daemon1"
)]
pub trait BrowserApi {
    fn get_tabs(&self, token: &str) -> zbus::Result<(TabPage,)>;
    fn activate_tab(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
    fn close_tab(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: &str, focused: bool) -> BrowserWindow {
        BrowserWindow {
            id: id.into(),
            window_token: String::new(),
            focused,
        }
    }
    fn tab(id: &str, window_id: &str, active: bool) -> Tab {
        Tab {
            id: id.into(),
            window_id: window_id.into(),
            title: "Видео".into(),
            url: "https://video.example/watch?v=1".into(),
            active,
            audible: false,
        }
    }
    fn page() -> TabPage {
        TabPage {
            generation: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            windows: vec![window("w1", true)],
            tabs: vec![tab("t1", "w1", true)],
            next_token: String::new(),
        }
    }

    #[test]
    fn wire_signatures_and_roundtrip() {
        assert_eq!(BrowserWindow::SIGNATURE.to_string(), "(ssb)");
        assert_eq!(Tab::SIGNATURE.to_string(), "(ssssbb)");
        assert_eq!(TabPage::SIGNATURE.to_string(), "(sta(ssb)a(ssssbb)s)");
        let page = page();
        let bytes = to_bytes(Context::new_dbus(LE, 0), &(&page,)).unwrap();
        let (decoded,): (TabPage,) = bytes.deserialize().unwrap().0;
        assert_eq!(decoded, page);
    }

    #[test]
    fn tab_urls_are_bounded_single_line_and_may_be_internal_or_empty() {
        for good in [
            "",
            "chrome://newtab/",
            "about:blank",
            "https://e.org/?q=a%20b",
        ] {
            assert_eq!(tab_url(good), Ok(()), "{good:?}");
        }
        let long = format!("https://e.org/{}", "x".repeat(validation::MAX_URL_BYTES));
        for bad in ["https://e.org/\n", "https://e.org/ x", long.as_str()] {
            assert_eq!(tab_url(bad), Err(ErrorCode::InvalidArgument), "{bad:?}");
        }
    }

    #[test]
    fn pages_are_bounded_unique_and_revisioned() {
        let mut page = page();
        assert_eq!(page.validate(), Ok(()));
        page.revision = 0;
        assert_eq!(page.validate(), Err(ErrorCode::InvalidArgument));
        page.revision = 1;
        page.generation = "x".into();
        assert_eq!(page.validate(), Err(ErrorCode::InvalidArgument));
        let mut page = self::page();
        page.tabs.push(tab("t1", "w1", false));
        assert_eq!(page.validate(), Err(ErrorCode::InvalidArgument));
        let mut page = self::page();
        page.tabs = (0..MAX_PAGE_ENTRIES)
            .map(|i| tab(&i.to_string(), "w1", false))
            .collect();
        assert_eq!(page.validate(), Err(ErrorCode::InvalidArgument));
        let mut page = self::page();
        page.tabs = (0..40)
            .map(|i| Tab {
                title: "x".repeat(validation::MAX_TEXT_BYTES),
                ..tab(&i.to_string(), "w1", false)
            })
            .collect();
        assert!(page.encoded_len().unwrap() > MAX_BODY_BYTES);
        assert_eq!(page.validate(), Err(ErrorCode::InvalidArgument));
    }

    #[test]
    fn snapshots_link_tabs_to_windows_consistently() {
        let windows = vec![window("w1", true), window("w2", false)];
        let tabs = vec![
            tab("a", "w1", true),
            tab("b", "w2", true),
            tab("c", "w2", false),
        ];
        assert_eq!(validate_entries(&windows, &tabs), Ok(()));
        assert_eq!(validate_entries(&[], &[]), Ok(()));
        for (windows, tabs) in [
            // A tab in an unknown window.
            (vec![window("w1", false)], vec![tab("a", "w9", false)]),
            // Two active tabs in one window.
            (
                vec![window("w1", false)],
                vec![tab("a", "w1", true), tab("b", "w1", true)],
            ),
            // Two focused windows.
            (vec![window("w1", true), window("w2", true)], vec![]),
            // Duplicate window or tab IDs.
            (vec![window("w1", false), window("w1", false)], vec![]),
            (
                vec![window("w1", false)],
                vec![tab("a", "w1", false), tab("a", "w1", false)],
            ),
        ] {
            assert_eq!(
                validate_entries(&windows, &tabs),
                Err(ErrorCode::InvalidArgument)
            );
        }
    }
}
