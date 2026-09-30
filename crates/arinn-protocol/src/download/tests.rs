// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

fn job() -> Job {
    Job {
        id: "job-1".into(),
        title: "Film".into(),
        state: STATE_DOWNLOADING.into(),
        completed_bytes: 50,
        has_total: true,
        total_bytes: 100,
        rate_bytes_per_second: 10,
        eta_seconds: 5,
        has_progress: true,
        progress: 0.5,
        actions: vec![ACTION_PAUSE.into(), ACTION_START_NOW.into()],
        error_code: String::new(),
        error_message: String::new(),
        outputs: vec![Output {
            id: "out-1".into(),
            title: "film.mkv".into(),
            file_uri: "file:///srv/downloads/film.mkv".into(),
        }],
    }
}

fn library_ref() -> EntityRef {
    EntityRef {
        provider_id: "library".into(),
        capability: "library".into(),
        local_id: "film-1".into(),
    }
}

#[test]
fn wire_signatures_round_trip() {
    assert_eq!(Output::SIGNATURE.to_string(), "(sss)");
    assert_eq!(Job::SIGNATURE.to_string(), "(ssstbttxbdasssa(sss))");
    assert_eq!(
        JobPage::SIGNATURE.to_string(),
        "(sta(ssstbttxbdasssa(sss))s)"
    );
    assert_eq!(
        Download::SIGNATURE.to_string(),
        "(ssstbttxbdasssa(ss)sa(sss))"
    );
    assert_eq!(
        DownloadPage::SIGNATURE.to_string(),
        "(sta(ssstbttxbdasssa(ss)sa(sss))s)"
    );
    let page = JobPage {
        generation: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        jobs: vec![job()],
        next_token: String::new(),
    };
    page.validate().unwrap();
    let bytes = to_bytes(Context::new_dbus(LE, 0), &(&page,)).unwrap();
    let (decoded,): (JobPage,) = bytes.deserialize().unwrap().0;
    assert_eq!(decoded, page);
}

#[test]
fn unknown_sizes_are_never_a_fake_percentage() {
    let mut item = job();
    item.has_total = false;
    assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));
    item.total_bytes = 0;
    item.has_progress = false;
    assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));
    item.progress = 0.0;
    item.eta_seconds = -1;
    assert_eq!(item.validate(), Ok(()));
    item.eta_seconds = -2;
    assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));

    let mut item = job();
    item.completed_bytes = 101;
    assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));
    for bad in [1.5, -0.1, f64::NAN] {
        let mut item = job();
        item.progress = bad;
        assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument), "{bad}");
    }
}

#[test]
fn an_error_code_goes_with_the_error_state_only() {
    let mut item = job();
    item.state = STATE_ERROR.into();
    assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));
    item.error_code = "Unavailable".into();
    item.error_message = "Disk full".into();
    assert_eq!(item.validate(), Ok(()));
    item.error_code = "DiskFull".into();
    assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));
    let mut item = job();
    item.error_code = "garbage".into();
    assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));
    let mut item = job();
    item.error_message = "stale".into();
    assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));
}

#[test]
fn actions_and_outputs_are_unique_and_outputs_are_local() {
    let mut item = job();
    item.actions.push(ACTION_PAUSE.into());
    assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));
    let mut item = job();
    item.outputs.push(item.outputs[0].clone());
    assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));
    let mut item = job();
    item.outputs[0].file_uri = String::new();
    assert_eq!(item.validate(), Ok(()));
    for bad in [
        "/srv/downloads/film.mkv",
        "https://example.org/film.mkv",
        "file:///srv/../etc/passwd",
    ] {
        let mut item = job();
        item.outputs[0].file_uri = bad.into();
        assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument), "{bad}");
    }
    assert_eq!(
        validate_jobs(&[job(), job()]),
        Err(ErrorCode::InvalidArgument)
    );
}

#[test]
fn the_shell_view_drops_file_uris_and_carries_the_import() {
    let mut item = job();
    item.state = STATE_COMPLETED.into();
    item.completed_bytes = 100;
    item.progress = 1.0;
    item.eta_seconds = -1;
    item.actions.clear();
    let download = item.to_download(IMPORT_IMPORTED, vec![library_ref()]);
    download.validate().unwrap();
    assert_eq!(download.outputs[0].title, "film.mkv");
    let page = DownloadPage {
        generation: uuid::Uuid::new_v4().to_string(),
        revision: 3,
        downloads: vec![download.clone()],
        next_token: String::new(),
    };
    page.validate().unwrap();
    let bytes = to_bytes(Context::new_dbus(LE, 0), &(&page,)).unwrap();
    assert!(
        !String::from_utf8_lossy(bytes.bytes()).contains("file://"),
        "a URI reached the shell's page"
    );

    // Refs exist only once imported; the import state is a known token.
    let pending = item.to_download(IMPORT_PENDING, vec![library_ref()]);
    assert_eq!(pending.validate(), Err(ErrorCode::InvalidArgument));
    let unknown = item.to_download("done", Vec::new());
    assert_eq!(unknown.validate(), Err(ErrorCode::InvalidArgument));
    for state in IMPORT_STATES {
        item.to_download(state, Vec::new()).validate().unwrap();
    }
    assert_eq!(
        validate_downloads(&[download.clone(), download]),
        Err(ErrorCode::InvalidArgument)
    );
}
