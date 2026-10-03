// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! ffmpeg thumbnails are cancellable and absent when no frame can be made.

use arinn_plugin_mpv::{Extractor, KEPT_THUMBNAILS, ThumbnailAssets, poster_due};
use arinn_plugin_sdk::runtime::AssetHandler;
use arinn_protocol::validation::ErrorCode;
use std::{os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc, time::Duration};

fn dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("arinn-mpv-thumb-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[tokio::test]
async fn missing_ffmpeg_is_unsupported() {
    let folder = dir();
    let extractor = Extractor::default();
    let error = extractor
        .extract(
            PathBuf::from("/no/such/ffmpeg").as_path(),
            &folder,
            PathBuf::from("/library/clip.mkv").as_path(),
            0,
        )
        .await;
    assert_eq!(error, Err(ErrorCode::Unsupported));
    let _ = std::fs::remove_dir_all(&folder);
}

#[tokio::test]
async fn a_newer_request_cancels_the_one_in_flight() {
    let folder = dir();
    let script = folder.join("slow-ffmpeg");
    std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let extractor = Arc::new(Extractor::default());
    let first = {
        let extractor = Arc::clone(&extractor);
        let script = script.clone();
        let folder = folder.clone();
        tokio::spawn(async move {
            extractor
                .extract(
                    &script,
                    &folder,
                    PathBuf::from("/library/clip.mkv").as_path(),
                    0,
                )
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(30)).await;
    let second = {
        let extractor = Arc::clone(&extractor);
        let script = script.clone();
        let folder = folder.clone();
        tokio::spawn(async move {
            extractor
                .extract(
                    &script,
                    &folder,
                    PathBuf::from("/library/clip.mkv").as_path(),
                    1_000_000,
                )
                .await
        })
    };
    let first = tokio::time::timeout(Duration::from_secs(2), first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first, Err(ErrorCode::Cancelled));
    second.abort();
    let _ = std::fs::remove_dir_all(&folder);
}

#[tokio::test]
async fn ffmpeg_writes_a_jpeg_without_seeking_a_player() {
    if !PathBuf::from("/usr/bin/ffmpeg").is_file() {
        eprintln!("ffmpeg is not installed; thumbnail extraction was not run");
        return;
    }
    let folder = dir();
    let clip = folder.join("clip.mkv");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x120:rate=10:duration=1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&clip)
        .status()
        .unwrap();
    assert!(status.success());
    let extractor = Extractor::default();
    let thumbnail = extractor
        .extract(
            PathBuf::from("/usr/bin/ffmpeg").as_path(),
            &folder,
            &clip,
            200_000,
        )
        .await
        .unwrap();
    assert_eq!(thumbnail.timestamp_us, 200_000);
    let bytes = std::fs::read(folder.join(format!("{}.jpg", thumbnail.asset))).unwrap();
    assert!(bytes.starts_with(&[0xff, 0xd8]), "jpeg");
    let _ = std::fs::remove_dir_all(&folder);
}

#[tokio::test]
async fn cache_keeps_only_the_newest_frames_and_ffmpeg_reads_a_local_file() {
    let folder = dir();
    let script = folder.join("fake-ffmpeg");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$(dirname \"$0\")/args\"\nout=\nfor arg in \"$@\"; do out=\"$arg\"; done\nprintf '\\377\\330' > \"$out\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let oldest = (0..KEPT_THUMBNAILS + 3)
        .map(|index| {
            let path = folder.join(format!("old-{index}.jpg"));
            std::fs::write(&path, b"old").unwrap();
            let file = std::fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(index as u64),
            )
            .unwrap();
            path
        })
        .collect::<Vec<_>>();
    let source = folder.join("clip.mkv");
    std::fs::write(&source, b"video").unwrap();
    let extractor = Extractor::default();
    let thumbnail = extractor
        .extract(&script, &folder, &source, 200_000)
        .await
        .unwrap();
    let kept: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jpg"))
        .collect();
    assert_eq!(kept.len(), KEPT_THUMBNAILS);
    assert!(
        kept.iter()
            .any(|path| path.ends_with(format!("{}.jpg", thumbnail.asset)))
    );
    for path in oldest.iter().take(3) {
        assert!(!path.exists(), "{}", path.display());
    }
    let args = std::fs::read_to_string(folder.join("args")).unwrap();
    let lines: Vec<_> = args.lines().collect();
    let whitelist = lines
        .iter()
        .position(|line| *line == "-protocol_whitelist")
        .unwrap();
    assert_eq!(lines[whitelist + 1], "file");
    let input = lines.iter().position(|line| *line == "-i").unwrap();
    assert!(input > whitelist);
    assert_eq!(lines[input + 1], format!("file:{}", source.display()));
    let _ = std::fs::remove_dir_all(&folder);
}

// #278: the frames are served by asset id; an id never names a path.
#[tokio::test]
async fn frames_are_served_by_their_uuid_only() {
    let folder = dir();
    let id = uuid::Uuid::new_v4().to_string();
    let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F'];
    std::fs::write(folder.join(format!("{id}.jpg")), jpeg).unwrap();
    std::fs::write(folder.join("not-an-image.jpg"), b"text").unwrap();
    let fake = uuid::Uuid::new_v4().to_string();
    std::fs::write(folder.join(format!("{fake}.jpg")), b"not a jpeg").unwrap();
    std::fs::write(folder.parent().unwrap().join("outside.jpg"), jpeg).unwrap();
    let assets = ThumbnailAssets::new(folder.clone());

    let asset = assets.asset(id.clone()).await.unwrap();
    assert_eq!(asset.mime, "image/jpeg");
    assert_eq!(asset.data, jpeg);
    assert!(asset.validate().is_ok());

    for bad in [
        uuid::Uuid::new_v4().to_string(),
        id.to_uppercase(),
        format!("{id}.jpg"),
        "../outside".into(),
        "not-an-image".into(),
        fake,
        uuid::Uuid::nil().to_string(),
    ] {
        assert_eq!(
            assets.asset(bad.clone()).await.map(|a| a.mime),
            Err(ErrorCode::NotFound),
            "{bad}"
        );
    }
    let _ = std::fs::remove_file(folder.parent().unwrap().join("outside.jpg"));
    let _ = std::fs::remove_dir_all(&folder);
}

// #292: when a session card frame is captured.
#[test]
fn poster_frames_follow_session_position_and_state() {
    let step = 30_000_000;
    let last = ("s".to_string(), 60_000_000);
    // First frame, and a new session.
    assert!(poster_due(None, "s", 0, true, step));
    assert!(poster_due(Some(&last), "other", 60_000_000, true, step));
    // Playing: only after the step, in either direction.
    assert!(!poster_due(Some(&last), "s", 89_000_000, true, step));
    assert!(poster_due(Some(&last), "s", 90_000_000, true, step));
    assert!(poster_due(Some(&last), "s", 30_000_000, true, step));
    // Paused or stopped: once per new position.
    assert!(!poster_due(Some(&last), "s", 60_000_000, false, step));
    assert!(poster_due(Some(&last), "s", 61_000_000, false, step));
}

// #292: card frames sit in posters/ and are served by the same rule.
#[tokio::test]
async fn poster_frames_are_served_too() {
    let folder = dir();
    let posters = folder.join("posters");
    std::fs::create_dir_all(&posters).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F'];
    std::fs::write(posters.join(format!("{id}.jpg")), jpeg).unwrap();
    let assets = ThumbnailAssets::new(folder.clone());
    assert_eq!(assets.asset(id).await.unwrap().data, jpeg);
    let _ = std::fs::remove_dir_all(&folder);
}
