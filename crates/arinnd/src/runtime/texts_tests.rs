// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::test_support::served;
use arinn_protocol::localization::{Message, Reference, ResolvedText, TextsProxy};
use arinn_protocol::music::{Entity, Page};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::Mutex as StdMutex,
};
use tokio::sync::Notify;

const PLUGIN: &str = "third.party/radio";
struct PlainMusic;
#[zbus::interface(name = "org.arinn.MusicProvider1")]
impl PlainMusic {
    async fn get_state(&self, _generation: &str) -> String {
        arinn_protocol::music::encode(&arinn_protocol::music::State::default()).unwrap()
    }
    async fn get_page(&self, generation: &str, _client: &str, _query: &str) -> String {
        arinn_protocol::music::encode(&page(generation, 7, &["fixed-section"])).unwrap()
    }
}
#[zbus::proxy(interface = "org.arinn.Music1", default_service = "org.arinn.Daemon1")]
trait PlainMusicApi {
    fn get_state(&self, generation: &str) -> zbus::Result<String>;
    fn get_page(&self, generation: &str, query: &str) -> zbus::Result<String>;
}
fn catalog() -> Catalog {
    Catalog {
        format_version: 1,
        plugin_id: PLUGIN.into(),
        source_locale: "en".into(),
        locales: vec!["de".into()],
        messages: BTreeMap::from([(
            "label".into(),
            Message {
                revision: 1,
                source: "Fixed section".into(),
                arguments: BTreeMap::new(),
                numerus: false,
                comment: String::new(),
            },
        )]),
    }
}
fn request(id: &str, revision: u64) -> TextRequest {
    TextRequest {
        scope: Scope::MusicEntity,
        object_id: id.into(),
        revision,
        field: "title".into(),
        fallback_text: "Fixed section".into(),
    }
}
fn page(generation: &str, revision: u64, ids: &[&str]) -> Page {
    Page {
        generation: generation.into(),
        revision,
        entries: ids
            .iter()
            .map(|id| Entity {
                id: (*id).into(),
                title: "Fixed section".into(),
                kind: "playlist".into(),
                available: true,
                ..Entity::default()
            })
            .collect(),
        next_token: String::new(),
    }
}
struct Data {
    catalog: String,
    qm: Vec<u8>,
    reply: Option<String>,
    block: &'static str,
    calls: Vec<(String, String, String)>,
}
#[derive(Clone)]
struct RawProvider {
    data: Arc<StdMutex<Data>>,
    entered: Arc<Notify>,
    release: Arc<Notify>,
}
impl RawProvider {
    async fn wait(&self, method: &str) {
        let blocked = self.data.lock().unwrap().block == method;
        if blocked {
            self.entered.notify_one();
            self.release.notified().await;
        }
    }
    fn record(&self, method: &str, generation: &str, header: &Header<'_>) {
        self.data.lock().unwrap().calls.push((
            method.into(),
            generation.into(),
            sender(header).unwrap(),
        ));
    }
}
#[zbus::interface(name = "org.arinn.ProviderTexts1")]
impl RawProvider {
    async fn get_catalog(&self, generation: &str, #[zbus(header)] header: Header<'_>) -> String {
        self.record("catalog", generation, &header);
        let json = self.data.lock().unwrap().catalog.clone();
        self.wait("catalog").await;
        json
    }
    async fn get_translation(
        &self,
        generation: &str,
        locale: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> Vec<u8> {
        self.record(locale, generation, &header);
        let bytes = self.data.lock().unwrap().qm.clone();
        self.wait("translation").await;
        bytes
    }
    async fn resolve(
        &self,
        generation: &str,
        requests: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> String {
        self.record("resolve", generation, &header);
        let raw = self.data.lock().unwrap().reply.clone();
        let json = raw.unwrap_or_else(|| {
            let requests = localization::requests_from_json(requests).unwrap();
            let results: Vec<_> = requests
                .iter()
                .enumerate()
                .filter(|(_, request)| {
                    request.object_id.starts_with("fixed") || request.scope == Scope::Provider
                })
                .map(|(index, request)| ResolvedText {
                    request_index: index as u32,
                    fallback_text: request.fallback_text.clone(),
                    message: Reference {
                        message_key: "label".into(),
                        message_revision: 1,
                        arguments: BTreeMap::new(),
                        count: None,
                    },
                })
                .collect();
            localization::resolved_to_json(&catalog(), &requests, &results).unwrap()
        });
        self.wait("resolve").await;
        json
    }
}

struct Fixture {
    bus: Child,
    folder: PathBuf,
    core: Core,
    client: Connection,
    provider: Connection,
    raw: RawProvider,
    generation: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.bus.kill();
        let _ = self.bus.wait();
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}
fn provider_record(owner: &Connection, generation: &str, folder: &std::path::Path) -> Provider {
    Provider {
        info: ProviderInfo {
            id: "radio".into(),
            plugin_id: PLUGIN.into(),
            source_path: String::new(),
            name: "Example provider".into(),
            version: "1".into(),
            path: "/org/arinn/Daemon1/providers/radio".try_into().unwrap(),
            capabilities: vec!["music".into()],
            runtime_state: "ready".into(),
            backend_state: "ready".into(),
            generation: generation.into(),
            revision: 1,
            last_error: String::new(),
            error_at: 0,
            recovered_at: 0,
            restarts_since_daemon_start: 0,
            cumulative_restart_count: 0,
        },
        manifest: None,
        unit: "synthetic.service".into(),
        token_file: folder.join("token"),
        token: String::new(),
        owner: owner.unique_name().unwrap().to_string(),
        cache: None,
        applications: vec![],
        running: vec![],
        tab_cache: None,
        windows: vec![],
        tabs: vec![],
        session_cache: None,
        sessions: vec![],
        features: vec![],
        library_cache: None,
        collections: vec![],
        items: vec![],
        resume_cache: None,
        resumes: vec![],
        settings_schema: None,
        started: Instant::now(),
        enabled: true,
    }
}
impl Fixture {
    async fn new(export: bool) -> Self {
        let mut bus = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(bus.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        async fn connect(address: &str) -> Connection {
            zbus::connection::Builder::address(address.trim())
                .unwrap()
                .build()
                .await
                .unwrap()
        }
        let server = connect(&address).await;
        server.request_name(production::SERVICE).await.unwrap();
        let client = connect(&address).await;
        let provider = connect(&address).await;
        let folder = std::env::temp_dir().join(format!("arinn-texts-{}", Uuid::new_v4()));
        std::fs::create_dir(&folder).unwrap();
        let core = Core::new(
            &server,
            Paths {
                config: folder.join("config"),
                data: folder.join("data"),
                state: folder.join("state"),
            },
            folder.clone(),
        );
        core.export_texts().await.unwrap();
        let generation = Uuid::new_v4().to_string();
        let raw = RawProvider {
            data: Arc::new(StdMutex::new(Data {
                catalog: catalog().to_json().unwrap(),
                qm: b"opaque QM bytes".to_vec(),
                reply: None,
                block: "",
                calls: vec![],
            })),
            entered: Arc::default(),
            release: Arc::default(),
        };
        if export {
            provider
                .object_server()
                .at(production::PROVIDER_PATH, raw.clone())
                .await
                .unwrap();
        } else {
            let _ = provider.object_server();
        }
        core.state.lock().await.providers.insert(
            "radio".into(),
            provider_record(&provider, &generation, &folder),
        );
        served(&client, &server).await;
        served(&client, &provider).await;
        Self {
            bus,
            folder,
            core,
            client,
            provider,
            raw,
            generation,
        }
    }
    async fn observe(&self, revision: u64, ids: &[&str]) {
        self.core
            .remember_text_page(
                "radio",
                &self.generation,
                self.provider.unique_name().unwrap().as_str(),
                &page(&self.generation, revision, ids),
            )
            .await
            .unwrap();
    }
    async fn api(&self) -> TextsProxy<'_> {
        TextsProxy::new(&self.client).await.unwrap()
    }
}
fn error<T>(result: zbus::Result<T>) -> String {
    match result {
        Err(zbus::Error::MethodError(name, _, _)) => name.to_string(),
        _ => panic!("expected a D-Bus method error"),
    }
}

#[tokio::test]
async fn optional_observation_preserves_plain_music_reads_before_the_first_poll() {
    let f = Fixture::new(false).await;
    f.provider
        .object_server()
        .at(production::PROVIDER_PATH, PlainMusic)
        .await
        .unwrap();
    let path = {
        let mut state = f.core.state.lock().await;
        let provider = state.providers.get_mut("radio").unwrap();
        provider.info.runtime_state = "starting".into();
        provider.info.path.clone()
    };
    f.core
        .connection
        .object_server()
        .at(
            path.clone(),
            music_routing::MusicEndpoint {
                core: f.core.clone(),
                id: "radio".into(),
            },
        )
        .await
        .unwrap();
    let api = PlainMusicApiProxy::builder(&f.client)
        .path(path)
        .unwrap()
        .build()
        .await
        .unwrap();
    assert!(api.get_state(&f.generation).await.is_ok());
    let query = arinn_protocol::music::Query {
        facet: "playlists".into(),
        parent: String::new(),
        search: String::new(),
        token: String::new(),
    };
    let reply = api
        .get_page(
            &f.generation,
            &arinn_protocol::music::encode(&query).unwrap(),
        )
        .await
        .unwrap();
    let observed: Page = arinn_protocol::music::decode(&reply).unwrap();
    assert_eq!(observed.entries[0].title, "Fixed section");
    assert_eq!(
        error(f.api().await.get_catalog("radio").await),
        "org.arinn.Error1.Unavailable"
    );
}

#[tokio::test]
async fn catalogues_and_exact_locale_bytes_use_only_the_registered_owner() {
    let f = Fixture::new(true).await;
    let api = f.api().await;
    for _ in 0..2 {
        let (generation, json) = api.get_catalog("radio").await.unwrap();
        assert_eq!(generation, f.generation);
        assert_eq!(Catalog::from_json(&json).unwrap(), catalog());
    }
    assert_eq!(
        api.get_translation("radio", &f.generation, "de")
            .await
            .unwrap(),
        b"opaque QM bytes"
    );
    assert_eq!(
        error(api.get_translation("radio", &f.generation, "de-DE").await),
        "org.arinn.Error1.NotFound"
    );
    assert_eq!(
        error(api.get_translation("radio", &f.generation, "../de").await),
        "org.arinn.Error1.InvalidArgument"
    );
    let data = f.raw.data.lock().unwrap();
    assert_eq!(
        data.calls
            .iter()
            .filter(|(method, _, _)| method == "catalog")
            .count(),
        1
    );
    assert!(
        data.calls
            .iter()
            .all(|(_, generation, owner)| generation == &f.generation
                && owner == f.core.connection.unique_name().unwrap().as_str())
    );
}

#[tokio::test]
async fn associations_omit_content_and_rebind_filtered_indices() {
    let f = Fixture::new(true).await;
    f.observe(7, &["fixed-section", "content-playlist"]).await;
    let requests = vec![
        request("unknown", 7),
        request("fixed-section", 7),
        request("content-playlist", 7),
        request("fixed-section", 6),
    ];
    let json = f
        .api()
        .await
        .resolve(
            "radio",
            &f.generation,
            &localization::requests_to_json(&requests).unwrap(),
        )
        .await
        .unwrap();
    let resolved = localization::resolved_from_json(&catalog(), &requests, &json).unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].request_index, 1);
    assert_eq!(resolved[0].fallback_text, "Fixed section");
}

#[tokio::test]
async fn missing_optional_interface_and_stale_generation_keep_plain_data_usable() {
    let f = Fixture::new(false).await;
    assert_eq!(
        error(f.api().await.get_catalog("radio").await),
        "org.arinn.Error1.Unsupported"
    );
    assert_eq!(
        error(f.api().await.resolve("radio", "old", "[]").await),
        "org.arinn.Error1.Conflict"
    );
    assert_eq!(
        f.core.state.lock().await.providers["radio"].info.name,
        "Example provider"
    );
    f.provider
        .object_server()
        .at(production::PROVIDER_PATH, f.raw.clone())
        .await
        .unwrap();
    assert_eq!(
        error(f.api().await.get_catalog("radio").await),
        "org.arinn.Error1.Unsupported"
    );
    f.core
        .state
        .lock()
        .await
        .providers
        .get_mut("radio")
        .unwrap()
        .info
        .generation = Uuid::new_v4().to_string();
    assert!(f.api().await.get_catalog("radio").await.is_ok());
}

#[tokio::test]
async fn blocked_localization_is_bounded_without_blocking_plain_provider_state() {
    let f = Fixture::new(true).await;
    f.raw.data.lock().unwrap().block = "catalog";
    let mut calls = Vec::new();
    for _ in 0..16 {
        let client = f.client.clone();
        calls.push(tokio::spawn(async move {
            TextsProxy::new(&client)
                .await
                .unwrap()
                .get_catalog("radio")
                .await
        }));
    }
    tokio::time::timeout(Duration::from_secs(1), async {
        while f.core.texts.slots.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        error(f.api().await.get_catalog("radio").await),
        "org.arinn.Error1.Unavailable"
    );
    assert_eq!(
        f.core.state.lock().await.providers["radio"]
            .info
            .runtime_state,
        "ready"
    );
    for call in calls {
        assert_eq!(error(call.await.unwrap()), "org.arinn.Error1.Timeout");
    }
    assert_eq!(f.core.texts.slots.available_permits(), 16);
}

#[tokio::test]
async fn endpoint_deadline_also_bounds_waiting_for_internal_locks() {
    let f = Fixture::new(true).await;
    let _guard = f.core.state.lock().await;
    assert_eq!(
        error(f.api().await.get_catalog("radio").await),
        "org.arinn.Error1.Timeout"
    );
    assert!(f.raw.data.lock().unwrap().calls.is_empty());
}

#[tokio::test]
async fn owner_changes_during_translation_or_resolution_reject_the_reply() {
    for method in ["translation", "resolve"] {
        let f = Fixture::new(true).await;
        f.observe(7, &["fixed-section"]).await;
        f.api().await.get_catalog("radio").await.unwrap();
        f.raw.data.lock().unwrap().block = method;
        let payload = localization::requests_to_json(&[request("fixed-section", 7)]).unwrap();
        let call = async {
            let api = f.api().await;
            if method == "translation" {
                error(api.get_translation("radio", &f.generation, "de").await)
            } else {
                error(api.resolve("radio", &f.generation, &payload).await)
            }
        };
        let replace = async {
            f.raw.entered.notified().await;
            f.core
                .state
                .lock()
                .await
                .providers
                .get_mut("radio")
                .unwrap()
                .owner = f.client.unique_name().unwrap().to_string();
            f.raw.release.notify_one();
        };
        let (result, _) = tokio::join!(call, replace);
        assert_eq!(result, "org.arinn.Error1.Conflict");
    }
}

#[tokio::test]
async fn station_associations_use_the_state_revision_beside_retained_pages() {
    let f = Fixture::new(true).await;
    f.observe(7, &["fixed-section"]).await;
    let station = page(&f.generation, 7, &["fixed-station"]).entries.remove(0);
    f.core
        .music
        .cache(
            "radio",
            &f.generation,
            f.provider.unique_name().unwrap().as_str(),
            arinn_protocol::music::State {
                revision: 40,
                catalogue_revision: 8,
                station: Some(station),
                ..Default::default()
            },
        )
        .await;
    let mut station_request = request("fixed-station", 40);
    station_request.scope = Scope::MusicStation;
    let requests = vec![request("fixed-section", 7), station_request];
    let reply = f
        .api()
        .await
        .resolve(
            "radio",
            &f.generation,
            &localization::requests_to_json(&requests).unwrap(),
        )
        .await
        .unwrap();
    let resolved = localization::resolved_from_json(&catalog(), &requests, &reply).unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].request_index, 0);
    assert_eq!(resolved[1].request_index, 1);
}

#[tokio::test]
async fn retained_facet_proofs_survive_newer_pages_but_not_eviction_or_ambiguity_during_resolve() {
    for mode in ["new-facet", "evicted", "ambiguous"] {
        let f = Fixture::new(true).await;
        f.observe(7, &["fixed-section"]).await;
        f.api().await.get_catalog("radio").await.unwrap();
        f.raw.data.lock().unwrap().block = "resolve";
        let requests = vec![request("fixed-section", 7)];
        let payload = localization::requests_to_json(&requests).unwrap();
        let call = async {
            f.api()
                .await
                .resolve("radio", &f.generation, &payload)
                .await
                .unwrap()
        };
        let update = async {
            f.raw.entered.notified().await;
            f.observe(12, &["content-track"]).await;
            f.core
                .music
                .cache(
                    "radio",
                    &f.generation,
                    f.provider.unique_name().unwrap().as_str(),
                    arinn_protocol::music::State {
                        revision: 40,
                        catalogue_revision: 12,
                        ..Default::default()
                    },
                )
                .await;
            if mode == "evicted" {
                for index in 0..MAX_PAGE_FIELDS + 1 {
                    f.observe(12, &[&format!("content-{index}")]).await;
                }
            } else if mode == "ambiguous" {
                let mut changed = page(&f.generation, 7, &["fixed-section"]);
                changed.entries[0].title = "Changed under the same revision".into();
                f.core
                    .remember_text_page(
                        "radio",
                        &f.generation,
                        f.provider.unique_name().unwrap().as_str(),
                        &changed,
                    )
                    .await
                    .unwrap();
            }
            f.raw.release.notify_one();
        };
        let (reply, _) = tokio::join!(call, update);
        let resolved = localization::resolved_from_json(&catalog(), &requests, &reply).unwrap();
        assert_eq!(resolved.len(), usize::from(mode == "new-facet"), "{mode}");
    }
}

#[tokio::test]
async fn media_titles_bind_to_the_current_session_publication() {
    let f = Fixture::new(true).await;
    {
        let mut state = f.core.state.lock().await;
        let provider = state.providers.get_mut("radio").unwrap();
        provider.info.capabilities = vec!["media".into()];
        provider.info.revision = 9;
        provider.sessions = vec![arinn_protocol::media::Session {
            id: "fixed-player".into(),
            title: "Fixed section".into(),
            subtitle: String::new(),
            artwork: String::new(),
            state: "stopped".into(),
            duration_us: -1,
            position_us: 0,
            anchor_us: 0,
            actions: vec![],
            has_content: false,
            content: arinn_protocol::production::EntityRef {
                provider_id: String::new(),
                capability: String::new(),
                local_id: String::new(),
            },
        }];
    }
    let mut current = request("fixed-player", 9);
    current.scope = Scope::MediaSession;
    let mut old = current.clone();
    old.revision = 8;
    let mut changed = current.clone();
    changed.fallback_text = "Different title".into();
    let requests = vec![old, current];
    let reply = f
        .api()
        .await
        .resolve(
            "radio",
            &f.generation,
            &localization::requests_to_json(&requests).unwrap(),
        )
        .await
        .unwrap();
    let resolved = localization::resolved_from_json(&catalog(), &requests, &reply).unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].request_index, 1);
    assert_eq!(
        f.api()
            .await
            .resolve(
                "radio",
                &f.generation,
                &localization::requests_to_json(&[changed]).unwrap()
            )
            .await
            .unwrap(),
        "[]"
    );
}

#[tokio::test]
async fn owner_or_generation_changes_during_catalogue_fetch_reject_the_reply() {
    for owner_change in [false, true] {
        let f = Fixture::new(true).await;
        f.raw.data.lock().unwrap().block = "catalog";
        let call = async { f.api().await.get_catalog("radio").await };
        let replace = async {
            f.raw.entered.notified().await;
            let mut state = f.core.state.lock().await;
            let provider = state.providers.get_mut("radio").unwrap();
            if owner_change {
                provider.owner = f.client.unique_name().unwrap().to_string();
            } else {
                provider.info.generation = Uuid::new_v4().to_string();
            }
            drop(state);
            f.raw.release.notify_one();
        };
        let (result, _) = tokio::join!(call, replace);
        assert_eq!(error(result), "org.arinn.Error1.Conflict");
    }
}

#[tokio::test]
async fn same_text_new_snapshot_expires_an_awaited_reference() {
    let f = Fixture::new(true).await;
    f.observe(7, &["fixed-section"]).await;
    f.raw.data.lock().unwrap().block = "resolve";
    let requests = vec![request("fixed-section", 7)];
    let payload = localization::requests_to_json(&requests).unwrap();
    let call = async {
        f.api()
            .await
            .resolve("radio", &f.generation, &payload)
            .await
    };
    let update = async {
        f.raw.entered.notified().await;
        f.observe(8, &["fixed-section"]).await;
        f.raw.release.notify_one();
    };
    let (result, _) = tokio::join!(call, update);
    assert_eq!(result.unwrap(), "[]");
}

#[tokio::test]
async fn malformed_or_impersonating_catalogues_are_not_cached_as_valid() {
    for json in [
        "{}".to_owned(),
        catalog()
            .to_json()
            .unwrap()
            .replace(PLUGIN, "another-plugin"),
    ] {
        let f = Fixture::new(true).await;
        f.raw.data.lock().unwrap().catalog = json;
        assert_eq!(
            error(f.api().await.get_catalog("radio").await),
            "org.arinn.Error1.Unavailable"
        );
        f.raw.data.lock().unwrap().catalog = catalog().to_json().unwrap();
        assert!(f.api().await.get_catalog("radio").await.is_ok());
    }
}

#[tokio::test]
async fn invalid_caller_payloads_do_not_reach_the_provider_and_invalid_replies_do_not_reach_the_client()
 {
    let f = Fixture::new(true).await;
    let api = f.api().await;
    assert_eq!(
        error(api.resolve("radio", &f.generation, "{} ").await),
        "org.arinn.Error1.InvalidArgument"
    );
    let oversized = format!("[]{}", " ".repeat(localization::MAX_JSON_BYTES));
    assert_eq!(
        error(api.resolve("radio", &f.generation, &oversized).await),
        "org.arinn.Error1.InvalidArgument"
    );
    assert!(f.raw.data.lock().unwrap().calls.is_empty());
    f.observe(7, &["fixed-section"]).await;
    let requests = vec![request("fixed-section", 7)];
    let payload = localization::requests_to_json(&requests).unwrap();
    for reply in [
        "{}",
        r#"[{"requestIndex":99,"fallbackText":"Fixed section","message":{"messageKey":"label","messageRevision":1}}]"#,
        r#"[{"requestIndex":0,"fallbackText":"Wrong field","message":{"messageKey":"label","messageRevision":1}}]"#,
    ] {
        f.raw.data.lock().unwrap().reply = Some(reply.into());
        assert_eq!(
            error(api.resolve("radio", &f.generation, &payload).await),
            "org.arinn.Error1.Unavailable"
        );
    }
}

#[tokio::test]
async fn changed_or_oversized_qm_is_rejected_without_affecting_the_provider() {
    let f = Fixture::new(true).await;
    let api = f.api().await;
    assert!(
        api.get_translation("radio", &f.generation, "de")
            .await
            .is_ok()
    );
    f.raw.data.lock().unwrap().qm = b"changed bytes".to_vec();
    assert_eq!(
        error(api.get_translation("radio", &f.generation, "de").await),
        "org.arinn.Error1.Unavailable"
    );
    f.raw.data.lock().unwrap().qm = vec![0; localization::MAX_QM_BYTES + 1];
    assert_eq!(
        error(api.get_translation("radio", &f.generation, "de").await),
        "org.arinn.Error1.Unavailable"
    );
    assert_eq!(
        f.core.state.lock().await.providers["radio"]
            .info
            .runtime_state,
        "ready"
    );
}

#[test]
fn observed_page_fields_are_bounded_and_revision_reuse_does_not_rebind_text() {
    let mut fields = PageFields::default();
    fields.observe(&page("generation", 7, &["fixed-one"]));
    assert!(fields.matches(&request("fixed-one", 7)));
    fields.observe(&page("generation", 12, &["fixed-other"]));
    assert!(fields.matches(&request("fixed-one", 7)));
    assert!(fields.matches(&request("fixed-other", 12)));
    fields.observe(&page("generation", 6, &["fixed-older-facet"]));
    assert!(fields.matches(&request("fixed-older-facet", 6)));
    let mut changed = page("generation", 7, &["fixed-one"]);
    changed.entries[0].title = "Different text".into();
    fields.observe(&changed);
    assert!(!fields.matches(&request("fixed-one", 7)));
    for index in 0..MAX_PAGE_FIELDS + 5 {
        fields.observe(&page("generation", 8, &[&format!("fixed-{index}")]));
    }
    assert!(fields.fields.len() <= MAX_PAGE_FIELDS);
    assert!(fields.bytes <= MAX_PAGE_TEXT_BYTES);
    assert!(!fields.matches(&request("fixed-one", 7)));
}

#[tokio::test]
async fn aggregate_translation_budget_is_bounded_per_registration() {
    let f = Fixture::new(true).await;
    let mut source = catalog();
    source.locales = (0..40)
        .map(|index| {
            format!(
                "q{}{}",
                char::from(b'a' + index / 26),
                char::from(b'a' + index % 26)
            )
        })
        .collect();
    {
        let mut data = f.raw.data.lock().unwrap();
        data.catalog = source.to_json().unwrap();
        data.qm = vec![0; localization::MAX_QM_BYTES];
    }
    let api = f.api().await;
    let fitting = localization::MAX_TOTAL_QM_BYTES / localization::MAX_QM_BYTES;
    for locale in &source.locales[..fitting] {
        assert!(
            api.get_translation("radio", &f.generation, locale)
                .await
                .is_ok()
        );
    }
    assert_eq!(
        error(
            api.get_translation("radio", &f.generation, &source.locales[fitting])
                .await
        ),
        "org.arinn.Error1.Unavailable"
    );
    // An already counted exact locale still works after rejecting a new one.
    assert!(
        api.get_translation("radio", &f.generation, &source.locales[0])
            .await
            .is_ok()
    );
}
