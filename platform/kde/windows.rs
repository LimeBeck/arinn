// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Authoritative KWin inventory and acknowledged controls. No application discovery.
use arinn_protocol::{
    kwin,
    platform::{self, Window, WindowPage},
    production,
    validation::{self, ErrorCode},
};
use futures_util::StreamExt;
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Notify, oneshot};
use zbus::{Connection, message::Header};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub provider: String,
    pub owner: String,
    pub registration: String,
}
struct Snapshot {
    pages: Vec<WindowPage>,
    bytes: usize,
}
struct Lease {
    scope: Scope,
    created: Instant,
    snapshot: Arc<Snapshot>,
}
struct Stage {
    revision: i32,
    count: usize,
    created: Instant,
    windows: BTreeMap<i32, Window>,
    bytes: usize,
}
struct Control {
    id: String,
    action: String,
    token: String,
    scope: Scope,
    deadline: Instant,
    reply: oneshot::Sender<Result<(), ErrorCode>>,
}
struct State {
    owner: String,
    instance: String,
    generation: String,
    seen: Option<Instant>,
    revision: i32,
    windows: Vec<Window>,
    retired: HashSet<String>,
    snapshot: Option<Arc<Snapshot>>,
    stage: Option<Stage>,
    leases: HashMap<String, Lease>,
    queued: VecDeque<Control>,
    pending: Option<Control>,
    polling: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            owner: String::new(),
            instance: String::new(),
            generation: uuid::Uuid::new_v4().to_string(),
            seen: None,
            revision: 0,
            windows: Vec::new(),
            retired: HashSet::new(),
            snapshot: None,
            stage: None,
            leases: HashMap::new(),
            queued: VecDeque::new(),
            pending: None,
            polling: false,
        }
    }
}
impl State {
    fn reset(&mut self) {
        for command in self.queued.drain(..).chain(self.pending.take()) {
            let _ = command.reply.send(Err(ErrorCode::Unavailable));
        }
        *self = Self::default();
    }
    fn live(&self, owner: &str) -> Result<(), ErrorCode> {
        if self.owner != owner
            || self
                .seen
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(5))
        {
            Err(ErrorCode::Unavailable)
        } else {
            Ok(())
        }
    }
    fn generation(&self, generation: &str) -> Result<(), ErrorCode> {
        platform::generation(generation)?;
        if self.generation != generation {
            Err(ErrorCode::Conflict)
        } else {
            Ok(())
        }
    }
    fn prune(&mut self) {
        self.leases
            .retain(|_, l| l.created.elapsed() < Duration::from_secs(30));
        if self
            .stage
            .as_ref()
            .is_some_and(|s| s.created.elapsed() >= Duration::from_secs(3))
        {
            self.stage = None;
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.deadline <= Instant::now() || p.reply.is_closed())
            && let Some(p) = self.pending.take()
        {
            let _ = p.reply.send(Err(ErrorCode::Timeout));
        }
        self.queued.retain(|p| !p.reply.is_closed());
        let mut live = VecDeque::new();
        for p in self.queued.drain(..) {
            if p.deadline <= Instant::now() {
                let _ = p.reply.send(Err(ErrorCode::Timeout));
            } else {
                live.push_back(p);
            }
        }
        self.queued = live;
    }
}
#[derive(Clone, Default)]
pub struct Windows {
    state: Arc<Mutex<State>>,
    notify: Arc<Notify>,
}
async fn owner(c: &Connection) -> Result<String, ErrorCode> {
    tokio::time::timeout(Duration::from_secs(3), async {
        zbus::fdo::DBusProxy::new(c)
            .await
            .map_err(|_| ErrorCode::Unavailable)?
            .get_name_owner("org.kde.KWin".try_into().unwrap())
            .await
            .map(|o| o.to_string())
            .map_err(|_| ErrorCode::Unavailable)
    })
    .await
    .unwrap_or(Err(ErrorCode::Unavailable))
}
async fn trusted(c: &Connection, h: &Header<'_>) -> production::Result<String> {
    let current = owner(c).await.map_err(|_| ErrorCode::PermissionDenied)?;
    if h.sender().map(|s| s.as_str()) != Some(current.as_str()) {
        return Err(ErrorCode::PermissionDenied.into());
    }
    Ok(current)
}
async fn announce(c: &Connection, generation: &str, revision: u64) {
    let _ = c
        .emit_signal(
            None::<&str>,
            production::PATH,
            platform::INTERFACE,
            "WindowsChanged",
            &(generation, revision),
        )
        .await;
}
impl Windows {
    pub async fn start(&self, c: &Connection) -> Result<(), ErrorCode> {
        let bus = zbus::fdo::DBusProxy::new(c)
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let mut changes = bus
            .receive_name_owner_changed()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let backend = self.clone();
        let connection = c.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(250));
            loop {
                let lost = tokio::select! {
                    change=changes.next()=>{let Some(change)=change else{break;}; match change.args(){Ok(a) if a.name.as_str()=="org.kde.KWin"=>a.old_owner.as_ref().map(|o|o.to_string()),_=>None}},
                    _=tick.tick()=>None,
                };
                let mut s = backend.state.lock().await;
                s.prune();
                if lost.as_ref().is_some_and(|o| o == &s.owner)
                    || s.seen
                        .is_some_and(|t| t.elapsed() >= Duration::from_secs(5))
                {
                    s.reset();
                    let generation_id = s.generation.clone();
                    drop(s);
                    backend.notify.notify_waiters();
                    announce(&connection, &generation_id, 1).await;
                }
            }
        });
        Ok(())
    }
    pub async fn disconnect(&self, owner: &str) {
        let mut s = self.state.lock().await;
        s.leases.retain(|_, l| l.scope.owner != owner);
        let mut keep = VecDeque::new();
        for p in s.queued.drain(..) {
            if p.scope.owner == owner {
                let _ = p.reply.send(Err(ErrorCode::Unavailable));
            } else {
                keep.push_back(p);
            }
        }
        s.queued = keep;
        if s.pending.as_ref().is_some_and(|p| p.scope.owner == owner)
            && let Some(p) = s.pending.take()
        {
            let _ = p.reply.send(Err(ErrorCode::Unavailable));
        }
    }
    pub async fn page(
        &self,
        c: &Connection,
        scope: &Scope,
        token: &str,
    ) -> Result<WindowPage, ErrorCode> {
        if !token.is_empty() {
            validation::id(token)?;
        }
        let current = owner(c).await?;
        let mut s = self.state.lock().await;
        s.live(&current)?;
        s.prune();
        if token.is_empty() {
            let snapshot = s.snapshot.clone().ok_or(ErrorCode::Unavailable)?;
            if snapshot.pages.len() == 1 {
                let mut page = snapshot.pages[0].clone();
                page.next_token.clear();
                return Ok(page);
            }
            if s.leases.len() >= 32 {
                return Err(ErrorCode::Unavailable);
            }
            let key = uuid::Uuid::new_v4().simple().to_string();
            let lease = Lease {
                scope: scope.clone(),
                created: Instant::now(),
                snapshot,
            };
            let page = leased(&key, &lease, 0)?;
            s.leases.insert(key, lease);
            return Ok(page);
        }
        let (key, index) = token.split_once(':').ok_or(ErrorCode::Conflict)?;
        let lease = s.leases.get(key).ok_or(ErrorCode::Conflict)?;
        if lease.scope.owner != scope.owner {
            return Err(ErrorCode::PermissionDenied);
        }
        if &lease.scope != scope {
            return Err(ErrorCode::Conflict);
        }
        leased(key, lease, index.parse().map_err(|_| ErrorCode::Conflict)?)
    }
    pub async fn check(
        &self,
        c: &Connection,
        generation: &str,
        token: &str,
        action: &str,
    ) -> Result<(), ErrorCode> {
        let current = owner(c).await?;
        let s = self.state.lock().await;
        s.live(&current)?;
        s.generation(generation)?;
        check(&s, token, action)
    }
    pub async fn enqueue(
        &self,
        c: &Connection,
        scope: Scope,
        generation: &str,
        token: &str,
        action: &str,
    ) -> Result<oneshot::Receiver<Result<(), ErrorCode>>, ErrorCode> {
        let current = owner(c).await?;
        let (send, receive) = oneshot::channel();
        {
            let mut s = self.state.lock().await;
            s.live(&current)?;
            s.generation(generation)?;
            s.prune();
            check(&s, token, action)?;
            if s.queued.len() + usize::from(s.pending.is_some()) >= 1024 {
                return Err(ErrorCode::Unavailable);
            }
            s.queued.push_back(Control {
                id: uuid::Uuid::new_v4().to_string(),
                action: action.into(),
                token: token.into(),
                scope,
                deadline: Instant::now() + Duration::from_secs(5),
                reply: send,
            });
        }
        self.notify.notify_one();
        Ok(receive)
    }
}
fn check(s: &State, token: &str, action: &str) -> Result<(), ErrorCode> {
    let window = s
        .windows
        .iter()
        .find(|w| w.token == token)
        .ok_or(ErrorCode::NotFound)?;
    if if action == "activate" {
        !window.can_activate
    } else {
        !window.can_close
    } {
        return Err(ErrorCode::Unsupported);
    }
    Ok(())
}
fn take_control(s: &mut State) -> Option<Vec<String>> {
    while let Some(p) = s.queued.pop_front() {
        if let Err(error) = check(s, &p.token, &p.action) {
            let _ = p.reply.send(Err(error));
            continue;
        }
        let result = vec![p.id.clone(), p.action.clone(), p.token.clone()];
        s.pending = Some(p);
        return Some(result);
    }
    None
}
fn leased(key: &str, lease: &Lease, index: usize) -> Result<WindowPage, ErrorCode> {
    let mut page = lease
        .snapshot
        .pages
        .get(index)
        .ok_or(ErrorCode::Conflict)?
        .clone();
    page.next_token = if index + 1 < lease.snapshot.pages.len() {
        format!("{key}:{}", index + 1)
    } else {
        String::new()
    };
    Ok(page)
}
fn snapshot(generation: &str, revision: i32, windows: &[Window]) -> Result<Snapshot, ErrorCode> {
    platform::validate_windows(windows)?;
    let mut page = WindowPage {
        generation: generation.into(),
        revision: revision as u64,
        windows: Vec::new(),
        next_token: "0".repeat(43),
    };
    let mut pages = Vec::new();
    let mut bytes = 0;
    for w in windows {
        page.windows.push(w.clone());
        if page.windows.len() > validation::MAX_PAGE_ENTRIES
            || page.encoded_len()? > validation::MAX_BODY_BYTES
        {
            let last = page.windows.pop().unwrap();
            bytes += page.encoded_len()?;
            pages.push(page.clone());
            page.windows.clear();
            page.windows.push(last);
        }
        if bytes > platform::MAX_SNAPSHOT_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    bytes += page.encoded_len()?;
    if bytes > platform::MAX_SNAPSHOT_BYTES {
        return Err(ErrorCode::InvalidArgument);
    }
    pages.push(page);
    Ok(Snapshot { pages, bytes })
}

#[zbus::interface(name = "org.arinn.KWinBridge1")]
impl Windows {
    async fn connect(
        &self,
        instance: &str,
        #[zbus(connection)] c: &Connection,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<String> {
        let current = trusted(c, &h).await?;
        platform::generation(instance)?;
        let mut s = self.state.lock().await;
        if s.instance == instance && s.live(&current).is_ok() {
            s.seen = Some(Instant::now());
            return Ok(s.generation.clone());
        }
        s.reset();
        s.instance = instance.into();
        s.owner = current;
        s.seen = Some(Instant::now());
        let generation_id = s.generation.clone();
        drop(s);
        self.notify.notify_waiters();
        announce(c, &generation_id, 1).await;
        Ok(generation_id)
    }
    async fn begin_inventory(
        &self,
        generation: &str,
        revision: i32,
        count: i32,
        #[zbus(connection)] c: &Connection,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        let current = trusted(c, &h).await?;
        kwin::validate_begin(generation, revision, count)?;
        let mut s = self.state.lock().await;
        s.live(&current)?;
        s.generation(generation)?;
        s.prune();
        if revision <= s.revision {
            return Err(ErrorCode::Conflict.into());
        }
        if s.stage.is_some() {
            return Err(ErrorCode::Conflict.into());
        }
        s.stage = Some(Stage {
            revision,
            count: count as usize,
            created: Instant::now(),
            windows: BTreeMap::new(),
            bytes: 0,
        });
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    async fn put_window(
        &self,
        generation: &str,
        revision: i32,
        index: i32,
        token: &str,
        desktop_id: &str,
        output: &str,
        flags: i32,
        #[zbus(connection)] c: &Connection,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        let current = trusted(c, &h).await?;
        let mut s = self.state.lock().await;
        s.live(&current)?;
        s.generation(generation)?;
        s.prune();
        let stage = s
            .stage
            .as_mut()
            .filter(|stage| stage.revision == revision)
            .ok_or(ErrorCode::Conflict)?;
        let window = kwin::window(token, desktop_id, output, flags);
        if index < 0
            || index as usize >= stage.count
            || stage.windows.contains_key(&index)
            || window.is_err()
        {
            s.stage = None;
            return Err(ErrorCode::InvalidArgument.into());
        }
        let window = window?;
        stage.bytes += token.len() + desktop_id.len() + output.len() + 64;
        if stage.bytes > platform::MAX_SNAPSHOT_BYTES {
            s.stage = None;
            return Err(ErrorCode::InvalidArgument.into());
        }
        stage.windows.insert(index, window);
        Ok(())
    }
    async fn commit_inventory(
        &self,
        generation: &str,
        revision: i32,
        #[zbus(connection)] c: &Connection,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        let current = trusted(c, &h).await?;
        let mut s = self.state.lock().await;
        s.live(&current)?;
        s.generation(generation)?;
        s.prune();
        if s.stage
            .as_ref()
            .is_none_or(|stage| stage.revision != revision)
        {
            return Err(ErrorCode::Conflict.into());
        }
        let stage = s.stage.take().unwrap();
        if stage.windows.len() != stage.count {
            return Err(ErrorCode::InvalidArgument.into());
        }
        let windows: Vec<_> = stage.windows.into_values().collect();
        if windows.iter().any(|w| s.retired.contains(&w.token)) {
            return Err(ErrorCode::Conflict.into());
        }
        if s.revision > 0 && windows == s.windows {
            return Err(ErrorCode::Conflict.into());
        }
        let next = snapshot(generation, revision, &windows)?;
        let mut revisions = HashSet::new();
        let retained: usize = s
            .leases
            .values()
            .filter(|l| revisions.insert(l.snapshot.pages[0].revision))
            .map(|l| l.snapshot.bytes)
            .sum();
        if retained + next.bytes > platform::MAX_SNAPSHOT_BYTES {
            return Err(ErrorCode::Unavailable.into());
        }
        let live: HashSet<_> = windows.iter().map(|w| &w.token).collect();
        let removed: Vec<_> = s
            .windows
            .iter()
            .filter(|w| !live.contains(&w.token))
            .map(|w| w.token.clone())
            .collect();
        if s.retired.len() + removed.len() > platform::MAX_SNAPSHOT_ENTRIES {
            s.reset();
            let generation_id = s.generation.clone();
            drop(s);
            announce(c, &generation_id, 1).await;
            return Err(ErrorCode::Unavailable.into());
        }
        s.retired.extend(removed);
        s.windows = windows;
        s.snapshot = Some(Arc::new(next));
        s.revision = revision;
        drop(s);
        announce(c, generation, revision as u64).await;
        Ok(())
    }
    async fn next_control(
        &self,
        generation: &str,
        #[zbus(connection)] c: &Connection,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<Vec<String>> {
        let current = trusted(c, &h).await?;
        let notified = self.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        {
            let mut s = self.state.lock().await;
            s.live(&current)?;
            s.generation(generation)?;
            s.prune();
            if s.polling {
                return Err(ErrorCode::Conflict.into());
            }
            s.seen = Some(Instant::now());
            if s.pending.is_some() {
                return Ok(Vec::new());
            }
            if let Some(result) = take_control(&mut s) {
                return Ok(result);
            }
            s.polling = true;
        }
        let _ = tokio::time::timeout(Duration::from_secs(1), notified).await;
        let mut s = self.state.lock().await;
        s.generation(generation)?;
        s.polling = false;
        s.live(&current)?;
        s.prune();
        Ok(take_control(&mut s).unwrap_or_default())
    }

    async fn complete_control(
        &self,
        generation: &str,
        request_id: &str,
        error: &str,
        #[zbus(connection)] c: &Connection,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        let current = trusted(c, &h).await?;
        kwin::validate_completion(request_id, error)?;
        let mut s = self.state.lock().await;
        s.live(&current)?;
        s.generation(generation)?;
        s.prune();
        if s.pending.as_ref().is_none_or(|p| p.id != request_id) {
            return Err(ErrorCode::Conflict.into());
        }
        let p = s.pending.take().unwrap();
        let result = if error.is_empty() {
            Ok(())
        } else {
            Err(match error {
                "NotFound" => ErrorCode::NotFound,
                "Unsupported" => ErrorCode::Unsupported,
                "PermissionDenied" => ErrorCode::PermissionDenied,
                "Timeout" => ErrorCode::Timeout,
                "InvalidArgument" => ErrorCode::InvalidArgument,
                "Conflict" => ErrorCode::Conflict,
                "Internal" => ErrorCode::Internal,
                "Cancelled" => ErrorCode::Cancelled,
                _ => ErrorCode::Unavailable,
            })
        };
        let _ = p.reply.send(result);
        Ok(())
    }
}
