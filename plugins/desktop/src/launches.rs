// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The lifetime of a desktop launch after dispatch (spec §6.5, §7.2,
//! ADR-0110): it ends when the application shows a window, fails at once when
//! its application unit ends with no window, and stops that unit only when
//! the daemon cancels it.
use crate::discovery::Entry;
use arinn_plugin_sdk::platform::WindowSnapshot;
use arinn_protocol::validation::ErrorCode;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::watch;

/// How often a starting launch looks at the window inventory and its unit.
pub const POLL: Duration = Duration::from_millis(200);
/// How long `cancel` waits for a launch to stop its unit.
const STOP_WAIT: Duration = Duration::from_secs(10);
/// How long a stopped launch waits for its processes to exit. `StopUnit`
/// only queues the stop; systemd escalates to SIGKILL after the unit's
/// stop timeout (90 s by default), so this is a backstop.
const STOP_SETTLE: Duration = Duration::from_secs(100);

/// Stop the launch's unit and wait until its processes have exited, so a
/// cancel is reported only once they are gone. `Timeout` if they outlive
/// [`STOP_SETTLE`].
pub async fn stop<S: Scope>(scope: &S) -> Result<(), ErrorCode> {
    scope.stop().await?;
    tokio::time::timeout(STOP_SETTLE, async {
        while scope.running().await {
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .map_err(|_| ErrorCode::Timeout)
}
/// How long the window inventory may be missing during a launch before the
/// launch counts as dispatched. A reconnecting platform adapter is a gap,
/// not the end of the launch.
pub const INVENTORY_GRACE: Duration = Duration::from_secs(5);

/// What a launch observes; implemented by the systemd unit and by tests.
pub trait Scope {
    /// Whether any process of the launch still runs.
    fn running(&self) -> impl Future<Output = bool> + Send;
    /// Stop the processes of this launch only.
    fn stop(&self) -> impl Future<Output = Result<(), ErrorCode>> + Send;
}
impl Scope for crate::launcher::LaunchUnit {
    async fn running(&self) -> bool {
        crate::launcher::LaunchUnit::running(self).await
    }
    async fn stop(&self) -> Result<(), ErrorCode> {
        crate::launcher::LaunchUnit::stop(self).await
    }
}

/// The windows present when the launch started.
pub struct Baseline {
    generation: String,
    tokens: HashSet<String>,
}
impl Baseline {
    pub fn new(snapshot: &WindowSnapshot) -> Self {
        Self {
            generation: snapshot.generation.clone(),
            tokens: snapshot.windows.iter().map(|w| w.token.clone()).collect(),
        }
    }
    /// The first inventory after a launch that started without one: what
    /// was there before is unknown, so a window with exactly the launched
    /// desktop ID counts as new; every other window counts as present before.
    pub fn after_gap(snapshot: &WindowSnapshot, id: &str) -> Self {
        Self {
            generation: snapshot.generation.clone(),
            tokens: snapshot
                .windows
                .iter()
                .filter(|w| w.desktop_id != id)
                .map(|w| w.token.clone())
                .collect(),
        }
    }
    /// A window this launch produced: new since the launch started, and not
    /// proven to belong to another catalogue application. Exact desktop IDs
    /// are often missing for games and wrapped launchers, so an unknown or
    /// empty ID counts; the Activity itself is still matched exactly.
    fn produced(
        &mut self,
        id: &str,
        entries: &BTreeMap<String, Entry>,
        snapshot: &WindowSnapshot,
    ) -> bool {
        if snapshot.generation != self.generation {
            // A new platform generation renames every window; nothing is
            // new because of that alone.
            *self = Self::new(snapshot);
            return false;
        }
        snapshot.windows.iter().any(|w| {
            !self.tokens.contains(&w.token)
                && (w.desktop_id == id || !entries.contains_key(&w.desktop_id))
        })
    }
}

/// Wait for the launched application's window. `cached` returns the latest
/// inventory the provider holds; `fresh` reads it again, used once the unit
/// has ended so that a window reported just before is not missed. `None`
/// means there is no inventory. If it stays missing for `grace`, the launch
/// counts as dispatched, as for a provider that cannot observe windows; that
/// includes an inventory that is missing when the launch starts (`baseline`
/// is `None`), whose first snapshot then becomes the baseline.
#[allow(clippy::too_many_arguments)] // The inputs of one decision.
pub async fn wait<S, C, F, Fut>(
    id: &str,
    entries: &BTreeMap<String, Entry>,
    baseline: Option<Baseline>,
    cached: C,
    fresh: F,
    scope: &S,
    cancel: &mut watch::Receiver<bool>,
    grace: Duration,
) -> Result<(), ErrorCode>
where
    S: Scope,
    C: Fn() -> Option<WindowSnapshot>,
    F: Fn() -> Fut,
    Fut: Future<Output = Option<WindowSnapshot>>,
{
    let mut missing_since: Option<tokio::time::Instant> = None;
    let mut baseline = baseline;
    loop {
        if *cancel.borrow_and_update() {
            stop(scope).await?;
            return Err(ErrorCode::Cancelled);
        }
        match cached() {
            Some(snapshot) => {
                missing_since = None;
                if baseline
                    .get_or_insert_with(|| Baseline::after_gap(&snapshot, id))
                    .produced(id, entries, &snapshot)
                {
                    return Ok(());
                }
            }
            None => {
                let since = *missing_since.get_or_insert_with(tokio::time::Instant::now);
                if since.elapsed() >= grace {
                    return Ok(());
                }
            }
        }
        if !scope.running().await {
            let Some(snapshot) = fresh().await else {
                // Nothing to decide with: dispatched, but only once the
                // inventory has been missing for the grace, as above.
                let since = *missing_since.get_or_insert_with(tokio::time::Instant::now);
                if since.elapsed() >= grace {
                    return Ok(());
                }
                tokio::select! {
                    _ = tokio::time::sleep(POLL) => {}
                    _ = cancel.changed() => {}
                }
                continue;
            };
            // Handed to a running instance: its existing window is the
            // Activity (spec §7.2).
            let mut baseline = baseline.unwrap_or_else(|| Baseline::after_gap(&snapshot, id));
            return if baseline.produced(id, entries, &snapshot)
                || snapshot.windows.iter().any(|w| w.desktop_id == id)
            {
                Ok(())
            } else {
                Err(ErrorCode::Unavailable)
            };
        }
        tokio::select! {
            _ = tokio::time::sleep(POLL) => {}
            _ = cancel.changed() => {}
        }
    }
}

type Stopped = Option<Result<(), ErrorCode>>;
struct Pending {
    sequence: u64,
    cancel: watch::Sender<bool>,
    stopped: watch::Receiver<Stopped>,
}
#[derive(Default)]
struct Inner {
    next: AtomicU64,
    pending: Mutex<HashMap<String, Vec<Pending>>>,
}
/// Unfinished launches by application ID, for `CancelLaunch`.
#[derive(Clone, Default)]
pub struct Launches(Arc<Inner>);
/// One unfinished launch. Dropping it ends the launch's registration.
pub struct Ticket {
    launches: Launches,
    id: String,
    sequence: u64,
    pub cancel: watch::Receiver<bool>,
    stopped: watch::Sender<Stopped>,
}
impl Ticket {
    pub fn cancelled(&self) -> bool {
        *self.cancel.borrow()
    }
    /// Report the outcome of a cancellation to `Launches::cancel`.
    pub fn stopped(&self, result: Result<(), ErrorCode>) {
        self.stopped.send_replace(Some(result));
    }
}
impl Drop for Ticket {
    fn drop(&mut self) {
        let mut pending = self.launches.0.pending.lock().unwrap();
        if let Some(list) = pending.get_mut(&self.id) {
            list.retain(|p| p.sequence != self.sequence);
            if list.is_empty() {
                pending.remove(&self.id);
            }
        }
    }
}
impl Launches {
    pub fn begin(&self, id: &str) -> Ticket {
        let sequence = self.0.next.fetch_add(1, Ordering::Relaxed);
        let (cancel, cancel_rx) = watch::channel(false);
        let (stopped, stopped_rx) = watch::channel(None);
        self.0
            .pending
            .lock()
            .unwrap()
            .entry(id.into())
            .or_default()
            .push(Pending {
                sequence,
                cancel,
                stopped: stopped_rx,
            });
        Ticket {
            launches: self.clone(),
            id: id.into(),
            sequence,
            cancel: cancel_rx,
            stopped,
        }
    }
    /// Cancel every unfinished launch of `id` and wait until each has stopped
    /// its unit. `NotFound` when none was pending, or when every launch ended
    /// some other way first (for example, its window appeared).
    pub async fn cancel(&self, id: &str) -> Result<(), ErrorCode> {
        let waiting: Vec<_> = {
            let pending = self.0.pending.lock().unwrap();
            let Some(list) = pending.get(id) else {
                return Err(ErrorCode::NotFound);
            };
            list.iter()
                .map(|p| {
                    p.cancel.send_replace(true);
                    p.stopped.clone()
                })
                .collect()
        };
        let mut result = Err(ErrorCode::NotFound);
        for mut stopped in waiting {
            let outcome = tokio::time::timeout(STOP_WAIT, stopped.wait_for(Option::is_some))
                .await
                .map_err(|_| ErrorCode::Timeout)
                .and_then(|value| value.map(|v| *v).map_err(|_| ErrorCode::NotFound));
            match outcome {
                Ok(Some(Ok(()))) => {
                    if result == Err(ErrorCode::NotFound) {
                        result = Ok(());
                    }
                }
                Ok(Some(Err(error))) | Err(error) if error != ErrorCode::NotFound => {
                    result = Err(error)
                }
                _ => {}
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arinn_protocol::{application::Application, platform::Window};
    use std::sync::atomic::AtomicBool;

    #[derive(Default)]
    struct Fixture {
        running: AtomicBool,
        stops: AtomicU64,
        /// After a stop, the processes still run for this many reads, as
        /// while systemd stops the unit.
        linger: Mutex<u64>,
    }
    impl Scope for Fixture {
        async fn running(&self) -> bool {
            if self.stops.load(Ordering::SeqCst) > 0 {
                let mut linger = self.linger.lock().unwrap();
                if *linger > 0 {
                    *linger -= 1;
                    if *linger == 0 {
                        self.running.store(false, Ordering::SeqCst);
                    }
                }
            }
            self.running.load(Ordering::SeqCst)
        }
        async fn stop(&self) -> Result<(), ErrorCode> {
            self.stops.fetch_add(1, Ordering::SeqCst);
            if *self.linger.lock().unwrap() == 0 {
                self.running.store(false, Ordering::SeqCst);
            }
            Ok(())
        }
    }
    fn entries() -> BTreeMap<String, Entry> {
        ["game.desktop", "other.desktop"]
            .into_iter()
            .map(|id| {
                (
                    id.to_string(),
                    Entry {
                        application: Application {
                            id: id.into(),
                            title: id.into(),
                            can_launch: true,
                            group: "games".into(),
                            icon: String::new(),
                        },
                        path: format!("/fixture/{id}").into(),
                        dbus_name: None,
                    },
                )
            })
            .collect()
    }
    fn window(token: &str, desktop_id: &str) -> Window {
        Window {
            token: token.into(),
            desktop_id: desktop_id.into(),
            output: String::new(),
            active: false,
            fullscreen: false,
            can_activate: true,
            can_close: true,
        }
    }
    fn snapshot(windows: Vec<Window>) -> WindowSnapshot {
        WindowSnapshot {
            generation: "g".into(),
            revision: 1,
            windows,
        }
    }
    async fn run(
        before: Vec<Window>,
        later: Vec<Window>,
        running: bool,
    ) -> (Result<(), ErrorCode>, u64) {
        let scope = Fixture::default();
        scope.running.store(running, Ordering::SeqCst);
        let (_cancel, mut cancel_rx) = watch::channel(false);
        let baseline = Baseline::new(&snapshot(before));
        let later = snapshot(later);
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            wait(
                "game.desktop",
                &entries(),
                Some(baseline),
                || Some(later.clone()),
                || async { Some(later.clone()) },
                &scope,
                &mut cancel_rx,
                INVENTORY_GRACE,
            ),
        )
        .await
        .unwrap_or(Err(ErrorCode::Timeout));
        (result, scope.stops.load(Ordering::SeqCst))
    }

    #[tokio::test]
    async fn a_new_window_ends_the_launch_but_not_another_applications_window() {
        // The exact application, or a window with no proven application.
        for desktop_id in ["game.desktop", "", "steam_app_1"] {
            assert_eq!(
                run(vec![], vec![window("w", desktop_id)], true).await,
                (Ok(()), 0),
                "{desktop_id}"
            );
        }
        // Another catalogue application's window, or one that was already
        // there, keeps a running launch starting.
        assert_eq!(
            run(vec![], vec![window("w", "other.desktop")], true).await,
            (Err(ErrorCode::Timeout), 0)
        );
        assert_eq!(
            run(
                vec![window("old", "game.desktop")],
                vec![window("old", "game.desktop")],
                true
            )
            .await,
            (Err(ErrorCode::Timeout), 0)
        );
    }

    #[tokio::test]
    async fn an_ended_scope_fails_at_once_unless_the_application_has_a_window() {
        assert_eq!(
            run(vec![], vec![], false).await,
            (Err(ErrorCode::Unavailable), 0)
        );
        assert_eq!(
            run(vec![], vec![window("w", "other.desktop")], false).await,
            (Err(ErrorCode::Unavailable), 0)
        );
        // Handed to a running instance whose window already existed.
        assert_eq!(
            run(
                vec![window("old", "game.desktop")],
                vec![window("old", "game.desktop")],
                false
            )
            .await,
            (Ok(()), 0)
        );
    }

    #[tokio::test]
    async fn a_new_platform_generation_is_not_a_new_window() {
        let scope = Fixture::default();
        scope.running.store(true, Ordering::SeqCst);
        let (_cancel, mut cancel_rx) = watch::channel(false);
        let mut renamed = snapshot(vec![window("renamed", "game.desktop")]);
        renamed.generation = "h".into();
        let result = tokio::time::timeout(
            Duration::from_millis(600),
            wait(
                "game.desktop",
                &entries(),
                Some(Baseline::new(&snapshot(vec![window(
                    "old",
                    "game.desktop",
                )]))),
                || Some(renamed.clone()),
                || async { Some(renamed.clone()) },
                &scope,
                &mut cancel_rx,
                INVENTORY_GRACE,
            ),
        )
        .await;
        assert!(result.is_err(), "{result:?}");
    }

    /// An inventory missing when the launch starts gets the same grace as a
    /// gap during it; its first snapshot is the baseline, where only a window
    /// with the launched desktop ID counts as new.
    #[tokio::test]
    async fn a_launch_started_without_an_inventory_waits_for_one() {
        let scope = Fixture::default();
        scope.running.store(true, Ordering::SeqCst);
        let (_cancel, mut cancel_rx) = watch::channel(false);
        let reads = AtomicU64::new(0);
        let inventory = Mutex::new(snapshot(vec![window("old", "unproven")]));
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            wait(
                "game.desktop",
                &entries(),
                None,
                || {
                    // Missing for the first reads, as while an adapter reconnects.
                    (reads.fetch_add(1, Ordering::SeqCst) >= 3)
                        .then(|| inventory.lock().unwrap().clone())
                },
                || async { Some(inventory.lock().unwrap().clone()) },
                &scope,
                &mut cancel_rx,
                Duration::from_secs(2),
            ),
        )
        .await;
        // A window that was there before is not the launch's: still waiting.
        assert!(result.is_err(), "{result:?}");
        *inventory.lock().unwrap() = snapshot(vec![
            window("old", "unproven"),
            window("new", "game.desktop"),
        ]);
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            wait(
                "game.desktop",
                &entries(),
                None,
                || Some(inventory.lock().unwrap().clone()),
                || async { Some(inventory.lock().unwrap().clone()) },
                &scope,
                &mut cancel_rx,
                Duration::from_secs(2),
            ),
        )
        .await;
        assert_eq!(result, Ok(Ok(())));
    }

    /// The unit already ended and there is no inventory to decide with: the
    /// launch is dispatched only after the grace, and an inventory that
    /// arrives within it decides as usual.
    #[tokio::test]
    async fn an_early_exit_without_an_inventory_waits_for_the_grace() {
        let scope = Fixture::default();
        let (_cancel, mut cancel_rx) = watch::channel(false);
        let grace = Duration::from_millis(600);
        let started = tokio::time::Instant::now();
        let result = wait(
            "game.desktop",
            &entries(),
            None,
            || None,
            || async { None },
            &scope,
            &mut cancel_rx,
            grace,
        )
        .await;
        assert_eq!(result, Ok(()));
        assert!(started.elapsed() >= grace, "{:?}", started.elapsed());
        // An inventory with no window of the application arrives in time:
        // the ended unit fails the launch.
        let reads = AtomicU64::new(0);
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            wait(
                "game.desktop",
                &entries(),
                None,
                || None,
                || async {
                    (reads.fetch_add(1, Ordering::SeqCst) >= 2)
                        .then(|| snapshot(vec![window("old", "unproven")]))
                },
                &scope,
                &mut cancel_rx,
                grace,
            ),
        )
        .await;
        assert_eq!(result, Ok(Err(ErrorCode::Unavailable)));
    }

    #[tokio::test]
    async fn a_lasting_inventory_gap_counts_as_dispatched_a_short_one_does_not() {
        let scope = Fixture::default();
        scope.running.store(true, Ordering::SeqCst);
        let (_cancel, mut cancel_rx) = watch::channel(false);
        let grace = Duration::from_millis(500);
        let started = tokio::time::Instant::now();
        let result = wait(
            "game.desktop",
            &entries(),
            Some(Baseline::new(&snapshot(vec![]))),
            || None,
            || async { None },
            &scope,
            &mut cancel_rx,
            grace,
        )
        .await;
        assert_eq!(result, Ok(()));
        assert!(started.elapsed() >= grace);
        // The inventory returns within the grace: still starting.
        let reads = AtomicU64::new(0);
        let result = tokio::time::timeout(
            Duration::from_millis(1500),
            wait(
                "game.desktop",
                &entries(),
                Some(Baseline::new(&snapshot(vec![]))),
                || (reads.fetch_add(1, Ordering::SeqCst) % 2 == 1).then(|| snapshot(vec![])),
                || async { Some(snapshot(vec![])) },
                &scope,
                &mut cancel_rx,
                grace,
            ),
        )
        .await;
        assert!(result.is_err(), "{result:?}");
    }

    #[tokio::test]
    async fn cancel_stops_each_pending_launch_of_that_application_only() {
        let launches = Launches::default();
        assert_eq!(launches.cancel("game").await, Err(ErrorCode::NotFound));
        let scopes: Vec<_> = (0..3).map(|_| Arc::new(Fixture::default())).collect();
        let mut tasks = Vec::new();
        for (index, scope) in scopes.iter().enumerate() {
            scope.running.store(true, Ordering::SeqCst);
            let mut ticket = launches.begin(if index < 2 { "game" } else { "other" });
            let scope = scope.clone();
            tasks.push(tokio::spawn(async move {
                let result = wait(
                    "game.desktop",
                    &entries(),
                    Some(Baseline::new(&snapshot(vec![]))),
                    || Some(snapshot(vec![])),
                    || async { Some(snapshot(vec![])) },
                    &*scope,
                    &mut ticket.cancel,
                    INVENTORY_GRACE,
                )
                .await;
                if result == Err(ErrorCode::Cancelled) {
                    ticket.stopped(Ok(()));
                }
                result
            }));
        }
        tokio::task::yield_now().await;
        assert_eq!(launches.cancel("game").await, Ok(()));
        for task in tasks.drain(..2) {
            assert_eq!(task.await.unwrap(), Err(ErrorCode::Cancelled));
        }
        assert_eq!(scopes[0].stops.load(Ordering::SeqCst), 1);
        assert_eq!(scopes[1].stops.load(Ordering::SeqCst), 1);
        assert_eq!(scopes[2].stops.load(Ordering::SeqCst), 0);
        assert!(scopes[2].running.load(Ordering::SeqCst));
        // Finished launches are no longer cancellable.
        assert_eq!(launches.cancel("game").await, Err(ErrorCode::NotFound));
        tasks[0].abort();
        let _ = tasks.pop().unwrap().await;
        assert_eq!(launches.cancel("other").await, Err(ErrorCode::NotFound));
    }

    /// `StopUnit` only queues the stop: the launch counts as cancelled once
    /// its processes have exited, not when the stop was queued.
    #[tokio::test]
    async fn a_cancelled_launch_ends_once_its_processes_have_exited() {
        let scope = Fixture::default();
        scope.running.store(true, Ordering::SeqCst);
        *scope.linger.lock().unwrap() = 3;
        let (cancel, mut cancel_rx) = watch::channel(true);
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            wait(
                "game.desktop",
                &entries(),
                Some(Baseline::new(&snapshot(vec![]))),
                || Some(snapshot(vec![])),
                || async { Some(snapshot(vec![])) },
                &scope,
                &mut cancel_rx,
                INVENTORY_GRACE,
            ),
        )
        .await
        .unwrap();
        drop(cancel);
        assert_eq!(result, Err(ErrorCode::Cancelled));
        assert_eq!(scope.stops.load(Ordering::SeqCst), 1);
        assert!(
            !scope.running.load(Ordering::SeqCst),
            "reported cancelled while the processes still run"
        );
    }

    #[tokio::test]
    async fn a_launch_that_ends_without_stopping_is_not_reported_cancelled() {
        let launches = Launches::default();
        let ticket = launches.begin("game");
        let cancel = {
            let launches = launches.clone();
            tokio::spawn(async move { launches.cancel("game").await })
        };
        tokio::task::yield_now().await;
        // Its window appeared first: the ticket ends without a stop.
        drop(ticket);
        assert_eq!(cancel.await.unwrap(), Err(ErrorCode::NotFound));
    }
}
