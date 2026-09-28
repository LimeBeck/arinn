// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! JSON messages between the extension and the host. Unknown fields and
//! message types are rejected; the extension and host ship together.
use crate::{WireTab, WireWindow};
use serde::{Deserialize, Serialize};

pub const MAX_WINDOWS: usize = 1_000;
pub const MAX_TABS: usize = 10_000;
pub const ACTIONS: &[&str] = &["activate", "close", "open"];

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum FromExtension {
    /// The complete state of this browser connection.
    Snapshot {
        windows: Vec<Window>,
        tabs: Vec<Tab>,
    },
    /// The outcome of command `serial`: empty on success, else an error code.
    Result { serial: u64, error: String },
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub id: i64,
    pub focused: bool,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tab {
    pub id: i64,
    pub window_id: i64,
    pub title: String,
    pub url: String,
    pub active: bool,
    pub audible: bool,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToExtension {
    #[serde(rename_all = "camelCase")]
    Command {
        serial: u64,
        action: String,
        tab_id: i64,
        url: String,
    },
}

/// Parse and bound a message from the extension.
pub fn parse(body: &[u8]) -> Result<FromExtension, &'static str> {
    let message: FromExtension = serde_json::from_slice(body).map_err(|_| "invalid message")?;
    if let FromExtension::Snapshot { windows, tabs } = &message
        && (windows.len() > MAX_WINDOWS || tabs.len() > MAX_TABS)
    {
        return Err("snapshot too large");
    }
    Ok(message)
}

pub fn wire(windows: Vec<Window>, tabs: Vec<Tab>) -> (Vec<WireWindow>, Vec<WireTab>) {
    (
        windows.into_iter().map(|w| (w.id, w.focused)).collect(),
        tabs.into_iter()
            .map(|t| (t.id, t.window_id, t.title, t.url, t.active, t.audible))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_and_results_parse_strictly() {
        let snapshot = br#"{"type":"snapshot","windows":[{"id":1,"focused":true}],
            "tabs":[{"id":7,"windowId":1,"title":"T","url":"https://e.org/","active":true,"audible":false}]}"#;
        let FromExtension::Snapshot { windows, tabs } = parse(snapshot).unwrap() else {
            panic!("snapshot expected");
        };
        assert_eq!(
            windows,
            vec![Window {
                id: 1,
                focused: true
            }]
        );
        assert_eq!((tabs[0].id, tabs[0].window_id), (7, 1));
        assert_eq!(
            parse(br#"{"type":"result","serial":3,"error":""}"#).unwrap(),
            FromExtension::Result {
                serial: 3,
                error: String::new()
            }
        );
        for bad in [
            &br#"{"type":"eval","code":"x"}"#[..],
            br#"{"type":"result","serial":3,"error":"","extra":1}"#,
            br#"{"type":"snapshot","windows":[],"tabs":[{"id":1}]}"#,
            b"not json",
        ] {
            assert!(parse(bad).is_err(), "{}", String::from_utf8_lossy(bad));
        }
        let many = format!(
            r#"{{"type":"snapshot","windows":[{}],"tabs":[]}}"#,
            vec![r#"{"id":1,"focused":false}"#; MAX_WINDOWS + 1].join(",")
        );
        assert_eq!(parse(many.as_bytes()), Err("snapshot too large"));
    }

    #[test]
    fn commands_serialize_in_the_extension_format() {
        let command = ToExtension::Command {
            serial: 4,
            action: "activate".into(),
            tab_id: 9,
            url: String::new(),
        };
        assert_eq!(
            serde_json::to_string(&command).unwrap(),
            r#"{"type":"command","serial":4,"action":"activate","tabId":9,"url":""}"#
        );
    }
}
