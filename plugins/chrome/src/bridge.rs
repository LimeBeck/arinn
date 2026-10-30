// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider side of the host link: `org.arinn.ChromeBridge1`. Each running
//! host attaches as one browser connection and publishes complete snapshots;
//! the provider merges them into one browser snapshot with opaque IDs
//! `c<connection>:w<window>` and `c<connection>:t<tab>`.
use crate::{WireTab, WireWindow};
use arinn_protocol::{
    browser::{self, BrowserWindow, Tab},
    validation::{ErrorCode, MAX_TEXT_BYTES, MAX_URL_BYTES},
};
use futures_util::StreamExt;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::watch;
use zbus::{fdo, message::Header};

pub const MAX_CONNECTIONS: usize = 16;
/// Titles are display text; longer ones are cut, not rejected.
pub const MAX_TITLE_BYTES: usize = 1024;
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(6);

struct Connection {
    owner: String,
    /// The browser process that started this host (ADR-0096), or 0.
    browser_process: u32,
    windows: Vec<WireWindow>,
    tabs: Vec<WireTab>,
    /// Order of the last time this connection reported a focused window.
    focused_at: u64,
    attached_at: u64,
}
#[derive(Default)]
struct State {
    connections: BTreeMap<String, Connection>,
    counter: u64,
}

#[derive(Clone)]
pub struct Bridge {
    state: Arc<Mutex<State>>,
    changes: watch::Sender<u64>,
}
impl Default for Bridge {
    fn default() -> Self {
        Self {
            state: Arc::default(),
            changes: watch::channel(0).0,
        }
    }
}

/// Where a provider-level tab command goes.
pub struct Route {
    pub host: String,
    pub tab: i64,
}

fn cut(value: &str, max: usize) -> String {
    let mut end = value.len().min(max);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].replace(|c: char| c.is_control(), " ")
}

impl Bridge {
    /// Notified with a counter on every change of the merged snapshot.
    pub fn changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }
    fn changed(&self) {
        self.changes.send_modify(|n| *n += 1);
    }

    /// The merged snapshot and whether any browser is connected. Titles and
    /// URLs are made to fit the wire rules instead of rejecting a snapshot.
    pub fn snapshot(&self) -> (Vec<BrowserWindow>, Vec<Tab>, bool) {
        let state = self.state.lock().unwrap();
        // Only the most recently focused connection keeps its focused window.
        let focus = state
            .connections
            .iter()
            .filter(|(_, c)| c.windows.iter().any(|w| w.1))
            .max_by_key(|(_, c)| c.focused_at)
            .map(|(id, _)| id.clone());
        let mut windows = Vec::new();
        let mut tabs = Vec::new();
        for (id, connection) in &state.connections {
            let mut active = std::collections::HashSet::new();
            let mut known = std::collections::HashSet::new();
            for &(window, focused) in &connection.windows {
                if known.insert(window) {
                    windows.push(BrowserWindow {
                        id: format!("{id}:w{window}"),
                        window_token: String::new(),
                        focused: focused && focus.as_deref() == Some(id),
                    });
                }
            }
            let mut seen = std::collections::HashSet::new();
            for (tab, window, title, url, is_active, audible) in &connection.tabs {
                if !known.contains(window) || !seen.insert(*tab) {
                    continue;
                }
                let url = if url.len() > MAX_URL_BYTES
                    || url.chars().any(|c| c.is_control() || c.is_whitespace())
                {
                    String::new()
                } else {
                    url.clone()
                };
                tabs.push(Tab {
                    id: format!("{id}:t{tab}"),
                    window_id: format!("{id}:w{window}"),
                    title: cut(title, MAX_TITLE_BYTES.min(MAX_TEXT_BYTES)),
                    url,
                    active: *is_active && active.insert(*window),
                    audible: *audible,
                });
            }
        }
        let connected = !state.connections.is_empty();
        debug_assert_eq!(browser::validate_entries(&windows, &tabs), Ok(()));
        (windows, tabs, connected)
    }

    /// The browser process of a provider window ID (ADR-0096); 0 when it
    /// could not be established. Never logged.
    pub fn browser_process(&self, window_id: &str) -> Result<u32, ErrorCode> {
        let (connection, window) = window_id.rsplit_once(":w").ok_or(ErrorCode::NotFound)?;
        let window: i64 = window.parse().map_err(|_| ErrorCode::NotFound)?;
        let state = self.state.lock().unwrap();
        let c = state
            .connections
            .get(connection)
            .ok_or(ErrorCode::NotFound)?;
        if !c.windows.iter().any(|w| w.0 == window) {
            return Err(ErrorCode::NotFound);
        }
        Ok(c.browser_process)
    }

    /// The host and browser tab ID behind a provider tab ID.
    pub fn route(&self, id: &str) -> Result<Route, ErrorCode> {
        let (connection, tab) = id.rsplit_once(":t").ok_or(ErrorCode::NotFound)?;
        let tab: i64 = tab.parse().map_err(|_| ErrorCode::NotFound)?;
        let state = self.state.lock().unwrap();
        let c = state
            .connections
            .get(connection)
            .ok_or(ErrorCode::NotFound)?;
        if !c.tabs.iter().any(|t| t.0 == tab) {
            return Err(ErrorCode::NotFound);
        }
        Ok(Route {
            host: c.owner.clone(),
            tab,
        })
    }

    /// The host that opens new tabs: the most recently focused connection,
    /// else the most recently attached one.
    pub fn default_host(&self) -> Result<String, ErrorCode> {
        let state = self.state.lock().unwrap();
        state
            .connections
            .values()
            .max_by_key(|c| (c.focused_at, c.attached_at))
            .map(|c| c.owner.clone())
            .ok_or(ErrorCode::Unavailable)
    }

    /// Forget every connection of a host whose bus name went away.
    pub fn drop_host(&self, owner: &str) {
        let removed = {
            let mut state = self.state.lock().unwrap();
            let before = state.connections.len();
            state.connections.retain(|_, c| c.owner != owner);
            before != state.connections.len()
        };
        if removed {
            self.changed();
        }
    }

    /// Send a command to a host and map its reply to an error code.
    pub async fn command(
        connection: &zbus::Connection,
        host: &str,
        action: &str,
        tab: i64,
        url: &str,
    ) -> Result<(), ErrorCode> {
        Self::command_result(connection, host, action, tab, url)
            .await
            .map(|_| ())
    }

    async fn command_result(
        connection: &zbus::Connection,
        host: &str,
        action: &str,
        tab: i64,
        url: &str,
    ) -> Result<(i64, bool), ErrorCode> {
        let proxy = HostProxy::builder(connection)
            .destination(host.to_owned())
            .map_err(|_| ErrorCode::Unavailable)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let reply = tokio::time::timeout(COMMAND_TIMEOUT, proxy.command(action, tab, url))
            .await
            .map_err(|_| ErrorCode::Timeout)?
            .map_err(|_| ErrorCode::Unavailable)?;
        match reply.0.as_str() {
            "" => Ok((reply.1, reply.2)),
            "NotFound" => Err(ErrorCode::NotFound),
            "InvalidArgument" => Err(ErrorCode::InvalidArgument),
            "Timeout" => Err(ErrorCode::Timeout),
            "PermissionDenied" => Err(ErrorCode::PermissionDenied),
            _ => Err(ErrorCode::Unavailable),
        }
    }

    /// Open through one host and qualify its selected window ID with that
    /// exact connection. Numeric IDs from different profiles may collide.
    pub async fn open(
        &self,
        connection: &zbus::Connection,
        url: &str,
    ) -> Result<(String, bool), ErrorCode> {
        let (id, host) = {
            let state = self.state.lock().unwrap();
            state
                .connections
                .iter()
                .max_by_key(|(_, c)| (c.focused_at, c.attached_at))
                .map(|(id, c)| (id.clone(), c.owner.clone()))
                .ok_or(ErrorCode::Unavailable)?
        };
        let (window, created) = Self::command_result(connection, &host, "open", 0, url).await?;
        if window < 0 {
            return Err(ErrorCode::Unavailable);
        }
        let state = self.state.lock().unwrap();
        state
            .connections
            .get(&id)
            .filter(|c| c.owner == host)
            .ok_or(ErrorCode::Unavailable)?;
        Ok((format!("{id}:w{window}"), created))
    }

    /// Drop a host's connections as soon as its unique name disappears.
    fn watch_host(&self, connection: &zbus::Connection, owner: String) {
        let bridge = self.clone();
        let connection = connection.clone();
        tokio::spawn(async move {
            let Ok(bus) = fdo::DBusProxy::new(&connection).await else {
                return bridge.drop_host(&owner);
            };
            let Ok(mut changes) = bus
                .receive_name_owner_changed_with_args(&[(0, owner.as_str())])
                .await
            else {
                return bridge.drop_host(&owner);
            };
            // The host may already be gone before the match was installed.
            if bus.name_has_owner(owner.as_str().try_into().unwrap()).await != Ok(true) {
                return bridge.drop_host(&owner);
            }
            while let Some(change) = changes.next().await {
                if change.args().is_ok_and(|a| a.new_owner.is_none()) {
                    break;
                }
            }
            bridge.drop_host(&owner);
        });
    }
}

/// The browser launches the native host directly, so the host's parent is
/// the browser process. 0 when either step fails.
async fn host_parent(connection: &zbus::Connection, owner: &str) -> u32 {
    let Ok(bus) = fdo::DBusProxy::new(connection).await else {
        return 0;
    };
    let Ok(name) = zbus::names::BusName::try_from(owner) else {
        return 0;
    };
    let Ok(Ok(host)) =
        tokio::time::timeout(COMMAND_TIMEOUT, bus.get_connection_unix_process_id(name)).await
    else {
        return 0;
    };
    std::fs::read_to_string(format!("/proc/{host}/stat"))
        .ok()
        .and_then(|stat| parent_of(&stat))
        .unwrap_or(0)
}

/// The parent process from `/proc/<pid>/stat`. The command name may contain
/// spaces and parentheses, so fields are counted after its last `)`.
fn parent_of(stat: &str) -> Option<u32> {
    let rest = &stat[stat.rfind(')')? + 1..];
    let mut fields = rest.split_whitespace();
    let _state = fields.next()?;
    fields.next()?.parse().ok().filter(|&p| p > 1)
}

fn sender(header: &Header<'_>) -> fdo::Result<String> {
    header
        .sender()
        .map(|s| s.to_string())
        .ok_or_else(|| fdo::Error::AccessDenied("no sender".into()))
}

#[zbus::interface(name = "org.arinn.ChromeBridge1")]
impl Bridge {
    /// Start a browser connection for the calling host; returns its ID.
    async fn attach(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> fdo::Result<String> {
        let owner = sender(&header)?;
        let browser_process = host_parent(connection, &owner).await;
        let id = {
            let mut state = self.state.lock().unwrap();
            if state.connections.len() >= MAX_CONNECTIONS {
                return Err(fdo::Error::LimitsExceeded("too many browsers".into()));
            }
            state.counter += 1;
            let id = format!("c{}", state.counter);
            let attached_at = state.counter;
            state.connections.insert(
                id.clone(),
                Connection {
                    owner: owner.clone(),
                    browser_process,
                    windows: Vec::new(),
                    tabs: Vec::new(),
                    focused_at: 0,
                    attached_at,
                },
            );
            id
        };
        self.watch_host(connection, owner);
        self.changed();
        Ok(id)
    }

    /// Replace the state of one connection. Only its host may publish.
    async fn publish(
        &self,
        connection: String,
        windows: Vec<WireWindow>,
        tabs: Vec<WireTab>,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let owner = sender(&header)?;
        if windows.len() > crate::messages::MAX_WINDOWS || tabs.len() > crate::messages::MAX_TABS {
            return Err(fdo::Error::LimitsExceeded("snapshot too large".into()));
        }
        {
            let mut state = self.state.lock().unwrap();
            state.counter += 1;
            let counter = state.counter;
            let c = state
                .connections
                .get_mut(&connection)
                .filter(|c| c.owner == owner)
                .ok_or_else(|| fdo::Error::AccessDenied("unknown connection".into()))?;
            let before = c.windows.iter().find(|w| w.1).map(|w| w.0);
            let after = windows.iter().find(|w| w.1).map(|w| w.0);
            if after.is_some() && after != before {
                c.focused_at = counter;
            }
            c.windows = windows;
            c.tabs = tabs;
        }
        self.changed();
        Ok(())
    }
}

/// `org.arinn.ChromeHost1`, exported by each host at [`crate::HOST_PATH`].
#[zbus::proxy(
    interface = "org.arinn.ChromeHost1",
    default_path = "/org/arinn/ChromeHost1"
)]
pub trait Host {
    /// `action` is `activate`, `close` or `open`; returns "" or an error code.
    fn command(
        &self,
        action: &str,
        tab: i64,
        url: &str,
    ) -> zbus::Result<crate::host::CommandResult>;
}

/// `org.arinn.ChromeBridge1` client used by the host.
#[zbus::proxy(
    interface = "org.arinn.ChromeBridge1",
    default_service = "org.arinn.ChromeBridge1",
    default_path = "/org/arinn/ChromeBridge1"
)]
pub trait BridgeClient {
    fn attach(&self) -> zbus::Result<String>;
    fn publish(
        &self,
        connection: &str,
        windows: Vec<WireWindow>,
        tabs: Vec<WireTab>,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (connection ID, host owner, windows, tabs, focused_at)
    type Fixture<'a> = (&'a str, &'a str, Vec<WireWindow>, Vec<WireTab>, u64);
    fn bridge_with(connections: Vec<Fixture>) -> Bridge {
        let bridge = Bridge::default();
        {
            let mut state = bridge.state.lock().unwrap();
            for (id, owner, windows, tabs, focused_at) in connections {
                state.connections.insert(
                    id.into(),
                    Connection {
                        owner: owner.into(),
                        browser_process: 4000 + focused_at as u32,
                        windows,
                        tabs,
                        focused_at,
                        attached_at: 0,
                    },
                );
            }
        }
        bridge
    }
    fn tab(id: i64, window: i64, active: bool) -> WireTab {
        (
            id,
            window,
            "T".into(),
            "https://e.org/".into(),
            active,
            false,
        )
    }

    #[test]
    fn merged_snapshots_are_valid_and_routable() {
        let bridge = bridge_with(vec![
            ("c1", ":1.5", vec![(1, true)], vec![tab(10, 1, true)], 1),
            (
                "c2",
                ":1.6",
                vec![(1, true), (2, false)],
                vec![tab(10, 1, true), tab(11, 2, true), tab(12, 9, false)],
                2,
            ),
        ]);
        let (windows, tabs, connected) = bridge.snapshot();
        assert!(connected);
        browser::validate_entries(&windows, &tabs).unwrap();
        // Only the most recently focused connection keeps its focus.
        assert_eq!(
            windows
                .iter()
                .filter(|w| w.focused)
                .map(|w| w.id.as_str())
                .collect::<Vec<_>>(),
            vec!["c2:w1"]
        );
        // A tab in an unknown window is dropped.
        assert_eq!(tabs.len(), 3);
        let route = bridge.route("c2:t11").unwrap();
        assert_eq!((route.host.as_str(), route.tab), (":1.6", 11));
        assert!(matches!(bridge.route("c2:t99"), Err(ErrorCode::NotFound)));
        assert!(matches!(bridge.route("bogus"), Err(ErrorCode::NotFound)));
        assert_eq!(bridge.default_host().unwrap(), ":1.6");
        bridge.drop_host(":1.6");
        assert_eq!(bridge.default_host().unwrap(), ":1.5");
        bridge.drop_host(":1.5");
        assert!(matches!(bridge.default_host(), Err(ErrorCode::Unavailable)));
        assert!(!bridge.snapshot().2);
    }

    #[test]
    fn windows_report_their_connection_browser_process() {
        let bridge = bridge_with(vec![
            ("c1", ":1.5", vec![(1, true)], vec![tab(10, 1, true)], 1),
            ("c2", ":1.6", vec![(1, false), (2, false)], vec![], 2),
        ]);
        assert_eq!(bridge.browser_process("c1:w1").unwrap(), 4001);
        assert_eq!(bridge.browser_process("c2:w2").unwrap(), 4002);
        for missing in ["c2:w9", "c9:w1", "c1:t10", "bogus", "c1:wx"] {
            assert!(matches!(
                bridge.browser_process(missing),
                Err(ErrorCode::NotFound)
            ));
        }
    }

    #[test]
    fn parent_process_is_read_after_the_command_name() {
        assert_eq!(
            parent_of("812 (arinn-chrome-ho) S 640 812 640 0 -1"),
            Some(640)
        );
        // A command name with spaces and a closing parenthesis.
        assert_eq!(parent_of("812 (a) b) c) S 77 1 1"), Some(77));
        // init or a missing parent is not a browser.
        assert_eq!(parent_of("812 (x) S 1 1 1"), None);
        assert_eq!(parent_of("812 (x) S 0 1 1"), None);
        assert_eq!(parent_of("garbage"), None);
        assert_eq!(parent_of("812 (x) S"), None);
    }

    #[test]
    fn titles_and_urls_are_fitted_to_the_wire_rules() {
        let long_title = "я".repeat(MAX_TITLE_BYTES);
        let bridge = bridge_with(vec![(
            "c1",
            ":1.5",
            vec![(1, false)],
            vec![
                (1, 1, long_title, "https://e.org/a b".into(), true, false),
                (
                    2,
                    1,
                    "a\nb".into(),
                    format!("data:{}", "x".repeat(MAX_URL_BYTES)),
                    true,
                    true,
                ),
            ],
            0,
        )]);
        let (windows, tabs, _) = bridge.snapshot();
        browser::validate_entries(&windows, &tabs).unwrap();
        assert!(tabs[0].title.len() <= MAX_TITLE_BYTES);
        assert_eq!(tabs[0].url, "");
        assert_eq!((tabs[1].title.as_str(), tabs[1].url.as_str()), ("a b", ""));
        // Two tabs claiming to be active in one window: only the first stays active.
        assert_eq!((tabs[0].active, tabs[1].active), (true, false));
    }
}
