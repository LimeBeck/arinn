// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::catalogue::Catalogue;
use arinn_protocol::{
    assets::{Asset, MAX_ASSET_BYTES, sniff},
    validation::ErrorCode,
};
use std::{
    fs::File,
    io::{ErrorKind, Read},
    os::{fd::AsRawFd, unix::process::CommandExt},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn image(data: Vec<u8>) -> Result<Asset, ErrorCode> {
    let mime = sniff(&data).ok_or(ErrorCode::NotFound)?.into();
    let asset = Asset { mime, data };
    asset.validate()?;
    Ok(asset)
}
pub fn load(catalogue: &Catalogue, id: &str) -> Result<Asset, ErrorCode> {
    let id = id.strip_prefix("cover:").ok_or(ErrorCode::NotFound)?;
    if let Ok(file) = catalogue.open_cover(id) {
        let mut data = Vec::new();
        file.take(MAX_ASSET_BYTES as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|_| ErrorCode::Unavailable)?;
        if let Ok(asset) = image(data) {
            return Ok(asset);
        }
    }
    if !catalogue
        .tracks()
        .iter()
        .any(|t| t.id == id && t.embedded_artwork)
    {
        return Err(ErrorCode::NotFound);
    }
    attached(
        catalogue
            .open_track(id)
            .map_err(|_| ErrorCode::Unavailable)?,
    )
}
fn attached(file: File) -> Result<Asset, ErrorCode> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-nostdin",
            "-max_alloc",
            "67108864",
            "-threads",
            "1",
            "-protocol_whitelist",
            "file,pipe",
            "-format_whitelist",
            "mp3,flac,ogg,mov,aac,wav",
            "-enable_drefs",
            "0",
            "-use_absolute_path",
            "0",
            "-i",
            "/proc/self/fd/0",
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=800:800:force_original_aspect_ratio=decrease",
            "-f",
            "image2pipe",
            "-c:v",
            "mjpeg",
            "pipe:1",
        ])
        .stdin(Stdio::from(file))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|_| ErrorCode::Unavailable)?;
    let result = (|| {
        let mut stdout = child.stdout.take().ok_or(ErrorCode::Unavailable)?;
        let fd = stdout.as_raw_fd();
        // SAFETY: stdout owns the descriptor for the whole read loop.
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
                return Err(ErrorCode::Unavailable);
            }
        }
        let start = Instant::now();
        let mut data = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            if start.elapsed() > Duration::from_secs(3) {
                return Err(ErrorCode::Timeout);
            }
            for _ in 0..32 {
                match stdout.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        if data.len() + n > MAX_ASSET_BYTES {
                            return Err(ErrorCode::Unavailable);
                        }
                        data.extend_from_slice(&buffer[..n]);
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(_) => return Err(ErrorCode::Unavailable),
                }
            }
            if let Some(success) =
                crate::probe::process_status(child.id()).map_err(|_| ErrorCode::Unavailable)?
            {
                if !success {
                    return Err(ErrorCode::NotFound);
                }
                match stdout.read(&mut buffer) {
                    Ok(0) => return image(data),
                    Ok(n) => {
                        if data.len() + n > MAX_ASSET_BYTES {
                            return Err(ErrorCode::Unavailable);
                        }
                        data.extend_from_slice(&buffer[..n]);
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                    Err(_) => return Err(ErrorCode::Unavailable),
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    })();
    // SAFETY: the unreaped child owns this private process group.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.wait();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        catalogue::Root,
        probe::{Error, Probe},
    };
    use arinn_music_common::Track;
    struct Metadata;
    impl Probe for Metadata {
        fn inspect(&self, _: &std::path::Path) -> Result<Track, Error> {
            Ok(Track {
                available: true,
                ..Track::default()
            })
        }
    }
    #[test]
    fn sibling_cover_reopens_under_the_root_and_rejects_links_and_oversized_images() {
        use std::os::unix::fs::symlink;
        let directory =
            std::env::temp_dir().join(format!("arinn-artwork-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let root = directory.join("audio");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("track.wav"), b"synthetic-audio").unwrap();
        let png = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10];
        std::fs::write(root.join("cover.png"), &png).unwrap();
        let mut catalogue = Catalogue::default();
        catalogue.scan(
            &[Root {
                id: "root".into(),
                path: root.clone(),
            }],
            &Metadata,
        );
        let id = format!("cover:{}", catalogue.tracks()[0].id);
        assert_eq!(load(&catalogue, &id).unwrap().data, png);
        std::fs::remove_file(root.join("cover.png")).unwrap();
        let outside = directory.join("outside.png");
        std::fs::write(&outside, &png).unwrap();
        symlink(&outside, root.join("cover.png")).unwrap();
        assert!(load(&catalogue, &id).is_err());
        std::fs::remove_file(root.join("cover.png")).unwrap();
        let mut oversized = png;
        oversized.resize(MAX_ASSET_BYTES + 1, 0);
        std::fs::write(root.join("cover.png"), oversized).unwrap();
        assert!(load(&catalogue, &id).is_err());
        std::fs::rename(&root, directory.join("old-audio")).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("cover.png"), b"synthetic-replacement").unwrap();
        assert!(load(&catalogue, &id).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
