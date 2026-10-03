// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Real mpv with a null output, when the binary is installed.
//! The Rust CI image does not install mpv; this test skips there.

use arinn_plugin_mpv::{Launch, Player, Settings, ThumbnailAssets, arguments};
use arinn_plugin_sdk::runtime::{AssetHandler, MediaAction};
use arinn_protocol::{media::MediaSource, production::EntityRef};
use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

struct PlayerProcess(std::process::Child);
impl Drop for PlayerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn null_output_plays_a_generated_file() {
    if !PathBuf::from("/usr/bin/mpv").is_file() || !PathBuf::from("/usr/bin/ffmpeg").is_file() {
        eprintln!("mpv or ffmpeg is not installed; real playback was not run");
        return;
    }
    let folder = std::env::temp_dir().join(format!("arinn-mpv-real-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    let clip = folder.join("clip.mkv");
    let encoded = Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x120:rate=10:duration=3",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=3",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&clip)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(encoded.success());
    let socket = folder.join("ipc");
    let mut command = Command::new("/usr/bin/mpv");
    command
        .args(arguments(&socket))
        .args(["--vo=null", "--ao=null"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = PlayerProcess(command.spawn().unwrap());
    for _ in 0..50 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(socket.exists(), "mpv did not create its socket");
    let player = Player::start(Settings {
        socket,
        state_path: folder.join("session"),
        launch: Launch::External,
        publish_interval: Duration::from_secs(1),
        ffmpeg: Some(PathBuf::from("/usr/bin/ffmpeg")),
        thumb_dir: folder.join("thumbs"),
        backend: "available".into(),
    })
    .unwrap();
    let uri = format!("file://{}", clip.display());
    let id = player
        .open(MediaSource {
            content: EntityRef {
                provider_id: "library".into(),
                capability: "library".into(),
                local_id: "film-1".into(),
            },
            uri,
            start_position_us: 500_000,
            audio_language: String::new(),
            subtitle_language: String::new(),
        })
        .await
        .expect("open");
    let view = player.view().await.unwrap();
    let session = view
        .sessions
        .iter()
        .find(|session| session.id == id)
        .unwrap();
    assert_eq!(session.state, "playing");
    assert!(session.anchor_us > 0);
    assert!(session.position_us >= 500_000);
    player
        .control(MediaAction::Pause, id.clone(), 0)
        .await
        .unwrap();
    let paused = player.view().await.unwrap();
    assert_eq!(paused.sessions[0].state, "paused");
    // #292: a paused session gets a card frame of its position, once.
    let mut last = None;
    let mut asset = None;
    for _ in 0..20 {
        asset = player
            .refresh_poster(&mut last, Duration::from_secs(30))
            .await;
        if asset.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let asset = asset.expect("a card frame of the paused position");
    let mut published = String::new();
    for _ in 0..30 {
        published = player.view().await.unwrap().sessions[0].artwork.clone();
        if published == asset {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(published, asset);
    let served = ThumbnailAssets::new(folder.join("thumbs"))
        .asset(asset.clone())
        .await
        .unwrap();
    assert_eq!(served.mime, "image/jpeg");
    assert!(
        player
            .refresh_poster(&mut last, Duration::from_secs(30))
            .await
            .is_none(),
        "same paused position: no new frame"
    );
    player
        .control(MediaAction::Seek, id.clone(), 1_500_000)
        .await
        .unwrap();
    let sought = &player.view().await.unwrap().sessions[0];
    assert_eq!(sought.position_us, 1_500_000);
    let tracks = player.tracks(id.clone()).await.unwrap();
    assert!(
        tracks
            .iter()
            .any(|track| track.kind == "audio" && track.selected)
    );
    player.disable_subtitles(id.clone()).await.unwrap();
    player.control(MediaAction::Stop, id, 0).await.unwrap();
    assert_eq!(player.view().await.unwrap().sessions[0].state, "stopped");
    drop(player);
    drop(child);
    let _ = std::fs::remove_dir_all(&folder);
}
