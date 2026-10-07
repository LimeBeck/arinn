// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Download snapshots, operation routing and completion checkpoints (ADR-0067).
use super::*;
use crate::storage::DownloadImport;
use arinn_plugin_sdk::snapshots::{JobSnapshots, Row, RowStore};
use arinn_protocol::{
    download::{self, Download, DownloadPage, DownloadProviderProxy, Job},
    library::{self, ImportCandidate, LibraryImportProxy},
};
use sha2::{Digest, Sha256};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const IMPORT_RETRY: Duration = Duration::from_secs(5);

#[derive(Default)]
pub(super) struct DownloadState {
    jobs: Vec<Job>,
    source: Option<JobSnapshots>,
    cache: Option<RowStore<DownloadRows>>,
    rows: Vec<Download>,
    imports: BTreeMap<String, DownloadImport>,
    next_import: Option<Instant>,
}
struct DownloadRows;
impl Row for DownloadRows {
    type Entry = Download;
    type Page = DownloadPage;
    fn validate(rows: &[Download]) -> Result<(), ErrorCode> {
        download::validate_downloads(rows)
    }
    fn text_bytes(rows: &[Download]) -> usize {
        rows.iter()
            .map(|r| {
                r.id.len()
                    + r.title.len()
                    + r.state.len()
                    + r.error_code.len()
                    + r.error_message.len()
                    + r.import_state.len()
                    + r.actions.iter().map(String::len).sum::<usize>()
                    + r.outputs
                        .iter()
                        .map(|o| o.id.len() + o.title.len())
                        .sum::<usize>()
                    + r.library_refs
                        .iter()
                        .map(|r| r.provider_id.len() + r.capability.len() + r.local_id.len())
                        .sum::<usize>()
            })
            .sum()
    }
    fn empty_page(generation: &str, revision: u64) -> DownloadPage {
        DownloadPage {
            generation: generation.into(),
            revision,
            downloads: vec![],
            next_token: String::new(),
        }
    }
    fn entries(page: &DownloadPage) -> &[Download] {
        &page.downloads
    }
    fn entries_mut(page: &mut DownloadPage) -> &mut Vec<Download> {
        &mut page.downloads
    }
    fn set_revision(page: &mut DownloadPage, revision: u64) {
        page.revision = revision;
    }
    fn next_token(page: &mut DownloadPage) -> &mut String {
        &mut page.next_token
    }
    fn encoded_len(page: &DownloadPage) -> Result<usize, ErrorCode> {
        page.encoded_len()
    }
}

// Persist only a digest of output identities/mappings, never the file URIs.
fn fingerprint(job: &Job) -> String {
    let mut outputs: Vec<_> = job.outputs.iter().map(|o| (&o.id, &o.file_uri)).collect();
    outputs.sort();
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&outputs).expect("string tuples serialize"))
    )
}
fn project(job: &Job, record: Option<&DownloadImport>) -> Download {
    if job.state != download::STATE_COMPLETED {
        return job.to_download(download::IMPORT_NONE, vec![]);
    }
    if let Some(record) = record.filter(|r| r.fingerprint == fingerprint(job)) {
        return job.to_download(&record.state, record.refs.clone());
    }
    job.to_download(
        if job.outputs.iter().any(|o| !o.file_uri.is_empty()) {
            download::IMPORT_PENDING
        } else {
            download::IMPORT_UNAVAILABLE
        },
        vec![],
    )
}

async fn read_jobs(
    proxy: &DownloadProviderProxy<'_>,
    generation: &str,
) -> Result<(u64, Vec<Job>, String), ErrorCode> {
    let mut token = String::new();
    let mut tokens = HashSet::new();
    let mut revision = 0;
    let mut jobs = Vec::new();
    let mut bytes = 0;
    loop {
        let (page,) = proxy.get_jobs(&token).await.map_err(normalize_error)?;
        page.validate()?;
        bytes += page.encoded_len()?;
        if bytes > 16 * 1024 * 1024 || jobs.len() + page.jobs.len() > 100_000 {
            return Err(ErrorCode::InvalidArgument);
        }
        if page.generation != generation || (revision != 0 && revision != page.revision) {
            return Err(ErrorCode::Conflict);
        }
        revision = page.revision;
        jobs.extend(page.jobs);
        token = page.next_token;
        if token.is_empty() {
            break;
        }
        if !tokens.insert(token.clone()) {
            return Err(ErrorCode::Conflict);
        }
    }
    download::validate_jobs(&jobs)?;
    let backend = proxy.backend_state().await.map_err(normalize_error)?;
    if !["available", "degraded", "unavailable"].contains(&backend.as_str()) {
        return Err(ErrorCode::Unsupported);
    }
    Ok((revision, jobs, backend))
}

/// Keep both the contract count and the encoded D-Bus body within their limits.
fn import_batches(
    generation: &str,
    candidates: &[ImportCandidate],
) -> Result<Vec<Vec<ImportCandidate>>, ErrorCode> {
    use zbus::zvariant::{LE, serialized::Context, to_bytes};
    let mut batches = Vec::new();
    let mut batch = Vec::new();
    for candidate in candidates {
        batch.push(candidate.clone());
        let bytes = to_bytes(Context::new_dbus(LE, 0), &(generation, &batch))
            .map_err(|_| ErrorCode::InvalidArgument)?;
        if batch.len() > library::MAX_IMPORTS || bytes.len() > validation::MAX_BODY_BYTES {
            let last = batch.pop().ok_or(ErrorCode::InvalidArgument)?;
            if batch.is_empty() {
                return Err(ErrorCode::InvalidArgument);
            }
            batches.push(std::mem::take(&mut batch));
            batch.push(last);
        }
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    Ok(batches)
}

impl Core {
    pub(super) async fn watch_downloads(&self, id: String, owner: String, generation: String) {
        let Ok(proxy) = DownloadProviderProxy::builder(&self.connection)
            .destination(owner.as_str())
            .unwrap()
            .build()
            .await
        else {
            return;
        };
        let Ok(mut changes) = proxy.receive_provider_state_changed().await else {
            return;
        };
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! { _=tick.tick()=>{}, event=changes.next()=>{ if event.is_none() { return; } } }
            if !self.download_current(&id, &owner, &generation).await {
                return;
            }
            let result =
                tokio::time::timeout(Duration::from_secs(3), read_jobs(&proxy, &generation))
                    .await
                    .unwrap_or(Err(ErrorCode::Timeout));
            let mut state = self.state.lock().await;
            let Some(p) = state.providers.get_mut(&id) else {
                return;
            };
            if p.owner != owner || p.info.generation != generation {
                return;
            }
            let mut downloads = self.downloads.lock().await;
            let d = downloads.entry(id.clone()).or_default();
            let mut ready = false;
            match result {
                Ok((revision, jobs, backend)) => {
                    let cache = d
                        .source
                        .get_or_insert_with(|| JobSnapshots::new(&generation).unwrap());
                    match cache.import(revision, jobs.clone()) {
                        Ok(()) => {
                            d.jobs = jobs;
                            p.info.backend_state = backend;
                            p.info.runtime_state = "ready".into();
                            if !p.info.last_error.is_empty() && p.info.recovered_at == 0 {
                                p.info.recovered_at = now();
                            }
                            ready = true;
                        }
                        Err(error) => failed(&mut p.info, error),
                    }
                }
                Err(error) => failed(&mut p.info, error),
            }
            drop(downloads);
            drop(state);
            if ready {
                if let Err(error) = self
                    .refresh_download_imports(&id, &owner, &generation)
                    .await
                {
                    let mut state = self.state.lock().await;
                    if let Some(p) = state
                        .providers
                        .get_mut(&id)
                        .filter(|p| p.owner == owner && p.info.generation == generation)
                    {
                        failed(&mut p.info, error);
                    }
                }
                self.publish_downloads(&id, &owner, &generation).await;
            }
            self.changed().await;
        }
    }
    async fn download_current(&self, id: &str, owner: &str, generation: &str) -> bool {
        self.state
            .lock()
            .await
            .providers
            .get(id)
            .is_some_and(|p| p.owner == owner && p.info.generation == generation)
    }
    async fn publish_downloads(&self, id: &str, owner: &str, generation: &str) {
        let mut state = self.state.lock().await;
        let Some(p) = state
            .providers
            .get_mut(id)
            .filter(|p| p.owner == owner && p.info.generation == generation)
        else {
            return;
        };
        let mut downloads = self.downloads.lock().await;
        let d = downloads.entry(id.to_owned()).or_default();
        let rows: Vec<_> = d
            .jobs
            .iter()
            .map(|j| project(j, d.imports.get(&j.id)))
            .collect();
        let cache = d
            .cache
            .get_or_insert_with(|| RowStore::new(generation).unwrap());
        let revision = match cache.publish(rows.clone()) {
            Ok(revision) => revision,
            Err(error) => {
                failed(&mut p.info, error);
                return;
            }
        };
        d.rows = rows;
        let changed = p.info.revision != revision;
        p.info.revision = revision;
        drop(downloads);
        drop(state);
        if changed && let Ok(emitter) = SignalEmitter::new(&self.connection, production::PATH) {
            let _ = Daemon::state_changed(&emitter, id, "download", generation, revision).await;
        }
    }
    async fn import_library(&self, pinned: &str) -> Result<(String, String, String), ErrorCode> {
        let config = self.paths.config.clone();
        let preferred = tokio::task::spawn_blocking(move || {
            Config::load(&config)
                .ok()
                .and_then(|c| c.defaults.get("library").cloned())
        })
        .await
        .map_err(|_| ErrorCode::Internal)?;
        let state = self.state.lock().await;
        let usable = |p: &Provider| {
            p.capability() == "library"
                && p.info.runtime_state == "ready"
                && p.info.backend_state != "unavailable"
                && !p.owner.is_empty()
                && p.features.iter().any(|f| f == library::FEATURE_IMPORT)
        };
        let chosen = if pinned.is_empty() {
            preferred
                .as_ref()
                .and_then(|id| state.providers.get_key_value(id))
                .filter(|(_, p)| usable(p))
                .or_else(|| state.providers.iter().find(|(_, p)| usable(p)))
        } else {
            state
                .providers
                .get_key_value(pinned)
                .filter(|(_, p)| usable(p))
        }
        .ok_or(ErrorCode::Unavailable)?;
        Ok((
            chosen.0.clone(),
            chosen.1.owner.clone(),
            chosen.1.info.generation.clone(),
        ))
    }
    async fn save_import(
        &self,
        provider: &str,
        job: &str,
        record: &DownloadImport,
    ) -> Result<(), ErrorCode> {
        let (path, provider, job, record) = (
            self.paths.state.clone(),
            provider.to_owned(),
            job.to_owned(),
            record.clone(),
        );
        tokio::task::spawn_blocking(move || {
            Store::open(&path)?.save_download_import(&provider, &job, &record)
        })
        .await
        .map_err(|_| ErrorCode::Internal)?
    }
    async fn refresh_download_imports(
        &self,
        id: &str,
        owner: &str,
        generation: &str,
    ) -> Result<(), ErrorCode> {
        // Generation watchers cannot race checkpoints, including across re-registration.
        let _serial = self.download_import_lock.lock().await;
        let (jobs, backend, due) = {
            let mut state = self.state.lock().await;
            let p = state
                .providers
                .get_mut(id)
                .filter(|p| p.owner == owner && p.info.generation == generation)
                .ok_or(ErrorCode::Conflict)?;
            let mut downloads = self.downloads.lock().await;
            let d = downloads.entry(id.to_owned()).or_default();
            let due = d.next_import.is_none_or(|next| Instant::now() >= next);
            if due {
                d.next_import = Some(Instant::now() + IMPORT_RETRY);
            }
            (d.jobs.clone(), p.info.backend_state.clone(), due)
        };
        let (directory, provider, retained) = (
            self.paths.state.clone(),
            id.to_owned(),
            jobs.iter().map(|j| j.id.clone()).collect::<Vec<_>>(),
        );
        let prune = backend == "available";
        let mut records = tokio::task::spawn_blocking(move || {
            let mut store = Store::open(&directory)?;
            if prune {
                store.prune_download_imports(&provider, &retained)?;
            }
            store.download_imports(&provider)
        })
        .await
        .map_err(|_| ErrorCode::Internal)??;
        {
            let mut state = self.state.lock().await;
            let p = state
                .providers
                .get_mut(id)
                .filter(|p| p.owner == owner && p.info.generation == generation)
                .ok_or(ErrorCode::Conflict)?;
            let _ = p;
            self.downloads
                .lock()
                .await
                .entry(id.to_owned())
                .or_default()
                .imports = records.clone();
        }
        // Publish pending separately from completion; download success is not import success.
        self.publish_downloads(id, owner, generation).await;
        if !due || backend == "unavailable" {
            return Ok(());
        }
        for job in jobs.iter().filter(|j| j.state == download::STATE_COMPLETED) {
            if !self.download_current(id, owner, generation).await {
                return Err(ErrorCode::Conflict);
            }
            let hash = fingerprint(job);
            let previous = records.get(&job.id);
            if previous.is_some_and(|r| {
                r.fingerprint == hash
                    && [
                        download::IMPORT_IMPORTED,
                        download::IMPORT_UNSUPPORTED,
                        download::IMPORT_UNAVAILABLE,
                    ]
                    .contains(&r.state.as_str())
            }) {
                continue;
            }
            let mut record = DownloadImport {
                fingerprint: hash,
                library: previous.map(|r| r.library.clone()).unwrap_or_default(),
                state: download::IMPORT_PENDING.into(),
                refs: vec![],
            };
            let candidates: Vec<_> = job
                .outputs
                .iter()
                .filter(|o| !o.file_uri.is_empty())
                .map(|o| ImportCandidate {
                    source: EntityRef {
                        provider_id: id.into(),
                        capability: "download".into(),
                        local_id: job.id.clone(),
                    },
                    file_uri: o.file_uri.clone(),
                    completion_id: o.id.clone(),
                })
                .collect();
            if candidates.is_empty() {
                record.state = download::IMPORT_UNAVAILABLE.into();
            } else if let Ok((library_id, library_owner, library_generation)) =
                self.import_library(&record.library).await
            {
                record.library = library_id.clone();
                // Checkpoint before sending: a crash or timeout replays the SAME library ledger keys.
                self.save_import(id, &job.id, &record).await?;
                let result = async {
                    let proxy = LibraryImportProxy::builder(&self.connection)
                        .destination(library_owner.as_str())
                        .map_err(|_| ErrorCode::Internal)?
                        .build()
                        .await
                        .map_err(|_| ErrorCode::Unavailable)?;
                    let mut ids = HashSet::new();
                    for chunk in import_batches(&library_generation, &candidates)? {
                        let imported = proxy
                            .import_completed(&library_generation, chunk)
                            .await
                            .map_err(normalize_error)?;
                        if imported.len() > download::MAX_OUTPUTS {
                            return Err(ErrorCode::InvalidArgument);
                        }
                        for local_id in imported {
                            validation::id(&local_id)?;
                            ids.insert(local_id);
                        }
                        if ids.len() > download::MAX_OUTPUTS {
                            return Err(ErrorCode::InvalidArgument);
                        }
                    }
                    let valid = self
                        .state
                        .lock()
                        .await
                        .providers
                        .get(&library_id)
                        .is_some_and(|p| {
                            p.owner == library_owner && p.info.generation == library_generation
                        });
                    if !valid {
                        return Err(ErrorCode::Conflict);
                    }
                    let mut ids: Vec<_> = ids.into_iter().collect();
                    ids.sort();
                    Ok(ids
                        .into_iter()
                        .map(|local_id| EntityRef {
                            provider_id: library_id.clone(),
                            capability: "library".into(),
                            local_id,
                        })
                        .collect::<Vec<_>>())
                };
                match tokio::time::timeout(Duration::from_secs(30), result)
                    .await
                    .unwrap_or(Err(ErrorCode::Timeout))
                {
                    Ok(refs) => {
                        record.state = if refs.is_empty() {
                            download::IMPORT_UNSUPPORTED
                        } else {
                            download::IMPORT_IMPORTED
                        }
                        .into();
                        record.refs = refs;
                    }
                    Err(ErrorCode::Unsupported) => {
                        record.state = download::IMPORT_UNSUPPORTED.into()
                    }
                    Err(_) => {} // Keep pending; bounded retry is safe through the library's completion ledger.
                }
            }
            if !self.download_current(id, owner, generation).await {
                return Err(ErrorCode::Conflict);
            }
            self.save_import(id, &job.id, &record).await?;
            records.insert(job.id.clone(), record);
        }
        let mut state = self.state.lock().await;
        let p = state
            .providers
            .get_mut(id)
            .filter(|p| p.owner == owner && p.info.generation == generation)
            .ok_or(ErrorCode::Conflict)?;
        let _ = p;
        self.downloads
            .lock()
            .await
            .entry(id.to_owned())
            .or_default()
            .imports = records;
        Ok(())
    }
    pub(super) async fn download_command(
        &self,
        id: &str,
        action: &'static str,
        generation: &str,
        job: &str,
        request_id: &str,
        caller: &str,
    ) -> Result<OwnedObjectPath, ErrorCode> {
        validation::id(job)?;
        let fingerprint = serde_json::to_string(&(id, &action, generation, job))
            .map_err(|_| ErrorCode::Internal)?;
        if let Some(path) = self
            .operations
            .existing(caller, request_id, &fingerprint)
            .await?
        {
            return Ok(path);
        }
        let state = self.state.lock().await;
        let p = state.providers.get(id).ok_or(ErrorCode::NotFound)?;
        if p.capability() != "download" {
            return Err(ErrorCode::Unsupported);
        }
        if p.info.generation != generation {
            return Err(ErrorCode::Conflict);
        }
        if p.info.runtime_state != "ready" || p.info.backend_state == "unavailable" {
            return Err(ErrorCode::Unavailable);
        }
        let downloads = self.downloads.lock().await;
        let d = downloads.get(id).ok_or(ErrorCode::Unavailable)?;
        let entry = d
            .jobs
            .iter()
            .find(|s| s.id == job)
            .ok_or(ErrorCode::NotFound)?;
        if !entry.actions.iter().any(|a| a == action) {
            return Err(ErrorCode::Unsupported);
        }
        drop(downloads);
        let kind = match action {
            "pause" => "download.pause",
            "resume" => "download.resume",
            "start-now" => "download.start-now",
            _ => "download.retry",
        };
        let label = Label::new(kind).provider("download", &p.info.plugin_id);
        let owner = p.owner.clone();
        let (path, op, fresh) = self
            .operations
            .admit(
                &self.connection,
                caller,
                request_id,
                &fingerprint,
                id,
                label,
            )
            .await?;
        drop(state);
        if !fresh {
            return Ok(path);
        }
        let core = self.clone();
        let (id, generation, job) = (id.to_owned(), generation.to_owned(), job.to_owned());
        let path_copy = path.clone();
        tokio::spawn(async move {
            {
                let mut value = op.value.lock().await;
                if value.state != "queued" {
                    return;
                }
                value.state = "running".into();
                value.started_at = now();
            }
            let work = async {
                let proxy = DownloadProviderProxy::builder(&core.connection)
                    .destination(owner.as_str())
                    .map_err(|_| ErrorCode::Internal)?
                    .build()
                    .await
                    .map_err(|_| ErrorCode::Unavailable)?;
                let (g, j) = (generation.as_str(), job.as_str());
                match action {
                    "pause" => proxy.pause(g, j).await,
                    "resume" => proxy.resume(g, j).await,
                    "start-now" => proxy.start_now(g, j).await,
                    "retry" => proxy.retry(g, j).await,
                    _ => return Err(ErrorCode::Unsupported),
                }
                .map_err(normalize_error)
            };
            let result =
                tokio::time::timeout(COMMAND_TIMEOUT, op.phase("provider.call", op.kind(), work))
                    .await
                    .unwrap_or(Err(ErrorCode::Timeout));
            let valid = core
                .state
                .lock()
                .await
                .providers
                .get(&id)
                .is_some_and(|p| p.owner == owner && p.info.generation == generation);
            let result = if valid {
                result
            } else {
                op.reason("provider_changed");
                Err(ErrorCode::Unavailable)
            };
            let result = result.map(|()| {
                vec![EntityRef {
                    provider_id: id.clone(),
                    capability: "download".into(),
                    local_id: job.clone(),
                }]
            });
            op.finish(&core.connection, &path_copy, result).await;
        });
        Ok(path)
    }
}

pub(super) struct DownloadsEndpoint {
    pub(super) core: Core,
    pub(super) id: String,
}
#[zbus::interface(name = "org.arinn.Downloads1")]
impl DownloadsEndpoint {
    async fn get_downloads(
        &self,
        token: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<(DownloadPage,)> {
        let owner = sender(&h)?;
        let mut state = self.core.state.lock().await;
        let p = state
            .providers
            .get_mut(&self.id)
            .ok_or(ErrorCode::NotFound)?;
        let _ = p;
        let mut downloads = self.core.downloads.lock().await;
        let d = downloads.get_mut(&self.id).ok_or(ErrorCode::Unavailable)?;
        Ok((d
            .cache
            .as_mut()
            .ok_or(ErrorCode::Unavailable)?
            .page(&owner, token, Instant::now())?,))
    }
    async fn get_download(&self, generation: &str, id: &str) -> production::Result<(Download,)> {
        validation::id(id)?;
        let state = self.core.state.lock().await;
        let p = state.providers.get(&self.id).ok_or(ErrorCode::NotFound)?;
        if p.info.generation != generation {
            return Err(ErrorCode::Conflict.into());
        }
        let downloads = self.core.downloads.lock().await;
        let d = downloads.get(&self.id).ok_or(ErrorCode::Unavailable)?;
        Ok((d
            .rows
            .iter()
            .find(|j| j.id == id)
            .cloned()
            .ok_or(ErrorCode::NotFound)?,))
    }
    async fn pause(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .download_command(&self.id, "pause", generation, id, request_id, &sender(&h)?)
            .await?)
    }
    async fn resume(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .download_command(&self.id, "resume", generation, id, request_id, &sender(&h)?)
            .await?)
    }
    async fn start_now(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .download_command(
                &self.id,
                "start-now",
                generation,
                id,
                request_id,
                &sender(&h)?,
            )
            .await?)
    }
    async fn retry(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<OwnedObjectPath> {
        Ok(self
            .core
            .download_command(&self.id, "retry", generation, id, request_id, &sender(&h)?)
            .await?)
    }
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
