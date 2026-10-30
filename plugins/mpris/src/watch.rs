// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Watch `org.mpris.MediaPlayer2.*` and serve `org.arinn.MediaProvider1`.
//! No rich interfaces. Artwork URLs are not read or forwarded.
use crate::map::{self, Caps, RawPlayer, Sample};
use arinn_plugin_sdk::runtime::{
    MediaAction, MediaHandler, MediaService, RegistrationContext, run_media_on,
};
use arinn_protocol::{media::Session, media_association, production, validation::ErrorCode};
use futures_util::StreamExt;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Notify};
use zbus::{
    Connection,
    proxy::CacheProperties,
    zvariant::{ObjectPath, OwnedValue},
};

const PLAYER_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const NO_TRACK: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";
/// One player that never answers must not hold the snapshot loop.
const READ_TIMEOUT: Duration = Duration::from_secs(1);

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait Player {
    fn play(&self) -> zbus::Result<()>;
    fn pause(&self) -> zbus::Result<()>;
    fn play_pause(&self) -> zbus::Result<()>;
    fn stop(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;
    fn seek(&self, offset: i64) -> zbus::Result<()>;
    fn set_position(&self, track_id: ObjectPath<'_>, position: i64) -> zbus::Result<()>;
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Clone)]
struct ObservedPlayer {
    raw: RawPlayer,
    process: u32,
    proxy_for: u32,
}

#[derive(Clone)]
enum PlayerRead {
    Pending,
    Failed,
    Ready(ObservedPlayer),
}

struct Snapshot {
    id: String,
    previous: Option<Sample>,
    published: Option<Session>,
    latest: Arc<Mutex<PlayerRead>>,
}

struct Tracked {
    bus_name: String,
    sample: Option<Sample>,
    session: Option<Session>,
    track_id: Option<String>,
    latest: Arc<Mutex<PlayerRead>>,
    listener: Option<tokio::task::JoinHandle<()>>,
    reader: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
struct Bridge {
    connection: Connection,
    players: Arc<Mutex<BTreeMap<String, Tracked>>>,
    force: Arc<Mutex<HashSet<String>>>,
    /// A tracked bus name changed owner since the last publish. The next
    /// publish takes a new snapshot revision even if the sessions look the
    /// same, so the daemon re-reads process IDs (ADR-0096).
    moved: Arc<AtomicBool>,
    wake: Arc<Notify>,
}

pub async fn serve(connection: Connection) -> Result<(), ErrorCode> {
    let bridge = Bridge {
        connection: connection.clone(),
        players: Arc::new(Mutex::new(BTreeMap::new())),
        force: Arc::new(Mutex::new(HashSet::new())),
        moved: Arc::new(AtomicBool::new(false)),
        wake: Arc::new(Notify::new()),
    };
    let service = MediaService::new(bridge.clone(), Vec::new(), "available")?;
    // Exported before registration so the daemon finds it by introspection.
    connection
        .object_server()
        .at(
            production::PROVIDER_PATH,
            Processes {
                bridge: bridge.clone(),
                registrations: service.registrations(),
            },
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    tokio::select! {
        result = bridge.watch(service.clone()) => result,
        result = run_media_on(connection, service) => result.map_err(|_| ErrorCode::Unavailable),
    }
}

/// `MediaProcess1` (ADR-0096): the process that owns a player's bus name,
/// for the daemon only. Read at call time, so a name that moves to another
/// process never reports a stale ID. Never logged.
struct Processes {
    bridge: Bridge,
    registrations: tokio::sync::watch::Receiver<Option<RegistrationContext>>,
}
impl Processes {
    /// The caller first, then the arguments: anyone but the daemon gets
    /// `PermissionDenied` whatever it sends.
    fn check(&self, sender: &str, generation: &str, id: &str) -> Result<(), ErrorCode> {
        let current = self.registrations.borrow();
        // Unregistered, nobody is the daemon: refuse every caller alike, so
        // the error does not reveal the registration state.
        let current = current.as_ref().ok_or(ErrorCode::PermissionDenied)?;
        if current.daemon_owner != sender {
            return Err(ErrorCode::PermissionDenied);
        }
        media_association::validate_process_query(generation, id)?;
        if current.generation != generation {
            return Err(ErrorCode::Conflict);
        }
        Ok(())
    }
}
#[zbus::interface(name = "org.arinn.MediaProcess1")]
impl Processes {
    async fn get_process_id(
        &self,
        generation: &str,
        id: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> production::Result<u32> {
        let sender = header.sender().ok_or(ErrorCode::PermissionDenied)?.as_str();
        self.check(sender, generation, id)?;
        let bus_name = {
            let players = self.bridge.players.lock().await;
            let tracked = players.get(id).ok_or(ErrorCode::NotFound)?;
            if tracked.session.is_none() {
                return Err(ErrorCode::NotFound.into());
            }
            tracked.bus_name.clone()
        };
        let Ok(name) = zbus::names::BusName::try_from(bus_name.as_str()) else {
            return Ok(media_association::UNKNOWN_PROCESS);
        };
        let process = match zbus::fdo::DBusProxy::new(&self.bridge.connection).await {
            Ok(bus) => tokio::time::timeout(READ_TIMEOUT, bus.get_connection_unix_process_id(name))
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or(media_association::UNKNOWN_PROCESS),
            Err(_) => media_association::UNKNOWN_PROCESS,
        };
        // A registration that changed meanwhile must not get this answer.
        self.check(sender, generation, id)?;
        Ok(process)
    }
}

impl Bridge {
    async fn watch(&self, service: MediaService) -> Result<(), ErrorCode> {
        let bus = zbus::fdo::DBusProxy::new(&self.connection)
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        // Poll the signal stream on its own task. An unpolled stream fills the
        // connection and method replies, including registration, never arrive.
        let mut owners = bus
            .receive_name_owner_changed()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let (names, mut name_events) = tokio::sync::mpsc::unbounded_channel();
        let _forward = AbortOnDrop(tokio::spawn(async move {
            while let Some(event) = owners.next().await {
                if names.send(event).is_err() {
                    break;
                }
            }
        }));
        self.rescan(&bus).await;
        self.publish(&service).await?;
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            // Name changes first. A slow read must not sit ahead of them.
            tokio::select! {
                biased;
                event = name_events.recv() => {
                    let Some(event) = event else {
                        return Err(ErrorCode::Unavailable);
                    };
                    self.on_name(&bus, event).await;
                    continue;
                }
                _ = self.wake.notified() => {}
                _ = tick.tick() => {}
            }
            while let Ok(event) = name_events.try_recv() {
                self.on_name(&bus, event).await;
            }
            self.publish(&service).await?;
        }
    }

    async fn on_name(&self, bus: &zbus::fdo::DBusProxy<'_>, event: zbus::fdo::NameOwnerChanged) {
        let Ok(args) = event.args() else {
            return;
        };
        if map::session_id(args.name.as_str()).is_none() {
            return;
        }
        if args.old_owner.is_some() {
            self.moved.store(true, Ordering::SeqCst);
            if let Some(id) = map::session_id(args.name.as_str()) {
                let mut players = self.players.lock().await;
                if let Some(tracked) = players.get_mut(id) {
                    tracked.reader.abort();
                    if let Some(listener) = tracked.listener.take() {
                        listener.abort();
                    }
                    // Keep the transport sample/anchor across an identical
                    // owner replacement, but discard all process evidence.
                    tracked.latest = Arc::new(Mutex::new(PlayerRead::Pending));
                    tracked.reader = spawn_reader(
                        self.connection.clone(),
                        tracked.bus_name.clone(),
                        tracked.latest.clone(),
                        self.wake.clone(),
                    );
                    tracked.listener = Some(listen(
                        self.connection.clone(),
                        tracked.bus_name.clone(),
                        self.wake.clone(),
                    ));
                }
            }
        }
        self.rescan(bus).await;
    }

    async fn rescan(&self, bus: &zbus::fdo::DBusProxy<'_>) {
        let Ok(listed) = bus.list_names().await else {
            return;
        };
        let desired: BTreeMap<String, String> = listed
            .iter()
            .filter_map(|name| {
                let name = name.as_str();
                map::session_id(name).map(|id| (id.to_string(), name.to_string()))
            })
            .collect();
        let mut players = self.players.lock().await;
        let stale: Vec<String> = players
            .keys()
            .filter(|id| !desired.contains_key(id.as_str()))
            .cloned()
            .collect();
        for id in stale {
            if let Some(tracked) = players.remove(&id) {
                tracked.reader.abort();
                if let Some(listener) = tracked.listener {
                    listener.abort();
                }
            }
        }
        for (id, bus_name) in desired {
            if players.contains_key(&id) {
                continue;
            }
            let latest = Arc::new(Mutex::new(PlayerRead::Pending));
            let reader = spawn_reader(
                self.connection.clone(),
                bus_name.clone(),
                latest.clone(),
                self.wake.clone(),
            );
            let listener = listen(self.connection.clone(), bus_name.clone(), self.wake.clone());
            players.insert(
                id,
                Tracked {
                    bus_name,
                    sample: None,
                    session: None,
                    track_id: None,
                    latest,
                    listener: Some(listener),
                    reader,
                },
            );
        }
    }

    async fn publish(&self, service: &MediaService) -> Result<(), ErrorCode> {
        // Taken first: a move during this publish is seen by the next one.
        let moved = self.moved.swap(false, Ordering::SeqCst);
        let names: Vec<Snapshot> = {
            let players = self.players.lock().await;
            players
                .iter()
                .map(|(id, tracked)| Snapshot {
                    id: id.clone(),
                    previous: tracked.sample.clone(),
                    published: tracked.session.clone(),
                    latest: tracked.latest.clone(),
                })
                .collect()
        };
        let force = {
            let mut force = self.force.lock().await;
            std::mem::take(&mut *force)
        };
        // Freeze one set of completed reads for this publish. Only a fresh,
        // valid native candidate can suppress a verified proxy (ADR-0117).
        let mut observed = BTreeMap::new();
        for player in &names {
            if let PlayerRead::Ready(read) = player.latest.lock().await.clone() {
                observed.insert(player.id.clone(), read);
            }
        }
        let mut sessions = Vec::new();
        let mut samples = BTreeMap::new();
        let mut tracks = BTreeMap::new();
        let mut published = BTreeMap::new();
        for player in names {
            let Some(read) = observed.get(&player.id) else {
                if let Some(last) = player.published {
                    sessions.push(last);
                }
                continue;
            };
            if read.proxy_for != 0
                && observed
                    .iter()
                    .filter(|(id, candidate)| {
                        !plasma_proxy(&format!("{}{}", map::NAME_PREFIX, id))
                            && candidate.process == read.proxy_for
                    })
                    .count()
                    == 1
            {
                continue;
            }
            let (session, sample) = map::project(
                &player.id,
                &read.raw,
                player.previous.as_ref(),
                force.contains(&player.id),
                Instant::now(),
                map::monotonic_us(),
            );
            if session.validate().is_err() {
                if let Some(last) = player.published {
                    sessions.push(last);
                }
                continue;
            }
            tracks.insert(player.id.clone(), read.raw.track_id.clone());
            samples.insert(player.id.clone(), sample);
            published.insert(player.id, session.clone());
            sessions.push(session);
        }
        {
            let mut players = self.players.lock().await;
            for (id, sample) in samples {
                if let Some(tracked) = players.get_mut(&id) {
                    tracked.track_id = tracks.remove(&id).flatten();
                    tracked.sample = Some(sample);
                    tracked.session = published.remove(&id);
                }
            }
        }
        service.replace(sessions, "available").await?;
        // Before registration there is no revision to move, and nothing the
        // daemon could have cached.
        if moved {
            let _ = service.touch().await;
        }
        Ok(())
    }

    async fn dispatch(
        &self,
        action: MediaAction,
        id: String,
        position_us: i64,
    ) -> Result<(), ErrorCode> {
        let (bus_name, stored_track, sample) = {
            let players = self.players.lock().await;
            let tracked = players.get(&id).ok_or(ErrorCode::NotFound)?;
            (
                tracked.bus_name.clone(),
                tracked.track_id.clone(),
                tracked.sample.clone(),
            )
        };
        let proxy = PlayerProxy::builder(&self.connection)
            .cache_properties(CacheProperties::No)
            .destination(bus_name.as_str())
            .map_err(|_| ErrorCode::Unavailable)?
            .path(PLAYER_PATH)
            .map_err(|_| ErrorCode::Unavailable)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let result = match action {
            MediaAction::Play => bounded(proxy.play()).await,
            MediaAction::Pause => bounded(proxy.pause()).await,
            MediaAction::PlayPause => bounded(proxy.play_pause()).await,
            MediaAction::Next => bounded(proxy.next()).await,
            MediaAction::Previous => bounded(proxy.previous()).await,
            MediaAction::Stop => bounded(proxy.stop()).await,
            MediaAction::Seek => {
                self.seek(&proxy, &bus_name, stored_track, sample, position_us)
                    .await
            }
        };
        result?;
        if action == MediaAction::Seek {
            self.force.lock().await.insert(id);
        }
        self.wake.notify_one();
        Ok(())
    }
}

impl Bridge {
    async fn seek(
        &self,
        proxy: &PlayerProxy<'_>,
        bus_name: &str,
        stored_track: Option<String>,
        sample: Option<Sample>,
        position_us: i64,
    ) -> Result<(), ErrorCode> {
        let (track_id, current) = match read_player(&self.connection, bus_name).await {
            Ok(raw) => (raw.track_id, raw.position_us),
            Err(_) => {
                let current = sample
                    .as_ref()
                    .map(|sample| map::extrapolated_position(sample, Instant::now()))
                    .unwrap_or(0);
                (stored_track, current)
            }
        };
        match track_id.as_deref() {
            Some(track) => {
                let path = ObjectPath::try_from(track).map_err(|_| ErrorCode::Unavailable)?;
                bounded(proxy.set_position(path, position_us)).await
            }
            None => bounded(proxy.seek(position_us.saturating_sub(current))).await,
        }
    }
}

impl MediaHandler for Bridge {
    fn control(
        &self,
        action: MediaAction,
        id: String,
        position_us: i64,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        Box::pin(self.dispatch(action, id, position_us))
    }
}

fn spawn_reader(
    connection: Connection,
    bus_name: String,
    latest: Arc<Mutex<PlayerRead>>,
    wake: Arc<Notify>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // The snapshot loop never waits on this task. A lost or stalled read
        // is abandoned and tried again so a player that appears after its name
        // is still published (AC-09).
        loop {
            let next =
                match tokio::time::timeout(READ_TIMEOUT, read_observed(&connection, &bus_name))
                    .await
                {
                    Ok(Ok(raw)) => PlayerRead::Ready(raw),
                    _ => PlayerRead::Failed,
                };
            *latest.lock().await = next;
            wake.notify_one();
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
}

fn listen(
    connection: Connection,
    bus_name: String,
    wake: Arc<Notify>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(builder) = zbus::fdo::PropertiesProxy::builder(&connection)
            .cache_properties(CacheProperties::No)
            .destination(bus_name.as_str())
        else {
            return;
        };
        let Ok(builder) = builder.path(PLAYER_PATH) else {
            return;
        };
        let Ok(properties) = builder.build().await else {
            return;
        };
        let Ok(mut changes) = properties.receive_properties_changed().await else {
            return;
        };
        while changes.next().await.is_some() {
            wake.notify_one();
        }
    })
}

async fn bounded(call: impl Future<Output = zbus::Result<()>>) -> Result<(), ErrorCode> {
    match tokio::time::timeout(READ_TIMEOUT, call).await {
        Ok(Ok(())) => Ok(()),
        _ => Err(ErrorCode::Unavailable),
    }
}

async fn read_player(connection: &Connection, bus_name: &str) -> Result<RawPlayer, ErrorCode> {
    match tokio::time::timeout(READ_TIMEOUT, read_player_once(connection, bus_name)).await {
        Ok(result) => result,
        Err(_) => Err(ErrorCode::Unavailable),
    }
}

async fn read_player_once(connection: &Connection, bus_name: &str) -> Result<RawPlayer, ErrorCode> {
    Ok(decode(&read_values(connection, bus_name).await?))
}

async fn read_values(
    connection: &Connection,
    bus_name: &str,
) -> Result<HashMap<String, OwnedValue>, ErrorCode> {
    let reply = connection
        .call_method(
            Some(bus_name),
            PLAYER_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "GetAll",
            &PLAYER_INTERFACE,
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    let values = reply
        .body()
        .deserialize()
        .map_err(|_| ErrorCode::Unavailable)?;
    Ok(values)
}

// Provider-specific proxy recognition stays here, never in core routing/QML.
fn plasma_proxy(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix("org.mpris.MediaPlayer2.plasma-browser-integration")
    else {
        return false;
    };
    suffix.is_empty() || suffix.starts_with('-')
}

async fn parent_process(process: u32) -> Option<u32> {
    if process == 0 {
        return None;
    }
    let stat = tokio::fs::read_to_string(format!("/proc/{process}/stat"))
        .await
        .ok()?;
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

async fn read_observed(connection: &Connection, name: &str) -> Result<ObservedPlayer, ErrorCode> {
    let bus = zbus::fdo::DBusProxy::new(connection)
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    let well_known = zbus::names::BusName::try_from(name).map_err(|_| ErrorCode::Unavailable)?;
    let owner = bus
        .get_name_owner(well_known.clone())
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    let process = bus
        .get_connection_unix_process_id(owner.as_ref().into())
        .await
        .unwrap_or(0);
    let values = read_values(connection, owner.as_str()).await?;
    let reported = values
        .get("Metadata")
        .map(dictionary)
        .and_then(|metadata| metadata.get("kde:pid").and_then(integer))
        .and_then(|pid| u32::try_from(pid).ok())
        .unwrap_or(0);
    let proxy_for =
        if plasma_proxy(name) && reported > 1 && parent_process(process).await == Some(reported) {
            reported
        } else {
            0
        };
    if bus.get_name_owner(well_known).await.ok().as_ref() != Some(&owner) {
        return Err(ErrorCode::Unavailable);
    }
    Ok(ObservedPlayer {
        raw: decode(&values),
        process,
        // Unverified proxies remain independent; they must not become native
        // candidates merely because their parent could not be established.
        proxy_for,
    })
}

fn decode(values: &HashMap<String, OwnedValue>) -> RawPlayer {
    let status = values
        .get("PlaybackStatus")
        .and_then(text_of)
        .unwrap_or_default();
    let metadata = values.get("Metadata").map(dictionary).unwrap_or_default();
    let title = metadata
        .get("xesam:title")
        .and_then(text_of)
        .map(|title| map::bounded(&title))
        .unwrap_or_default();
    let artists = metadata
        .get("xesam:artist")
        .map(string_list)
        .unwrap_or_default();
    let album = metadata
        .get("xesam:album")
        .and_then(text_of)
        .unwrap_or_default();
    let duration = metadata
        .get("mpris:length")
        .and_then(integer)
        .filter(|value| *value >= 0)
        .unwrap_or(-1);
    let position = map::clamp_position(
        values.get("Position").and_then(integer).unwrap_or(0),
        duration,
    );
    let track_id = metadata.get("mpris:trackid").and_then(object_path);
    RawPlayer {
        title,
        subtitle: map::subtitle(&artists, &album),
        state: map::playback_state(&status).to_string(),
        duration_us: duration,
        position_us: position,
        actions: map::actions(&Caps {
            play: flag(values, "CanPlay"),
            pause: flag(values, "CanPause"),
            seek: flag(values, "CanSeek"),
            next: flag(values, "CanGoNext"),
            previous: flag(values, "CanGoPrevious"),
            control: flag(values, "CanControl"),
        }),
        track_id,
    }
}

fn flag(values: &HashMap<String, OwnedValue>, name: &str) -> bool {
    values.get(name).and_then(boolean).unwrap_or(false)
}

fn text_of(value: &OwnedValue) -> Option<String> {
    String::try_from(&**value).ok()
}

fn boolean(value: &OwnedValue) -> Option<bool> {
    value.downcast_ref().ok()
}

fn integer(value: &OwnedValue) -> Option<i64> {
    if let Ok(value) = value.downcast_ref::<i64>() {
        return Some(value);
    }
    if let Ok(value) = value.downcast_ref::<u64>() {
        return i64::try_from(value).ok();
    }
    if let Ok(value) = value.downcast_ref::<i32>() {
        return Some(i64::from(value));
    }
    if let Ok(value) = value.downcast_ref::<u32>() {
        return Some(i64::from(value));
    }
    None
}

fn object_path(value: &OwnedValue) -> Option<String> {
    let Ok(path) = value.downcast_ref::<ObjectPath>() else {
        return None;
    };
    let path = path.as_str();
    (!path.is_empty() && path != "/" && path != NO_TRACK).then(|| path.to_owned())
}

fn string_list(value: &OwnedValue) -> Vec<String> {
    if let Some(text) = text_of(value) {
        return vec![text];
    }
    let Ok(array) = value.downcast_ref::<zbus::zvariant::Array>() else {
        return Vec::new();
    };
    array
        .inner()
        .iter()
        .filter_map(|item| String::try_from(item).ok())
        .collect()
}

fn dictionary(value: &OwnedValue) -> HashMap<String, OwnedValue> {
    let mut map = HashMap::<String, OwnedValue>::try_from(value.clone()).unwrap_or_default();
    // Artwork URLs stay out of the map the session is built from.
    map.remove("mpris:artUrl");
    map
}
