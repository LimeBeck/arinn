// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Links browser windows to platform window tokens (ADR-0052).
//!
//! Chrome reports a window as focused only while it has the operating
//! system's focus, and the platform reports exactly that window as the
//! active external window. When one browser window is focused and one
//! platform window is active, and both reports stay the same for
//! [`SETTLE`], they are the same window. Titles, URLs and process details
//! are never used. A link ends when either side disappears.
use arinn_protocol::{browser::BrowserWindow, platform::Window};
use std::{collections::BTreeMap, time::Duration};
use tokio::time::Instant;

/// How long a focus pair must stay unchanged before it becomes a link.
/// Snapshots from the browser and the platform arrive separately; this
/// hides the moment in which one side has already moved on.
pub const SETTLE: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct Association {
    /// Browser window ID → platform window token.
    links: BTreeMap<String, String>,
    /// The focus pair seen last and since when.
    pending: Option<(String, String, Instant)>,
}

impl Association {
    /// Update the links from the latest browser and platform windows.
    /// Returns whether a link changed.
    pub fn observe(
        &mut self,
        browser: &[BrowserWindow],
        platform: &[Window],
        now: Instant,
    ) -> bool {
        let before = self.links.clone();
        self.links.retain(|id, token| {
            browser.iter().any(|w| &w.id == id) && platform.iter().any(|w| &w.token == token)
        });
        let mut focused = browser.iter().filter(|w| w.focused);
        let mut active = platform.iter().filter(|w| w.active);
        let pair = match (focused.next(), focused.next(), active.next(), active.next()) {
            (Some(window), None, Some(platform), None) => {
                Some((window.id.clone(), platform.token.clone()))
            }
            _ => None,
        };
        match pair {
            None => self.pending = None,
            Some((id, token)) if self.links.get(&id) == Some(&token) => self.pending = None,
            Some((id, token)) => {
                let since = match &self.pending {
                    Some((i, t, since)) if *i == id && *t == token => *since,
                    _ => now,
                };
                if now.duration_since(since) >= SETTLE {
                    // Tokens and windows are one to one: a token that now
                    // belongs to this window no longer belongs to another.
                    self.links.retain(|_, t| *t != token);
                    self.links.insert(id, token);
                    self.pending = None;
                } else {
                    self.pending = Some((id, token, since));
                }
            }
        }
        self.links != before
    }

    /// Whether a focus pair is waiting for [`SETTLE`] to pass.
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Fill the window tokens of linked browser windows.
    pub fn apply(&self, windows: &mut [BrowserWindow]) {
        for window in windows {
            window.window_token = self.links.get(&window.id).cloned().unwrap_or_default();
        }
    }

    /// Forget everything, for example after the platform generation changed.
    pub fn clear(&mut self) {
        self.links.clear();
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser(windows: &[(&str, bool)]) -> Vec<BrowserWindow> {
        windows
            .iter()
            .map(|&(id, focused)| BrowserWindow {
                id: id.into(),
                window_token: String::new(),
                focused,
            })
            .collect()
    }
    fn platform(windows: &[(&str, bool)]) -> Vec<Window> {
        windows
            .iter()
            .map(|&(token, active)| Window {
                token: token.into(),
                desktop_id: String::new(),
                output: String::new(),
                active,
                fullscreen: true,
                can_activate: true,
                can_close: true,
            })
            .collect()
    }
    fn tokens(a: &Association, windows: &[BrowserWindow]) -> Vec<String> {
        let mut windows = windows.to_vec();
        a.apply(&mut windows);
        windows.into_iter().map(|w| w.window_token).collect()
    }

    #[test]
    fn a_settled_focus_pair_becomes_a_link() {
        let mut a = Association::default();
        let t0 = Instant::now();
        let b = browser(&[("c1:w1", true), ("c1:w2", false)]);
        let p = platform(&[("k1", true), ("k2", false)]);
        assert!(!a.observe(&b, &p, t0));
        assert!(a.pending());
        assert_eq!(tokens(&a, &b), ["", ""]);
        assert!(!a.observe(&b, &p, t0 + SETTLE / 2));
        assert!(a.observe(&b, &p, t0 + SETTLE));
        assert_eq!(tokens(&a, &b), ["k1", ""]);
        assert!(!a.pending());
    }

    #[test]
    fn a_changing_pair_restarts_the_wait() {
        let mut a = Association::default();
        let t0 = Instant::now();
        let p = platform(&[("k1", true), ("k2", false)]);
        a.observe(&browser(&[("c1:w1", true), ("c1:w2", false)]), &p, t0);
        // The browser moved focus before the platform caught up.
        let b = browser(&[("c1:w1", false), ("c1:w2", true)]);
        assert!(!a.observe(&b, &p, t0 + SETTLE));
        assert_eq!(tokens(&a, &b), ["", ""]);
        assert!(a.observe(&b, &p, t0 + SETTLE * 2));
        assert_eq!(tokens(&a, &b), ["", "k1"]);
    }

    #[test]
    fn no_link_without_exactly_one_focused_and_one_active_window() {
        let mut a = Association::default();
        let t0 = Instant::now();
        let later = t0 + SETTLE * 4;
        // Home is shown: no browser window has focus.
        let cases = [
            (browser(&[("c1:w1", false)]), platform(&[("k1", true)])),
            // Another application is active.
            (browser(&[("c1:w1", true)]), platform(&[("k1", false)])),
            // Two browser connections claim focus.
            (
                browser(&[("c1:w1", true), ("c2:w1", true)]),
                platform(&[("k1", true)]),
            ),
        ];
        for (b, p) in &cases {
            a.observe(b, p, t0);
            assert!(!a.observe(b, p, later));
            assert!(tokens(&a, b).iter().all(String::is_empty));
        }
    }

    #[test]
    fn links_end_when_either_window_disappears() {
        let mut a = Association::default();
        let t0 = Instant::now();
        let b = browser(&[("c1:w1", true), ("c1:w2", false)]);
        let p = platform(&[("k1", true), ("k2", false)]);
        a.observe(&b, &p, t0);
        a.observe(&b, &p, t0 + SETTLE);
        // The compositor remapped the window: the old token is gone.
        let remapped = platform(&[("k3", false), ("k2", false)]);
        assert!(a.observe(&b, &remapped, t0 + SETTLE * 2));
        assert_eq!(tokens(&a, &b), ["", ""]);

        a.observe(&b, &p, t0 + SETTLE * 3);
        a.observe(&b, &p, t0 + SETTLE * 4);
        assert_eq!(tokens(&a, &b), ["k1", ""]);
        // The browser window closed.
        let closed = browser(&[("c1:w2", false)]);
        assert!(a.observe(&closed, &platform(&[("k1", false)]), t0 + SETTLE * 5));
        assert_eq!(tokens(&a, &b), ["", ""]);
    }

    #[test]
    fn a_token_moves_to_the_window_that_now_owns_it() {
        let mut a = Association::default();
        let t0 = Instant::now();
        let p = platform(&[("k1", true)]);
        let first = browser(&[("c1:w1", true), ("c1:w2", false)]);
        a.observe(&first, &p, t0);
        a.observe(&first, &p, t0 + SETTLE);
        let second = browser(&[("c1:w1", false), ("c1:w2", true)]);
        a.observe(&second, &p, t0 + SETTLE * 2);
        a.observe(&second, &p, t0 + SETTLE * 3);
        // One token never belongs to two windows.
        assert_eq!(tokens(&a, &second), ["", "k1"]);
    }

    #[test]
    fn clear_forgets_links() {
        let mut a = Association::default();
        let t0 = Instant::now();
        let b = browser(&[("c1:w1", true)]);
        let p = platform(&[("k1", true)]);
        a.observe(&b, &p, t0);
        a.observe(&b, &p, t0 + SETTLE);
        a.clear();
        assert_eq!(tokens(&a, &b), [""]);
        assert!(!a.pending());
    }
}
