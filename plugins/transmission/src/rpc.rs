// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Transmission RPC over HTTP. A 409 session challenge is repeated once with
//! the new token. A timeout is not repeated. The session id, the authorization
//! header and the URL are not logged.

use crate::config::{MIN_RPC_VERSION, ParsedEndpoint};
use arinn_protocol::validation::ErrorCode;
use rustls::pki_types::ServerName;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::client::TlsStream;

const MAX_HEADER: usize = 64 * 1024;
const MAX_BODY: usize = 8 * 1024 * 1024;

pub struct Rpc {
    endpoint: ParsedEndpoint,
    auth_header: Option<String>,
    session_id: String,
    timeout: Duration,
    pub rpc_version: Option<i64>,
    tls: Option<tokio_rustls::TlsConnector>,
}

impl Rpc {
    pub fn new(
        endpoint: ParsedEndpoint,
        auth: Option<(String, String)>,
        timeout: Duration,
    ) -> Self {
        Self {
            endpoint,
            auth_header: auth.map(|(user, password)| basic_header(&user, &password)),
            session_id: String::new(),
            timeout,
            rpc_version: None,
            tls: None,
        }
    }

    pub async fn negotiate(&mut self) -> Result<i64, ErrorCode> {
        let arguments = self
            .call("session-get", json!({"fields": ["rpc-version"]}))
            .await?;
        let version = arguments
            .get("rpc-version")
            .and_then(Value::as_i64)
            .ok_or(ErrorCode::Unsupported)?;
        self.rpc_version = Some(version);
        if version < MIN_RPC_VERSION {
            return Err(ErrorCode::Unsupported);
        }
        Ok(version)
    }

    /// One RPC method. On 409 the same body is sent again with the session
    /// id from the challenge. That is not a second execution. Any other
    /// failure, including a timeout, returns immediately.
    pub async fn call(&mut self, method: &str, arguments: Value) -> Result<Value, ErrorCode> {
        self.ensure_tls()?;
        let body = serde_json::to_vec(&json!({
            "method": method,
            "arguments": arguments,
            "tag": 1,
        }))
        .map_err(|_| ErrorCode::Internal)?;
        let first = self.post(&body).await?;
        if first.status != 409 {
            return self.finish(first);
        }
        let session = first
            .header("x-transmission-session-id")
            .filter(|value| session_ok(value))
            .ok_or(ErrorCode::Unavailable)?;
        self.session_id = session;
        let second = self.post(&body).await?;
        if second.status == 409 {
            return Err(ErrorCode::Unavailable);
        }
        self.finish(second)
    }

    fn finish(&self, response: HttpResponse) -> Result<Value, ErrorCode> {
        match response.status {
            200 => parse_success(&response.body),
            401 | 403 => Err(ErrorCode::PermissionDenied),
            _ => Err(ErrorCode::Unavailable),
        }
    }

    async fn post(&self, body: &[u8]) -> Result<HttpResponse, ErrorCode> {
        let result = tokio::time::timeout(self.timeout, self.post_once(body)).await;
        match result {
            Ok(inner) => inner,
            Err(_) => Err(ErrorCode::Timeout),
        }
    }

    async fn post_once(&self, body: &[u8]) -> Result<HttpResponse, ErrorCode> {
        let mut stream = self.connect().await?;
        let mut request = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.endpoint.path,
            self.endpoint.host_header(),
            body.len(),
        );
        if !self.session_id.is_empty() {
            request.push_str("X-Transmission-Session-Id: ");
            request.push_str(&self.session_id);
            request.push_str("\r\n");
        }
        if let Some(header) = &self.auth_header {
            request.push_str("Authorization: ");
            request.push_str(header);
            request.push_str("\r\n");
        }
        request.push_str("\r\n");
        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        stream
            .write_all(body)
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        read_response(&mut stream).await
    }

    async fn connect(&self) -> Result<Stream, ErrorCode> {
        let address = (self.endpoint.host.as_str(), self.endpoint.port);
        let tcp = TcpStream::connect(address)
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        if !self.endpoint.https {
            return Ok(Stream::Tcp(tcp));
        }
        let connector = self.tls.clone().ok_or(ErrorCode::Unavailable)?;
        let name = ServerName::try_from(self.endpoint.host.clone())
            .map_err(|_| ErrorCode::InvalidArgument)?;
        let tls = connector
            .connect(name, tcp)
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        Ok(Stream::Tls(Box::new(tls)))
    }

    pub fn ensure_tls(&mut self) -> Result<(), ErrorCode> {
        if !self.endpoint.https || self.tls.is_some() {
            return Ok(());
        }
        self.tls = Some(tls_connector()?);
        Ok(())
    }
}

fn tls_connector() -> Result<tokio_rustls::TlsConnector, ErrorCode> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let native = rustls_native_certs::load_native_certs();
    let mut roots = rustls::RootCertStore::empty();
    for cert in native.certs {
        let _ = roots.add(cert);
    }
    if roots.is_empty() {
        return Err(ErrorCode::Unavailable);
    }
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

fn session_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
}

fn parse_success(body: &[u8]) -> Result<Value, ErrorCode> {
    let value: Value = serde_json::from_slice(body).map_err(|_| ErrorCode::Unavailable)?;
    if value.get("result").and_then(Value::as_str) != Some("success") {
        return Err(ErrorCode::Unavailable);
    }
    Ok(value.get("arguments").cloned().unwrap_or_else(|| json!({})))
}

fn basic_header(user: &str, password: &str) -> String {
    format!("Basic {}", base64(format!("{user}:{password}").as_bytes()))
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let n = (u32::from(bytes[index]) << 16)
            | (u32::from(bytes[index + 1]) << 8)
            | u32::from(bytes[index + 2]);
        for shift in [18, 12, 6, 0] {
            out.push(char::from(TABLE[((n >> shift) & 0x3f) as usize]));
        }
        index += 3;
    }
    let rest = bytes.len() - index;
    if rest == 1 {
        let n = u32::from(bytes[index]) << 16;
        out.push(char::from(TABLE[((n >> 18) & 0x3f) as usize]));
        out.push(char::from(TABLE[((n >> 12) & 0x3f) as usize]));
        out.push('=');
        out.push('=');
    } else if rest == 2 {
        let n = (u32::from(bytes[index]) << 16) | (u32::from(bytes[index + 1]) << 8);
        out.push(char::from(TABLE[((n >> 18) & 0x3f) as usize]));
        out.push(char::from(TABLE[((n >> 12) & 0x3f) as usize]));
        out.push(char::from(TABLE[((n >> 6) & 0x3f) as usize]));
        out.push('=');
    }
    out
}

struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn header(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
    }
}

enum Stream {
    Tcp(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl Stream {
    async fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.write_all(bytes).await,
            Self::Tls(stream) => stream.write_all(bytes).await,
        }
    }

    async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.read(buf).await,
            Self::Tls(stream) => stream.read(buf).await,
        }
    }
}

async fn read_response(stream: &mut Stream) -> Result<HttpResponse, ErrorCode> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        if buf.len() > MAX_HEADER {
            return Err(ErrorCode::Unavailable);
        }
        let n = stream
            .read(&mut tmp)
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        if n == 0 {
            return Err(ErrorCode::Unavailable);
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(end) = find_header_end(&buf) {
            break end;
        }
    };
    let header_bytes = &buf[..header_end];
    let header_text = std::str::from_utf8(header_bytes).map_err(|_| ErrorCode::Unavailable)?;
    let mut lines = header_text.split("\r\n");
    let status_line = lines.next().ok_or(ErrorCode::Unavailable)?;
    let status = status_line
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or(ErrorCode::Unavailable)?;
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(ErrorCode::Unavailable);
        };
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    let mut body = buf[header_end..].to_vec();
    if status == 200 {
        let length = headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, value)| value.parse::<usize>().ok())
            .ok_or(ErrorCode::Unavailable)?;
        if length > MAX_BODY {
            return Err(ErrorCode::Unavailable);
        }
        while body.len() < length {
            let n = stream
                .read(&mut tmp)
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&tmp[..n]);
            if body.len() > MAX_BODY {
                return Err(ErrorCode::Unavailable);
            }
        }
        if body.len() < length {
            return Err(ErrorCode::Unavailable);
        }
        body.truncate(length);
    }
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

#[cfg(test)]
mod tests {
    use super::Rpc;
    use crate::config::ParsedEndpoint;
    use arinn_protocol::validation::ErrorCode;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    fn endpoint(port: u16) -> ParsedEndpoint {
        ParsedEndpoint {
            https: false,
            host: "127.0.0.1".into(),
            port,
            path: "/transmission/rpc".into(),
        }
    }

    struct Script {
        hits: std::sync::Mutex<Vec<String>>,
        hang: bool,
        auth: Option<String>,
    }

    async fn serve(port_slot: Arc<std::sync::Mutex<u16>>, script: Arc<Script>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        *port_slot.lock().unwrap() = listener.local_addr().unwrap().port();
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let script = Arc::clone(&script);
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 2048];
                loop {
                    let Ok(n) = socket.read(&mut tmp).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&buf);
                if script.hang {
                    script.hits.lock().unwrap().push("hang".into());
                    std::future::pending::<()>().await;
                }
                let unauthorized = script.auth.as_ref().is_some_and(|expected| {
                    !text
                        .lines()
                        .any(|line| line == format!("Authorization: {expected}"))
                });
                if unauthorized {
                    let body = b"unauthorized";
                    let _ = socket
                        .write_all(format!("HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes())
                        .await;
                    let _ = socket.write_all(body).await;
                    return;
                }
                let has_session = text
                    .to_ascii_lowercase()
                    .contains("x-transmission-session-id: fixture-session");
                if !has_session {
                    let _ = socket
                        .write_all(b"HTTP/1.1 409 Conflict\r\nX-Transmission-Session-Id: fixture-session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                    return;
                }
                let method = text
                    .split("\"method\":\"")
                    .nth(1)
                    .and_then(|rest| rest.split('"').next())
                    .unwrap_or("unknown");
                script.hits.lock().unwrap().push(method.to_string());
                let body = br#"{"result":"success","arguments":{"rpc-version":17,"torrents":[]}}"#;
                let _ = socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await;
                let _ = socket.write_all(body).await;
            });
        }
    }

    #[tokio::test]
    async fn challenge_is_not_a_second_command_and_timeout_is_not_retried() {
        let port = Arc::new(std::sync::Mutex::new(0u16));
        let script = Arc::new(Script {
            hits: std::sync::Mutex::new(Vec::new()),
            hang: false,
            auth: Some("Basic Zml4dHVyZTpzZWNyZXQ=".into()),
        });
        tokio::spawn(serve(Arc::clone(&port), Arc::clone(&script)));
        let port = wait_port(&port).await;
        let mut rpc = Rpc::new(
            endpoint(port),
            Some(("fixture".into(), "secret".into())),
            Duration::from_millis(300),
        );
        let version = rpc.negotiate().await.unwrap();
        assert_eq!(version, 17);
        assert_eq!(script.hits.lock().unwrap().as_slice(), ["session-get"]);

        let hang_port = Arc::new(std::sync::Mutex::new(0u16));
        let hang = Arc::new(Script {
            hits: std::sync::Mutex::new(Vec::new()),
            hang: true,
            auth: None,
        });
        tokio::spawn(serve(Arc::clone(&hang_port), Arc::clone(&hang)));
        let hang_port = wait_port(&hang_port).await;
        let mut rpc = Rpc::new(endpoint(hang_port), None, Duration::from_millis(150));
        let error = rpc
            .call("torrent-stop", json!({"ids": ["abc"]}))
            .await
            .unwrap_err();
        assert_eq!(error, ErrorCode::Timeout);
        assert_eq!(hang.hits.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn auth_failure_is_permission_denied() {
        let port = Arc::new(std::sync::Mutex::new(0u16));
        let script = Arc::new(Script {
            hits: std::sync::Mutex::new(Vec::new()),
            hang: false,
            auth: Some("Basic bm8=".into()),
        });
        tokio::spawn(serve(Arc::clone(&port), Arc::clone(&script)));
        let port = wait_port(&port).await;
        let mut rpc = Rpc::new(
            endpoint(port),
            Some(("fixture".into(), "secret".into())),
            Duration::from_millis(300),
        );
        assert_eq!(
            rpc.call("session-get", json!({})).await,
            Err(ErrorCode::PermissionDenied)
        );
        assert!(script.hits.lock().unwrap().is_empty());
    }

    async fn wait_port(port: &std::sync::Mutex<u16>) -> u16 {
        for _ in 0..50 {
            let value = *port.lock().unwrap();
            if value != 0 {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("listener did not bind");
    }
}
