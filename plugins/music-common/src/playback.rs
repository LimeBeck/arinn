// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{
    Track,
    player::{Player, Source},
    queue::{MAX_UPCOMING, Queue},
};
use arinn_protocol::{
    music::{self, Command, Entity, QueueEntry, State},
    validation::ErrorCode,
};
use serde_json::json;
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{Arc, Weak},
};
use tokio::sync::Mutex;
pub trait Resolver: Send + Sync + 'static {
    fn resolve(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>>;
}
struct Data {
    queue: Queue,
    tracks: BTreeMap<String, Track>,
    tail: VecDeque<String>,
    session: String,
    token: u64,
    revision: u64,
    position: u64,
    duration: u64,
    status: String,
    error: Option<ErrorCode>,
    loaded: bool,
    /// The target of a seek to or past the reported duration. The end that
    /// follows is not a natural end; once playback runs a second past the
    /// target the duration was wrong and a later end is natural again.
    seek_end: Option<u64>,
    /// Consecutive failed polls; a single slow reply is not a stop.
    failures: u8,
}
pub struct Playback {
    data: Mutex<Data>,
    io: Mutex<()>,
    player: Player,
    resolver: Arc<dyn Resolver>,
}
impl Playback {
    pub fn new(player: Player, resolver: Arc<dyn Resolver>) -> Arc<Self> {
        let this = Arc::new(Self {
            data: Mutex::new(Data {
                queue: Queue::default(),
                tracks: BTreeMap::new(),
                tail: VecDeque::new(),
                session: String::new(),
                token: 0,
                revision: 1,
                position: 0,
                duration: 0,
                status: "stopped".into(),
                error: None,
                loaded: false,
                seek_end: None,
                failures: 0,
            }),
            io: Mutex::new(()),
            player,
            resolver,
        });
        let weak = Arc::downgrade(&this);
        tokio::spawn(async move {
            Self::poll_loop(weak).await;
        });
        this
    }
    pub async fn state(&self) -> State {
        let d = self.data.lock().await;
        let entry = |e: &crate::queue::Entry| {
            d.tracks.get(&e.track).map(|t| QueueEntry {
                occurrence: e.occurrence.to_string(),
                track: entity(t),
            })
        };
        let cursor = d.queue.current().map(|e| e.occurrence);
        let index = d
            .queue
            .entries()
            .iter()
            .position(|e| Some(e.occurrence) == cursor)
            .unwrap_or(0);
        let mut actions = vec!["play".into()];
        if cursor.is_some() {
            actions.push("stop".into());
            if d.status == "playing" {
                actions.push("pause".into());
            }
            if d.status == "paused" {
                actions.push("resume".into());
            }
            if d.duration > 0 && d.loaded && ["playing", "paused"].contains(&d.status.as_str()) {
                actions.push("seek".into());
            }
            if index > 0 {
                actions.push("previous".into());
            }
            if d.queue.upcoming() > 0 || !d.tail.is_empty() {
                actions.push("next".into());
                actions.push("remove".into());
            }
        }
        State {
            revision: d.revision,
            backend: "available".into(),
            account: "local".into(),
            facets: music::FACETS[..4].iter().map(|s| s.to_string()).collect(),
            actions,
            session: d.session.clone(),
            playback: d.status.clone(),
            position_us: d.position,
            current: d.queue.current().and_then(entry).map(|mut current| {
                if d.duration > 0 {
                    current.track.duration_us = d.duration;
                }
                current
            }),
            history: d
                .queue
                .entries()
                .iter()
                .take(index)
                .filter_map(entry)
                .collect(),
            upcoming: d
                .queue
                .entries()
                .iter()
                .skip(index + 1)
                .filter_map(entry)
                .collect(),
            error: d.error,
            ..State::default()
        }
    }
    pub async fn play(self: &Arc<Self>, tracks: Vec<Track>) -> Result<State, ErrorCode> {
        if tracks.is_empty() || tracks.len() > 100_000 {
            return Err(ErrorCode::NotFound);
        }
        let token = {
            let mut d = self.data.lock().await;
            d.token = d.token.checked_add(1).ok_or(ErrorCode::Internal)?;
            d.session = uuid::Uuid::new_v4().to_string();
            d.tail = tracks.iter().skip(65).map(|t| t.id.clone()).collect();
            d.queue
                .start(tracks.iter().take(65).map(|t| t.id.clone()).collect(), None)
                .map_err(|_| ErrorCode::InvalidArgument)?;
            d.tracks = tracks.into_iter().map(|t| (t.id.clone(), t)).collect();
            d.status = "loading".into();
            d.loaded = false;
            d.position = 0;
            d.duration = 0;
            d.seek_end = None;
            d.error = None;
            d.revision += 1;
            d.token
        };
        let owner = self.clone();
        tokio::spawn(async move {
            owner.load_current(token).await;
        });
        Ok(self.state().await)
    }
    async fn load_current(self: &Arc<Self>, token: u64) {
        let id = {
            let d = self.data.lock().await;
            if d.token != token {
                return;
            }
            let Some(entry) = d.queue.current() else {
                return;
            };
            entry.track.clone()
        };
        {
            let _io = self.io.lock().await;
            if self.data.lock().await.token != token {
                return;
            }
            if let Err(error) = self.player.stop().await {
                let mut d = self.data.lock().await;
                if d.token == token {
                    d.status = "stopped".into();
                    d.error = Some(error);
                    d.revision += 1;
                }
                return;
            }
        }
        let source = self.resolver.resolve(id).await;
        let _io = self.io.lock().await;
        if self.data.lock().await.token != token {
            return;
        }
        let result = match source {
            Ok(source) => self.player.load(source).await,
            Err(e) => Err(e),
        };
        if result.is_err() {
            let _ = self.player.stop().await;
        }
        let mut d = self.data.lock().await;
        if d.token != token {
            return;
        }
        match result {
            Ok(()) => {
                d.loaded = true;
            }
            Err(error) => {
                d.loaded = false;
                d.status = "stopped".into();
                d.error = Some(error);
            }
        }
        d.revision += 1;
    }
    pub async fn stop(&self) -> Result<State, ErrorCode> {
        let token = {
            let mut d = self.data.lock().await;
            d.token += 1;
            d.status = "loading".into();
            d.loaded = false;
            d.token
        };
        let _io = self.io.lock().await;
        // Without a player thread no audio process is left running, so the
        // session ends either way; the error is still reported, so a
        // replacement start waits for a confirmed stop.
        let stopped = self.player.stop().await;
        let mut d = self.data.lock().await;
        if d.token == token {
            d.queue.stop();
            d.tail.clear();
            d.tracks.clear();
            d.session.clear();
            d.status = "stopped".into();
            d.position = 0;
            d.error = stopped.err();
            d.revision += 1;
        }
        drop(d);
        stopped?;
        Ok(self.state().await)
    }
    pub async fn command(self: &Arc<Self>, command: &Command) -> Result<State, ErrorCode> {
        let expected = {
            let d = self.data.lock().await;
            if d.session != command.session || d.session.is_empty() {
                return Err(ErrorCode::Conflict);
            }
            if command.action == "seek" && d.duration == 0 {
                return Err(ErrorCode::Unsupported);
            }
            d.token
        };
        match command.action.as_str() {
            "stop" => return self.stop().await,
            "next" | "previous" => {
                let token = {
                    let mut d = self.data.lock().await;
                    step(&mut d, command.action == "next")?;
                    d.token += 1;
                    d.position = 0;
                    d.duration = 0;
                    d.loaded = false;
                    d.seek_end = None;
                    d.status = "loading".into();
                    d.revision += 1;
                    d.token
                };
                let owner = self.clone();
                tokio::spawn(async move {
                    owner.load_current(token).await;
                });
            }
            "remove" => {
                let mut d = self.data.lock().await;
                let occurrence = command
                    .occurrence
                    .parse()
                    .map_err(|_| ErrorCode::InvalidArgument)?;
                d.queue
                    .remove(occurrence)
                    .map_err(|_| ErrorCode::Unsupported)?;
                refill(&mut d)?;
                d.revision += 1;
            }
            "pause" | "resume" | "seek" => {
                let _io = self.io.lock().await;
                let seek_end = {
                    let d = self.data.lock().await;
                    if d.token != expected {
                        return Err(ErrorCode::Conflict);
                    }
                    (command.action == "seek"
                        && d.duration > 0
                        && command.position_us >= d.duration)
                        .then_some(command.position_us)
                };
                let value = if command.action == "seek" {
                    json!([
                        "seek",
                        command.position_us as f64 / 1_000_000.0,
                        "absolute+exact"
                    ])
                } else {
                    json!(["set_property", "pause", command.action == "pause"])
                };
                self.player.command(value).await?;
                let snapshot = self.player.poll().await?;
                let mut d = self.data.lock().await;
                if d.token != expected {
                    return Err(ErrorCode::Conflict);
                }
                d.position = snapshot.position_us;
                if snapshot.failed {
                    d.error = Some(ErrorCode::Unavailable);
                    d.loaded = false;
                }
                if snapshot.duration_us > 0 {
                    d.duration = snapshot.duration_us;
                }
                d.status = if snapshot.failed {
                    "stopped"
                } else if snapshot.idle {
                    "ended"
                } else if !snapshot.ready {
                    "loading"
                } else if snapshot.paused {
                    "paused"
                } else {
                    "playing"
                }
                .into();
                if command.action == "seek" {
                    d.seek_end = seek_end;
                }
                d.revision += 1;
            }
            _ => return Err(ErrorCode::Unsupported),
        }
        Ok(self.state().await)
    }
    async fn poll_loop(owner: Weak<Self>) {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let Some(this) = owner.upgrade() else {
                return;
            };
            let token = {
                let d = this.data.lock().await;
                if !d.loaded {
                    continue;
                }
                d.token
            };
            let io = this.io.lock().await;
            let result = this.player.poll().await;
            let mut d = this.data.lock().await;
            if d.token != token {
                continue;
            }
            let mut advance = false;
            let mut acknowledge = None;
            match result {
                Ok(snapshot) => {
                    d.failures = 0;
                    if d.seek_end.is_some_and(|target| {
                        snapshot.position_us > target.saturating_add(1_000_000)
                    }) {
                        d.seek_end = None;
                    }
                    d.position = snapshot.position_us;
                    if snapshot.failed {
                        d.error = Some(ErrorCode::Unavailable);
                        d.loaded = false;
                    }
                    if snapshot.duration_us > 0 {
                        d.duration = snapshot.duration_us;
                    }
                    let status = if snapshot.failed {
                        "stopped"
                    } else if snapshot.idle {
                        "ended"
                    } else if !snapshot.ready {
                        "loading"
                    } else if snapshot.paused {
                        "paused"
                    } else {
                        "playing"
                    };
                    if d.status != status {
                        d.status = status.into();
                        d.revision += 1;
                    }
                    if let Some(eof) = snapshot.eof {
                        acknowledge = Some(eof);
                        d.loaded = false;
                        if d.seek_end.is_none() {
                            let _ = refill(&mut d);
                            if let Some(occurrence) = d.queue.current().map(|e| e.occurrence)
                                && d.queue.confirmed_end(occurrence).is_ok()
                            {
                                d.token += 1;
                                d.position = 0;
                                d.duration = 0;
                                d.status = "loading".into();
                                d.seek_end = None;
                                d.revision += 1;
                                advance = true;
                            }
                        }
                    }
                }
                // One slow or garbled reply is not a stop. After three failed
                // polls in a row the player is stopped, so the reported
                // "stopped" is true and no unobserved audio keeps playing.
                Err(error) => {
                    d.failures = d.failures.saturating_add(1);
                    if d.failures >= 3 {
                        d.failures = 0;
                        d.error = Some(error);
                        d.status = "stopped".into();
                        d.loaded = false;
                        d.revision += 1;
                        drop(d);
                        let _ = this.player.stop().await;
                        drop(io);
                        continue;
                    }
                }
            }
            let next = d.token;
            drop(d);
            if let Some(id) = acknowledge {
                let _ = this.player.acknowledge_eof(id).await;
            }
            drop(io);
            if advance {
                this.load_current(next).await;
            }
        }
    }
}
/// Moves the finite queue one entry forward or back. The window stays within
/// its bounds: an entry pushed out by stepping back returns to the tail.
fn step(d: &mut Data, forward: bool) -> Result<(), ErrorCode> {
    refill(d)?;
    if forward {
        d.queue.advance().map_err(|_| ErrorCode::NotFound)?;
    } else if let Some(track) = d.queue.previous().map_err(|_| ErrorCode::NotFound)? {
        d.tail.push_front(track);
    }
    Ok(())
}
/// Tops the loaded window up from the finite tail. The tail is drained only
/// after the queue accepted the tracks, so a refused append loses nothing.
fn refill(d: &mut Data) -> Result<(), ErrorCode> {
    let count = MAX_UPCOMING
        .saturating_sub(d.queue.upcoming())
        .min(d.tail.len());
    if count == 0 {
        return Ok(());
    }
    let tracks: Vec<String> = d.tail.iter().take(count).cloned().collect();
    d.queue
        .append_finite(tracks)
        .map_err(|_| ErrorCode::Internal)?;
    d.tail.drain(..count);
    Ok(())
}
pub fn entity(t: &Track) -> Entity {
    let mut value = Entity {
        id: t.id.clone(),
        kind: "track".into(),
        title: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        artwork: String::new(),
        duration_us: t.duration_us,
        available: t.available,
    };
    // Reserve enough budget for all 97 retained queue occurrences, including
    // JSON escaping. Catalogue/private metadata keeps its original text.
    while serde_json::to_vec(&value).map_or(usize::MAX, |b| b.len()) > 2048 {
        let longest = [&mut value.title, &mut value.artist, &mut value.album]
            .into_iter()
            .max_by_key(|s| s.len())
            .expect("three text fields");
        if longest.is_empty() {
            break;
        }
        let end = longest
            .char_indices()
            .nth(longest.chars().count() / 2)
            .map(|(i, _)| i)
            .unwrap_or(0);
        longest.truncate(end);
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escaped_queue_metadata_fits_inside_active_state_envelope() {
        let track = Track {
            id: "x".repeat(256),
            title: "\\\"".repeat(2048),
            artist: "\\\"".repeat(2048),
            album: "\\\"".repeat(2048),
            available: true,
            ..Track::default()
        };
        let entity = entity(&track);
        assert!(serde_json::to_vec(&entity).unwrap().len() <= 2048);
        let entry = |i| QueueEntry {
            occurrence: format!("occurrence-{i}"),
            track: entity.clone(),
        };
        let state = State {
            session: "session".into(),
            current: Some(entry(32)),
            history: (0..32).map(entry).collect(),
            upcoming: (33..97).map(entry).collect(),
            ..State::default()
        };
        state.validate().unwrap();
        let envelope = music::Active {
            provider_id: "x".repeat(256),
            generation: "g".repeat(256),
            state,
        };
        music::encode(&Some(envelope)).unwrap();
    }
    /// Review of #710: stepping back on a full 64-entry window left 65
    /// upcoming entries (an invalid state), and the next refill underflowed
    /// `64 - 65`, panicking in debug builds and dropping the tail in release.
    #[test]
    fn stepping_back_on_a_full_window_keeps_bounds_and_every_track() {
        let ids: Vec<String> = (0..100).map(|i| format!("t{i}")).collect();
        let mut d = Data {
            queue: Queue::default(),
            tracks: BTreeMap::new(),
            tail: ids.iter().skip(65).cloned().collect(),
            session: String::new(),
            token: 0,
            revision: 1,
            position: 0,
            duration: 0,
            status: "stopped".into(),
            error: None,
            loaded: false,
            seek_end: None,
            failures: 0,
        };
        d.queue
            .start(ids.iter().take(65).cloned().collect(), None)
            .unwrap();
        let remaining = |d: &Data| d.queue.upcoming() + d.tail.len();
        assert_eq!(remaining(&d), 99);
        for forward in [true, true, false, false, true, false] {
            step(&mut d, forward).unwrap();
            assert!(d.queue.upcoming() <= MAX_UPCOMING);
        }
        assert_eq!(remaining(&d), 99, "no track was lost");
        // Every track is still reached once, in order, without an error.
        let mut played = Vec::new();
        while step(&mut d, true).is_ok() {
            assert!(d.queue.upcoming() <= MAX_UPCOMING);
            played.push(d.queue.current().unwrap().track.clone());
        }
        assert_eq!(played, ids[1..].to_vec());
    }
    struct FixtureResolver;
    impl Resolver for FixtureResolver {
        fn resolve(
            &self,
            _id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Source, ErrorCode>> + Send + '_>> {
            Box::pin(async { Ok(Source::File(std::fs::File::open("/dev/null").unwrap())) })
        }
    }
    /// Review of #710: one poll that missed its 2-second deadline reported
    /// "stopped" while the audio kept playing, and polling stopped for good.
    #[tokio::test]
    async fn a_single_slow_poll_is_not_a_stop() {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let directory = std::env::temp_dir().join(format!(
            "am-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let executable = directory.join("fixture.py");
        std::fs::write(&executable,r##"#!/usr/bin/python3
import json,socket,sys,time
path=next(x.split('=',1)[1] for x in sys.argv if x.startswith('--input-ipc-server='))
server=socket.socket(socket.AF_UNIX);server.bind(path);server.listen(1)
connection,_=server.accept();stream=connection.makefile('rwb',buffering=0)
positions=0
for line in stream:
 request=json.loads(line);command=request['command'];data=None;error='success'
 if command[0]=='loadfile':
  stream.write((json.dumps({'event':'start-file','playlist_entry_id':7})+'\n').encode())
  stream.write(b'{"event":"file-loaded"}\n')
 elif command[0]=='get_property':
  prop=command[1]
  if prop=='playlist/0/id':data=7
  elif prop in ('idle-active','pause'):data=False
  elif prop=='time-pos':
   positions+=1;data=positions
   if positions==3:time.sleep(2.5)
  elif prop=='duration':data=600
 stream.write((json.dumps({'request_id':request['request_id'],'data':data,'error':error})+'\n').encode())
"##).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let playback = Playback::new(
            Player::new(&directory, executable).unwrap(),
            Arc::new(FixtureResolver),
        );
        playback
            .play(vec![Track {
                id: "one".into(),
                available: true,
                ..Track::default()
            }])
            .await
            .unwrap();
        // The slow reply comes on the third poll; wait well past it.
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;
        let state = playback.state().await;
        assert_eq!(state.playback, "playing");
        assert_eq!(state.error, None);
        assert!(state.position_us >= 4_000_000, "polling continued");
        playback.stop().await.unwrap();
        drop(playback);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[tokio::test]
    async fn seeking_to_end_with_immediate_idle_eof_does_not_advance() {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let directory = std::env::temp_dir().join(format!("am-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let executable = directory.join("fixture.py");
        std::fs::write(&executable,r##"#!/usr/bin/python3
import json,socket,sys
path=next(x.split('=',1)[1] for x in sys.argv if x.startswith('--input-ipc-server='))
server=socket.socket(socket.AF_UNIX);server.bind(path);server.listen(1)
connection,_=server.accept();stream=connection.makefile('rwb',buffering=0)
idle=False
for line in stream:
 request=json.loads(line);command=request['command'];data=None;error='success'
 if command[0]=='loadfile':
  stream.write((json.dumps({'event':'start-file','playlist_entry_id':7})+'\n').encode())
  stream.write(b'{"event":"file-loaded"}\n')
 elif command[0]=='seek':
  idle=True
  stream.write(b'{"event":"end-file","playlist_entry_id":7,"reason":"eof"}\n')
 elif command[0]=='get_property':
  prop=command[1]
  if prop=='playlist/0/id':data=7
  elif prop=='idle-active':data=idle
  elif prop=='pause':data=False
  elif prop=='time-pos':data=0
  elif prop=='duration':data=10
 stream.write((json.dumps({'request_id':request['request_id'],'data':data,'error':error})+'\n').encode())
"##).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let playback = Playback::new(
            Player::new(&directory, executable).unwrap(),
            Arc::new(FixtureResolver),
        );
        playback
            .play(vec![
                Track {
                    id: "one".into(),
                    available: true,
                    ..Track::default()
                },
                Track {
                    id: "two".into(),
                    available: true,
                    ..Track::default()
                },
            ])
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            loop {
                if playback.state().await.playback == "playing" {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let session = playback.state().await.session;
        let result = playback
            .command(&Command {
                request_id: uuid::Uuid::new_v4().to_string(),
                action: "seek".into(),
                session,
                entity: String::new(),
                occurrence: String::new(),
                position_us: 10_000_000,
                replaces_provider: String::new(),
                replaces_session: String::new(),
            })
            .await
            .unwrap();
        assert_eq!(result.playback, "ended");
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                if !playback.data.lock().await.loaded {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(playback.state().await.current.unwrap().track.id, "one");
        playback.stop().await.unwrap();
        drop(playback);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
