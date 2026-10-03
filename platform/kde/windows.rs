// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Authoritative KWin inventory and acknowledged controls. No application discovery.
use arinn_protocol::{
    kwin, media_window,
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
    home_revision: u64,
    association: ReturnAssociation,
    exhausted: bool,
    media: MediaState,
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
            home_revision: 0,
            association: ReturnAssociation {
                epoch: String::new(),
                revision: 1,
                target: String::new(),
                window: Vec::new(),
            },
            exhausted: false,
            media: MediaState::default(),
        }
    }
}
impl State {
    fn reset(&mut self) {
        self.media.fail_all(ErrorCode::Unavailable);
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
        let expired_present = self.media.pending.as_ref().is_some_and(|p| {
            p.command.action == "present"
                && (p.command.deadline <= Instant::now() || p.command.reply.is_closed())
        });
        self.media.prune();
        if expired_present {
            let capacity = 1024 - self.queued.len() - usize::from(self.pending.is_some());
            self.media.teardown(capacity);
        }
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
    media_changed: Arc<Notify>,
    media_invalidated: Arc<Notify>,
}
async fn owner_lookup(c: &Connection) -> Result<Option<String>, ErrorCode> {
    let Some(service) = crate::platform::adapter_service() else {
        return Ok(None);
    };
    tokio::time::timeout(Duration::from_secs(3), async {
        match zbus::fdo::DBusProxy::new(c)
            .await
            .map_err(|_| ErrorCode::Unavailable)?
            .get_name_owner(service.try_into().unwrap())
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
            media_changed: Arc::default(),
            media_invalidated: Arc::default(),
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
        // Snapshot before the lookup and recheck after it: another owner's
        // Home commit (even identical content), or a session reset, invalidates
        // this continuation without holding a shared lock over authorization.
        let fence = {
            let s = self.state.lock().await;
            (s.generation.clone(), s.home_revision)
        };
        authorization.await?;
        validation::text(target)?;
        validation::text(output)?;
        let mut s = self.state.lock().await;
        if s.generation != fence.0 || s.home_revision != fence.1 {
            return Err(ErrorCode::Conflict.into());
        }
        let next = s
            .home_revision
            .checked_add(1)
            .ok_or(ErrorCode::Unavailable)?;
        platform.commit_home(visible, target, output).await;
        s.home_revision = next;
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
                    change=changes.next()=>{let Some(change)=change else{break;}; match change.args(){Ok(a) if Some(a.name.as_str())==crate::platform::adapter_service()=>a.old_owner.as_ref().map(|o|o.to_string()),_=>None}},
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
                    backend.invalidate_media();
                    announce(&connection, &generation_id, 1).await;
                }
            }
        });
        Ok(())
    }
    pub async fn disconnect(&self, owner: &str) {
        let mut s = self.state.lock().await;
        let capacity = 1024 - s.queued.len() - usize::from(s.pending.is_some());
        s.media.disconnect(owner, capacity);
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
        drop(s);
        self.invalidate_media();
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
            if s.queued.len() + usize::from(s.pending.is_some()) + s.media.count() >= 1023 {
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
        self.invalidate_media();
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
        s.media.inventory(&windows);
        if s.media
            .presentation
            .as_ref()
            .is_some_and(|(_, _, token)| !windows.iter().any(|w| w.token == *token))
        {
            let capacity = 1024 - s.queued.len() - usize::from(s.pending.is_some());
            s.media.teardown(capacity);
        }
        s.windows = windows;
        s.snapshot = Some(Arc::new(next));
        s.revision = revision;
        s.announce_return(c, previous).await;
        drop(s);
        self.invalidate_media();
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

// The optional playback lane has independent polling and acknowledgement.
struct MediaControl {
    command: Control,
    session: String,
    launch: String,
    cleanup_of: Option<String>,
}
#[derive(Default)]
struct MediaState {
    supported: bool,
    polling: bool,
    reports: HashMap<String, String>,
    queued: VecDeque<MediaControl>,
    pending: Option<MediaControl>,
    presentation: Option<(Scope, String, String)>,
    presentation_launch: String,
    presentation_request: String,
    cleanup_attempted: bool,
}
impl MediaState {
    fn count(&self) -> usize {
        self.queued.len() + usize::from(self.pending.is_some())
    }
    fn fail_all(&mut self, error: ErrorCode) {
        for p in self.queued.drain(..).chain(self.pending.take()) {
            let _ = p.command.reply.send(Err(error));
        }
        self.presentation = None;
        self.presentation_request.clear();
        self.cleanup_attempted = false;
    }
    fn prune(&mut self) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.command.deadline <= Instant::now() || p.command.reply.is_closed())
        {
            let p = self.pending.take().unwrap();
            let _ = p.command.reply.send(Err(ErrorCode::Timeout));
        }
        let mut keep = VecDeque::new();
        for p in self.queued.drain(..) {
            if p.command.deadline <= Instant::now() || p.command.reply.is_closed() {
                let _ = p.command.reply.send(Err(ErrorCode::Timeout));
            } else {
                keep.push_back(p);
            }
        }
        self.queued = keep;
    }
    fn disconnect(&mut self, owner: &str, capacity: usize) {
        let mut keep = VecDeque::new();
        for p in self.queued.drain(..) {
            if p.command.scope.owner == owner && p.command.action != "release" {
                let _ = p.command.reply.send(Err(ErrorCode::Unavailable));
            } else {
                keep.push_back(p);
            }
        }
        self.queued = keep;
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.command.scope.owner == owner && p.command.action != "release")
        {
            let p = self.pending.take().unwrap();
            let _ = p.command.reply.send(Err(ErrorCode::Unavailable));
        }
        if self
            .presentation
            .as_ref()
            .is_some_and(|(scope, _, _)| scope.owner == owner)
        {
            self.teardown(capacity);
        }
    }
    fn teardown(&mut self, capacity: usize) {
        let Some((scope, session, token)) = self.presentation.clone() else {
            return;
        };
        if self.cleanup_attempted {
            return;
        }
        if self.queued.iter().chain(self.pending.iter()).any(|p| {
            p.command.action == "release"
                && p.command.scope == scope
                && p.session == session
                && p.cleanup_of
                    .as_ref()
                    .is_none_or(|id| id == &self.presentation_request)
        }) {
            self.cleanup_attempted = true;
            return;
        }
        if self.count() >= capacity {
            return;
        }
        self.cleanup_attempted = true;
        let (reply, receive) = oneshot::channel();
        self.queued.push_back(MediaControl {
            command: Control {
                id: uuid::Uuid::new_v4().to_string(),
                action: "release".into(),
                token,
                scope,
                deadline: Instant::now() + Duration::from_secs(5),
                reply,
            },
            session,
            launch: String::new(),
            cleanup_of: Some(self.presentation_request.clone()),
        });
        tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_secs(5), receive).await;
        });
    }
    fn inventory(&mut self, windows: &[Window]) {
        self.reports.clear();
        let live: HashSet<_> = windows.iter().map(|w| w.token.as_str()).collect();
        let mut keep = VecDeque::new();
        for p in self.queued.drain(..) {
            if p.command.action == "present" && !live.contains(p.command.token.as_str()) {
                let _ = p.command.reply.send(Err(ErrorCode::NotFound));
            } else {
                keep.push_back(p);
            }
        }
        self.queued = keep;
        if self.pending.as_ref().is_some_and(|p| {
            p.command.action == "present" && !live.contains(p.command.token.as_str())
        }) {
            let p = self.pending.take().unwrap();
            let _ = p.command.reply.send(Err(ErrorCode::NotFound));
        }
    }
    fn resolve(&self, windows: &[Window], launch: &str) -> Option<String> {
        if launch.is_empty() || self.reports.len() != windows.len() {
            return None;
        }
        let mut candidates = self.reports.iter().filter(|(_, id)| id.as_str() == launch);
        let first = candidates.next()?.0.clone();
        candidates.next().is_none().then_some(first)
    }
}
impl Windows {
    fn invalidate_media(&self) {
        self.media_changed.notify_waiters();
        // The single signal relay can be busy emitting when another change
        // arrives. Retain one coalesced invalidation for its next iteration.
        self.media_invalidated.notify_one();
    }
    pub async fn media_changed(&self) {
        self.media_invalidated.notified().await;
    }
    pub async fn sync_media_sessions(
        &self,
        scope: &Scope,
        sessions: &[arinn_protocol::media::Session],
    ) {
        let mut s = self.state.lock().await;
        let valid = |p: &MediaControl| {
            p.command.scope.provider != scope.provider
                || p.command.action != "present"
                || (p.command.scope == *scope
                    && sessions.iter().any(|entry| {
                        entry.id == p.session
                            && ["playing", "paused", "ended"].contains(&entry.state.as_str())
                    }))
        };
        let mut keep = VecDeque::new();
        for p in s.media.queued.drain(..) {
            if valid(&p) {
                keep.push_back(p);
            } else {
                let _ = p.command.reply.send(Err(ErrorCode::Unavailable));
            }
        }
        s.media.queued = keep;
        if s.media.pending.as_ref().is_some_and(|p| !valid(p)) {
            let p = s.media.pending.take().unwrap();
            let _ = p.command.reply.send(Err(ErrorCode::Unavailable));
        }
        if s.media
            .presentation
            .as_ref()
            .is_some_and(|(previous, session, _)| {
                previous.provider == scope.provider
                    && (previous != scope
                        || !sessions.iter().any(|entry| {
                            entry.id == *session
                                && ["playing", "paused", "ended"].contains(&entry.state.as_str())
                        }))
            })
        {
            let capacity = 1024 - s.queued.len() - usize::from(s.pending.is_some());
            s.media.teardown(capacity);
        }
        drop(s);
        self.invalidate_media();
    }
    pub async fn media_window(
        &self,
        c: &Connection,
        launch: &str,
        expected_generation: &str,
    ) -> Result<Vec<WindowRef>, ErrorCode> {
        media_window::validate_launch(launch)?;
        let current = owner(c).await?;
        let s = self.state.lock().await;
        s.live(&current)?;
        s.generation(expected_generation)?;
        Ok(s.media
            .resolve(&s.windows, launch)
            .map(|token| {
                vec![WindowRef {
                    generation: s.generation.clone(),
                    token,
                }]
            })
            .unwrap_or_default())
    }
    pub async fn media_supported(&self, c: &Connection) -> Result<(), ErrorCode> {
        let current = owner(c).await?;
        let s = self.state.lock().await;
        s.live(&current)?;
        if s.media.supported {
            Ok(())
        } else {
            Err(ErrorCode::Unsupported)
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue_media(
        &self,
        c: &Connection,
        scope: Scope,
        session: &str,
        launch: &str,
        window: Option<&WindowRef>,
        present: bool,
    ) -> Result<Option<oneshot::Receiver<Result<(), ErrorCode>>>, ErrorCode> {
        let current = owner(c).await?;
        let mut s = self.state.lock().await;
        s.live(&current)?;
        s.prune();
        if !s.media.supported {
            return Err(ErrorCode::Unsupported);
        }
        let token = if present {
            let window = window.ok_or(ErrorCode::NotFound)?;
            s.generation(&window.generation)?;
            if s.media.resolve(&s.windows, launch).as_deref() != Some(window.token.as_str()) {
                return Err(ErrorCode::Conflict);
            }
            window.token.clone()
        } else {
            let stored = s
                .media
                .presentation
                .as_ref()
                .filter(|(previous, id, _)| previous == &scope && id == session)
                .map(|(_, _, token)| token.clone());
            let admitted = s
                .media
                .pending
                .iter()
                .chain(s.media.queued.iter())
                .rfind(|p| {
                    p.command.action == "present"
                        && p.command.scope == scope
                        && p.session == session
                })
                .map(|p| p.command.token.clone());
            let Some(token) = admitted.or(stored) else {
                return Ok(None);
            };
            if s.windows.iter().any(|w| w.token == token) {
                token
            } else {
                String::new()
            }
        };
        if s.queued.len() + usize::from(s.pending.is_some()) + s.media.count() >= 1023 {
            return Err(ErrorCode::Unavailable);
        }
        let (send, receive) = oneshot::channel();
        s.media.queued.push_back(MediaControl {
            command: Control {
                id: uuid::Uuid::new_v4().to_string(),
                action: if present { "present" } else { "release" }.into(),
                token,
                scope,
                deadline: Instant::now() + Duration::from_secs(5),
                reply: send,
            },
            session: session.into(),
            launch: launch.into(),
            cleanup_of: None,
        });
        drop(s);
        self.invalidate_media();
        Ok(Some(receive))
    }
}
fn take_media_control(s: &mut State) -> Option<Vec<String>> {
    if s.media.pending.is_some() {
        return None;
    }
    while !s.media.queued.is_empty() {
        if s.media.reports.len() != s.windows.len()
            && s.media
                .queued
                .front()
                .is_some_and(|p| p.command.action == "present")
        {
            return None;
        }
        let mut p = s.media.queued.pop_front().unwrap();
        if p.command.action == "present"
            && s.media.resolve(&s.windows, &p.launch).as_deref() != Some(p.command.token.as_str())
        {
            let _ = p.command.reply.send(Err(ErrorCode::Conflict));
            continue;
        }
        // An older session's release must not tear down a newer presentation.
        if p.command.action == "release"
            && s.media
                .presentation
                .as_ref()
                .is_none_or(|(scope, session, _)| {
                    scope != &p.command.scope
                        || session != &p.session
                        || p.cleanup_of
                            .as_ref()
                            .is_some_and(|id| id != &s.media.presentation_request)
                })
        {
            let _ = p.command.reply.send(Ok(()));
            continue;
        }
        if p.command.action == "release" && !s.windows.iter().any(|w| w.token == p.command.token) {
            p.command.token.clear();
        }
        let record = vec![
            p.command.id.clone(),
            p.command.action.clone(),
            p.command.token.clone(),
        ];
        if p.command.action == "present" {
            s.media.cleanup_attempted = false;
            s.media.presentation_request = p.command.id.clone();
            s.media.presentation_launch = p.launch.clone();
            s.media.presentation = Some((
                p.command.scope.clone(),
                p.session.clone(),
                p.command.token.clone(),
            ));
        } else {
            s.media.cleanup_attempted = true;
        }
        s.media.pending = Some(p);
        return Some(record);
    }
    None
}
pub struct MediaBridge(pub Windows);
#[zbus::interface(name = "org.arinn.KWinMedia1")]
impl MediaBridge {
    async fn report(
        &self,
        generation: &str,
        revision: i32,
        token: &str,
        launch: &str,
        #[zbus(connection)] c: &Connection,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<()> {
        let current = trusted(c, &h).await?;
        media_window::validate_report(generation, revision, token, launch)?;
        let mut s = self.0.state.lock().await;
        s.live(&current)?;
        s.generation(generation)?;
        if s.revision != revision {
            return Err(ErrorCode::Conflict.into());
        }
        if !s.windows.iter().any(|w| w.token == token) {
            return Err(ErrorCode::NotFound.into());
        }
        s.media.reports.insert(token.into(), launch.into());
        if s.media
            .presentation
            .as_ref()
            .is_some_and(|(_, _, presented)| {
                (presented == token && s.media.presentation_launch != launch)
                    || (s.media.reports.len() == s.windows.len()
                        && s.media
                            .resolve(&s.windows, &s.media.presentation_launch)
                            .as_deref()
                            != Some(presented.as_str()))
            })
        {
            let capacity = 1024 - s.queued.len() - usize::from(s.pending.is_some());
            s.media.teardown(capacity);
        }
        self.0.invalidate_media();
        if s.media.pending.as_ref().is_some_and(|p| {
            p.command.action == "present"
                && ((p.command.token == token && p.launch != launch)
                    || (s.media.reports.len() == s.windows.len()
                        && s.media.resolve(&s.windows, &p.launch).as_deref()
                            != Some(p.command.token.as_str())))
        }) {
            let p = s.media.pending.take().unwrap();
            let _ = p.command.reply.send(Err(ErrorCode::Conflict));
        }
        Ok(())
    }
    async fn next_control(
        &self,
        generation: &str,
        #[zbus(connection)] c: &Connection,
        #[zbus(header)] h: Header<'_>,
    ) -> production::Result<Vec<String>> {
        let current = trusted(c, &h).await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        let mut admitted = false;
        loop {
            let notified = self.0.media_changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let mut s = self.0.state.lock().await;
            s.generation(generation)?;
            if admitted {
                s.media.polling = false;
            } else if s.media.polling {
                return Err(ErrorCode::Conflict.into());
            }
            s.live(&current)?;
            s.prune();
            s.media.supported = true;
            if let Some(record) = take_media_control(&mut s) {
                return Ok(record);
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(Vec::new());
            }
            s.media.polling = true;
            admitted = true;
            drop(s);
            // Inventory/association notifications can wake a poll without
            // admitting a command. Keep the original bounded idle deadline.
            let _ = tokio::time::timeout_at(deadline, notified).await;
        }
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
        let mut s = loop {
            let notified = self.0.media_changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let mut s = self.0.state.lock().await;
            s.live(&current)?;
            s.generation(generation)?;
            s.prune();
            let pending = s
                .media
                .pending
                .as_ref()
                .filter(|p| p.command.id == request_id)
                .ok_or(ErrorCode::Conflict)?;
            if !error.is_empty() || pending.command.action == "release" {
                break s;
            }
            let deadline = pending.command.deadline;
            if s.media.reports.len() == s.windows.len() {
                if s.media.resolve(&s.windows, &pending.launch).as_deref()
                    != Some(pending.command.token.as_str())
                {
                    return Err(ErrorCode::Conflict.into());
                }
                break s;
            }
            drop(s);
            tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), notified)
                .await
                .map_err(|_| ErrorCode::Timeout)?;
        };
        let p = s.media.pending.take().unwrap();
        let result = if error.is_empty() {
            if p.command.action == "present" {
                s.media.presentation =
                    Some((p.command.scope.clone(), p.session, p.command.token.clone()));
            } else {
                s.media.presentation = None;
                s.media.presentation_request.clear();
                s.media.cleanup_attempted = false;
            }
            Ok(())
        } else {
            if p.command.action == "present" {
                // Dispatch may have partially changed compositor state. Keep
                // the exact tentative scope until its cleanup is acknowledged.
                let capacity = 1024 - s.queued.len() - usize::from(s.pending.is_some());
                s.media.teardown(capacity);
            }
            Err(media_error(error))
        };
        let _ = p.command.reply.send(result);
        drop(s);
        self.0.invalidate_media();
        Ok(())
    }
}
fn media_error(error: &str) -> ErrorCode {
    match error {
        "NotFound" => ErrorCode::NotFound,
        "Unsupported" => ErrorCode::Unsupported,
        "PermissionDenied" => ErrorCode::PermissionDenied,
        "Timeout" => ErrorCode::Timeout,
        "InvalidArgument" => ErrorCode::InvalidArgument,
        "Conflict" => ErrorCode::Conflict,
        "Internal" => ErrorCode::Internal,
        "Cancelled" => ErrorCode::Cancelled,
        _ => ErrorCode::Unavailable,
    }
}

#[cfg(test)]
mod media_lifetime_tests {
    use super::*;
    #[tokio::test]
    async fn media_relay_retains_invalidation_during_processing() {
        let windows = Windows::default();
        let first = windows.media_changed();
        tokio::pin!(first);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut first)
                .await
                .is_err()
        );
        windows.disconnect(":1.4").await;
        first.await;
        // The relay is processing its first signal, without an active waiter.
        windows.disconnect(":1.5").await;
        tokio::time::timeout(Duration::from_millis(100), windows.media_changed())
            .await
            .expect("second invalidation must be retained");
    }
    fn scope() -> Scope {
        Scope {
            provider: "video".into(),
            owner: ":1.4".into(),
            registration: uuid::Uuid::new_v4().to_string(),
        }
    }
    fn command(
        scope: Scope,
        action: &str,
    ) -> (MediaControl, oneshot::Receiver<Result<(), ErrorCode>>) {
        let (reply, receive) = oneshot::channel();
        (
            MediaControl {
                command: Control {
                    id: uuid::Uuid::new_v4().to_string(),
                    action: action.into(),
                    token: "window".into(),
                    scope,
                    deadline: Instant::now() + Duration::from_secs(5),
                    reply,
                },
                session: "session".into(),
                launch: uuid::Uuid::new_v4().to_string(),
                cleanup_of: None,
            },
            receive,
        )
    }
    #[tokio::test]
    async fn removal_timeout_and_owner_loss_fail_pending_work_without_redelivery() {
        let mut s = MediaState::default();
        let (p, r) = command(scope(), "present");
        s.pending = Some(p);
        s.inventory(&[]);
        assert_eq!(r.await.unwrap(), Err(ErrorCode::NotFound));
        assert!(s.pending.is_none());
        let (mut p, r) = command(scope(), "present");
        p.command.deadline = Instant::now();
        s.queued.push_back(p);
        s.prune();
        assert_eq!(r.await.unwrap(), Err(ErrorCode::Timeout));
        assert_eq!(s.count(), 0);
        let (p, r) = command(scope(), "present");
        s.pending = Some(p);
        s.disconnect(":1.4", 1024);
        assert_eq!(r.await.unwrap(), Err(ErrorCode::Unavailable));
        assert_eq!(s.count(), 0);
    }
    #[tokio::test]
    async fn teardown_does_not_duplicate_a_matching_pending_release() {
        let mut s = MediaState::default();
        let original = scope();
        s.presentation = Some((original.clone(), "session".into(), "window".into()));
        let (p, receive) = command(original, "release");
        s.pending = Some(p);
        s.teardown(1024);
        assert_eq!(s.count(), 1, "pending Release already owns exact cleanup");
        assert!(s.queued.is_empty());
        let p = s.pending.take().unwrap();
        let _ = p.command.reply.send(Err(ErrorCode::Unavailable));
        assert_eq!(receive.await.unwrap(), Err(ErrorCode::Unavailable));
        assert!(
            s.queued.is_empty(),
            "failed Release has no queued automatic retry"
        );
        s.teardown(1024);
        assert!(
            s.queued.is_empty(),
            "later invalidation cannot retry the failed Release"
        );
        let remembered = s.presentation.clone();
        let mut s = MediaState {
            presentation: remembered,
            ..MediaState::default()
        };
        let (p, _receive) = command(scope(), "release");
        s.pending = Some(p);
        s.teardown(1024);
        assert_eq!(
            s.count(),
            2,
            "another scope must not suppress original cleanup"
        );
        assert_eq!(
            s.queued.front().unwrap().command.scope,
            s.presentation.as_ref().unwrap().0
        );
    }
    #[tokio::test]
    async fn stale_release_keeps_the_next_present_behind_the_report_barrier() {
        let mut s = State {
            windows: vec![kwin::window("window", "", "screen", 6).unwrap()],
            ..State::default()
        };
        let current = scope();
        s.media.presentation = Some((current.clone(), "session".into(), "window".into()));
        let (mut old, old_reply) = command(current.clone(), "release");
        old.session = "old-session".into();
        s.media.queued.push_back(old);
        let (present, mut present_reply) = command(current, "present");
        let launch = present.launch.clone();
        s.media.queued.push_back(present);
        s.media.inventory(&s.windows);
        assert!(take_media_control(&mut s).is_none());
        assert_eq!(old_reply.await.unwrap(), Ok(()));
        assert_eq!(
            s.media.queued.len(),
            1,
            "Present waits for complete reports"
        );
        assert_eq!(
            present_reply.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        );
        s.media.reports.insert("window".into(), launch);
        let command = take_media_control(&mut s).unwrap();
        assert_eq!(&command[1..], &["present", "window"]);
        assert!(
            !s.media.cleanup_attempted,
            "new Present permits one new cleanup attempt"
        );
    }
    #[tokio::test]
    async fn failed_or_expired_release_does_not_reopen_automatic_cleanup() {
        for expired in [false, true] {
            let mut s = State::default();
            let current = scope();
            s.media.presentation = Some((current.clone(), "session".into(), "window".into()));
            let (release, reply) = command(current, "release");
            s.media.queued.push_back(release);
            assert_eq!(take_media_control(&mut s).unwrap()[1], "release");
            if expired {
                s.media.pending.as_mut().unwrap().command.deadline = Instant::now();
                s.prune();
                assert_eq!(reply.await.unwrap(), Err(ErrorCode::Timeout));
            } else {
                let release = s.media.pending.take().unwrap();
                let _ = release.command.reply.send(Err(ErrorCode::Unavailable));
                assert_eq!(reply.await.unwrap(), Err(ErrorCode::Unavailable));
            }
            s.media.teardown(1024);
            s.media.teardown(1024);
            assert!(
                s.media.queued.is_empty(),
                "failed dispatched cleanup is terminal for automatic invalidations"
            );
            assert!(
                s.media.presentation.is_some(),
                "explicit Release can still use remembered scope"
            );
        }
    }
    #[tokio::test]
    async fn provider_loss_preserves_already_admitted_cleanup() {
        let mut s = MediaState {
            presentation: Some((scope(), "session".into(), "window".into())),
            ..MediaState::default()
        };
        s.teardown(1024);
        let original = s.queued.front().unwrap().command.id.clone();
        s.disconnect(":1.4", 1024);
        assert_eq!(s.count(), 1);
        assert_eq!(s.queued.front().unwrap().command.id, original);
        assert!(s.cleanup_attempted);
        let p = s.queued.pop_front().unwrap();
        s.pending = Some(p);
        s.disconnect(":1.4", 1024);
        assert_eq!(s.count(), 1);
        assert_eq!(s.pending.as_ref().unwrap().command.id, original);
    }
    #[tokio::test]
    async fn old_cleanup_cannot_teardown_a_new_present_of_the_same_session() {
        let mut s = State::default();
        let current = scope();
        s.media.presentation = Some((current.clone(), "session".into(), "window".into()));
        s.media.presentation_request = "old-presentation".into();
        let (mut present, _reply) = command(current, "present");
        present.command.token = "replacement".into();
        let launch = present.launch.clone();
        s.media.queued.push_back(present);
        s.media.teardown(1024);
        s.windows = vec![kwin::window("replacement", "", "screen", 6).unwrap()];
        s.media.inventory(&s.windows);
        s.media.reports.insert("replacement".into(), launch);
        assert_eq!(
            &take_media_control(&mut s).unwrap()[1..],
            &["present", "replacement"]
        );
        let presented = s.media.pending.take().unwrap();
        let _ = presented.command.reply.send(Ok(()));
        assert!(
            take_media_control(&mut s).is_none(),
            "old cleanup is a no-op, not an empty-token teardown"
        );
        assert_eq!(s.media.presentation.as_ref().unwrap().2, "replacement");
        assert!(!s.media.cleanup_attempted);
        s.media.teardown(1024);
        assert_eq!(
            s.media.queued.len(),
            1,
            "new presentation has its own cleanup attempt"
        );
    }
    #[tokio::test]
    async fn loss_teardown_obeys_shared_capacity() {
        let mut s = MediaState::default();
        let previous = scope();
        s.presentation = Some((previous, "session".into(), "window".into()));
        let mut receivers = Vec::new();
        for _ in 0..1024 {
            let mut other = scope();
            other.owner = ":1.9".into();
            let (p, receive) = command(other, "present");
            s.queued.push_back(p);
            receivers.push(receive);
        }
        s.disconnect(":1.4", 1024);
        assert_eq!(
            s.count(),
            1024,
            "internal cleanup must also obey the total bound"
        );
        s.queued.pop_back();
        receivers.pop();
        s.disconnect(":1.4", 1024);
        assert_eq!(s.count(), 1024, "the reserved slot admits teardown");
        assert_eq!(s.queued.back().unwrap().command.action, "release");
    }
    #[test]
    fn reset_removes_adapter_support_and_observations() {
        let mut s = State::default();
        s.media.supported = true;
        s.media
            .reports
            .insert("window".into(), uuid::Uuid::new_v4().to_string());
        s.media.presentation = Some((scope(), "session".into(), "window".into()));
        let g = s.generation.clone();
        s.reset();
        assert_ne!(s.generation, g);
        assert!(!s.media.supported);
        assert!(s.media.reports.is_empty());
        assert!(s.media.presentation.is_none());
    }
}
