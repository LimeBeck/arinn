// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An owned audio-only process. IPC/file/stream data never leaves this module.
use arinn_protocol::validation::ErrorCode;
use serde_json::{Value, json};
use std::{
    fs::{self, DirBuilder, File},
    io::{BufRead, BufReader, Write},
    os::{
        fd::AsRawFd,
        unix::{fs::DirBuilderExt, net::UnixStream, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

pub enum Source {
    File(File),
    Stream { url: String, headers: Vec<String> },
}
pub struct Snapshot {
    pub paused: bool,
    pub idle: bool,
    pub ready: bool,
    pub failed: bool,
    pub position_us: u64,
    pub duration_us: u64,
    pub eof: Option<u64>,
}
enum Operation {
    Ack(u64, tokio::sync::oneshot::Sender<Result<(), ErrorCode>>),
    Load(Source, tokio::sync::oneshot::Sender<Result<(), ErrorCode>>),
    Command(Value, tokio::sync::oneshot::Sender<Result<(), ErrorCode>>),
    Poll(tokio::sync::oneshot::Sender<Result<Snapshot, ErrorCode>>),
    Stop(tokio::sync::oneshot::Sender<Result<(), ErrorCode>>),
}
#[derive(Clone)]
pub struct Player {
    sender: mpsc::SyncSender<Operation>,
}
impl Player {
    pub fn new(runtime: &Path, executable: PathBuf) -> Result<Self, ErrorCode> {
        let base = runtime.to_path_buf();
        let (sender, receiver) = mpsc::sync_channel(32);
        std::thread::Builder::new()
            .name("music-audio".into())
            .spawn(move || {
                let mut process: Option<Process> = None;
                while let Ok(operation) = receiver.recv() {
                    match operation {
                        Operation::Ack(id, reply) => {
                            if let Some(p) = process.as_mut() {
                                p.events.eof.retain(|entry| *entry != id);
                            }
                            let _ = reply.send(Ok(()));
                        }
                        Operation::Load(source, reply) => {
                            let result = (|| {
                                if process.is_none() {
                                    process = Some(Process::start(&base, &executable)?);
                                }
                                process.as_mut().ok_or(ErrorCode::Unavailable)?.load(source)
                            })();
                            let _ = reply.send(result);
                        }
                        Operation::Command(command, reply) => {
                            let result = process
                                .as_mut()
                                .ok_or(ErrorCode::Unavailable)
                                .and_then(|p| p.request(command).map(|_| ()));
                            let _ = reply.send(result);
                        }
                        Operation::Poll(reply) => {
                            let result = process
                                .as_mut()
                                .ok_or(ErrorCode::Unavailable)
                                .and_then(Process::poll);
                            let _ = reply.send(result);
                        }
                        Operation::Stop(reply) => {
                            drop(process.take());
                            let _ = reply.send(Ok(()));
                        }
                    }
                }
            })
            .map_err(|_| ErrorCode::Unavailable)?;
        Ok(Self { sender })
    }
    pub async fn acknowledge_eof(&self, id: u64) -> Result<(), ErrorCode> {
        let (s, r) = tokio::sync::oneshot::channel();
        self.sender
            .try_send(Operation::Ack(id, s))
            .map_err(|_| ErrorCode::Unavailable)?;
        r.await.map_err(|_| ErrorCode::Unavailable)?
    }
    pub async fn load(&self, source: Source) -> Result<(), ErrorCode> {
        let (s, r) = tokio::sync::oneshot::channel();
        self.sender
            .try_send(Operation::Load(source, s))
            .map_err(|_| ErrorCode::Unavailable)?;
        r.await.map_err(|_| ErrorCode::Unavailable)?
    }
    pub async fn command(&self, command: Value) -> Result<(), ErrorCode> {
        let (s, r) = tokio::sync::oneshot::channel();
        self.sender
            .try_send(Operation::Command(command, s))
            .map_err(|_| ErrorCode::Unavailable)?;
        r.await.map_err(|_| ErrorCode::Unavailable)?
    }
    pub async fn poll(&self) -> Result<Snapshot, ErrorCode> {
        let (s, r) = tokio::sync::oneshot::channel();
        self.sender
            .try_send(Operation::Poll(s))
            .map_err(|_| ErrorCode::Unavailable)?;
        r.await.map_err(|_| ErrorCode::Unavailable)?
    }
    pub async fn stop(&self) -> Result<(), ErrorCode> {
        let (s, r) = tokio::sync::oneshot::channel();
        self.sender
            .try_send(Operation::Stop(s))
            .map_err(|_| ErrorCode::Unavailable)?;
        r.await.map_err(|_| ErrorCode::Unavailable)?
    }
}
#[derive(Default)]
struct Events {
    started: Option<u64>,
    loaded: Vec<u64>,
    eof: Vec<u64>,
    failed: Vec<u64>,
}
impl Events {
    fn observe(&mut self, value: &Value) {
        match value.get("event").and_then(Value::as_str) {
            Some("start-file") => {
                self.started = value.get("playlist_entry_id").and_then(Value::as_u64)
            }
            Some("file-loaded") => {
                if let Some(id) = self.started {
                    remember(&mut self.loaded, id);
                }
            }
            Some("end-file") => {
                if let Some(id) = value.get("playlist_entry_id").and_then(Value::as_u64) {
                    match value.get("reason").and_then(Value::as_str) {
                        Some("eof") => remember(&mut self.eof, id),
                        Some("error") => remember(&mut self.failed, id),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}
fn remember(values: &mut Vec<u64>, id: u64) {
    if !values.contains(&id) {
        values.push(id);
    }
    while values.len() > 64 {
        values.remove(0);
    }
}
struct Process {
    child: Child,
    directory: PathBuf,
    reader: BufReader<UnixStream>,
    serial: u64,
    entry: Option<u64>,
    events: Events,
    file: Option<File>,
}
impl Process {
    fn start(base: &Path, executable: &Path) -> Result<Self, ErrorCode> {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(base).map_err(|_| ErrorCode::Unavailable)?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o022 != 0
        {
            return Err(ErrorCode::PermissionDenied);
        }
        let directory = base.join(format!("arinn-music-{}", uuid::Uuid::new_v4()));
        DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .map_err(|_| ErrorCode::Unavailable)?;
        let socket = directory.join("audio.sock");
        let parent = std::process::id() as libc::pid_t;
        let mut command = Command::new(executable);
        // SAFETY: the child-side hook uses only async-signal-safe C calls.
        // Kill this owned audio process if its creating provider thread dies.
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
                }
                Ok(())
            });
        }
        let child = command
            .args([
                "--no-config",
                "--idle=yes",
                "--no-video",
                "--force-window=no",
                "--audio-display=no",
                "--terminal=no",
                "--osc=no",
                "--input-default-bindings=no",
                "--ytdl=no",
            ])
            .arg(format!(
                "--input-ipc-server={}",
                socket.to_str().ok_or(ErrorCode::InvalidArgument)?
            ))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(_) => {
                let _ = fs::remove_dir(&directory);
                return Err(ErrorCode::Unavailable);
            }
        };
        let begun = Instant::now();
        let stream = loop {
            match UnixStream::connect(&socket) {
                Ok(s) => break s,
                Err(_) => {
                    if begun.elapsed() > Duration::from_secs(3)
                        || child.try_wait().ok().flatten().is_some()
                    {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = fs::remove_dir_all(&directory);
                        return Err(ErrorCode::Unavailable);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        };
        let this = Self {
            child,
            directory,
            reader: BufReader::new(stream),
            serial: 0,
            entry: None,
            events: Events::default(),
            file: None,
        };
        this.reader
            .get_ref()
            .set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|_| ErrorCode::Unavailable)?;
        this.reader
            .get_ref()
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|_| ErrorCode::Unavailable)?;
        Ok(this)
    }
    fn request(&mut self, command: Value) -> Result<Value, ErrorCode> {
        self.serial = self.serial.checked_add(1).ok_or(ErrorCode::Internal)?;
        let mut line = serde_json::to_vec(&json!({"command":command,"request_id":self.serial}))
            .map_err(|_| ErrorCode::Internal)?;
        line.push(b'\n');
        self.reader
            .get_mut()
            .write_all(&line)
            .map_err(|_| ErrorCode::Unavailable)?;
        let begun = Instant::now();
        let mut line = Vec::new();
        loop {
            if begun.elapsed() > Duration::from_secs(2) {
                return Err(ErrorCode::Timeout);
            }
            // Read a bounded frame; no mpv diagnostics or source fields escape.
            let available = match self.reader.fill_buf() {
                Ok(b) => b,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue;
                }
                Err(_) => return Err(ErrorCode::Unavailable),
            };
            if available.is_empty() {
                return Err(ErrorCode::Unavailable);
            }
            let count = available
                .iter()
                .position(|b| *b == b'\n')
                .map(|n| n + 1)
                .unwrap_or(available.len());
            if line.len() + count > 256 * 1024 {
                return Err(ErrorCode::InvalidArgument);
            }
            let complete = available[count - 1] == b'\n';
            line.extend_from_slice(&available[..count]);
            self.reader.consume(count);
            if !complete {
                continue;
            }
            let value: Value =
                serde_json::from_slice(&line).map_err(|_| ErrorCode::InvalidArgument)?;
            line.clear();
            self.events.observe(&value);
            if value.get("request_id").and_then(Value::as_u64) == Some(self.serial) {
                if value.get("error").and_then(Value::as_str) != Some("success") {
                    return Err(match value.get("error").and_then(Value::as_str) {
                        Some("property unavailable" | "property not found") => ErrorCode::NotFound,
                        _ => ErrorCode::Unavailable,
                    });
                }
                return Ok(value.get("data").cloned().unwrap_or(Value::Null));
            }
        }
    }
    fn load(&mut self, source: Source) -> Result<(), ErrorCode> {
        let (path, headers, file) = match source {
            Source::File(file) => (
                format!("/proc/{}/fd/{}", std::process::id(), file.as_raw_fd()),
                vec![],
                Some(file),
            ),
            Source::Stream { url, headers } => {
                if !url.starts_with("https://")
                    || url.len() > 16 * 1024
                    || headers.len() > 16
                    || headers
                        .iter()
                        .any(|h| h.len() > 8192 || h.chars().any(char::is_control))
                {
                    return Err(ErrorCode::InvalidArgument);
                }
                (url, headers, None)
            }
        };
        self.request(json!(["set_property", "http-header-fields", headers]))?;
        self.request(json!(["loadfile", path, "replace"]))?;
        self.file = file;
        self.entry = self
            .request(json!(["get_property", "playlist/0/id"]))?
            .as_u64();
        if self.entry.is_none() {
            return Err(ErrorCode::Unavailable);
        }
        self.request(json!(["set_property", "pause", false]))?;
        Ok(())
    }
    fn optional_time(&mut self, name: &str) -> Result<u64, ErrorCode> {
        match self.request(json!(["get_property", name])) {
            Ok(value) => Ok(micros(value)),
            Err(ErrorCode::NotFound) => Ok(0),
            Err(error) => Err(error),
        }
    }
    fn poll(&mut self) -> Result<Snapshot, ErrorCode> {
        let idle = self
            .request(json!(["get_property", "idle-active"]))?
            .as_bool()
            .ok_or(ErrorCode::Unavailable)?;
        let paused = self
            .request(json!(["get_property", "pause"]))?
            .as_bool()
            .ok_or(ErrorCode::Unavailable)?;
        let (position_us, duration_us) = if idle {
            (0, 0)
        } else {
            (
                self.optional_time("time-pos")?,
                self.optional_time("duration")?,
            )
        };
        let entry = self.entry;
        let eof = entry.filter(|id| self.events.eof.contains(id));
        let ready = entry.is_some_and(|id| self.events.loaded.contains(&id));
        let failed = entry.is_some_and(|id| self.events.failed.contains(&id));
        Ok(Snapshot {
            paused,
            idle: idle || eof.is_some(),
            ready,
            failed,
            position_us,
            duration_us,
            eof,
        })
    }
}
fn micros(value: Value) -> u64 {
    value
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0 && *n <= 31_536_000.0)
        .map(|n| (n * 1_000_000.0) as u64)
        .unwrap_or(0)
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    // The process is an owned sleep fixture; all player replies use a private
    // socket pair. This never launches mpv, touches audio or reads user files.
    fn fixture() -> (Process, Arc<Mutex<Vec<Value>>>) {
        let directory =
            std::env::temp_dir().join(format!("arinn-music-ipc-test-{}", uuid::Uuid::new_v4()));
        DirBuilder::new().mode(0o700).create(&directory).unwrap();
        let (client, server) = UnixStream::pair().unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let events = Arc::new(Mutex::new(Vec::<Value>::new()));
        let outbound = events.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(server);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let request: Value = serde_json::from_str(&line).unwrap();
                for event in outbound.lock().unwrap().drain(..) {
                    if writeln!(reader.get_mut(), "{event}").is_err() {
                        return;
                    }
                }
                let command = &request["command"];
                let (data, error) = match (command[0].as_str(), command[1].as_str()) {
                    (Some("get_property"), Some("playlist/0/id")) => (json!(7), "success"),
                    (Some("get_property"), Some("idle-active" | "pause")) => {
                        (json!(false), "success")
                    }
                    (Some("get_property"), Some("time-pos" | "duration")) => {
                        (Value::Null, "property unavailable")
                    }
                    _ => (Value::Null, "success"),
                };
                let reply = json!({"request_id":request["request_id"],"error":error,"data":data});
                if writeln!(reader.get_mut(), "{reply}").is_err() {
                    return;
                }
            }
        });
        (
            Process {
                child: Command::new("sleep").arg("60").spawn().unwrap(),
                directory,
                reader: BufReader::new(client),
                serial: 0,
                entry: None,
                events: Events::default(),
                file: None,
            },
            events,
        )
    }
    fn source() -> Source {
        Source::File(File::open("/dev/null").unwrap())
    }
    #[test]
    fn load_receipt_is_not_file_loaded_and_unknown_timing_is_safe() {
        let (mut process, events) = fixture();
        process.load(source()).unwrap();
        let snapshot = process.poll().unwrap();
        assert!(!snapshot.ready);
        assert!(!snapshot.failed);
        assert_eq!(snapshot.duration_us, 0);
        events.lock().unwrap().extend([
            json!({"event":"start-file","playlist_entry_id":7}),
            json!({"event":"file-loaded"}),
        ]);
        assert!(process.poll().unwrap().ready);
    }
    #[test]
    fn eof_interleaved_with_transport_survives_until_acknowledged() {
        let (mut process, events) = fixture();
        process.load(source()).unwrap();
        events
            .lock()
            .unwrap()
            .push(json!({"event":"end-file","playlist_entry_id":7,"reason":"eof"}));
        process
            .request(json!(["set_property", "pause", true]))
            .unwrap();
        assert_eq!(process.poll().unwrap().eof, Some(7));
        assert_eq!(process.poll().unwrap().eof, Some(7));
        process.events.eof.retain(|id| *id != 7);
        assert_eq!(process.poll().unwrap().eof, None);
    }
    #[test]
    fn native_failure_is_not_a_natural_end() {
        let (mut process, events) = fixture();
        process.load(source()).unwrap();
        events
            .lock()
            .unwrap()
            .push(json!({"event":"end-file","playlist_entry_id":7,"reason":"error"}));
        let snapshot = process.poll().unwrap();
        assert!(snapshot.failed);
        assert_eq!(snapshot.eof, None);
    }
    // Executed only as an owned subprocess by the parent-death regression.
    #[test]
    #[ignore]
    fn provider_owner_fixture() {
        let directory = PathBuf::from(std::env::var("ARINN_TEST_AUDIO_OWNER_DIRECTORY").unwrap());
        let process = Process::start(&directory, &directory.join("fixture.py")).unwrap();
        std::fs::write(directory.join("audio.pid"), process.child.id().to_string()).unwrap();
        loop {
            std::thread::park();
        }
    }
    #[test]
    fn killing_provider_also_terminates_its_owned_audio_child() {
        use std::os::unix::fs::PermissionsExt;
        let directory = std::env::temp_dir().join(format!("am-owner-{}", uuid::Uuid::new_v4()));
        DirBuilder::new().mode(0o700).create(&directory).unwrap();
        let executable = directory.join("fixture.py");
        std::fs::write(
            &executable,
            r#"#!/usr/bin/python3
import socket,sys,time
path=next(a.split('=',1)[1] for a in sys.argv if a.startswith('--input-ipc-server='))
server=socket.socket(socket.AF_UNIX);server.bind(path);server.listen(1)
connection,_=server.accept()
time.sleep(30)
"#,
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut owner = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "player::tests::provider_owner_fixture",
            ])
            .env("ARINN_TEST_AUDIO_OWNER_DIRECTORY", &directory)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let start = Instant::now();
        let pid = loop {
            if let Ok(bytes) = std::fs::read_to_string(directory.join("audio.pid")) {
                break bytes.parse::<i32>().unwrap();
            }
            if start.elapsed() > Duration::from_secs(8) {
                let _ = owner.kill();
                let _ = owner.wait();
                panic!("owned provider fixture did not start");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        // SAFETY: pid came from our live child fixture; pidfd pins its identity.
        let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
        assert!(descriptor >= 0);
        owner.kill().unwrap();
        owner.wait().unwrap();
        let mut poll = libc::pollfd {
            fd: descriptor,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll points to one valid pollfd; close releases our descriptor.
        let exited = unsafe { libc::poll(&mut poll, 1, 3000) };
        if exited == 0 {
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    descriptor,
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                );
            }
        }
        unsafe {
            libc::close(descriptor);
        }
        assert!(
            exited > 0,
            "audio survived the death of its owning provider"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
