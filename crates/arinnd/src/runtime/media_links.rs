// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `org.arinn.MediaAssociation1` (ADR-0096): links a browser's media session
//! to its one audible tab by process identity. Process IDs are read from the
//! providers' optional interfaces, kept in memory only and never logged.
use super::Core;
use arinn_protocol::{
    media_association::{
        self, BrowserProcessProxy, HeldLink, MediaAssociation, MediaProcessProxy, ProcessSession,
        ProcessTab, ProcessWindow,
    },
    production,
    validation::ErrorCode,
};
use futures_util::StreamExt;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;
use zbus::object_server::SignalEmitter;

const POLL: Duration = Duration::from_secs(1);
const CALL_TIMEOUT: Duration = Duration::from_secs(2);
/// Process lookups per pass; the rest wait for the next pass.
const MAX_LOOKUPS: usize = 64;
/// Lookups in flight at once, so a pass stays within a few call timeouts.
const LOOKUP_CONCURRENCY: usize = 16;

struct Published {
    revision: u64,
    /// Links with the process each was verified with; only `link` is read.
    links: Vec<HeldLink>,
    /// The revision cannot grow any more: publish nothing.
    exhausted: bool,
}

#[derive(Clone)]
pub(super) struct MediaLinks {
    epoch: String,
    published: Arc<Mutex<Published>>,
}
impl MediaLinks {
    fn new(epoch: &str) -> Self {
        Self {
            epoch: epoch.into(),
            published: Arc::new(Mutex::new(Published {
                revision: 1,
                links: Vec::new(),
                exhausted: false,
            })),
        }
    }
    async fn links(&self) -> Vec<HeldLink> {
        self.published.lock().await.links.clone()
    }
    /// Replaces the links; returns the new revision when they changed.
    async fn replace(&self, links: Vec<HeldLink>) -> Option<u64> {
        let mut published = self.published.lock().await;
        if published.exhausted || published.links == links {
            return None;
        }
        match published.revision.checked_add(1) {
            Some(revision) => {
                published.revision = revision;
                published.links = links;
                Some(revision)
            }
            None => {
                published.exhausted = true;
                published.links.clear();
                None
            }
        }
    }
}
#[zbus::interface(name = "org.arinn.MediaAssociation1")]
impl MediaLinks {
    async fn get(&self) -> production::Result<(MediaAssociation,)> {
        let published = self.published.lock().await;
        let snapshot = MediaAssociation {
            epoch: self.epoch.clone(),
            revision: published.revision,
            links: published.links.iter().map(|h| h.link.clone()).collect(),
        };
        snapshot.validate().map_err(|_| ErrorCode::Internal)?;
        Ok((snapshot,))
    }
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>, epoch: &str, revision: u64) -> zbus::Result<()>;
}

/// (provider ID, owner, generation): one registration of a provider.
type Registration = (String, String, String);
/// (provider ID, generation, snapshot revision, local ID): one process ID
/// read. A new snapshot revision re-reads it, because a provider may reuse a
/// local ID for another process (ADR-0096).
type Key = (String, String, u64, String);

struct MediaSource {
    registration: Registration,
    revision: u64,
    sessions: Vec<String>,
}
struct BrowserSource {
    registration: Registration,
    revision: u64,
    windows: Vec<String>,
    /// (tab ID, window ID, audible)
    tabs: Vec<(String, String, bool)>,
}

/// What one pass learned; kept between passes.
#[derive(Default)]
pub(super) struct Cache {
    /// Whether a registration exports the optional interface.
    support: BTreeMap<Registration, bool>,
    processes: BTreeMap<Key, u32>,
    /// The pass that last asked for each missing process ID.
    tried: BTreeMap<Key, u64>,
    pass: u64,
}

/// One process ID to read.
enum Lookup {
    Media(Registration, String),
    Browser(Registration, String),
}

impl Core {
    /// Export `MediaAssociation1` at the daemon root and keep it current.
    pub async fn export_media_association(&self) -> Result<(), ErrorCode> {
        let links = MediaLinks::new(&self.epoch);
        self.connection
            .object_server()
            .at(production::PATH, links.clone())
            .await
            .map_err(|_| ErrorCode::Internal)?;
        let core = self.clone();
        tokio::spawn(async move {
            let mut cache = Cache::default();
            let mut tick = tokio::time::interval(POLL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                core.associate(&links, &mut cache).await;
            }
        });
        Ok(())
    }

    /// One pass: read current snapshots, resolve missing process IDs, join
    /// and publish. Never holds the state lock across a provider call.
    pub(super) async fn associate(&self, links: &MediaLinks, cache: &mut Cache) {
        let (media, browsers) = self.sources().await;
        let live: BTreeSet<&Registration> = media
            .iter()
            .map(|m| &m.registration)
            .chain(browsers.iter().map(|b| &b.registration))
            .collect();
        cache.support.retain(|r, _| live.contains(r));
        let mut current: BTreeSet<Key> = BTreeSet::new();
        for m in &media {
            let (id, _, generation) = &m.registration;
            current.extend(
                m.sessions
                    .iter()
                    .map(|s| (id.clone(), generation.clone(), m.revision, s.clone())),
            );
        }
        for b in &browsers {
            let (id, _, generation) = &b.registration;
            current.extend(
                b.windows
                    .iter()
                    .map(|w| (id.clone(), generation.clone(), b.revision, w.clone())),
            );
        }
        cache.processes.retain(|k, _| current.contains(k));

        // Which registrations export their interface. One that cannot be
        // asked yet stays unknown, and a pass with an unknown one makes no
        // link: its sessions or windows could share a process.
        for m in &media {
            self.supports(
                cache,
                &m.registration,
                media_association::MEDIA_PROCESS_INTERFACE,
            )
            .await;
        }
        for b in &browsers {
            self.supports(
                cache,
                &b.registration,
                media_association::BROWSER_PROCESS_INTERFACE,
            )
            .await;
        }
        let supported = |r: &Registration| cache.support.get(r).copied() == Some(true);
        // Missing process IDs, least recently tried first, so lookups that
        // keep failing cannot starve the rest. At most MAX_LOOKUPS per pass,
        // LOOKUP_CONCURRENCY at a time, each within CALL_TIMEOUT.
        let mut missing: Vec<(Key, Lookup)> = Vec::new();
        for m in media.iter().filter(|m| supported(&m.registration)) {
            for session in &m.sessions {
                let key = (
                    m.registration.0.clone(),
                    m.registration.2.clone(),
                    m.revision,
                    session.clone(),
                );
                if !cache.processes.contains_key(&key) {
                    missing.push((key, Lookup::Media(m.registration.clone(), session.clone())));
                }
            }
        }
        for b in browsers.iter().filter(|b| supported(&b.registration)) {
            for window in &b.windows {
                let key = (
                    b.registration.0.clone(),
                    b.registration.2.clone(),
                    b.revision,
                    window.clone(),
                );
                if !cache.processes.contains_key(&key) {
                    missing.push((key, Lookup::Browser(b.registration.clone(), window.clone())));
                }
            }
        }
        missing.sort_by_key(|(key, _)| cache.tried.get(key).copied().unwrap_or(0));
        missing.truncate(MAX_LOOKUPS);
        cache.pass += 1;
        for (key, _) in &missing {
            cache.tried.insert(key.clone(), cache.pass);
        }
        cache.tried.retain(|key, _| current.contains(key));
        let answers: Vec<(Key, Option<u32>)> = futures_util::stream::iter(missing)
            .map(|(key, lookup)| {
                let core = self.clone();
                async move {
                    let process = match lookup {
                        Lookup::Media(registration, session) => {
                            core.media_process(&registration, &session).await
                        }
                        Lookup::Browser(registration, window) => {
                            core.browser_process(&registration, &window).await
                        }
                    };
                    (key, process)
                }
            })
            .buffer_unordered(LOOKUP_CONCURRENCY)
            .collect()
            .await;
        for (key, process) in answers {
            if let Some(process) = process {
                cache.processes.insert(key, process);
            }
        }

        let process = |provider: &str, generation: &str, revision: u64, local: &str| {
            cache
                .processes
                .get(&(
                    provider.to_string(),
                    generation.to_string(),
                    revision,
                    local.to_string(),
                ))
                .copied()
        };
        let mut sessions = Vec::new();
        for m in &media {
            let (id, _, generation) = &m.registration;
            for session in &m.sessions {
                if let Some(p) = process(id, generation, m.revision, session) {
                    sessions.push(ProcessSession {
                        media_provider: id.clone(),
                        media_generation: generation.clone(),
                        session: session.clone(),
                        process: p,
                    });
                }
            }
        }
        let mut windows = Vec::new();
        let mut tabs = Vec::new();
        for b in &browsers {
            let (id, _, generation) = &b.registration;
            for window in &b.windows {
                if let Some(p) = process(id, generation, b.revision, window) {
                    windows.push(ProcessWindow {
                        browser_provider: id.clone(),
                        browser_generation: generation.clone(),
                        window: window.clone(),
                        process: p,
                    });
                }
            }
            for (tab, window, audible) in &b.tabs {
                tabs.push(ProcessTab {
                    browser_provider: id.clone(),
                    browser_generation: generation.clone(),
                    tab: tab.clone(),
                    window: window.clone(),
                    audible: *audible,
                });
            }
        }
        // A provider whose processes are not all known yet could hide a
        // competing candidate: wait for the next pass instead of guessing.
        // Unknown support is not "unsupported": it keeps the pass incomplete.
        let complete = media.iter().all(|m| {
            cache.support.get(&m.registration) == Some(&false)
                || cache.support.get(&m.registration) == Some(&true)
                    && m.sessions.iter().all(|s| {
                        process(&m.registration.0, &m.registration.2, m.revision, s).is_some()
                    })
        }) && browsers.iter().all(|b| {
            cache.support.get(&b.registration) == Some(&false)
                || cache.support.get(&b.registration) == Some(&true)
                    && b.windows.iter().all(|w| {
                        process(&b.registration.0, &b.registration.2, b.revision, w).is_some()
                    })
        });
        // GetProcessId answers for the provider's state at call time. If any
        // snapshot moved during the lookups, the answers may not belong to the
        // tabs above: judge nothing new until a pass sees one revision.
        let fenced = {
            let (media_now, browsers_now) = self.sources().await;
            snapshot_revisions(&media, &browsers) != snapshot_revisions(&media_now, &browsers_now)
        };
        let previous = links.links().await;
        let next = if complete && !fenced {
            media_association::join(&sessions, &windows, &tabs, &previous)
        } else {
            // Not every process is known yet, or a snapshot moved: make no new
            // link and move none. A held link stays only while its session
            // and its tab's window are current with the process it was
            // established with.
            previous
                .into_iter()
                .filter(|h| {
                    let l = &h.link;
                    sessions.iter().any(|s| {
                        s.media_provider == l.media_provider
                            && s.media_generation == l.media_generation
                            && s.session == l.session
                            && s.process == h.process
                    }) && tabs.iter().any(|t| {
                        t.browser_provider == l.browser_provider
                            && t.browser_generation == l.browser_generation
                            && t.tab == l.tab
                            && windows.iter().any(|w| {
                                w.browser_provider == t.browser_provider
                                    && w.browser_generation == t.browser_generation
                                    && w.window == t.window
                                    && w.process == h.process
                            })
                    })
                })
                .collect()
        };
        if let Some(revision) = links.replace(next).await
            && let Ok(emitter) = SignalEmitter::new(&self.connection, production::PATH)
        {
            let _ = MediaLinks::changed(&emitter, &self.epoch, revision).await;
        }
    }

    async fn sources(&self) -> (Vec<MediaSource>, Vec<BrowserSource>) {
        let state = self.state.lock().await;
        let mut media = Vec::new();
        let mut browsers = Vec::new();
        for (id, p) in &state.providers {
            if p.owner.is_empty() || p.info.runtime_state != "ready" {
                continue;
            }
            let registration = (id.clone(), p.owner.clone(), p.info.generation.clone());
            match p.capability() {
                "media" => media.push(MediaSource {
                    registration,
                    revision: p.info.revision,
                    sessions: p.sessions.iter().map(|s| s.id.clone()).collect(),
                }),
                "browser" => browsers.push(BrowserSource {
                    registration,
                    revision: p.info.revision,
                    windows: p.windows.iter().map(|w| w.id.clone()).collect(),
                    tabs: p
                        .tabs
                        .iter()
                        .map(|t| (t.id.clone(), t.window_id.clone(), t.audible))
                        .collect(),
                }),
                _ => {}
            }
        }
        (media, browsers)
    }

    async fn supports(
        &self,
        cache: &mut Cache,
        registration: &Registration,
        interface: &str,
    ) -> bool {
        if let Some(&known) = cache.support.get(registration) {
            return known;
        }
        let introspected = async {
            zbus::fdo::IntrospectableProxy::builder(&self.connection)
                .destination(registration.1.as_str())
                .ok()?
                .path(production::PROVIDER_PATH)
                .ok()?
                .build()
                .await
                .ok()?
                .introspect()
                .await
                .ok()
        };
        let Ok(Some(xml)) = tokio::time::timeout(CALL_TIMEOUT, introspected).await else {
            // Unreachable now; ask again on the next pass.
            return false;
        };
        let supported = super::web_icons::introspection(&xml).is_ok_and(|document| {
            document.root_element().children().any(|node| {
                node.has_tag_name("interface") && node.attribute("name") == Some(interface)
            })
        });
        cache.support.insert(registration.clone(), supported);
        supported
    }

    /// `None` retries on the next pass; a definite failure is unknown (0).
    async fn media_process(&self, registration: &Registration, session: &str) -> Option<u32> {
        let call = async {
            MediaProcessProxy::builder(&self.connection)
                .destination(registration.1.as_str())?
                .build()
                .await?
                .get_process_id(&registration.2, session)
                .await
        };
        settle(tokio::time::timeout(CALL_TIMEOUT, call).await)
    }
    async fn browser_process(&self, registration: &Registration, window: &str) -> Option<u32> {
        let call = async {
            BrowserProcessProxy::builder(&self.connection)
                .destination(registration.1.as_str())?
                .build()
                .await?
                .get_process_id(&registration.2, window)
                .await
        };
        settle(tokio::time::timeout(CALL_TIMEOUT, call).await)
    }
}

fn settle(result: Result<zbus::Result<u32>, tokio::time::error::Elapsed>) -> Option<u32> {
    match result {
        Ok(Ok(process)) => Some(process),
        // The entry changed under the call, or the provider is slow: retry.
        Err(_) => None,
        Ok(Err(zbus::Error::MethodError(name, _, _)))
            if name.as_str().ends_with(".NotFound") || name.as_str().ends_with(".Conflict") =>
        {
            None
        }
        Ok(Err(_)) => Some(media_association::UNKNOWN_PROCESS),
    }
}

#[cfg(test)]
#[path = "media_links_tests.rs"]
mod tests;

/// Each live registration with its snapshot revision.
fn snapshot_revisions<'a>(
    media: &'a [MediaSource],
    browsers: &'a [BrowserSource],
) -> BTreeSet<(&'a Registration, u64)> {
    media
        .iter()
        .map(|m| (&m.registration, m.revision))
        .chain(browsers.iter().map(|b| (&b.registration, b.revision)))
        .collect()
}
