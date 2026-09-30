// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-stable download contract (ADR-0025, ADR-0067).
//!
//! A job describes a download, not a torrent protocol. Output file URIs
//! travel from the provider to the daemon for the library import only; they
//! never reach the shell, and neither they nor titles are logged (spec §13).
use crate::{
    media::file_uri,
    production::EntityRef,
    validation::{self, ErrorCode, MAX_BODY_BYTES, MAX_PAGE_ENTRIES},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zbus::zvariant::{LE, OwnedObjectPath, Type, serialized::Context, to_bytes};

pub const PROVIDER_INTERFACE: &str = "org.arinn.DownloadProvider1";
pub const INTERFACE: &str = "org.arinn.Downloads1";

pub const STATE_QUEUED: &str = "queued";
pub const STATE_DOWNLOADING: &str = "downloading";
pub const STATE_PAUSED: &str = "paused";
/// The backend checks data it already has. The shell shows "Checking…"
/// without an invented ETA.
pub const STATE_VERIFYING: &str = "verifying";
/// Downloaded and verified by the backend. Seeding does not change it.
pub const STATE_COMPLETED: &str = "completed";
pub const STATE_ERROR: &str = "error";
/// States the shell knows. Any other token is shown as unsupported.
pub const STATES: &[&str] = &[
    STATE_QUEUED,
    STATE_DOWNLOADING,
    STATE_PAUSED,
    STATE_VERIFYING,
    STATE_COMPLETED,
    STATE_ERROR,
];

pub const ACTION_PAUSE: &str = "pause";
pub const ACTION_RESUME: &str = "resume";
pub const ACTION_START_NOW: &str = "start-now";
pub const ACTION_RETRY: &str = "retry";
pub const ACTIONS: &[&str] = &[ACTION_PAUSE, ACTION_RESUME, ACTION_START_NOW, ACTION_RETRY];

/// Import of a job's outputs into the library, owned by the daemon and
/// independent of the download state.
pub const IMPORT_NONE: &str = "none";
pub const IMPORT_PENDING: &str = "pending";
pub const IMPORT_IMPORTED: &str = "imported";
pub const IMPORT_UNAVAILABLE: &str = "unavailable";
pub const IMPORT_UNSUPPORTED: &str = "unsupported";
pub const IMPORT_STATES: &[&str] = &[
    IMPORT_NONE,
    IMPORT_PENDING,
    IMPORT_IMPORTED,
    IMPORT_UNAVAILABLE,
    IMPORT_UNSUPPORTED,
];

/// Normalized error codes a job may carry (the names of [`ErrorCode`]).
pub const ERROR_CODES: &[&str] = &[
    "NotFound",
    "Unavailable",
    "Unsupported",
    "PermissionDenied",
    "Timeout",
    "InvalidArgument",
    "Conflict",
    "Internal",
    "Cancelled",
];

pub const MAX_ACTIONS: usize = 8;
pub const MAX_OUTPUTS: usize = 64;
const MAX_TOKEN: usize = 32;

fn token(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty()
        || value.len() > MAX_TOKEN
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        Err(ErrorCode::InvalidArgument)
    } else {
        Ok(())
    }
}

/// One output of a job. `id` is the stable output identity: with the provider
/// and job id it keys the library's completion ledger. `file_uri` is the local
/// `file:///` URI the provider mapped the backend path to, or empty when no
/// mapping covers it (import is then unavailable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Output {
    pub id: String,
    pub title: String,
    pub file_uri: String,
}

/// A download job as the provider publishes it. Byte counts are unsigned;
/// `total_bytes` and `progress` carry a has-value flag, so an unknown size is
/// never a fake percentage. `eta_seconds` is −1 when unknown. `error_code`
/// is one of [`ERROR_CODES`] exactly when `state` is `error`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Job {
    pub id: String,
    pub title: String,
    pub state: String,
    pub completed_bytes: u64,
    pub has_total: bool,
    pub total_bytes: u64,
    pub rate_bytes_per_second: u64,
    pub eta_seconds: i64,
    pub has_progress: bool,
    pub progress: f64,
    pub actions: Vec<String>,
    pub error_code: String,
    pub error_message: String,
    pub outputs: Vec<Output>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct JobPage {
    pub generation: String,
    pub revision: u64,
    pub jobs: Vec<Job>,
    pub next_token: String,
}

/// An output as the shell sees it: no file URI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DownloadOutput {
    pub id: String,
    pub title: String,
}

/// A job as the daemon serves it to the shell: the provider's job without
/// file URIs, plus the daemon's import state and the library items the
/// import produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Download {
    pub id: String,
    pub title: String,
    pub state: String,
    pub completed_bytes: u64,
    pub has_total: bool,
    pub total_bytes: u64,
    pub rate_bytes_per_second: u64,
    pub eta_seconds: i64,
    pub has_progress: bool,
    pub progress: f64,
    pub actions: Vec<String>,
    pub error_code: String,
    pub error_message: String,
    pub outputs: Vec<DownloadOutput>,
    pub import_state: String,
    pub library_refs: Vec<EntityRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct DownloadPage {
    pub generation: String,
    pub revision: u64,
    pub downloads: Vec<Download>,
    pub next_token: String,
}

/// Checks shared by [`Job`] and [`Download`].
#[allow(clippy::too_many_arguments)]
fn validate_common(
    id: &str,
    title: &str,
    state: &str,
    completed_bytes: u64,
    has_total: bool,
    total_bytes: u64,
    eta_seconds: i64,
    has_progress: bool,
    progress: f64,
    actions: &[String],
    error_code: &str,
    error_message: &str,
) -> Result<(), ErrorCode> {
    validation::id(id)?;
    validation::text(title)?;
    token(state)?;
    if has_total {
        if completed_bytes > total_bytes {
            return Err(ErrorCode::InvalidArgument);
        }
    } else if total_bytes != 0 {
        return Err(ErrorCode::InvalidArgument);
    }
    if eta_seconds < -1 {
        return Err(ErrorCode::InvalidArgument);
    }
    if has_progress {
        if !(0.0..=1.0).contains(&progress) {
            return Err(ErrorCode::InvalidArgument);
        }
    } else if progress != 0.0 {
        return Err(ErrorCode::InvalidArgument);
    }
    if actions.len() > MAX_ACTIONS {
        return Err(ErrorCode::InvalidArgument);
    }
    let mut seen = HashSet::new();
    for action in actions {
        token(action)?;
        if !seen.insert(action.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    let coded = if state == STATE_ERROR {
        ERROR_CODES.contains(&error_code)
    } else {
        error_code.is_empty() && error_message.is_empty()
    };
    if !coded {
        return Err(ErrorCode::InvalidArgument);
    }
    validation::text(error_message)
}

fn unique_outputs<'a>(ids: impl Iterator<Item = &'a str>) -> Result<(), ErrorCode> {
    let mut seen = HashSet::new();
    let mut count = 0;
    for id in ids {
        validation::id(id)?;
        count += 1;
        if count > MAX_OUTPUTS || !seen.insert(id) {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    Ok(())
}

impl Output {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        validation::text(&self.title)?;
        if self.file_uri.is_empty() {
            Ok(())
        } else {
            file_uri(&self.file_uri)
        }
    }
}

impl Job {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validate_common(
            &self.id,
            &self.title,
            &self.state,
            self.completed_bytes,
            self.has_total,
            self.total_bytes,
            self.eta_seconds,
            self.has_progress,
            self.progress,
            &self.actions,
            &self.error_code,
            &self.error_message,
        )?;
        for output in &self.outputs {
            output.validate()?;
        }
        unique_outputs(self.outputs.iter().map(|o| o.id.as_str()))
    }

    /// The shell's view: file URIs dropped, import state and refs added.
    pub fn to_download(&self, import_state: &str, library_refs: Vec<EntityRef>) -> Download {
        Download {
            id: self.id.clone(),
            title: self.title.clone(),
            state: self.state.clone(),
            completed_bytes: self.completed_bytes,
            has_total: self.has_total,
            total_bytes: self.total_bytes,
            rate_bytes_per_second: self.rate_bytes_per_second,
            eta_seconds: self.eta_seconds,
            has_progress: self.has_progress,
            progress: self.progress,
            actions: self.actions.clone(),
            error_code: self.error_code.clone(),
            error_message: self.error_message.clone(),
            outputs: self
                .outputs
                .iter()
                .map(|o| DownloadOutput {
                    id: o.id.clone(),
                    title: o.title.clone(),
                })
                .collect(),
            import_state: import_state.into(),
            library_refs,
        }
    }
}

impl Download {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validate_common(
            &self.id,
            &self.title,
            &self.state,
            self.completed_bytes,
            self.has_total,
            self.total_bytes,
            self.eta_seconds,
            self.has_progress,
            self.progress,
            &self.actions,
            &self.error_code,
            &self.error_message,
        )?;
        for output in &self.outputs {
            validation::text(&output.title)?;
        }
        unique_outputs(self.outputs.iter().map(|o| o.id.as_str()))?;
        if !IMPORT_STATES.contains(&self.import_state.as_str())
            || self.library_refs.len() > MAX_OUTPUTS
            || (self.import_state != IMPORT_IMPORTED && !self.library_refs.is_empty())
        {
            return Err(ErrorCode::InvalidArgument);
        }
        for entity in &self.library_refs {
            validation::id(&entity.provider_id)?;
            validation::capability(&entity.capability)?;
            validation::id(&entity.local_id)?;
        }
        Ok(())
    }
}

pub fn validate_jobs(jobs: &[Job]) -> Result<(), ErrorCode> {
    let mut ids = HashSet::new();
    for job in jobs {
        job.validate()?;
        if !ids.insert(job.id.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    Ok(())
}

pub fn validate_downloads(downloads: &[Download]) -> Result<(), ErrorCode> {
    let mut ids = HashSet::new();
    for download in downloads {
        download.validate()?;
        if !ids.insert(download.id.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    Ok(())
}

fn page_header(generation: &str, revision: u64, next_token: &str) -> Result<(), ErrorCode> {
    uuid::Uuid::parse_str(generation).map_err(|_| ErrorCode::InvalidArgument)?;
    if !next_token.is_empty() {
        validation::id(next_token)?;
    }
    if revision == 0 {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(())
}

impl JobPage {
    pub fn encoded_len(&self) -> Result<usize, ErrorCode> {
        to_bytes(Context::new_dbus(LE, 0), &(self,))
            .map(|bytes| bytes.len())
            .map_err(|_| ErrorCode::InvalidArgument)
    }

    pub fn validate(&self) -> Result<(), ErrorCode> {
        page_header(&self.generation, self.revision, &self.next_token)?;
        if self.jobs.len() > MAX_PAGE_ENTRIES {
            return Err(ErrorCode::InvalidArgument);
        }
        validate_jobs(&self.jobs)?;
        if self.encoded_len()? > MAX_BODY_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}

impl DownloadPage {
    pub fn encoded_len(&self) -> Result<usize, ErrorCode> {
        to_bytes(Context::new_dbus(LE, 0), &(self,))
            .map(|bytes| bytes.len())
            .map_err(|_| ErrorCode::InvalidArgument)
    }

    pub fn validate(&self) -> Result<(), ErrorCode> {
        page_header(&self.generation, self.revision, &self.next_token)?;
        if self.downloads.len() > MAX_PAGE_ENTRIES {
            return Err(ErrorCode::InvalidArgument);
        }
        validate_downloads(&self.downloads)?;
        if self.encoded_len()? > MAX_BODY_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}

/// `org.arinn.DownloadProvider1` at `/org/arinn/Provider1`. Only the daemon
/// calls it. There is no method to add, delete or move data.
pub mod provider {
    use super::{Job, JobPage};
    #[zbus::proxy(
        interface = "org.arinn.DownloadProvider1",
        default_path = "/org/arinn/Provider1"
    )]
    pub trait DownloadProvider {
        fn get_jobs(&self, token: &str) -> zbus::Result<(JobPage,)>;
        fn get_job(&self, generation: &str, id: &str) -> zbus::Result<(Job,)>;
        fn backend_state(&self) -> zbus::Result<String>;
        fn features(&self) -> zbus::Result<Vec<String>>;
        fn pause(&self, generation: &str, id: &str) -> zbus::Result<()>;
        fn resume(&self, generation: &str, id: &str) -> zbus::Result<()>;
        fn start_now(&self, generation: &str, id: &str) -> zbus::Result<()>;
        fn retry(&self, generation: &str, id: &str) -> zbus::Result<()>;
        #[zbus(signal, name = "StateChanged")]
        fn provider_state_changed(&self, generation: &str, revision: u64) -> zbus::Result<()>;
    }
}
pub use provider::DownloadProviderProxy;

/// `org.arinn.Downloads1` on the provider's daemon path. Commands return
/// `Operation1` objects; changes arrive as `Daemon1.StateChanged` with the
/// capability `download`.
#[zbus::proxy(
    interface = "org.arinn.Downloads1",
    default_service = "org.arinn.Daemon1"
)]
pub trait DownloadsApi {
    fn get_downloads(&self, token: &str) -> zbus::Result<(DownloadPage,)>;
    fn get_download(&self, generation: &str, id: &str) -> zbus::Result<(Download,)>;
    fn pause(&self, generation: &str, id: &str, request_id: &str) -> zbus::Result<OwnedObjectPath>;
    fn resume(&self, generation: &str, id: &str, request_id: &str)
    -> zbus::Result<OwnedObjectPath>;
    fn start_now(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
    fn retry(&self, generation: &str, id: &str, request_id: &str) -> zbus::Result<OwnedObjectPath>;
}

#[cfg(test)]
#[path = "download/tests.rs"]
mod tests;
