// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider settings (ADR-0065). The endpoint and the path map are
//! configuration. The credential setting is a secret-storage label, never a
//! password (spec §11).

use arinn_protocol::{
    settings::{self, Setting},
    validation::ErrorCode,
};
use serde_json::{Map, Value};
use std::time::Duration;

pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:9091/transmission/rpc";
pub const MIN_RPC_VERSION: i64 = 15;

/// How often the provider reads the backend, and how long one RPC may take.
#[derive(Clone, Debug)]
pub struct Intervals {
    pub active: Duration,
    pub idle: Duration,
    pub backoff: [Duration; 6],
    pub timeout: Duration,
    pub jitter: bool,
}

impl Intervals {
    pub fn production() -> Self {
        Self {
            active: Duration::from_secs(1),
            idle: Duration::from_secs(10),
            backoff: [
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8),
                Duration::from_secs(16),
                Duration::from_secs(30),
            ],
            timeout: Duration::from_secs(5),
            jitter: true,
        }
    }

    /// Short delays for the fake-RPC tests.
    pub fn fast() -> Self {
        Self {
            active: Duration::from_millis(20),
            idle: Duration::from_millis(20),
            backoff: [Duration::from_millis(20); 6],
            timeout: Duration::from_millis(200),
            jitter: false,
        }
    }

    pub(crate) fn wait(&self, base: Duration, step: u32) -> Duration {
        if !self.jitter || base.is_zero() {
            return base;
        }
        let swing = i64::from(step.wrapping_mul(17).wrapping_add(3) % 21) - 10;
        let ms = base.as_millis() as i64;
        let jittered = ms + ms * swing / 100;
        Duration::from_millis(u64::try_from(jittered.max(1)).unwrap_or(1))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedEndpoint {
    pub https: bool,
    pub host: String,
    pub port: u16,
    pub path: String,
}

impl ParsedEndpoint {
    pub(crate) fn key(&self) -> String {
        let scheme = if self.https { "https" } else { "http" };
        format!("{scheme}://{}:{}{}", self.host, self.port, self.path)
    }

    pub(crate) fn host_header(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// One backend directory rewritten to a local mount. `backend` and `local`
/// have no trailing slash except when the path is `/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub backend: String,
    pub local: String,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub endpoint: ParsedEndpoint,
    pub credential_ref: String,
    pub mappings: Vec<Mapping>,
}

pub fn schema() -> Vec<Setting> {
    vec![
        Setting {
            id: "endpoint".into(),
            category: "network".into(),
            title: "Download service address".into(),
            description: "HTTP or HTTPS address of the download service. Do not put a password in the address.".into(),
            kind: "string".into(),
            value: json_string(DEFAULT_ENDPOINT),
            constraints: r#"{"max_length":2048}"#.into(),
            enabled: true,
        },
        Setting {
            id: "credential_ref".into(),
            category: "network".into(),
            title: "Credential name".into(),
            description: "Name of the entry in the system secret storage. Leave empty when the service needs no password.".into(),
            kind: "string".into(),
            value: json_string(""),
            constraints: r#"{"max_length":128}"#.into(),
            enabled: true,
        },
        Setting {
            id: "path_map".into(),
            category: "media".into(),
            title: "Folder mapping".into(),
            description: "Pairs of a folder on the download service and the same folder on this computer, written as service-folder>local-folder and separated by semicolons.".into(),
            kind: "string".into(),
            value: json_string(""),
            constraints: r#"{"max_length":4096}"#.into(),
            enabled: true,
        },
    ]
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

impl Config {
    pub fn from_values(values: &Map<String, Value>) -> Result<Self, ErrorCode> {
        let endpoint = parse_endpoint(required_str(values, "endpoint")?)?;
        let credential_ref = required_str(values, "credential_ref")?.to_string();
        if !credential_label(&credential_ref) {
            return Err(ErrorCode::InvalidArgument);
        }
        let mappings = parse_path_map(required_str(values, "path_map")?)?;
        Ok(Self {
            endpoint,
            credential_ref,
            mappings,
        })
    }
}

fn required_str<'a>(values: &'a Map<String, Value>, id: &str) -> Result<&'a str, ErrorCode> {
    values
        .get(id)
        .and_then(Value::as_str)
        .ok_or(ErrorCode::InvalidArgument)
}

/// `http` or `https`, with a host and no user info. Credentials never travel
/// in the URL.
pub fn parse_endpoint(value: &str) -> Result<ParsedEndpoint, ErrorCode> {
    if value.len() > 2048 || value.chars().any(char::is_control) {
        return Err(ErrorCode::InvalidArgument);
    }
    let (https, rest) = if let Some(rest) = value.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = value.strip_prefix("http://") {
        (false, rest)
    } else {
        return Err(ErrorCode::InvalidArgument);
    };
    if rest.is_empty() || rest.contains('@') {
        return Err(ErrorCode::InvalidArgument);
    }
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    if authority.is_empty() || path.contains([' ', '\\']) || !path.starts_with('/') {
        return Err(ErrorCode::InvalidArgument);
    }
    let (host, port) = split_authority(authority, if https { 443 } else { 80 })?;
    if !host_ok(&host) {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(ParsedEndpoint {
        https,
        host,
        port,
        path: path.to_string(),
    })
}

fn split_authority(authority: &str, default_port: u16) -> Result<(String, u16), ErrorCode> {
    if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']').ok_or(ErrorCode::InvalidArgument)?;
        let host = rest[..end].to_string();
        let after = &rest[end + 1..];
        let port = if let Some(port) = after.strip_prefix(':') {
            parse_port(port)?
        } else if after.is_empty() {
            default_port
        } else {
            return Err(ErrorCode::InvalidArgument);
        };
        return Ok((host, port));
    }
    if authority.contains(']') {
        return Err(ErrorCode::InvalidArgument);
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
            Ok((host.to_string(), parse_port(port)?))
        }
        Some(_) => Err(ErrorCode::InvalidArgument),
        None => Ok((authority.to_string(), default_port)),
    }
}

fn parse_port(value: &str) -> Result<u16, ErrorCode> {
    let port: u16 = value.parse().map_err(|_| ErrorCode::InvalidArgument)?;
    if port == 0 {
        Err(ErrorCode::InvalidArgument)
    } else {
        Ok(port)
    }
}

fn host_ok(host: &str) -> bool {
    !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'))
}

fn credential_label(value: &str) -> bool {
    value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// `backend>local` pairs separated by `;`. Both sides are absolute local
/// paths. An empty string is no mapping.
pub fn parse_path_map(value: &str) -> Result<Vec<Mapping>, ErrorCode> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    let mut mappings = Vec::new();
    for pair in value.split(';') {
        if pair.is_empty() {
            return Err(ErrorCode::InvalidArgument);
        }
        let Some((backend, local)) = pair.split_once('>') else {
            return Err(ErrorCode::InvalidArgument);
        };
        if !settings::path(backend) || !settings::path(local) {
            return Err(ErrorCode::InvalidArgument);
        }
        let backend = trim_dir(backend);
        let local = trim_dir(local);
        // A root prefix would rewrite every path. That is not a mapping.
        if backend == "/" || local.is_empty() {
            return Err(ErrorCode::InvalidArgument);
        }
        mappings.push(Mapping { backend, local });
    }
    Ok(mappings)
}

fn trim_dir(value: &str) -> String {
    if value.len() > 1 {
        value.trim_end_matches('/').to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schema_defaults_match_the_contract() {
        let schema = schema();
        settings::validate_schema(&schema).unwrap();
        let values = settings::effective_values(&schema, &Map::new());
        let config = Config::from_values(&values).unwrap();
        assert_eq!(config.endpoint.key(), DEFAULT_ENDPOINT);
        assert!(config.credential_ref.is_empty());
        assert!(config.mappings.is_empty());
    }

    #[test]
    fn endpoint_rejects_credentials_and_unknown_schemes() {
        assert!(parse_endpoint(DEFAULT_ENDPOINT).is_ok());
        assert!(parse_endpoint("https://downloads.example/transmission/rpc").is_ok());
        assert!(parse_endpoint("http://[::1]:9091/transmission/rpc").is_ok());
        assert_eq!(
            parse_endpoint("http://user:secret@127.0.0.1/transmission/rpc"),
            Err(ErrorCode::InvalidArgument)
        );
        assert_eq!(
            parse_endpoint("ftp://127.0.0.1/transmission/rpc"),
            Err(ErrorCode::InvalidArgument)
        );
    }

    #[test]
    fn path_map_keeps_the_longest_prefix_rules() {
        let values = settings::effective_values(
            &schema(),
            &json!({
                "path_map": "/srv/downloads>/mnt/media;/srv>/mnt/short",
                "credential_ref": "home-rpc"
            })
            .as_object()
            .unwrap()
            .clone(),
        );
        let config = Config::from_values(&values).unwrap();
        assert_eq!(config.credential_ref, "home-rpc");
        assert_eq!(
            config.mappings,
            vec![
                Mapping {
                    backend: "/srv/downloads".into(),
                    local: "/mnt/media".into(),
                },
                Mapping {
                    backend: "/srv".into(),
                    local: "/mnt/short".into(),
                },
            ]
        );
        assert_eq!(
            parse_path_map("/var/../downloads>/mnt/media"),
            Err(ErrorCode::InvalidArgument)
        );
        assert_eq!(
            parse_path_map("relative>/mnt/media"),
            Err(ErrorCode::InvalidArgument)
        );
        let stored = json!({"credential_ref": "not a label"});
        let values = settings::effective_values(&schema(), stored.as_object().unwrap());
        assert_eq!(
            Config::from_values(&values).map(|_| ()),
            Err(ErrorCode::InvalidArgument)
        );
    }
}
