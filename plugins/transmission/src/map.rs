// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Transmission torrent objects become download jobs. Peers, seeds, ratio,
//! trackers and backend error strings are not copied.

use crate::config::Mapping;
use arinn_protocol::{
    download::{self, Job, Output},
    validation,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_OUTPUTS: usize = download::MAX_OUTPUTS;

/// Fields read from `torrent-get`. Anything else, including peers and
/// trackers, is ignored.
pub const TORRENT_FIELDS: &[&str] = &[
    "hashString",
    "name",
    "status",
    "error",
    "sizeWhenDone",
    "leftUntilDone",
    "rateDownload",
    "eta",
    "downloadDir",
    "files",
];

/// Jobs sorted by id, and whether any torrent is still transferring.
pub fn jobs_from_list(torrents: &[Value], mappings: &[Mapping]) -> (Vec<Job>, bool) {
    let mut active = false;
    let mut jobs = Vec::new();
    for torrent in torrents {
        if transfer_active(int_field(torrent, "status")) {
            active = true;
        }
        if let Some(job) = job_from_torrent(torrent, mappings) {
            jobs.push(job);
        }
    }
    jobs.sort_by(|a, b| a.id.cmp(&b.id));
    jobs.dedup_by(|a, b| a.id == b.id);
    (jobs, active)
}

fn transfer_active(status: Option<i64>) -> bool {
    matches!(status, Some(1..=6))
}

fn job_from_torrent(torrent: &Value, mappings: &[Mapping]) -> Option<Job> {
    let id = torrent.get("hashString")?.as_str()?.to_string();
    if validation::id(&id).is_err() {
        return None;
    }
    let status = int_field(torrent, "status")?;
    let error = int_field(torrent, "error").unwrap_or(0);
    let total = uint_field(torrent, "sizeWhenDone");
    let left = uint_field(torrent, "leftUntilDone").min(total);
    let mut completed = total - left;
    let finished = total > 0 && left == 0;
    let (state, error_code, error_message) = classify(status, error, finished);
    if matches!(status, 5 | 6) && total > 0 {
        completed = total;
    }
    let (has_total, total_bytes, has_progress, progress) = if total == 0 {
        (false, 0, false, 0.0)
    } else {
        (true, total, true, completed as f64 / total as f64)
    };
    // Transmission reports the time until the seed ratio once the data is
    // done. That countdown is not a download ETA.
    let mut eta = int_field(torrent, "eta").unwrap_or(-1);
    if state != download::STATE_DOWNLOADING || eta < -1 {
        eta = -1;
    }
    let title = display_title(torrent.get("name").and_then(Value::as_str).unwrap_or(""));
    let download_dir = torrent
        .get("downloadDir")
        .and_then(Value::as_str)
        .unwrap_or("");
    let outputs = outputs_of(torrent.get("files"), download_dir, mappings);
    let job = Job {
        id,
        title,
        state: state.to_string(),
        completed_bytes: completed,
        has_total,
        total_bytes,
        rate_bytes_per_second: uint_field(torrent, "rateDownload"),
        eta_seconds: eta,
        has_progress,
        progress,
        actions: actions_for(state, status),
        error_code: error_code.to_string(),
        error_message: error_message.to_string(),
        outputs,
    };
    job.validate().ok()?;
    Some(job)
}

fn classify(status: i64, error: i64, finished: bool) -> (&'static str, &'static str, &'static str) {
    // A tracker error (`error` 2), including a failed announce on a finished
    // or paused torrent, does not replace the job state. Only a local error
    // (`error` 3) is `error`.
    if error == 3 {
        return (download::STATE_ERROR, "Internal", "The download stopped.");
    }
    let state = match status {
        3 => download::STATE_QUEUED,
        4 => download::STATE_DOWNLOADING,
        1 | 2 => download::STATE_VERIFYING,
        5 | 6 => download::STATE_COMPLETED,
        0 if finished => download::STATE_COMPLETED,
        0 => download::STATE_PAUSED,
        _ => {
            return (
                download::STATE_ERROR,
                "Unsupported",
                "The download is not supported.",
            );
        }
    };
    (state, "", "")
}

fn actions_for(state: &str, status: i64) -> Vec<String> {
    let names: &[&str] = if state == download::STATE_COMPLETED {
        // Seeding, waiting to seed, and stopped after finish only start or
        // stop seeding. The default UI does not expose those controls.
        &[]
    } else if state == download::STATE_ERROR {
        &[download::ACTION_RETRY]
    } else if state == download::STATE_VERIFYING && status == 2 {
        &[download::ACTION_PAUSE]
    } else if matches!(state, download::STATE_QUEUED | download::STATE_VERIFYING) {
        &[download::ACTION_PAUSE, download::ACTION_START_NOW]
    } else if state == download::STATE_DOWNLOADING {
        &[download::ACTION_PAUSE]
    } else {
        &[download::ACTION_RESUME, download::ACTION_START_NOW]
    };
    names.iter().map(|name| (*name).to_string()).collect()
}

fn outputs_of(files: Option<&Value>, download_dir: &str, mappings: &[Mapping]) -> Vec<Output> {
    let Some(files) = files.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut outputs = Vec::new();
    for (index, file) in files.iter().enumerate() {
        if outputs.len() == MAX_OUTPUTS {
            break;
        }
        let Some(name) = file.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name.is_empty() || name.contains('\0') {
            continue;
        }
        let length = uint_field(file, "length");
        let done = uint_field(file, "bytesCompleted");
        if length == 0 || done < length {
            continue;
        }
        let title = basename(name);
        if validation::text(title).is_err() {
            continue;
        }
        let id = output_id(index, name);
        let file_uri = mapped_uri(download_dir, name, mappings).unwrap_or_default();
        let output = Output {
            id,
            title: title.to_string(),
            file_uri,
        };
        if output.validate().is_ok() {
            outputs.push(output);
        }
    }
    outputs
}

/// File index plus the first 16 hex digits of the SHA-256 of the
/// torrent-relative path. The library ledger uses this as the completion id.
pub fn output_id(index: usize, relative_path: &str) -> String {
    let digest = Sha256::digest(relative_path.as_bytes());
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hash = String::with_capacity(16);
    for byte in &digest[..8] {
        hash.push(char::from(HEX[usize::from(byte >> 4)]));
        hash.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    format!("{index}-{hash}")
}

fn mapped_uri(download_dir: &str, name: &str, mappings: &[Mapping]) -> Option<String> {
    let backend = join_backend(download_dir, name)?;
    let local = apply_mapping(&backend, mappings)?;
    to_file_uri(&local)
}

fn join_backend(download_dir: &str, name: &str) -> Option<String> {
    if !download_dir.starts_with('/') || download_dir.contains('\0') || name.contains('\0') {
        return None;
    }
    let dir = download_dir.trim_end_matches('/');
    let name = name.trim_start_matches('/');
    if name.is_empty() {
        return None;
    }
    Some(format!("{dir}/{name}"))
}

fn apply_mapping(path: &str, mappings: &[Mapping]) -> Option<String> {
    let mut best: Option<&Mapping> = None;
    for mapping in mappings {
        let prefix = mapping.backend.as_str();
        let matched = path == prefix || path.starts_with(&format!("{prefix}/"));
        if matched && best.is_none_or(|current| prefix.len() > current.backend.len()) {
            best = Some(mapping);
        }
    }
    let mapping = best?;
    let rest = &path[mapping.backend.len()..];
    let local = mapping.local.trim_end_matches('/');
    Some(format!("{local}{rest}"))
}

fn to_file_uri(path: &str) -> Option<String> {
    let mut uri = String::from("file://");
    if !path.starts_with('/') {
        return None;
    }
    for segment in path.split('/') {
        if segment.is_empty() {
            uri.push('/');
            continue;
        }
        if segment == "." || segment == ".." {
            return None;
        }
        if !uri.ends_with('/') {
            uri.push('/');
        }
        for byte in segment.as_bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                uri.push(char::from(*byte));
            } else {
                uri.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    arinn_protocol::media::file_uri(&uri).ok()?;
    Some(uri)
}

fn basename(name: &str) -> &str {
    name.rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(name)
}

fn display_title(name: &str) -> String {
    let mut title = name.replace('\0', "");
    if title.len() > validation::MAX_TEXT_BYTES {
        title.truncate(validation::MAX_TEXT_BYTES);
        while !title.is_char_boundary(title.len()) {
            title.pop();
        }
    }
    title
}

fn int_field(value: &Value, name: &str) -> Option<i64> {
    let value = value.get(name)?;
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|n| i64::try_from(n).ok()))
        .or_else(|| {
            value
                .as_f64()
                .and_then(|n| if n.is_finite() { Some(n as i64) } else { None })
        })
}

fn uint_field(value: &Value, name: &str) -> u64 {
    let Some(value) = value.get(name) else {
        return 0;
    };
    if let Some(n) = value.as_u64() {
        return n;
    }
    if let Some(n) = value.as_i64() {
        return n.max(0) as u64;
    }
    value
        .as_f64()
        .filter(|n| n.is_finite() && *n > 0.0)
        .map(|n| n as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn torrent(status: i64, error: i64) -> Value {
        json!({
            "hashString": "0123456789abcdef0123456789abcdef01234567",
            "name": "Film",
            "status": status,
            "error": error,
            "errorString": "secret-path /srv/private",
            "peers": 9,
            "ratio": 1.5,
            "trackers": [{"announce": "http://tracker.example/announce"}],
            "sizeWhenDone": 1000,
            "leftUntilDone": 250,
            "rateDownload": 100,
            "eta": 40,
            "downloadDir": "/srv/downloads",
            "files": [
                {"name": "Film/movie.mkv", "length": 800, "bytesCompleted": 800},
                {"name": "Film/note.txt", "length": 200, "bytesCompleted": 50}
            ]
        })
    }

    #[test]
    fn states_actions_and_hidden_backend_fields() {
        let cases = [
            (3, 0, download::STATE_QUEUED, vec!["pause", "start-now"]),
            (4, 0, download::STATE_DOWNLOADING, vec!["pause"]),
            (0, 0, download::STATE_PAUSED, vec!["resume", "start-now"]),
            (2, 0, download::STATE_VERIFYING, vec!["pause"]),
            (6, 0, download::STATE_COMPLETED, vec![]),
            (0, 3, download::STATE_ERROR, vec!["retry"]),
        ];
        for (status, error, state, actions) in cases {
            let (jobs, _) = jobs_from_list(&[torrent(status, error)], &[]);
            let job = &jobs[0];
            assert_eq!(job.state, state, "status {status}");
            assert_eq!(job.actions, actions, "status {status}");
            let rendered = format!("{job:?}");
            assert!(!rendered.contains("secret-path"));
            assert!(!rendered.contains("tracker.example"));
            assert!(!rendered.contains("peers"));
        }
    }

    #[test]
    fn verifying_has_no_eta_and_seeding_stays_complete() {
        let (jobs, active) = jobs_from_list(&[torrent(2, 0)], &[]);
        assert!(active);
        assert_eq!(jobs[0].eta_seconds, -1);
        let (jobs, active) = jobs_from_list(&[torrent(6, 0)], &[]);
        assert!(active);
        assert_eq!(jobs[0].state, download::STATE_COMPLETED);
        assert!(jobs[0].actions.is_empty());
        assert_eq!(jobs[0].eta_seconds, -1);
        assert_eq!(jobs[0].completed_bytes, 1000);
        assert!((jobs[0].progress - 1.0).abs() < f64::EPSILON);
        let (jobs, _) = jobs_from_list(&[torrent(4, 0)], &[]);
        assert_eq!(jobs[0].eta_seconds, 40);
    }

    #[test]
    fn finished_downloads_hide_seeding_controls_and_countdown() {
        for status in [5, 6] {
            let mut value = torrent(status, 0);
            value["eta"] = json!(3600);
            value["leftUntilDone"] = json!(0);
            let (jobs, _) = jobs_from_list(&[value], &[]);
            let job = &jobs[0];
            assert_eq!(job.state, download::STATE_COMPLETED, "status {status}");
            assert!(
                job.actions.is_empty(),
                "status {status} advertised {:?}",
                job.actions
            );
            assert_eq!(job.eta_seconds, -1, "status {status}");
        }
        let mut stopped = torrent(0, 0);
        stopped["eta"] = json!(3600);
        stopped["leftUntilDone"] = json!(0);
        let (jobs, _) = jobs_from_list(&[stopped], &[]);
        assert_eq!(jobs[0].state, download::STATE_COMPLETED);
        assert!(jobs[0].actions.is_empty(), "{:?}", jobs[0].actions);
        assert_eq!(jobs[0].eta_seconds, -1);
    }

    #[test]
    fn tracker_error_does_not_reclassify_a_finished_or_paused_torrent() {
        let mut finished = torrent(0, 2);
        finished["leftUntilDone"] = json!(0);
        finished["eta"] = json!(3600);
        let (jobs, _) = jobs_from_list(&[finished], &[]);
        let job = &jobs[0];
        assert_eq!(job.state, download::STATE_COMPLETED);
        assert!(job.actions.is_empty(), "{:?}", job.actions);
        assert_eq!(job.error_code, "");
        assert_eq!(job.error_message, "");
        assert_eq!(job.eta_seconds, -1);

        let (jobs, _) = jobs_from_list(&[torrent(0, 2)], &[]);
        let job = &jobs[0];
        assert_eq!(job.state, download::STATE_PAUSED);
        assert_eq!(
            job.actions,
            vec![
                download::ACTION_RESUME.to_string(),
                download::ACTION_START_NOW.to_string()
            ]
        );
        assert_eq!(job.error_code, "");
        assert_eq!(job.error_message, "");
    }

    #[test]
    fn output_ids_are_stable_and_unmapped_uris_stay_empty() {
        let (jobs, _) = jobs_from_list(&[torrent(4, 0)], &[]);
        let output = &jobs[0].outputs[0];
        assert_eq!(output.title, "movie.mkv");
        assert_eq!(output.file_uri, "");
        assert_eq!(output.id, output_id(0, "Film/movie.mkv"));
        assert_eq!(jobs[0].outputs.len(), 1);
        let mappings = vec![Mapping {
            backend: "/srv/downloads".into(),
            local: "/mnt/media".into(),
        }];
        let (jobs, _) = jobs_from_list(&[torrent(6, 0)], &mappings);
        assert_eq!(
            jobs[0].outputs[0].file_uri,
            "file:///mnt/media/Film/movie.mkv"
        );
        let spaced = json!({
            "hashString": "abc",
            "name": "Film",
            "status": 6,
            "error": 0,
            "sizeWhenDone": 1,
            "leftUntilDone": 0,
            "downloadDir": "/srv/downloads",
            "files": [{"name": "my film.mkv", "length": 1, "bytesCompleted": 1}]
        });
        let (jobs, _) = jobs_from_list(&[spaced], &mappings);
        assert_eq!(
            jobs[0].outputs[0].file_uri,
            "file:///mnt/media/my%20film.mkv"
        );
    }

    #[test]
    fn unknown_size_is_not_a_percentage() {
        let mut value = torrent(4, 0);
        value["sizeWhenDone"] = json!(0);
        value["leftUntilDone"] = json!(0);
        let (jobs, _) = jobs_from_list(&[value], &[]);
        assert!(!jobs[0].has_total);
        assert!(!jobs[0].has_progress);
        assert_eq!(jobs[0].progress, 0.0);
    }
}
