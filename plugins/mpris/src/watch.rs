// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Watch `org.mpris.MediaPlayer2.*` and serve `org.arinn.MediaProvider1`.
//! No rich interfaces. Artwork URLs are not read or forwarded.
use crate::map::{self, Caps, RawPlayer, Sample};
use arinn_plugin_sdk::runtime::{MediaAction, MediaHandler, MediaService, run_media_on};
use arinn_protocol::validation::ErrorCode;
use futures_util::StreamExt;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Notify};
use zbus::{
    Connection,
    names::InterfaceName,
    zvariant::{ObjectPath, OwnedValue},
};

const PLAYER_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

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

struct Tracked {
    bus_name: String,
    sample: Option<Sample>,
    track_id: Option<String>,
    listener: Option<tokio::task::JoinHandle<()>>,
}

#[derive(Clone)]
struct Bridge {
    connection: Connection,
    players: Arc<Mutex<BTreeMap<String, Tracked>>>,
    force: Arc<Mutex<HashSet<String>>>,
    wake: Arc<Notify>,
}

pub async fn serve(connection: Connection) -> Result<(), ErrorCode> {
    let bridge = Bridge {
        connection: connection.clone(),
        players: Arc::new(Mutex::new(BTreeMap::new())),
        force: Arc::new(Mutex::new(HashSet::new())),
        wake: Arc::new(Notify::new()),
    };
    let service = MediaService::new(bridge.clone(), Vec::new(), "available")?;
    tokio::select! {
        result = bridge.watch(service.clone()) => result,
        result = run_media_on(connection, service) => result.map_err(|_| ErrorCode::Unavailable),
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
            tokio::select! {
                event = name_events.recv() => {
                    let Some(event) = event else {
                        return Err(ErrorCode::Unavailable);
                    };
                    let Ok(args) = event.args() else { continue };
                    if map::session_id(args.name.as_str()).is_none() {
                        continue;
                    }
                    self.rescan(&bus).await;
                }
                _ = self.wake.notified() => {}
                _ = tick.tick() => {}
            }
            self.publish(&service).await?;
        }
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
            if let Some(tracked) = players.remove(&id)
                && let Some(listener) = tracked.listener
            {
                listener.abort();
            }
        }
        for (id, bus_name) in desired {
            if players.contains_key(&id) {
                continue;
            }
            let listener = listen(self.connection.clone(), bus_name.clone(), self.wake.clone());
            players.insert(
                id,
                Tracked {
                    bus_name,
                    sample: None,
                    track_id: None,
                    listener: Some(listener),
                },
            );
        }
    }

    async fn publish(&self, service: &MediaService) -> Result<(), ErrorCode> {
        let names: Vec<(String, String, Option<Sample>)> = {
            let players = self.players.lock().await;
            players
                .iter()
                .map(|(id, tracked)| (id.clone(), tracked.bus_name.clone(), tracked.sample.clone()))
                .collect()
        };
        let force = {
            let mut force = self.force.lock().await;
            std::mem::take(&mut *force)
        };
        let now = Instant::now();
        let now_us = map::monotonic_us();
        let mut sessions = Vec::new();
        let mut dead = Vec::new();
        let mut samples = BTreeMap::new();
        let mut tracks = BTreeMap::new();
        for (id, bus_name, previous) in names {
            let Ok(raw) = read_player(&self.connection, &bus_name).await else {
                dead.push(id);
                continue;
            };
            let (session, sample) = map::project(
                &id,
                &raw,
                previous.as_ref(),
                force.contains(&id),
                now,
                now_us,
            );
            if session.validate().is_err() {
                dead.push(id);
                continue;
            }
            tracks.insert(id.clone(), raw.track_id);
            samples.insert(id, sample);
            sessions.push(session);
        }
        {
            let mut players = self.players.lock().await;
            for id in dead {
                if let Some(tracked) = players.remove(&id)
                    && let Some(listener) = tracked.listener
                {
                    listener.abort();
                }
            }
            for (id, sample) in samples {
                if let Some(tracked) = players.get_mut(&id) {
                    tracked.track_id = tracks.remove(&id).flatten();
                    tracked.sample = Some(sample);
                }
            }
        }
        service.replace(sessions, "available").await
    }

    async fn dispatch(
        &self,
        action: MediaAction,
        id: String,
        position_us: i64,
    ) -> Result<(), ErrorCode> {
        let (bus_name, track_id, current) = {
            let players = self.players.lock().await;
            let tracked = players.get(&id).ok_or(ErrorCode::NotFound)?;
            (
                tracked.bus_name.clone(),
                tracked.track_id.clone(),
                tracked
                    .sample
                    .as_ref()
                    .map(|sample| sample.position_us)
                    .unwrap_or(0),
            )
        };
        let proxy = PlayerProxy::builder(&self.connection)
            .destination(bus_name.as_str())
            .map_err(|_| ErrorCode::Unavailable)?
            .path(PLAYER_PATH)
            .map_err(|_| ErrorCode::Unavailable)?
            .build()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let result = match action {
            MediaAction::Play => proxy.play().await,
            MediaAction::Pause => proxy.pause().await,
            MediaAction::PlayPause => proxy.play_pause().await,
            MediaAction::Next => proxy.next().await,
            MediaAction::Previous => proxy.previous().await,
            MediaAction::Stop => proxy.stop().await,
            MediaAction::Seek => match track_id.as_deref() {
                Some(track) => {
                    let path = ObjectPath::try_from(track).map_err(|_| ErrorCode::Unavailable)?;
                    proxy.set_position(path, position_us).await
                }
                None => proxy.seek(position_us.saturating_sub(current)).await,
            },
        };
        result.map_err(|_| ErrorCode::Unavailable)?;
        if action == MediaAction::Seek {
            self.force.lock().await.insert(id);
        }
        self.wake.notify_one();
        Ok(())
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

fn listen(
    connection: Connection,
    bus_name: String,
    wake: Arc<Notify>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(builder) =
            zbus::fdo::PropertiesProxy::builder(&connection).destination(bus_name.as_str())
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

async fn read_player(connection: &Connection, bus_name: &str) -> Result<RawPlayer, ErrorCode> {
    let properties = zbus::fdo::PropertiesProxy::builder(connection)
        .destination(bus_name)
        .map_err(|_| ErrorCode::Unavailable)?
        .path(PLAYER_PATH)
        .map_err(|_| ErrorCode::Unavailable)?
        .build()
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    let interface =
        InterfaceName::try_from(PLAYER_INTERFACE).map_err(|_| ErrorCode::Unavailable)?;
    let values = properties
        .get_all(interface)
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    Ok(decode(&values))
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
    (!path.is_empty() && path != "/").then(|| path.to_owned())
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
