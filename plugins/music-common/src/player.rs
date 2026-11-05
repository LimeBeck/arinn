// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An owned audio-only process. IPC/file/stream data never leaves this module.
use arinn_protocol::validation::ErrorCode;
use serde_json::{Value, json};
use std::{
    fs::{self, DirBuilder, File},
    io::{BufRead, BufReader, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{fs::DirBuilderExt, net::UnixStream, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

/// One anonymous RAM buffer; preloading never writes audio to the SSD.
pub fn audio_buffer() -> Result<File, ErrorCode> {
    // SAFETY: fixed NUL-terminated name; returned descriptor has one owner.
    let fd = unsafe { libc::memfd_create(c"arinn-audio".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(ErrorCode::Unavailable);
    }
    // SAFETY: memfd_create returned a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}
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
    partial: Vec<u8>,
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
        let Some(socket_text) = socket.to_str().map(str::to_owned) else {
            let _ = fs::remove_dir(&directory);
            return Err(ErrorCode::InvalidArgument);
        };
        let parent = std::process::id() as libc::pid_t;
        let mut command = Command::new(executable);
        // mpv runs in its own process group, so a group kill of the provider
        // does not reach it. Tie it to this thread's life instead: it gets
        // SIGKILL when the provider dies, including SIGKILL (review of #710).
        // SAFETY: prctl and getppid are async-signal-safe and touch no
        // memory shared with the parent.
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    return Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
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
            .arg(format!("--input-ipc-server={socket_text}"))
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
            partial: Vec::new(),
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
        // A frame cut off by a timeout stays here and is completed by the
        // next request instead of being parsed from its middle.
        let mut line = std::mem::take(&mut self.partial);
        loop {
            if begun.elapsed() > Duration::from_secs(2) {
                self.partial = line;
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
                partial: Vec::new(),
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
    /// Review of #710: after a timeout the partial frame was dropped, and the
    /// next request parsed the rest of it from the middle and failed.
    #[test]
    fn a_frame_cut_off_by_a_timeout_is_completed_by_the_next_request() {
        let directory =
            std::env::temp_dir().join(format!("arinn-music-ipc-test-{}", uuid::Uuid::new_v4()));
        DirBuilder::new().mode(0o700).create(&directory).unwrap();
        let (client, server) = UnixStream::pair().unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(server);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let first: Value = serde_json::from_str(&line).unwrap();
            let reply =
                json!({"request_id":first["request_id"],"error":"success","data":1}).to_string();
            let (head, tail) = reply.split_at(reply.len() / 2);
            write!(reader.get_mut(), "{head}").unwrap();
            reader.get_mut().flush().unwrap();
            std::thread::sleep(Duration::from_millis(2300));
            writeln!(reader.get_mut(), "{tail}").unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            let second: Value = serde_json::from_str(&line).unwrap();
            let reply = json!({"request_id":second["request_id"],"error":"success","data":2});
            writeln!(reader.get_mut(), "{reply}").unwrap();
        });
        let mut process = Process {
            child: Command::new("sleep").arg("60").spawn().unwrap(),
            directory,
            reader: BufReader::new(client),
            partial: Vec::new(),
            serial: 0,
            entry: None,
            events: Events::default(),
            file: None,
        };
        assert_eq!(
            process.request(json!(["get_property", "pause"])),
            Err(ErrorCode::Timeout)
        );
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(
            process.request(json!(["get_property", "pause"])),
            Ok(json!(2))
        );
    }
    /// Review of #710: mpv ran in its own process group without a parent
    /// death signal, so a provider killed with SIGKILL left it playing. The
    /// test runs a second copy of itself as the "provider", kills it and
    /// checks the fixture player is gone. Like mpv, the fixture keeps running
    /// when its IPC client disconnects.
    #[test]
    fn the_audio_process_dies_with_a_killed_provider() {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(directory) = std::env::var("ARINN_MUSIC_ORPHAN_PROVIDER") {
            let directory = PathBuf::from(directory);
            let player = Player::new(&directory, directory.join("fake-mpv")).unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let _ = runtime.block_on(player.load(source()));
            std::fs::write(directory.join("loaded"), b"").unwrap();
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        // Short: the player's socket path below it must fit sun_path.
        let directory = std::env::temp_dir().join(format!(
            "amo-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        DirBuilder::new().mode(0o700).create(&directory).unwrap();
        let fake = directory.join("fake-mpv");
        std::fs::write(
            &fake,
            format!(
                "#!/usr/bin/python3\nimport json,os,socket,sys,time\n\
                 open({pid:?},'w').write(str(os.getpid()))\n\
                 path=next(x.split('=',1)[1] for x in sys.argv if x.startswith('--input-ipc-server='))\n\
                 s=socket.socket(socket.AF_UNIX);s.bind(path);s.listen(1);c,_=s.accept()\n\
                 f=c.makefile('rwb',buffering=0)\n\
                 for line in f:\n \
                 f.write((json.dumps({{'request_id':json.loads(line)['request_id'],'error':'success','data':None}})+'\\n').encode())\n\
                 time.sleep(120)\n",
                pid = directory.join("player.pid").to_str().unwrap()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut provider = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "player::tests::the_audio_process_dies_with_a_killed_provider",
                "--nocapture",
            ])
            .env("ARINN_MUSIC_ORPHAN_PROVIDER", &directory)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let begun = Instant::now();
        let pid = loop {
            if let Ok(text) = std::fs::read_to_string(directory.join("player.pid"))
                && let Ok(pid) = text.trim().parse::<libc::pid_t>()
                && directory.join("loaded").exists()
            {
                break pid;
            }
            assert!(
                begun.elapsed() < Duration::from_secs(10),
                "fixture player did not start"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        // Pin the owned fixture's identity before its provider exits. A
        // container's PID 1 may leave it as a zombie, for which kill(pid, 0)
        // still succeeds despite the process having exited.
        let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
        assert!(descriptor >= 0, "open the owned fixture pidfd");
        // SAFETY: pidfd_open returned a new descriptor owned by this test.
        let handle =
            unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(descriptor) };
        let mut event = libc::pollfd {
            fd: handle.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: event describes one valid descriptor; zero timeout only probes.
        assert_eq!(
            unsafe { libc::poll(&mut event, 1, 0) },
            0,
            "the fixture player runs"
        );
        provider.kill().unwrap();
        provider.wait().unwrap();
        // A readable pidfd means exit, independently of orphan reaping.
        // SAFETY: event and its owned descriptor remain valid during poll.
        let exited = unsafe { libc::poll(&mut event, 1, 5000) };
        if exited == 0 {
            // SAFETY: the pidfd identifies only our fixture, without PID reuse.
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    handle.as_raw_fd(),
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                );
            }
        }
        assert!(
            exited > 0 && event.revents & libc::POLLIN != 0,
            "the audio process outlived its provider"
        );
        let _ = std::fs::remove_dir_all(directory);
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
}
