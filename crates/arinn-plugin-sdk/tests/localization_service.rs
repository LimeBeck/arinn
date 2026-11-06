// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

mod common;
use arinn_plugin_sdk::runtime::{RegistrationContext, TextsHandler, serve_texts};
use arinn_protocol::{
    localization::{
        self, Catalog, ProviderTextsProxy, Reference, ResolvedText, Scope, TextRequest,
    },
    validation::ErrorCode,
};
use std::{
    collections::BTreeMap,
    future::Future,
    io::Write,
    pin::Pin,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, watch};

fn catalog() -> Catalog {
    Catalog::from_json(r#"{"formatVersion":1,"pluginId":"third-party-radio","sourceLocale":"en","locales":["ru"],"messages":{"stations.favorites":{"revision":1,"source":"Favorite stations"}}}"#).unwrap()
}
#[derive(Clone, Default)]
struct Handler {
    calls: Arc<AtomicUsize>,
    started: Arc<Notify>,
    release: Option<Arc<Notify>>,
    invalid_result: bool,
    invalid_catalog: bool,
    invalid_translation: bool,
    translation_bytes: usize,
}
impl TextsHandler for Handler {
    fn catalog(&self) -> Catalog {
        let mut c = catalog();
        if self.invalid_catalog {
            c.messages.get_mut("stations.favorites").unwrap().revision = 0;
        }
        c
    }
    fn translations(&self) -> BTreeMap<String, Vec<u8>> {
        // Opaque bytes test transport and exact-locale serving, not Qt loading.
        BTreeMap::from([(
            "ru".into(),
            if self.invalid_translation {
                vec![]
            } else if self.translation_bytes == 0 {
                b"opaque-catalog-fixture".to_vec()
            } else {
                vec![0; self.translation_bytes]
            },
        )])
    }
    fn resolve(
        &self,
        _generation: String,
        requests: Vec<TextRequest>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ResolvedText>, ErrorCode>> + Send + '_>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.notify_one();
        Box::pin(async move {
            if let Some(release) = &self.release {
                release.notified().await;
            }
            Ok(requests
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    r.scope == Scope::MusicEntity
                        && r.object_id == "favorites"
                        && r.revision == 7
                        && r.field == "title"
                        && r.fallback_text == "Favorite stations"
                })
                .map(|(i, r)| ResolvedText {
                    request_index: if self.invalid_result { 999 } else { i as u32 },
                    fallback_text: r.fallback_text.clone(),
                    message: Reference {
                        message_key: "stations.favorites".into(),
                        message_revision: 1,
                        arguments: BTreeMap::new(),
                        count: None,
                    },
                })
                .collect())
        })
    }
}
fn context(
    daemon: &zbus::Connection,
    provider: &zbus::Connection,
    generation: &str,
) -> RegistrationContext {
    RegistrationContext {
        connection: provider.clone(),
        daemon_owner: daemon.unique_name().unwrap().to_string(),
        daemon_epoch: "epoch".into(),
        generation: generation.into(),
    }
}
fn request() -> String {
    localization::requests_to_json(&[TextRequest {
        scope: Scope::MusicEntity,
        object_id: "favorites".into(),
        revision: 7,
        field: "title".into(),
        fallback_text: "Favorite stations".into(),
    }])
    .unwrap()
}
fn error(result: zbus::Result<impl std::fmt::Debug>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        other => format!("{other:?}"),
    }
}
async fn served(client: &zbus::Connection, server: &zbus::Connection) {
    let peer = zbus::fdo::PeerProxy::builder(client)
        .destination(server.unique_name().unwrap().to_string())
        .unwrap()
        .path("/")
        .unwrap()
        .build()
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !matches!(
            tokio::time::timeout(Duration::from_millis(100), peer.ping()).await,
            Ok(Ok(()))
        ) {}
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn fixed_text_is_additive_exact_locale_and_only_for_registered_daemon() {
    let bus = common::Bus::start();
    let daemon = bus.connection().await;
    let provider = bus.connection().await;
    let stranger = bus.connection().await;
    let (sender, receiver) = watch::channel(Some(context(&daemon, &provider, "generation")));
    let handler = Handler::default();
    serve_texts(&provider, handler.clone(), receiver.clone())
        .await
        .unwrap();
    assert!(
        serve_texts(&provider, handler.clone(), receiver)
            .await
            .is_err()
    );
    served(&daemon, &provider).await;
    let proxy = ProviderTextsProxy::builder(&daemon)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        Catalog::from_json(&proxy.get_catalog("generation").await.unwrap()).unwrap(),
        catalog()
    );
    assert_eq!(
        proxy.get_translation("generation", "ru").await.unwrap(),
        b"opaque-catalog-fixture"
    );
    assert_eq!(
        error(proxy.get_translation("generation", "ru-RU").await),
        "org.arinn.Error1.NotFound"
    );
    assert_eq!(
        error(proxy.get_translation("generation", "../ru").await),
        "org.arinn.Error1.InvalidArgument"
    );
    let original = localization::requests_from_json(&request()).unwrap();
    let resolved = proxy.resolve("generation", &request()).await.unwrap();
    assert_eq!(
        localization::resolved_from_json(&catalog(), &original, &resolved)
            .unwrap()
            .len(),
        1
    );
    let content = request().replace("Favorite stations", "My personal playlist");
    assert_eq!(proxy.resolve("generation", &content).await.unwrap(), "[]");
    // Identical visible text on an ordinary user playlist is not a fixed label.
    let content = request().replace("\"favorites\"", "\"user-playlist\"");
    assert_eq!(proxy.resolve("generation", &content).await.unwrap(), "[]");
    let stale_snapshot = request().replace("\"revision\":7", "\"revision\":8");
    assert_eq!(
        proxy.resolve("generation", &stale_snapshot).await.unwrap(),
        "[]"
    );
    let before = handler.calls.load(Ordering::SeqCst);
    for malformed in [
        "not json".into(),
        " ".repeat(localization::MAX_JSON_BYTES + 1),
    ] {
        assert_eq!(
            error(proxy.resolve("generation", &malformed).await),
            "org.arinn.Error1.InvalidArgument"
        );
    }
    assert_eq!(handler.calls.load(Ordering::SeqCst), before);
    let other = ProviderTextsProxy::builder(&stranger)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        error(other.get_catalog("generation").await),
        "org.arinn.Error1.PermissionDenied"
    );
    assert_eq!(
        error(other.get_translation("generation", "ru").await),
        "org.arinn.Error1.PermissionDenied"
    );
    assert_eq!(
        error(other.resolve("generation", &request()).await),
        "org.arinn.Error1.PermissionDenied"
    );
    assert_eq!(
        error(proxy.resolve("stale", &request()).await),
        "org.arinn.Error1.Conflict"
    );
    sender.send_replace(None);
    assert_eq!(
        error(proxy.get_catalog("generation").await),
        "org.arinn.Error1.Unavailable"
    );
    let live = zbus::fdo::IntrospectableProxy::builder(&daemon)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .path("/org/arinn/Provider1")
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    let expected = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/provider-texts1.xml"
    );
    let script = r#"import sys, xml.etree.ElementTree as E
name = 'org.arinn.ProviderTexts1'
actual = E.fromstring(sys.stdin.read()).find(f"interface[@name='{name}']")
expected = E.parse(sys.argv[1]).getroot().find(f"interface[@name='{name}']")
def signatures(node):
    return {m.attrib['name']: [(a.get('direction', 'in'), a.attrib['type'])
                              for a in m.findall('arg')]
            for m in node.findall('method')}
assert actual is not None and expected is not None
assert signatures(actual) == signatures(expected), 'ProviderTexts1 XML drift'
"#;
    let mut child = Command::new("python3")
        .args(["-c", script, expected])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(live.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
}

#[tokio::test]
async fn invalid_catalogs_and_handler_results_do_not_cross_the_interface() {
    let bus = common::Bus::start();
    let daemon = bus.connection().await;
    let provider = bus.connection().await;
    let (_, receiver) = watch::channel(Some(context(&daemon, &provider, "generation")));
    for handler in [
        Handler {
            invalid_catalog: true,
            ..Handler::default()
        },
        Handler {
            invalid_translation: true,
            ..Handler::default()
        },
        Handler {
            translation_bytes: localization::MAX_QM_BYTES + 1,
            ..Handler::default()
        },
    ] {
        assert!(
            serve_texts(&provider, handler, receiver.clone())
                .await
                .is_err()
        );
    }
    serve_texts(
        &provider,
        Handler {
            invalid_result: true,
            ..Handler::default()
        },
        receiver,
    )
    .await
    .unwrap();
    served(&daemon, &provider).await;
    let proxy = ProviderTextsProxy::builder(&daemon)
        .destination(provider.unique_name().unwrap().clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(
        error(proxy.resolve("generation", &request()).await),
        "org.arinn.Error1.InvalidArgument"
    );
}

#[tokio::test]
async fn a_registration_change_discards_an_in_flight_resolution() {
    let bus = common::Bus::start();
    let daemon = bus.connection().await;
    let provider = bus.connection().await;
    let (sender, receiver) = watch::channel(Some(context(&daemon, &provider, "old")));
    let handler = Handler {
        release: Some(Arc::new(Notify::new())),
        ..Handler::default()
    };
    serve_texts(&provider, handler.clone(), receiver)
        .await
        .unwrap();
    served(&daemon, &provider).await;
    let destination = provider.unique_name().unwrap().to_string();
    let task = tokio::spawn(async move {
        ProviderTextsProxy::builder(&daemon)
            .destination(destination)
            .unwrap()
            .build()
            .await
            .unwrap()
            .resolve("old", &request())
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), handler.started.notified())
        .await
        .unwrap();
    sender.send_modify(|c| c.as_mut().unwrap().generation = "new".into());
    handler.release.as_ref().unwrap().notify_one();
    assert_eq!(
        error(
            tokio::time::timeout(Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap()
        ),
        "org.arinn.Error1.Conflict"
    );
}
