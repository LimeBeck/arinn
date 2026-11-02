// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_plugin_music_local::{
    catalogue::{Catalogue, Root},
    probe::{Error, Ffprobe, Probe},
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("arinn-audio-fixture-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn script(&self, contents: &str) -> PathBuf {
        let path = self.0.join("probe");
        fs::write(&path, contents).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn audio(path: &Path) {
    let size = 8000_u32 * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&8000_u32.to_le_bytes());
    bytes.extend_from_slice(&16000_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.resize(bytes.len() + size as usize, 0);
    fs::write(path, bytes).unwrap();
}

#[test]
fn a_real_generated_wav_is_probed_and_reopened_without_exposing_its_path() {
    let fixture = Fixture::new();
    let path = fixture.0.join("Synthetic.wav");
    audio(&path);
    let probe = Ffprobe::default();
    let track = probe
        .inspect(&path)
        .expect("ffprobe is required for this integration test");
    assert_eq!(track.title, "Synthetic");
    assert_eq!(track.duration_us, 1_000_000);
    let roots = [Root {
        id: "fixture".into(),
        path: fixture.0.clone(),
    }];
    let mut catalogue = Catalogue::default();
    assert!(catalogue.scan(&roots, &probe)[0].available);
    let tracks = catalogue.tracks();
    let encoded = serde_json::to_string(&tracks).unwrap();
    assert!(!encoded.contains(fixture.0.to_str().unwrap()));
    let id = tracks[0].id.clone();
    assert_eq!(
        catalogue.open_track(&id).unwrap().metadata().unwrap().len(),
        fs::metadata(&path).unwrap().len()
    );
    let private_state = serde_json::to_vec(&catalogue).unwrap();
    let restored: Catalogue = serde_json::from_slice(&private_state).unwrap();
    assert_eq!(restored.tracks(), tracks);
    assert!(restored.open_track(&id).is_ok());
}

#[test]
fn probe_output_and_runtime_are_bounded_without_pipe_deadlock() {
    let fixture = Fixture::new();
    let path = fixture.0.join("Synthetic.wav");
    audio(&path);
    let executable = fixture.script("#!/bin/sh\nyes x\n");
    let probe = Ffprobe {
        executable,
        timeout: Duration::from_secs(2),
        max_output: 4096,
    };
    assert_eq!(probe.inspect(&path), Err(Error::TooLarge));
    let executable = fixture.script("#!/bin/sh\nsleep 30\n");
    let probe = Ffprobe {
        executable,
        timeout: Duration::from_millis(100),
        max_output: 4096,
    };
    let started = Instant::now();
    assert_eq!(probe.inspect(&path), Err(Error::Timeout));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn a_symlink_swap_after_scanning_cannot_redirect_playback_outside_the_root() {
    let fixture = Fixture::new();
    let outside = Fixture::new();
    let album = fixture.0.join("Album");
    fs::create_dir(&album).unwrap();
    audio(&album.join("Synthetic.wav"));
    audio(&outside.0.join("Synthetic.wav"));
    let mut catalogue = Catalogue::default();
    let roots = [Root {
        id: "fixture".into(),
        path: fixture.0.clone(),
    }];
    catalogue.scan(&roots, &Ffprobe::default());
    let id = catalogue.tracks()[0].id.clone();
    let moved = fixture.0.join("Moved");
    fs::rename(&album, &moved).unwrap();
    symlink(&outside.0, &album).unwrap();
    assert!(catalogue.open_track(&id).is_err());
    fs::remove_file(&album).unwrap();
    fs::rename(moved, &album).unwrap();
    assert!(catalogue.open_track(&id).is_ok());
}

#[test]
fn updating_a_renamed_root_reuses_track_identity_but_opens_the_current_path() {
    let fixture = Fixture::new();
    let original = fixture.0.join("Original");
    fs::create_dir(&original).unwrap();
    audio(&original.join("Synthetic.wav"));
    let mut catalogue = Catalogue::default();
    catalogue.scan(
        &[Root {
            id: "stable-root".into(),
            path: original.clone(),
        }],
        &Ffprobe::default(),
    );
    let id = catalogue.tracks()[0].id.clone();
    let renamed = fixture.0.join("Renamed");
    fs::rename(original, &renamed).unwrap();
    catalogue.scan(
        &[Root {
            id: "stable-root".into(),
            path: renamed,
        }],
        &Ffprobe::default(),
    );
    assert_eq!(catalogue.tracks()[0].id, id);
    assert!(catalogue.tracks()[0].available);
    assert!(catalogue.open_track(&id).is_ok());
}

#[test]
fn a_disguised_playlist_cannot_make_the_audio_probe_fetch_a_subordinate_url() {
    let fixture = Fixture::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let file = fixture.0.join("Synthetic.mp3");
    let contents = format!(
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:0\n#EXTINF:1.0,\nhttp://{}/track.aac\n#EXT-X-ENDLIST\n",
        listener.local_addr().unwrap()
    );
    fs::write(&file, contents).unwrap();
    // Exercise the production probe budget, including decoder startup on slow
    // CI/toolchain runners. The owned sleeping helper above tests short limits.
    let probe = Ffprobe::default();
    assert_eq!(probe.inspect(&file), Err(Error::Failed));
    assert_eq!(
        listener.accept().err().unwrap().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn packaged_decoders_recognize_all_seven_candidate_audio_formats() {
    let fixture = Fixture::new();
    let source = fixture.0.join("source.wav");
    audio(&source);
    let probe = Ffprobe::default();
    for (extension, encoder) in [
        ("mp3", "libmp3lame"),
        ("flac", "flac"),
        ("ogg", "libvorbis"),
        ("opus", "libopus"),
        ("m4a", "aac"),
        ("aac", "aac"),
        ("wav", "pcm_s16le"),
    ] {
        let output = fixture.0.join(format!("Synthetic.{extension}"));
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-i",
            ])
            .arg(&source)
            .args([
                "-vn",
                "-c:a",
                encoder,
                "-metadata",
                "title=Synthetic title",
                "-metadata",
                "artist=Synthetic artist",
                "-metadata",
                "album=Synthetic album",
            ])
            .arg(&output)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("packaged ffmpeg is required for audio format qualification");
        assert!(
            status.success(),
            "synthetic encoding failed for {extension}"
        );
        let track = probe
            .inspect(&output)
            .unwrap_or_else(|e| panic!("synthetic {extension} probe failed: {e:?}"));
        assert!(track.available);
        assert!(
            (500_000..=3_000_000).contains(&track.duration_us),
            "invalid duration for {extension}"
        );
        assert!(!track.embedded_artwork);
    }
}
