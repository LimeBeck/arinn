// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Transmission RPC as an Arinn `download` provider (spec §7.5, ADR-0025, ADR-0067).
//! Endpoints, paths, titles, credentials and the RPC session id are not logged.

mod config;
mod map;
mod provider;
mod rpc;
mod secrets;

pub use config::{Config, Intervals, schema};
pub use provider::{run, serve};

use arinn_protocol::validation::ErrorCode;

/// One JSON log line. `operation` is a fixed token and `error` is an Arinn
/// code, so a URL, path, title, credential or session id cannot be interpolated.
pub fn log_line(
    level: &str,
    operation: &'static str,
    error: Option<ErrorCode>,
    rpc_version: Option<i64>,
) -> String {
    let error = match error {
        Some(error) => format!("\"{error}\""),
        None => "null".to_string(),
    };
    let version = match rpc_version {
        Some(version) => format!(",\"rpc_version\":{version}"),
        None => String::new(),
    };
    format!(
        "{{\"level\":\"{level}\",\"component\":\"arinn-plugin-transmission\",\"provider_id\":\"transmission\",\"request_id\":null,\"operation\":\"{operation}\",\"duration_ms\":null,\"error\":{error}{version}}}"
    )
}

pub(crate) fn log_event(
    level: &str,
    operation: &'static str,
    error: Option<ErrorCode>,
    rpc_version: Option<i64>,
) {
    eprintln!("{}", log_line(level, operation, error, rpc_version));
}

#[cfg(test)]
mod tests {
    use super::log_line;

    #[test]
    fn log_line_records_the_rpc_version_and_nothing_else() {
        let line = log_line("info", "session", None, Some(17));
        assert_eq!(
            line,
            "{\"level\":\"info\",\"component\":\"arinn-plugin-transmission\",\"provider_id\":\"transmission\",\"request_id\":null,\"operation\":\"session\",\"duration_ms\":null,\"error\":null,\"rpc_version\":17}"
        );
    }
}
