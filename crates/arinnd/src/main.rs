use arinn_protocol::{PATH, ProviderProxy, SERVICE, Snapshot, Status};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;
use zbus::{message::Header, object_server::SignalEmitter, zvariant::OwnedObjectPath};

type Shared = Arc<Mutex<Model>>;
#[derive(Default)]
struct Model {
    owner: String,
    generation: String,
    revision: u64,
    items: Vec<(String, String)>,
    home: bool,
    target: String,
    active: String,
    class: String,
    fullscreen: bool,
    home_output: String,
    active_output: String,
    platform_seen: Option<std::time::Instant>,
    requests: HashMap<(String, String), (String, String, OwnedObjectPath)>,
}
impl Model {
    fn publish(
        &mut self,
        generation: &str,
        revision: u64,
        items: Vec<(String, String)>,
    ) -> zbus::fdo::Result<bool> {
        if generation != self.generation {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Conflict: stale generation".into(),
            ));
        }
        if revision == 0
            || revision < self.revision
            || (revision == self.revision && items != self.items)
        {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Conflict: invalid revision".into(),
            ));
        }
        if items.len() > 200
            || items
                .iter()
                .any(|(id, title)| id.is_empty() || id.len() > 256 || title.len() > 8192)
        {
            return Err(zbus::fdo::Error::InvalidArgs(
                "InvalidArgument: snapshot limits".into(),
            ));
        }
        let mut ids = std::collections::HashSet::new();
        if items.iter().any(|(id, _)| !ids.insert(id)) {
            return Err(zbus::fdo::Error::InvalidArgs("duplicate IDs".into()));
        }
        let changed = self.revision != revision;
        self.revision = revision;
        self.items = items;
        Ok(changed)
    }
}
struct Daemon {
    model: Shared,
    epoch: String,
    token: String,
    commands: mpsc::Sender<String>,
    receiver: Mutex<mpsc::Receiver<String>>,
}
fn caller(h: &Header<'_>) -> zbus::fdo::Result<String> {
    h.sender()
        .map(ToString::to_string)
        .ok_or_else(|| zbus::fdo::Error::AccessDenied("missing sender".into()))
}
async fn check_platform(c: &zbus::Connection, h: &Header<'_>) -> zbus::fdo::Result<()> {
    let dbus = zbus::fdo::DBusProxy::new(c).await?;
    let owner = dbus
        .get_name_owner("org.kde.KWin".try_into().unwrap())
        .await?;
    if caller(h)? != owner.as_str() {
        return Err(zbus::fdo::Error::AccessDenied("KWin only".into()));
    }
    Ok(())
}
#[zbus::interface(name = "org.arinn.Spike1")]
impl Daemon {
    async fn status(&self) -> (Status,) {
        let s = self.model.lock().await;
        (Status {
            epoch: self.epoch.clone(),
            provider_available: !s.owner.is_empty() && s.revision > 0,
            generation: s.generation.clone(),
            revision: s.revision,
            home_visible: s.home,
            return_window: s.target.clone(),
            active_window: s.active.clone(),
            active_class: s.class.clone(),
            active_fullscreen: s.fullscreen,
            home_output: s.home_output.clone(),
            active_output: s.active_output.clone(),
            platform_connected: s
                .platform_seen
                .is_some_and(|t| t.elapsed() < Duration::from_secs(30)),
        },)
    }
    async fn snapshot(&self) -> (Snapshot,) {
        let s = self.model.lock().await;
        (Snapshot {
            generation: s.generation.clone(),
            revision: s.revision,
            available: !s.owner.is_empty() && s.revision > 0,
            items: s.items.clone(),
        },)
    }
    async fn register(
        &self,
        token: &str,
        api_version: u32,
        #[zbus(header)] h: Header<'_>,
        #[zbus(connection)] c: &zbus::Connection,
    ) -> zbus::fdo::Result<String> {
        if token != self.token || token.is_empty() {
            return Err(zbus::fdo::Error::AccessDenied(
                "invalid launch token".into(),
            ));
        }
        if api_version != 1 {
            return Err(zbus::fdo::Error::NotSupported("API version".into()));
        }
        let sender = caller(&h)?;
        let mut s = self.model.lock().await;
        if sender == s.owner {
            return Ok(s.generation.clone());
        }
        if !s.owner.is_empty() {
            let dbus = zbus::fdo::DBusProxy::new(c).await?;
            if dbus
                .name_has_owner(s.owner.as_str().try_into().unwrap())
                .await?
            {
                return Err(zbus::fdo::Error::Failed(
                    "Conflict: provider already registered".into(),
                ));
            }
        }
        s.owner = sender;
        s.generation = Uuid::new_v4().to_string();
        s.revision = 0;
        s.items.clear();
        Ok(s.generation.clone())
    }
    async fn publish(
        &self,
        generation: &str,
        revision: u64,
        items: Vec<(String, String)>,
        #[zbus(header)] h: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let mut s = self.model.lock().await;
        if caller(&h)? != s.owner {
            return Err(zbus::fdo::Error::AccessDenied("not provider owner".into()));
        }
        let changed = s.publish(generation, revision, items)?;
        drop(s);
        if changed {
            Self::changed(&emitter).await?;
        }
        Ok(())
    }
    async fn activate(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
        #[zbus(header)] h: Header<'_>,
        #[zbus(connection)] c: &zbus::Connection,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let sender = caller(&h)?;
        if Uuid::parse_str(request_id).is_err() {
            return Err(zbus::fdo::Error::InvalidArgs(
                "request UUID required".into(),
            ));
        }
        let mut s = self.model.lock().await;
        if let Some((old_gen, old_id, path)) = s.requests.get(&(sender.clone(), request_id.into()))
        {
            if old_gen == generation && old_id == id {
                return Ok(path.clone());
            }
            return Err(zbus::fdo::Error::InvalidArgs(
                "Conflict: request ID reused".into(),
            ));
        }
        if s.owner.is_empty() || s.revision == 0 {
            return Err(zbus::fdo::Error::Failed("Unavailable".into()));
        }
        if s.generation != generation {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Conflict: stale generation".into(),
            ));
        }
        if !s.items.iter().any(|(i, _)| i == id) {
            return Err(zbus::fdo::Error::UnknownObject("NotFound".into()));
        }
        if s.requests.len() >= 128 {
            return Err(zbus::fdo::Error::LimitsExceeded("operation limit".into()));
        }
        let owner = s.owner.clone();
        let path = OwnedObjectPath::try_from(format!(
            "/org/arinn/Spike1/operations/o_{}",
            Uuid::new_v4().simple()
        ))
        .unwrap();
        let state = Arc::new(Mutex::new(("running".to_string(), String::new())));
        c.object_server()
            .at(
                path.clone(),
                Operation {
                    state: state.clone(),
                },
            )
            .await?;
        let key = (sender, request_id.to_string());
        s.requests
            .insert(key.clone(), (generation.into(), id.into(), path.clone()));
        drop(s);
        let c = c.clone();
        let id = id.to_string();
        let generation = generation.to_string();
        let model = self.model.clone();
        let path_copy = path.clone();
        tokio::spawn(async move {
            let work = async {
                let p = ProviderProxy::builder(&c)
                    .destination(owner.as_str())?
                    .build()
                    .await?;
                p.activate(&id).await
            };
            let result = tokio::time::timeout(Duration::from_secs(5), work).await;
            let valid = {
                let s = model.lock().await;
                s.owner == owner && s.generation == generation
            };
            let outcome = if !valid {
                ("failed", "Unavailable")
            } else {
                match result {
                    Ok(Ok(())) => ("succeeded", ""),
                    Ok(Err(_)) => ("failed", "Unavailable"),
                    Err(_) => ("failed", "Timeout"),
                }
            };
            *state.lock().await = (outcome.0.into(), outcome.1.into());
            if let Ok(iface) = c
                .object_server()
                .interface::<_, Operation>(path_copy.clone())
                .await
            {
                let guard = iface.get().await;
                let _ = guard.state_changed(iface.signal_emitter()).await;
                let _ = guard.error_changed(iface.signal_emitter()).await;
            }
            tokio::time::sleep(Duration::from_secs(300)).await;
            model.lock().await.requests.remove(&key);
            let _ = c.object_server().remove::<Operation, _>(path_copy).await;
        });
        Ok(path)
    }
    async fn home(&self) -> zbus::fdo::Result<()> {
        self.queue("home").await
    }
    async fn back(&self) -> zbus::fdo::Result<()> {
        self.queue("return").await
    }
    async fn take_command(
        &self,
        #[zbus(header)] h: Header<'_>,
        #[zbus(connection)] c: &zbus::Connection,
    ) -> zbus::fdo::Result<String> {
        check_platform(c, &h).await?;
        self.model.lock().await.platform_seen = Some(std::time::Instant::now());
        Ok(
            tokio::time::timeout(Duration::from_secs(20), self.receiver.lock().await.recv())
                .await
                .ok()
                .flatten()
                .unwrap_or("idle".into()),
        )
    }
    async fn report_home(
        &self,
        visible: bool,
        target: &str,
        output: &str,
        #[zbus(header)] h: Header<'_>,
        #[zbus(connection)] c: &zbus::Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        check_platform(c, &h).await?;
        let mut s = self.model.lock().await;
        s.home = visible;
        s.target = target.into();
        s.home_output = output.into();
        drop(s);
        Self::changed(&emitter).await?;
        Ok(())
    }
    async fn report_window(
        &self,
        id: &str,
        class: &str,
        fullscreen: bool,
        output: &str,
        #[zbus(header)] h: Header<'_>,
        #[zbus(connection)] c: &zbus::Connection,
    ) -> zbus::fdo::Result<()> {
        check_platform(c, &h).await?;
        let mut s = self.model.lock().await;
        s.active = id.into();
        s.class = class.into();
        s.fullscreen = fullscreen;
        s.active_output = output.into();
        Ok(())
    }
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}
impl Daemon {
    async fn queue(&self, command: &str) -> zbus::fdo::Result<()> {
        if !self
            .model
            .lock()
            .await
            .platform_seen
            .is_some_and(|t| t.elapsed() < Duration::from_secs(30))
        {
            return Err(zbus::fdo::Error::Failed("Unavailable: KWin adapter".into()));
        }
        self.commands
            .try_send(command.into())
            .map_err(|_| zbus::fdo::Error::LimitsExceeded("platform queue full".into()))
    }
}
struct Operation {
    state: Arc<Mutex<(String, String)>>,
}
#[zbus::interface(name = "org.arinn.SpikeOperation1")]
impl Operation {
    #[zbus(property)]
    async fn state(&self) -> String {
        self.state.lock().await.0.clone()
    }
    #[zbus(property)]
    async fn error(&self) -> String {
        self.state.lock().await.1.clone()
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let token = std::fs::read_to_string(std::env::var("ARINN_SPIKE_TOKEN_FILE")?)?
        .trim()
        .to_string();
    anyhow::ensure!(token.len() >= 32, "launch token too short");
    let model = Shared::default();
    let (tx, rx) = mpsc::channel(8);
    let daemon = Daemon {
        model: model.clone(),
        epoch: Uuid::new_v4().to_string(),
        token,
        commands: tx,
        receiver: Mutex::new(rx),
    };
    let c = zbus::connection::Builder::session()?
        .name(SERVICE)?
        .serve_at(PATH, daemon)?
        .build()
        .await?;
    let watch = c.clone();
    tokio::spawn(async move {
        let dbus = zbus::fdo::DBusProxy::new(&watch).await.unwrap();
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let owner = model.lock().await.owner.clone();
            if owner.is_empty() {
                continue;
            }
            if !dbus
                .name_has_owner(owner.as_str().try_into().unwrap())
                .await
                .unwrap_or(false)
            {
                let mut s = model.lock().await;
                if s.owner != owner {
                    continue;
                }
                s.owner.clear();
                s.items.clear();
                s.revision = 0;
                drop(s);
                if let Ok(emitter) = SignalEmitter::new(&watch, PATH) {
                    let _ = Daemon::changed(&emitter).await;
                }
            }
        }
    });
    eprintln!("{{\"component\":\"arinnd\",\"operation\":\"ready\"}}");
    tokio::signal::ctrl_c().await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revisions_reject_reused_and_stale_state() {
        let mut m = Model {
            generation: "g1".into(),
            ..Model::default()
        };
        assert!(m.publish("g1", 1, vec![("a".into(), "A".into())]).unwrap());
        assert!(!m.publish("g1", 1, vec![("a".into(), "A".into())]).unwrap());
        assert!(m.publish("g1", 1, vec![]).is_err());
        assert!(m.publish("old", 2, vec![]).is_err());
        assert!(m.publish("g1", 0, vec![]).is_err());
    }
    #[test]
    fn duplicate_ids_are_rejected() {
        let mut m = Model {
            generation: "g".into(),
            ..Model::default()
        };
        assert!(
            m.publish(
                "g",
                1,
                vec![("a".into(), "A".into()), ("a".into(), "B".into())]
            )
            .is_err()
        );
    }
}
