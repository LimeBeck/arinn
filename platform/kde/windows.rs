// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Authoritative KWin inventory and acknowledged controls. No application discovery.
use arinn_protocol::{
    kwin,
    platform::{self, Window, WindowPage},
    production,
    return_association::{self, ReturnAssociation, WindowRef},
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
    home: bool,
    association: ReturnAssociation,
    exhausted: bool,
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
            home: false,
            association: ReturnAssociation {
                epoch: String::new(),
                revision: 1,
                target: String::new(),
                window: Vec::new(),
            },
            exhausted: false,
        }
    }
}
impl State {
    fn reset(&mut self) {
        for command in self.queued.drain(..).chain(self.pending.take()) {
            let _ = command.reply.send(Err(ErrorCode::Unavailable));
        }
        let home = self.home;
        let association = self.association.clone();
        let exhausted = self.exhausted;
        *self = Self::default();
        self.home = home;
        self.association = association;
        self.exhausted = exhausted;
        self.publish(self.association.target.clone(), Vec::new());
    }
    // Exhaustion fails this optional API closed until a fresh daemon epoch.
    fn publish(&mut self, target: String, window: Vec<WindowRef>) -> bool {
        if self.association.target == target && self.association.window == window {
            return false;
        }
        let Some(revision) = self.association.revision.checked_add(1) else {
            self.exhausted = true;
            return false;
        };
        self.association.target = target;
        self.association.window = window;
        self.association.revision = revision;
        true
    }
    async fn announce_return(&self, c: &Connection, previous: u64) {
        if !self.exhausted && previous != self.association.revision {
            let _ = c
                .emit_signal(
                    None::<&str>,
                    production::PATH,
                    return_association::INTERFACE,
                    "Changed",
                    &(&self.association.epoch, self.association.revision),
                )
                .await;
        }
    }
    fn reconcile_owner(
        &mut self,
        current: Result<Option<&str>, ErrorCode>,
    ) -> Result<(), ErrorCode> {
        let current = current?;
        if current.is_none_or(|owner| self.live(owner).is_err()) {
            self.publish(self.association.target.clone(), Vec::new());
        }
        Ok(())
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
async fn owner_lookup(c: &Connection) -> Result<Option<String>, ErrorCode> {
    tokio::time::timeout(Duration::from_secs(3), async {
        match zbus::fdo::DBusProxy::new(c)
            .await
            .map_err(|_| ErrorCode::Unavailable)?
            .get_name_owner("org.kde.KWin".try_into().unwrap())
            .await
        {
            Ok(owner) => Ok(Some(owner.to_string())),
            Err(zbus::fdo::Error::NameHasNoOwner(_)) => Ok(None),
            Err(_) => Err(ErrorCode::Unavailable),
        }
    })
    .await
    .unwrap_or(Err(ErrorCode::Unavailable))
}
async fn owner(c: &Connection) -> Result<String, ErrorCode> {
    owner_lookup(c).await?.ok_or(ErrorCode::Unavailable)
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
    pub fn new(epoch: &str) -> Self {
        let mut state = State::default();
        state.association.epoch = epoch.into();
        Self {
            state: Arc::new(Mutex::new(state)),
            notify: Arc::default(),
        }
    }
    // Lock order: inventory, then Home bridge. Every production Home write goes
    // through here; association reports and inventory commits use the same lock.
    #[allow(clippy::too_many_arguments)]
    pub async fn report_home(
        &self,
        platform: &crate::platform::Platform,
        c: &Connection,
        h: Header<'_>,
        visible: bool,
        target: &str,
        output: &str,
    ) -> production::Result<()> {
        self.commit_home_after(
            platform,
            c,
            visible,
            target,
            output,
            crate::platform::Platform::authorize(c, &h),
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn commit_home_after(
        &self,
        platform: &crate::platform::Platform,
        c: &Connection,
        visible: bool,
        target: &str,
        output: &str,
        authorization: impl std::future::Future<Output = production::Result<()>>,
    ) -> production::Result<()> {
        authorization.await?;
        validation::text(target)?;
        validation::text(output)?;
        let mut s = self.state.lock().await;
        platform.commit_home(visible, target, output).await;
        let previous = s.association.revision;
        s.home = visible;
        s.publish(target.into(), Vec::new());
        s.announce_return(c, previous).await;
        Ok(())
    }
    pub async fn live_tokens(&self, c: &Connection) -> HashSet<String> {
        let Ok(current) = owner(c).await else {
            return HashSet::new();
        };
        let s = self.state.lock().await;
        if s.live(&current).is_err() {
            return HashSet::new();
        }
        s.windows.iter().map(|w| w.token.clone()).collect()
    }
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
                    let previous = s.association.revision;
                    s.reset();
                    s.announce_return(&connection, previous).await;
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
    /// The current platform generation, for daemon-initiated controls.
    pub async fn current_generation(&self) -> String {
        self.state.lock().await.generation.clone()
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
        let previous = s.association.revision;
        s.reset();
        s.announce_return(c, previous).await;
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
            let previous = s.association.revision;
            s.reset();
            s.announce_return(c, previous).await;
            let generation_id = s.generation.clone();
            drop(s);
            announce(c, &generation_id, 1).await;
            return Err(ErrorCode::Unavailable.into());
        }
        let previous = s.association.revision;
        if s.association
            .window
            .iter()
            .any(|w| !live.contains(&w.token))
        {
            let target = s.association.target.clone();
            s.publish(target, Vec::new());
        }
        s.retired.extend(removed);
        s.windows = windows;
        s.snapshot = Some(Arc::new(next));
        s.revision = revision;
        s.announce_return(c, previous).await;
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

// Separate interfaces preserve the existing KWinBridge1 wire surface.
pub struct ReturnEndpoint(pub Windows);
#[zbus::interface(name = "org.arinn.ReturnAssociation1")]
impl ReturnEndpoint {
    async fn get(
        &self,
        #[zbus(connection)] c: &Connection,
    ) -> production::Result<(ReturnAssociation,)> {
        let current = owner_lookup(c).await;
        let mut s = self.0.state.lock().await;
        let previous = s.association.revision;
        s.reconcile_owner(
            current
                .as_ref()
                .map(|owner| owner.as_deref())
                .map_err(|e| *e),
        )?;
        s.announce_return(c, previous).await;
        if s.exhausted {
            return Err(ErrorCode::Unavailable.into());
        }
        Ok((s.association.clone(),))
    }
    #[zbus(signal)]
    async fn changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        epoch: &str,
        revision: u64,
    ) -> zbus::Result<()>;
}
pub struct ReturnBridge(pub Windows);
#[zbus::interface(name = "org.arinn.KWinReturn1")]
impl ReturnBridge {
    async fn report(
        &self,
        generation: &str,
        inventory_revision: i32,
        target: &str,
        token: &str,
        #[zbus(connection)] c: &Connection,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        let current = trusted(c, &h).await?;
        let mut s = self.0.state.lock().await;
        if s.owner != current {
            return Err(ErrorCode::PermissionDenied.into());
        }
        return_association::validate_report(generation, inventory_revision, target, token)?;
        s.live(&current)?;
        if !s.home || s.snapshot.is_none() || s.exhausted {
            return Err(ErrorCode::Unavailable.into());
        }
        if s.generation != generation
            || s.revision != inventory_revision
            || s.association.target != target
        {
            return Err(ErrorCode::Conflict.into());
        }
        if !token.is_empty() && !s.windows.iter().any(|w| w.token == token) {
            return Err(ErrorCode::NotFound.into());
        }
        let window = if token.is_empty() {
            Vec::new()
        } else {
            vec![WindowRef {
                generation: generation.into(),
                token: token.into(),
            }]
        };
        let previous = s.association.revision;
        s.publish(target.into(), window);
        s.announce_return(c, previous).await;
        if s.exhausted {
            return Err(ErrorCode::Unavailable.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod return_owner_tests {
    use super::*;
    #[test]
    fn transient_owner_failure_is_not_published_as_identity_loss() {
        let mut s = State {
            owner: ":1.2".into(),
            seen: Some(Instant::now()),
            ..State::default()
        };
        s.association.epoch = uuid::Uuid::new_v4().to_string();
        s.publish(
            "opaque-return".into(),
            vec![WindowRef {
                generation: s.generation.clone(),
                token: "token".into(),
            }],
        );
        let good = s.association.clone();
        assert_eq!(
            s.reconcile_owner(Err(ErrorCode::Unavailable)),
            Err(ErrorCode::Unavailable)
        );
        assert_eq!(
            s.association, good,
            "a failed read must not publish a destructive clear"
        );
        s.reconcile_owner(Ok(Some(":1.2"))).unwrap();
        assert_eq!(
            s.association, good,
            "the next verified read recovers without another adapter report"
        );
        s.reconcile_owner(Ok(None)).unwrap();
        assert!(s.association.window.is_empty());
        assert_eq!(s.association.target, good.target);
        assert_eq!(s.association.revision, good.revision + 1);
    }
}
