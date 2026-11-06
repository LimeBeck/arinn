// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
};

pub(crate) struct Mock {
    pub(crate) api: Api,
    requests: Arc<Mutex<Vec<(String, Value)>>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Mock {
    pub(crate) fn new(responses: Vec<(u16, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let records = requests.clone();
        let worker = thread::spawn(move || {
            for (status, body) in responses {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut byte = [0];
                    socket.read_exact(&mut byte).unwrap();
                    bytes.push(byte[0]);
                    assert!(bytes.len() <= 16 * 1024);
                    if bytes.ends_with(b"\r\n\r\n") {
                        break bytes.len();
                    }
                };
                let headers = String::from_utf8(bytes).unwrap();
                assert!(
                    headers
                        .to_lowercase()
                        .contains("authorization: oauth synthetic-test-token")
                );
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                assert!(length <= 16 * 1024);
                let mut content = vec![0; length];
                socket.read_exact(&mut content).unwrap();
                let value = if content.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_slice(&content).unwrap_or(Value::Null)
                };
                records.lock().unwrap().push((
                    headers[..header_end].lines().next().unwrap().to_owned(),
                    value,
                ));
                let _ = write!(
                    socket,
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        // Test traffic is loopback only; no session proxy or real credentials.
        let mut token = HeaderValue::from_static("OAuth synthetic-test-token");
        token.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, token);
        let client = Client::builder()
            .no_proxy()
            .tls_built_in_root_certs(false)
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let api = Api {
            expired: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            lease: None,
            expires_at: None,
            deadline: None,
            station_images: Default::default(),
            station_lookups: Default::default(),
            downloads: Client::builder()
                .no_proxy()
                .tls_built_in_root_certs(false)
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap(),
            client,
            origin: Url::parse(&origin).unwrap(),
        };
        Self {
            api,
            requests,
            worker: Some(worker),
        }
    }
    pub(crate) fn finish(mut self) -> Vec<(String, Value)> {
        self.worker.take().unwrap().join().unwrap();
        self.requests.lock().unwrap().clone()
    }
}

fn response(value: Value) -> (u16, String) {
    (200, json!({"result":value}).to_string())
}

#[tokio::test]
async fn radio_uses_session_json_feedback_and_advances_with_the_completed_track() {
    let mock = Mock::new(vec![
        response(
            json!({"radioSessionId":"session-a","batchId":"batch-a.fixture","sequence":[{"type":"track","track":{"id":123,"available":true}}]}),
        ),
        response(json!(true)),
        response(
            json!({"batchId":"batch-b.fixture","sequence":[{"type":"track","track":{"id":124,"available":true}}]}),
        ),
        response(json!(true)),
    ]);
    let session = mock.api.start_radio("123:456").await.unwrap();
    assert_eq!(session.batch.tracks[0].id, "123");
    let batch = mock.api.next_batch(&session, "123:456").await.unwrap();
    assert_eq!(batch.id, "batch-b.fixture");
    assert_eq!(
        mock.api
            .feedback(&session, &batch, Feedback::TrackFinished("123", 1500))
            .await,
        Err(Error::InvalidInput)
    );
    mock.api
        .feedback(
            &session,
            &session.batch,
            Feedback::TrackFinished("123", 1500),
        )
        .await
        .unwrap();
    let requests = mock.finish();
    assert!(requests[0].0.starts_with("POST /rotor/session/new "));
    assert_eq!(requests[0].1["seeds"], json!(["track:123"]));
    assert_eq!(requests[1].1["batch_id"], "batch-a.fixture");
    assert_eq!(requests[1].1["event"]["type"], "radioStarted");
    assert!(requests[1].1["event"]["timestamp"].as_u64().unwrap() > 0);
    assert_eq!(requests[2].1["queue"], json!(["123"]));
    assert_eq!(requests[3].1["batch_id"], "batch-a.fixture");
    assert_eq!(requests[3].1["event"]["totalPlayedSeconds"], 1.5);
}

#[tokio::test]
async fn authentication_rate_limit_and_response_limits_are_not_empty_catalogues() {
    for (status, expected) in [
        (401, Error::Authentication),
        (403, Error::Forbidden),
        (429, Error::RateLimited),
        (404, Error::NotFound),
        (302, Error::Unavailable),
    ] {
        let mock = Mock::new(vec![(status, "{}".into())]);
        assert_eq!(
            mock.api.search_tracks("synthetic title", 0).await.err(),
            Some(expected)
        );
        mock.finish();
    }
    let mock = Mock::new(vec![(200, "x".repeat(MAX_RESPONSE + 1))]);
    assert_eq!(
        mock.api.search_tracks("synthetic title", 0).await.err(),
        Some(Error::TooLarge)
    );
    mock.finish();
}

#[test]
fn token_validation_never_echoes_a_private_header_value() {
    assert!(matches!(Api::new("bad\nsecret"), Err(Error::InvalidInput)));
    assert!(matches!(Api::new(""), Err(Error::InvalidInput)));
    let value = authorization_header("synthetic-test-token").unwrap();
    assert!(value.is_sensitive());
    assert!(!format!("{value:?}").contains("synthetic-test-token"));
}

#[tokio::test]
async fn api_known_expiry_and_revocation_cancel_work_without_waiting_for_http_401() {
    let mut mock = Mock::new(vec![]);
    mock.api.deadline = Some(tokio::time::Instant::now());
    assert!(mock.api.expired());
    assert_eq!(
        mock.api.guard(async { Ok(()) }).await,
        Err(Error::Authentication)
    );
    mock.api.deadline = Some(tokio::time::Instant::now() + Duration::from_millis(1));
    assert_eq!(
        mock.api
            .guard(std::future::pending::<Result<(), Error>>())
            .await,
        Err(Error::Authentication)
    );
    mock.api.deadline = None;
    let lease = Arc::new(std::sync::atomic::AtomicU8::new(auth::ACCEPTED));
    mock.api.lease = Some(lease.clone());
    lease.store(0, std::sync::atomic::Ordering::Release);
    assert!(mock.api.expired());
    assert_eq!(
        mock.api.guard(async { Ok(()) }).await,
        Err(Error::Authentication)
    );
    assert!(mock.finish().is_empty());
}
#[test]
fn anonymous_download_client_carries_no_oauth_header() {
    let mock = Mock::new(vec![]);
    let request = mock
        .api
        .downloads
        .get("https://storage.mds.yandex.net/fixture")
        .build()
        .unwrap();
    assert!(!request.headers().contains_key(AUTHORIZATION));
    assert!(mock.finish().is_empty());
}

/// #825: each cover kind reads its own API object: an album's `coverUri`, an
/// artist's `cover.uri` from its brief info, a playlist's from the account's
/// playlist list (no tracks). The reference becomes the 600 × 600 image on
/// the avatars CDN; any other host is refused.
#[tokio::test]
async fn every_cover_kind_reads_its_own_object() {
    use crate::parse::Cover;
    let mock = Mock::new(vec![
        response(json!({"id":7,"coverUri":"avatars.yandex.net/get-music-content/1/a/%%"})),
        response(
            json!({"artist":{"id":9,"cover":{"type":"from-album-cover","uri":"avatars.yandex.net/get-music-content/2/b/%%"}}}),
        ),
        response(json!([
            {"kind":4,"title":"Other","trackCount":1},
            {"kind":3,"title":"Pictured","trackCount":2,"ogImage":"avatars.yandex.net/get-yapic/3/c/%%"}
        ])),
        response(json!([{"id":123,"coverUri":"avatars.mds.yandex.net/get-music-content/4/d/%%"}])),
        response(json!({"id":8,"title":"Bare"})),
    ]);
    let api = &mock.api;
    assert_eq!(
        api.cover_reference(&Cover::Album("7".into()))
            .await
            .unwrap(),
        "avatars.yandex.net/get-music-content/1/a/%%"
    );
    assert_eq!(
        api.cover_reference(&Cover::Artist("9".into()))
            .await
            .unwrap(),
        "avatars.yandex.net/get-music-content/2/b/%%"
    );
    assert_eq!(
        api.cover_reference(&Cover::Playlist {
            uid: "5".into(),
            kind: "3".into()
        })
        .await
        .unwrap(),
        "avatars.yandex.net/get-yapic/3/c/%%"
    );
    assert_eq!(
        api.cover_reference(&Cover::Track("123".into()))
            .await
            .unwrap(),
        "avatars.mds.yandex.net/get-music-content/4/d/%%"
    );
    // No image: nothing to load, and no guess.
    assert_eq!(
        api.cover_reference(&Cover::Album("8".into())).await,
        Err(Error::Unavailable)
    );
    let requests: Vec<String> = mock.finish().into_iter().map(|(line, _)| line).collect();
    let paths: Vec<&str> = requests
        .iter()
        .map(|line| line.split(' ').nth(1).unwrap())
        .collect();
    assert_eq!(
        paths,
        [
            "/albums/7",
            "/artists/9/brief-info",
            "/users/5/playlists/list",
            "/tracks",
            "/albums/8"
        ]
    );
    let url = crate::stream::cover_url("avatars.yandex.net/get-music-content/1/a/%%").unwrap();
    assert_eq!(
        url.as_str(),
        "https://avatars.yandex.net/get-music-content/1/a/600x600"
    );
    assert!(crate::stream::cover_url("https://avatars.mds.yandex.net/get-yapic/3/c/%%").is_ok());
    for foreign in [
        "images.example.com/a/%%",
        "avatars.yandex.net.example.com/a/%%",
        "music.yandex.ru/a/%%",
        "avatars.yandex.net:8443/a/%%",
        "user@avatars.yandex.net/a/%%",
        "http://avatars.yandex.net/a/%%",
    ] {
        assert_eq!(
            crate::stream::cover_url(foreign),
            Err(Error::InvalidResponse),
            "{foreign}"
        );
    }
}
