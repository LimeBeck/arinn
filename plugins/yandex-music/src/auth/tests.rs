// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use serde_json::json;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex as StdMutex},
    thread,
};

#[derive(Clone, Default)]
pub(crate) struct MemoryStore {
    pub(crate) bytes: Arc<StdMutex<Option<Vec<u8>>>>,
}
impl CredentialStore for MemoryStore {
    async fn load(&self) -> Result<Option<Credentials>, Error> {
        self.bytes
            .lock()
            .unwrap()
            .as_ref()
            .map(|b| Credentials::decode(b))
            .transpose()
    }
    async fn save(&self, token: &Credentials) -> Result<(), Error> {
        *self.bytes.lock().unwrap() = Some(token.encode()?);
        Ok(())
    }
    async fn clear(&self) -> Result<(), Error> {
        *self.bytes.lock().unwrap() = None;
        Ok(())
    }
}

pub(crate) type Request = (String, HashMap<String, String>);
pub(crate) struct Mock {
    origin: Url,
    records: Arc<StdMutex<Vec<Request>>>,
    worker: thread::JoinHandle<()>,
}
impl Mock {
    pub(crate) fn new(replies: Vec<(u16, String, Duration)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let records = Arc::new(StdMutex::new(Vec::new()));
        let log = records.clone();
        let worker = thread::spawn(move || {
            for (status, body, delay) in replies {
                let start = std::time::Instant::now();
                let mut socket = loop {
                    match listener.accept() {
                        Ok((s, _)) => break s,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                start.elapsed() < Duration::from_secs(5),
                                "missing synthetic request"
                            );
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(_) => panic!("synthetic listener failed"),
                    }
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    socket.read_exact(&mut byte).unwrap();
                    head.push(byte[0]);
                    assert!(head.len() < 16 * 1024);
                }
                let head = String::from_utf8(head).unwrap();
                assert!(!head.to_ascii_lowercase().contains("authorization:"));
                let size: usize = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|n| n.trim().parse().ok())
                    })
                    .unwrap_or(0);
                assert!(size < 16 * 1024);
                let mut body_bytes = vec![0; size];
                socket.read_exact(&mut body_bytes).unwrap();
                let form = Url::parse(&format!(
                    "http://fixture.invalid/?{}",
                    String::from_utf8(body_bytes).unwrap()
                ))
                .unwrap()
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
                log.lock()
                    .unwrap()
                    .push((head.lines().next().unwrap().into(), form));
                thread::sleep(delay);
                let _ = write!(
                    socket,
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self {
            origin,
            records,
            worker,
        }
    }
    pub(crate) fn client<S: CredentialStore + 'static>(&self, store: S) -> DeviceAuth<S> {
        // Loopback fixtures do not perform TLS or load the host CA store.
        // Building production clients here spent the listener's setup budget
        // loading native roots repeatedly under parallel/proot test runners.
        let client = Client::builder()
            .no_proxy()
            .tls_built_in_root_certs(false)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let (generation, _) = watch::channel(0);
        DeviceAuth {
            client,
            origin: self.origin.clone(),
            client_id: "synthetic-client".into(),
            client_secret: Some("synthetic-secret".into()),
            device_id: "synthetic-device".into(),
            owner: Arc::new(()),
            session: Arc::new(Session {
                generation,
                gate: Mutex::new(None),
                store,
            }),
        }
    }

    pub(crate) fn finish(self) -> Vec<Request> {
        self.worker.join().unwrap();
        self.records.lock().unwrap().clone()
    }
}
pub(crate) fn reply(status: u16, value: Value) -> (u16, String, Duration) {
    (status, value.to_string(), Duration::ZERO)
}
pub(crate) fn code() -> Value {
    json!({"device_code":"synthetic-device-code", "user_code":"ABCD-EFGH", "verification_url":"https://oauth.yandex.ru/device", "expires_in":300, "interval":1})
}
pub(crate) fn token() -> Value {
    json!({"access_token":"synthetic-access-token", "token_type":"bearer", "expires_in":3600})
}

#[tokio::test]
async fn pending_confirmation_persists_then_restores_and_disconnect_removes_credentials() {
    let mock = Mock::new(vec![
        reply(200, code()),
        reply(400, json!({"error":"authorization_pending"})),
        reply(200, token()),
    ]);
    let store = MemoryStore::default();
    let auth = mock.client(store.clone());
    let mut attempt = auth.start().await.unwrap();
    assert_eq!(attempt.prompt.user_code, "ABCD-EFGH");
    attempt.interval = Duration::from_millis(1);
    auth.complete(attempt).await.unwrap();
    assert!(auth.api().await.is_ok());
    assert!(store.bytes.lock().unwrap().is_some());
    let second = mock.client(store.clone());
    assert!(second.restore().await.unwrap());
    assert!(second.api().await.is_ok());
    second.disconnect().await.unwrap();
    assert!(store.bytes.lock().unwrap().is_none());
    assert!(second.api().await.is_err());
    let requests = mock.finish();
    assert_eq!(requests[0].0, "POST /device/code HTTP/1.1");
    assert_eq!(requests[0].1["device_name"], "Arinn");
    assert_eq!(requests[1].0, "POST /token HTTP/1.1");
    assert_eq!(requests[1].1["code"], "synthetic-device-code");
    assert_eq!(requests[1].1["grant_type"], "device_code");
    assert_eq!(requests[1].1["client_secret"], "synthetic-secret");
}

#[tokio::test]
async fn cancellation_discards_an_in_flight_success_and_a_new_attempt_invalidates_old_codes() {
    let mock = Mock::new(vec![
        reply(200, code()),
        (200, token().to_string(), Duration::from_millis(100)),
        reply(200, code()),
    ]);
    let store = MemoryStore::default();
    let auth = mock.client(store.clone());
    let mut attempt = auth.start().await.unwrap();
    attempt.interval = Duration::from_millis(1);
    let cancellation = async {
        tokio::time::timeout(Duration::from_secs(3), async {
            while mock.records.lock().unwrap().len() < 2 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        auth.cancel();
    };
    let (result, ()) = tokio::join!(auth.complete(attempt), cancellation);
    assert_eq!(result, Err(Error::Cancelled));
    assert!(store.bytes.lock().unwrap().is_none());
    let old = auth.start().await.unwrap();
    auth.cancel();
    assert_eq!(auth.complete(old).await, Err(Error::Cancelled));
    assert_eq!(mock.finish().len(), 3);
}

#[tokio::test]
async fn denial_expiry_and_slow_down_are_distinct_and_cancellable() {
    for (oauth, expected) in [
        ("access_denied", Error::Denied),
        ("expired_token", Error::Expired),
        ("invalid_client", Error::ClientRejected),
    ] {
        let mock = Mock::new(vec![
            reply(200, code()),
            reply(
                400,
                json!({"error":oauth, "error_description":"synthetic detail must not escape"}),
            ),
        ]);
        let auth = mock.client(MemoryStore::default());
        let mut attempt = auth.start().await.unwrap();
        attempt.interval = Duration::from_millis(1);
        assert_eq!(auth.complete(attempt).await, Err(expected));
        mock.finish();
    }
    let mock = Mock::new(vec![
        reply(200, code()),
        reply(400, json!({"error":"slow_down"})),
    ]);
    let auth = mock.client(MemoryStore::default());
    let mut attempt = auth.start().await.unwrap();
    attempt.interval = Duration::from_millis(1);
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        auth.cancel();
    };
    let (result, ()) = tokio::join!(auth.complete(attempt), cancel);
    assert_eq!(result, Err(Error::Cancelled));
    assert_eq!(mock.finish().len(), 2); // no immediate hammering after slow_down
}

#[tokio::test]
async fn a_cancel_during_secret_storage_cleans_up_the_late_token() {
    struct SlowStore {
        memory: MemoryStore,
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }
    impl CredentialStore for SlowStore {
        async fn load(&self) -> Result<Option<Credentials>, Error> {
            self.memory.load().await
        }
        async fn save(&self, c: &Credentials) -> Result<(), Error> {
            self.entered.notify_one();
            self.release.notified().await;
            self.memory.save(c).await
        }
        async fn clear(&self) -> Result<(), Error> {
            self.memory.clear().await
        }
    }
    let mock = Mock::new(vec![reply(200, code()), reply(200, token())]);
    let store = SlowStore {
        memory: MemoryStore::default(),
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    };
    let auth = mock.client(store);
    let mut attempt = auth.start().await.unwrap();
    attempt.interval = Duration::from_millis(1);
    let cancel = async {
        auth.session.store.entered.notified().await;
        auth.cancel();
        auth.session.store.release.notify_one();
    };
    let (result, ()) = tokio::join!(auth.complete(attempt), cancel);
    assert_eq!(result, Err(Error::Cancelled));
    assert!(auth.session.store.memory.bytes.lock().unwrap().is_none());
    assert!(auth.api().await.is_err());
    mock.finish();
}

#[tokio::test]
async fn expired_saved_tokens_and_locally_expired_codes_require_a_new_connection() {
    let store = MemoryStore::default();
    store
        .save(&Credentials {
            nonce: uuid::Uuid::new_v4().to_string(),
            access_token: "synthetic-expired".into(),
            expires_at: Some(1),
        })
        .await
        .unwrap();
    let auth = DeviceAuth::new("synthetic", None, "synthetic", store.clone()).unwrap();
    assert!(!auth.restore().await.unwrap());
    assert!(store.bytes.lock().unwrap().is_none());
    let mut attempt = parse_code(code(), *auth.session.generation.borrow()).unwrap();
    attempt.owner = auth.owner.clone();
    attempt.deadline = Instant::now();
    assert_eq!(auth.complete(attempt).await, Err(Error::Expired));
}

#[tokio::test]
async fn redirect_and_oversized_responses_never_produce_a_prompt() {
    for response in [
        (302, "{}".into(), Duration::ZERO),
        (200, "x".repeat(RESPONSE_LIMIT + 1), Duration::ZERO),
    ] {
        let mock = Mock::new(vec![response]);
        let auth = mock.client(MemoryStore::default());
        assert!(auth.start().await.is_err());
        mock.finish();
    }
}

#[tokio::test]
async fn short_device_verification_address_uses_the_canonical_prompt() {
    for address in ["https://oauth.yandex.ru/device", "https://ya.ru/device"] {
        let mut response = code();
        response["verification_url"] = json!(address);
        let mock = Mock::new(vec![reply(200, response)]);
        let store = MemoryStore::default();
        let auth = mock.client(store.clone());
        let attempt = auth.start().await.unwrap();
        assert_eq!(
            attempt.prompt.verification_url,
            "https://oauth.yandex.ru/device"
        );
        assert_eq!(attempt.prompt.user_code, "ABCD-EFGH");
        assert_eq!(attempt.prompt.expires_in, 300);
        assert!(store.bytes.lock().unwrap().is_none());
        auth.cancel();
        assert_eq!(auth.complete(attempt).await, Err(Error::Cancelled));
        let requests = mock.finish();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0, "POST /device/code HTTP/1.1");
    }
}

#[test]
fn only_the_exact_short_device_address_is_normalized() {
    for address in [
        "http://ya.ru/device",
        "https://ya.ru/",
        "https://ya.ru/other",
        "https://ya.ru/device/",
        "https://ya.ru/%64evice",
        "https://ya.ru/device?code=synthetic",
        "https://ya.ru/device?",
        "https://ya.ru/device#synthetic",
        "https://ya.ru/device#",
        "https://ya.ru:443/device",
        "https://ya.ru:444/device",
        "https://synthetic:secret@ya.ru/device",
        "https://ya.ru@evil.invalid/device",
        "https://ya.ru.evil.invalid/device",
        "https://sub.ya.ru/device",
    ] {
        let mut response = code();
        response["verification_url"] = json!(address);
        assert!(matches!(
            parse_code(response, 0),
            Err(Error::InvalidResponse)
        ));
    }
}

#[test]
fn prompt_validation_excludes_secrets_and_untrusted_verification_hosts() {
    let mut boundary = code();
    boundary["interval"] = json!(120);
    let mut attempt = parse_code(boundary, 0).unwrap();
    attempt.slow_down();
    assert_eq!(attempt.interval, Duration::from_secs(125));
    for url in [
        "http://oauth.yandex.ru/device",
        "https://oauth.yandex.ru.evil.invalid/device",
        "https://synthetic:secret@oauth.yandex.ru/device",
        "https://oauth.yandex.ru/device?token=synthetic",
        "https://oauth.yandex.ru:444/device",
    ] {
        let mut value = code();
        value["verification_url"] = json!(url);
        assert!(matches!(parse_code(value, 0), Err(Error::InvalidResponse)));
    }
    for (key, value) in [
        ("interval", json!(0)),
        ("expires_in", json!(1801)),
        ("device_code", json!("bad\ncode")),
        ("user_code", json!("<script>")),
    ] {
        let mut response = code();
        response[key] = value;
        assert!(parse_code(response, 0).is_err());
    }
    assert!(parse_token(&json!({"access_token":"synthetic", "expires_in":-1})).is_err());
    assert!(
        Credentials::decode(b"{\"access_token\":\"bad\\nvalue\",\"expires_at\":null}").is_err()
    );
}

#[tokio::test]
async fn attempts_cannot_cross_controller_identity_even_at_the_same_generation() {
    let mock = Mock::new(vec![reply(200, code()), reply(200, code())]);
    let a = mock.client(MemoryStore::default());
    let b = mock.client(MemoryStore::default());
    let attempt_a = a.start().await.unwrap();
    let attempt_b = b.start().await.unwrap();
    assert_eq!(attempt_a.generation, attempt_b.generation);
    assert_eq!(b.complete(attempt_a).await, Err(Error::InvalidInput));
    b.cancel();
    assert_eq!(b.complete(attempt_b).await, Err(Error::Cancelled));
    assert_eq!(mock.finish().len(), 2);
}

#[tokio::test]
async fn dropping_complete_does_not_abandon_an_already_written_credential() {
    struct WrittenButPending {
        memory: MemoryStore,
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }
    impl CredentialStore for WrittenButPending {
        async fn load(&self) -> Result<Option<Credentials>, Error> {
            self.memory.load().await
        }
        async fn save(&self, c: &Credentials) -> Result<(), Error> {
            self.memory.save(c).await?;
            self.entered.notify_one();
            self.release.notified().await;
            Ok(())
        }
        async fn clear(&self) -> Result<(), Error> {
            self.memory.clear().await
        }
    }
    let mock = Mock::new(vec![reply(200, code()), reply(200, token())]);
    let auth = Arc::new(mock.client(WrittenButPending {
        memory: MemoryStore::default(),
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    }));
    let mut attempt = auth.start().await.unwrap();
    attempt.interval = Duration::from_millis(1);
    let caller = {
        let auth = auth.clone();
        tokio::spawn(async move { auth.complete(attempt).await })
    };
    tokio::time::timeout(
        Duration::from_secs(3),
        auth.session.store.entered.notified(),
    )
    .await
    .unwrap();
    caller.abort();
    let _ = caller.await;
    auth.session.store.release.notify_one();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if auth.session.store.memory.bytes.lock().unwrap().is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    assert!(!auth.restore().await.unwrap());
    assert!(auth.api().await.is_err());
    mock.finish();
}

#[tokio::test]
async fn startup_client_rejection_is_explicit_and_parallel_generation_allocation_is_unique() {
    let mock = Mock::new(vec![reply(400, json!({"error":"invalid_client"}))]);
    let auth = Arc::new(mock.client(MemoryStore::default()));
    assert!(matches!(auth.start().await, Err(Error::ClientRejected)));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let auth = auth.clone();
            thread::spawn(move || (0..100).map(|_| auth.advance()).collect::<Vec<_>>())
        })
        .collect();
    let values: std::collections::HashSet<_> = threads
        .into_iter()
        .flat_map(|t| t.join().unwrap())
        .collect();
    assert_eq!(values.len(), 800);
    let latest = *values.iter().max().unwrap();
    assert!(auth.current(latest).is_ok());
    assert_eq!(auth.current(latest - 1), Err(Error::Cancelled));
    mock.finish();
}

#[tokio::test]
async fn committed_but_unacknowledged_and_dropped_leases_never_yield_an_api() {
    struct ReceiptStore {
        memory: MemoryStore,
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }
    impl CredentialStore for ReceiptStore {
        async fn load(&self) -> Result<Option<Credentials>, Error> {
            self.memory.load().await
        }
        async fn save(&self, c: &Credentials) -> Result<(), Error> {
            self.memory.save(c).await?;
            self.entered.notify_one();
            self.release.notified().await;
            Ok(())
        }
        async fn clear(&self) -> Result<(), Error> {
            self.memory.clear().await
        }
    }
    let mock = Mock::new(vec![reply(200, code()), reply(200, token())]);
    let auth = mock.client(ReceiptStore {
        memory: MemoryStore::default(),
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let mut attempt = auth.start().await.unwrap();
    attempt.interval = Duration::from_millis(1);
    let mut completion = Box::pin(auth.complete(attempt));
    tokio::select! {
        result=&mut completion=>panic!("completion returned before receipt: {result:?}"),
        _=auth.session.store.entered.notified()=>{},
    }
    auth.session.store.release.notify_one();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if auth.session.gate.lock().await.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    // The owned worker finished, but its caller has not consumed the receipt.
    assert!(auth.api().await.is_err());
    drop(completion);
    assert!(auth.api().await.is_err()); // before asynchronous cleanup can acquire gate
    assert!(!auth.restore().await.unwrap());
    mock.finish();
}

#[tokio::test]
async fn cancelling_before_a_reserved_start_is_polled_sends_no_request() {
    let mock = Mock::new(vec![]);
    let auth = mock.client(MemoryStore::default());
    let start = auth.prepare();
    auth.cancel();
    assert!(matches!(
        auth.start_prepared(start).await,
        Err(Error::Cancelled)
    ));
    assert!(mock.finish().is_empty());
}

#[tokio::test]
async fn startup_restore_reserved_before_connect_cannot_invalidate_that_connect() {
    let mock = Mock::new(vec![]);
    let auth = mock.client(MemoryStore::default());
    let startup = auth.prepare();
    let connect = auth.prepare();
    assert_eq!(auth.restore_prepared(startup).await, Err(Error::Cancelled));
    auth.current(connect.generation).unwrap();
    assert!(mock.finish().is_empty());
}
