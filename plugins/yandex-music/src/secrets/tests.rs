// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex as StdMutex},
};
use tokio::sync::Notify;

const SESSION: &str = "/org/freedesktop/secrets/session/test";
const ITEMS: [&str; 2] = [
    "/org/freedesktop/secrets/collection/foreign/one",
    "/org/freedesktop/secrets/collection/foreign/two",
];
fn path(s: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(s).unwrap()
}
struct Record {
    attrs: HashMap<String, String>,
    bytes: Vec<u8>,
}
#[derive(Default)]
struct State {
    records: HashMap<String, Record>,
    locked: bool,
    creates: u32,
    updates: u32,
    opened: u32,
    closed: u32,
    stall_get: bool,
}
struct Shared {
    state: StdMutex<State>,
    delay_create: std::sync::atomic::AtomicBool,
    entered: Notify,
    release: Notify,
}
impl Shared {
    fn new() -> Self {
        Self {
            state: StdMutex::new(State::default()),
            delay_create: std::sync::atomic::AtomicBool::new(false),
            entered: Notify::new(),
            release: Notify::new(),
        }
    }
}
struct Service(Arc<Shared>);
#[zbus::interface(name = "org.freedesktop.Secret.Service")]
impl Service {
    fn open_session(&self, algorithm: &str, _input: OwnedValue) -> (OwnedValue, OwnedObjectPath) {
        assert_eq!(algorithm, "plain");
        self.0.state.lock().unwrap().opened += 1;
        (OwnedValue::from(Str::from("")), path(SESSION))
    }
    fn search_items(
        &self,
        attributes: HashMap<String, String>,
    ) -> (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) {
        assert_eq!(attributes["arinn"], "music-yandex");
        assert_eq!(attributes["credential"], "synthetic-reference");
        let state = self.0.state.lock().unwrap();
        let items = state
            .records
            .iter()
            .filter(|(_, r)| attributes.iter().all(|(k, v)| r.attrs.get(k) == Some(v)))
            .map(|(k, _)| path(k))
            .collect();
        if state.locked {
            (vec![], items)
        } else {
            (items, vec![])
        }
    }
}
struct Collection(Arc<Shared>);
#[zbus::interface(name = "org.freedesktop.Secret.Collection")]
impl Collection {
    async fn create_item(
        &self,
        mut properties: HashMap<String, OwnedValue>,
        secret: Secret,
        replace: bool,
    ) -> (OwnedObjectPath, OwnedObjectPath) {
        assert!(replace);
        assert_eq!(secret.0.as_str(), SESSION);
        let attrs = HashMap::<String, String>::try_from(
            properties
                .remove("org.freedesktop.Secret.Item.Attributes")
                .unwrap(),
        )
        .unwrap();
        if self
            .0
            .delay_create
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            self.0.entered.notify_one();
            self.0.release.notified().await;
        }
        let mut state = self.0.state.lock().unwrap();
        if state.locked {
            return (path("/"), path("/synthetic/prompt"));
        }
        let name = ITEMS
            .iter()
            .find(|p| !state.records.contains_key(**p))
            .unwrap()
            .to_string();
        state.records.insert(
            name.clone(),
            Record {
                attrs,
                bytes: secret.2,
            },
        );
        state.creates += 1;
        (path(&name), path("/"))
    }
}
struct Item(Arc<Shared>, &'static str);
#[zbus::interface(name = "org.freedesktop.Secret.Item")]
impl Item {
    async fn get_secret(&self, session: OwnedObjectPath) -> Secret {
        let stall = self.0.state.lock().unwrap().stall_get;
        if stall {
            tokio::time::sleep(Duration::from_secs(4)).await;
        }
        let bytes = self.0.state.lock().unwrap().records[self.1].bytes.clone();
        Secret(session, vec![], bytes, "application/json".into())
    }
    fn set_secret(&self, secret: Secret) {
        let mut state = self.0.state.lock().unwrap();
        state.records.get_mut(self.1).unwrap().bytes = secret.2;
        state.updates += 1;
    }
    fn delete(&self) -> OwnedObjectPath {
        let mut state = self.0.state.lock().unwrap();
        if state.locked {
            return path("/synthetic/prompt");
        }
        state.records.remove(self.1);
        path("/")
    }
}
struct Session(Arc<Shared>);
#[zbus::interface(name = "org.freedesktop.Secret.Session")]
impl Session {
    fn close(&self) {
        self.0.state.lock().unwrap().closed += 1;
    }
}

struct Fixture {
    bus: Child,
    directory: std::path::PathBuf,
    _server: Connection,
    client: Connection,
    shared: Arc<Shared>,
}
impl Fixture {
    async fn new(shared: Shared) -> Self {
        let mut bus = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(bus.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let shared = Arc::new(shared);
        let server = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .name(SERVICE)
            .unwrap()
            .serve_at(PATH, Service(shared.clone()))
            .unwrap()
            .serve_at(
                "/org/freedesktop/secrets/aliases/default",
                Collection(shared.clone()),
            )
            .unwrap()
            .serve_at(SESSION, Session(shared.clone()))
            .unwrap()
            .serve_at(ITEMS[0], Item(shared.clone(), ITEMS[0]))
            .unwrap()
            .serve_at(ITEMS[1], Item(shared.clone(), ITEMS[1]))
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .method_timeout(Duration::from_secs(5))
            .build()
            .await
            .unwrap();
        client
            .call_method(
                Some(SERVICE),
                PATH,
                Some("org.freedesktop.DBus.Peer"),
                "Ping",
                &(),
            )
            .await
            .unwrap();
        let directory =
            std::env::temp_dir().join(format!("arinn-secret-fixture-{}", uuid::Uuid::new_v4()));
        Self {
            bus,
            directory,
            _server: server,
            client,
            shared,
        }
    }
    fn store(&self) -> SecretService {
        SecretService::new(self.client.clone(), "synthetic-reference", &self.directory).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.bus.kill();
        let _ = self.bus.wait();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
fn credentials() -> Credentials {
    Credentials::decode(&serde_json::to_vec(&serde_json::json!({"nonce":uuid::Uuid::new_v4().to_string(),"access_token":"synthetic-access-token","expires_at":null})).unwrap()).unwrap()
}

#[tokio::test]
async fn real_bus_roundtrip_updates_the_existing_foreign_collection_item_and_clears() {
    let f = Fixture::new(Shared::new()).await;
    let store = f.store();
    let token = credentials();
    store.save(&token).await.unwrap();
    assert!(store.load().await.unwrap().is_none()); // staged is not restart-approved
    store.commit(&token).unwrap();
    assert_eq!(
        store.load().await.unwrap().unwrap().encode().unwrap(),
        token.encode().unwrap()
    );
    store.save(&token).await.unwrap(); // item is deliberately in a foreign collection
    {
        let state = f.shared.state.lock().unwrap();
        assert_eq!(state.creates, 1);
        assert_eq!(state.updates, 1);
        assert_eq!(state.opened, state.closed);
    }
    let checkpoint = std::fs::read(f.directory.join("state.json")).unwrap();
    assert!(
        !String::from_utf8(checkpoint)
            .unwrap()
            .contains("synthetic-access-token")
    );
    store.clear().await.unwrap();
    assert!(store.load().await.unwrap().is_none());
    assert!(f.shared.state.lock().unwrap().records.is_empty());
}

#[tokio::test]
async fn locked_and_ambiguous_items_fail_closed_and_sessions_close_on_errors() {
    let f = Fixture::new(Shared::new()).await;
    let store = f.store();
    let token = credentials();
    f.shared.state.lock().unwrap().locked = true;
    assert_eq!(store.save(&token).await, Err(Error::Locked)); // collection prompt is not invoked
    {
        let state = f.shared.state.lock().unwrap();
        assert_eq!(state.opened, state.closed);
    }
    f.shared.state.lock().unwrap().locked = false;
    store.save(&token).await.unwrap();
    store.commit(&token).unwrap();
    f.shared.state.lock().unwrap().locked = true;
    assert!(matches!(store.load().await, Err(Error::Locked)));
    assert_eq!(store.clear().await, Err(Error::Locked));
    assert!(store.load().await.unwrap().is_none()); // checkpoint revoked even with locked keyring
    f.shared.state.lock().unwrap().locked = false;
    store.clear().await.unwrap();
    let token = credentials();
    store.save(&token).await.unwrap();
    store.commit(&token).unwrap();
    {
        let mut state = f.shared.state.lock().unwrap();
        state.records.insert(
            ITEMS[1].into(),
            Record {
                attrs: store.attributes(Some(token.nonce())),
                bytes: token.encode().unwrap(),
            },
        );
    }
    assert!(matches!(store.load().await, Err(Error::Storage)));
    assert_eq!(store.save(&token).await, Err(Error::Storage));
    store.clear().await.unwrap();
}

#[tokio::test]
async fn a_late_server_write_after_dropping_save_is_not_approved_on_restart() {
    let shared = Shared::new();
    shared
        .delay_create
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let f = Fixture::new(shared).await;
    let store = Arc::new(f.store());
    let token = credentials();
    let old_nonce = token.nonce().to_owned();
    let writer = {
        let store = store.clone();
        tokio::spawn(async move { store.save(&token).await })
    };
    tokio::time::timeout(Duration::from_secs(3), f.shared.entered.notified())
        .await
        .unwrap();
    writer.abort();
    let _ = writer.await;
    // A newer successful grant is approved before old A arrives remotely.
    let newer = credentials();
    store.save(&newer).await.unwrap();
    store.commit(&newer).unwrap();
    f.shared.release.notify_one();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if f.shared.state.lock().unwrap().records.len() == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    store.revoke(Some(&old_nonce)).unwrap(); // stale cleanup must preserve B
    drop(store);
    let restarted = f.store();
    assert_eq!(
        restarted.load().await.unwrap().unwrap().nonce(),
        newer.nonce()
    );
    restarted.clear().await.unwrap();
}

#[tokio::test]
async fn stalled_keyring_calls_have_deadlines_and_known_sessions_are_closed() {
    let f = Fixture::new(Shared::new()).await;
    let store = f.store();
    let token = credentials();
    store.save(&token).await.unwrap();
    store.commit(&token).unwrap();
    f.shared.state.lock().unwrap().stall_get = true;
    let begun = std::time::Instant::now();
    assert!(matches!(store.load().await, Err(Error::Storage)));
    assert!(begun.elapsed() < Duration::from_secs(4));
    let state = f.shared.state.lock().unwrap();
    assert_eq!(state.opened, state.closed);
}

#[tokio::test]
async fn a_timed_out_remote_create_cannot_authorize_a_restart_when_it_later_finishes() {
    let shared = Shared::new();
    shared
        .delay_create
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let f = Fixture::new(shared).await;
    let store = f.store();
    let token = credentials();
    assert_eq!(store.save(&token).await, Err(Error::Storage));
    f.shared.release.notify_one();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if !f.shared.state.lock().unwrap().records.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    drop(store);
    let restarted = f.store();
    assert!(restarted.load().await.unwrap().is_none());
    restarted.clear().await.unwrap();
}

#[tokio::test]
async fn revoking_a_staged_grant_prevents_a_worker_parked_before_activation_from_committing() {
    let f = Fixture::new(Shared::new()).await;
    let store = Arc::new(f.store());
    let token = credentials();
    let nonce = token.nonce().to_owned();
    store.save(&token).await.unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let worker = {
        let store = store.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            barrier.wait();
            store.commit(&token)
        })
    };
    barrier.wait(); // worker reached activation, but has not acquired approval lock
    store.revoke(Some(&nonce)).unwrap();
    barrier.wait();
    assert_eq!(worker.join().unwrap(), Err(Error::Cancelled));
    assert!(store.load().await.unwrap().is_none());
    drop(store);
    assert!(f.store().load().await.unwrap().is_none());
}
