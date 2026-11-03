// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded, shell-free ffprobe subprocess. Failure values contain no private path.
use arinn_music_common::Track;
use std::{
    fs::File,
    io::{self, Read},
    os::{fd::AsRawFd, unix::process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unavailable,
    Failed,
    Timeout,
    TooLarge,
    Invalid,
}

pub trait Probe {
    fn inspect(&self, path: &Path) -> Result<Track, Error>;
    fn inspect_file(&self, path: &Path, _file: File) -> Result<Track, Error> {
        self.inspect(path)
    }
}

pub struct Ffprobe {
    pub executable: PathBuf,
    pub timeout: Duration,
    pub max_output: usize,
}

impl Default for Ffprobe {
    fn default() -> Self {
        Self {
            executable: "ffprobe".into(),
            timeout: Duration::from_secs(10),
            max_output: 256 * 1024,
        }
    }
}

impl Probe for Ffprobe {
    fn inspect(&self, path: &Path) -> Result<Track, Error> {
        let file = File::open(path).map_err(|_| Error::Unavailable)?;
        self.inspect_file(path, file)
    }
    fn inspect_file(&self, path: &Path, file: File) -> Result<Track, Error> {
        let mut child = Command::new(&self.executable)
            .args([
                "-v",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-format_whitelist",
                "mp3,flac,ogg,mov,aac,wav",
                "-enable_drefs",
                "0",
                "-use_absolute_path",
                "0",
                "-show_streams",
                "-show_format",
                "-of",
                "json",
                "-i",
            ])
            .arg("/proc/self/fd/0")
            .stdin(Stdio::from(file))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(|_| Error::Unavailable)?;
        let mut stdout = child.stdout.take().ok_or(Error::Failed)?;
        let fd = stdout.as_raw_fd();
        // SAFETY: stdout owns this live fd for the whole polling loop.
        let nonblocking = unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            flags >= 0 && libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) >= 0
        };
        let started = Instant::now();
        let mut bytes = Vec::new();
        let result = (|| {
            if !nonblocking {
                return Err(Error::Failed);
            }
            let mut buffer = [0; 8192];
            loop {
                if started.elapsed() >= self.timeout {
                    return Err(Error::Timeout);
                }
                // Bounded reads per cycle keep timeout checks live even if the
                // subprocess continuously fills the pipe.
                for _ in 0..32 {
                    match stdout.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            if n > self.max_output.saturating_sub(bytes.len()) {
                                return Err(Error::TooLarge);
                            }
                            bytes.extend_from_slice(&buffer[..n]);
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(_) => return Err(Error::Failed),
                    }
                }
                if let Some(success) = process_status(child.id()).map_err(|_| Error::Failed)? {
                    if !success {
                        return Err(Error::Failed);
                    }
                    // The process has exited: remaining pipe data is finite, but
                    // descendants still cannot extend our deadline or byte cap.
                    match stdout.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            if n > self.max_output.saturating_sub(bytes.len()) {
                                return Err(Error::TooLarge);
                            }
                            bytes.extend_from_slice(&buffer[..n]);
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                        Err(_) => return Err(Error::Failed),
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let value = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
            crate::metadata::parse(
                &value,
                path.file_stem().and_then(|p| p.to_str()).unwrap_or(""),
            )
            .ok_or(Error::Invalid)
        })();
        // SAFETY: a new private process group was created above, using the PID
        // of our owned subprocess; this cannot target the desktop's group.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
        let _ = child.wait();
        result
    }
}

// Observe exit without reaping: the owned PID cannot be reused before group
// cleanup, even if a helper leaves a descendant holding the output pipe open.
pub(crate) fn process_status(pid: u32) -> io::Result<Option<bool>> {
    // SAFETY: zero is a valid initialization for this C output struct.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    // SAFETY: info is writable and pid identifies our direct child.
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful waitid populated the SIGCHLD fields, or zeroed them.
    unsafe {
        Ok(if info.si_pid() == 0 {
            None
        } else {
            Some(info.si_code == libc::CLD_EXITED && info.si_status() == 0)
        })
    }
}
