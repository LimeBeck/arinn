// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One `Operation1.Cancel` stops only its own application launch (spec §6.5,
//! ADR-0110). `Provider1.CancelLaunch` names an application ID, not a launch,
//! so the daemon forwards it only while the cancelled operation is the only
//! running launch of that provider, generation and ID, and holds a new launch
//! of that ID back until an in-flight `CancelLaunch` has been answered.
use arinn_protocol::validation::ErrorCode;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

/// The scope of one `CancelLaunch`: what the provider can tell apart.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LaunchKey {
    pub provider: String,
    pub owner: String,
    pub generation: String,
    pub local_id: String,
}

struct Slot {
    /// Operation paths of the launches that have not ended yet.
    running: HashSet<String>,
    /// True while a `CancelLaunch` for this key is in flight.
    cancelling: watch::Sender<bool>,
}

#[derive(Clone, Default)]
pub(crate) struct LaunchGuard(Arc<Mutex<HashMap<LaunchKey, Slot>>>);

impl LaunchGuard {
    /// Count a launch as running from its admission until the ticket is
    /// dropped, which must happen only after its operation has ended.
    pub fn admit(&self, key: LaunchKey, operation: &str) -> LaunchTicket {
        let mut slots = self.0.lock().unwrap();
        let slot = slots.entry(key.clone()).or_insert_with(|| Slot {
            running: HashSet::new(),
            cancelling: watch::channel(false).0,
        });
        slot.running.insert(operation.to_owned());
        LaunchTicket {
            guard: self.clone(),
            key,
            operation: operation.to_owned(),
            cancelling: slot.cancelling.subscribe(),
        }
    }
    /// Start forwarding a cancel of `operation`: `NotFound` when it has
    /// already ended, `Conflict` when another launch of the same key runs or
    /// a cancel is already in flight. The cancel is in flight until the
    /// returned ticket is dropped.
    pub fn begin_cancel(
        &self,
        key: &LaunchKey,
        operation: &str,
    ) -> Result<CancelTicket, ErrorCode> {
        let slots = self.0.lock().unwrap();
        let slot = slots.get(key).ok_or(ErrorCode::NotFound)?;
        if !slot.running.contains(operation) {
            return Err(ErrorCode::NotFound);
        }
        if slot.running.len() > 1 || *slot.cancelling.borrow() {
            return Err(ErrorCode::Conflict);
        }
        slot.cancelling.send_replace(true);
        Ok(CancelTicket {
            guard: self.clone(),
            key: key.clone(),
        })
    }
    fn release(&self, key: &LaunchKey, operation: Option<&str>) {
        let mut slots = self.0.lock().unwrap();
        let Some(slot) = slots.get_mut(key) else {
            return;
        };
        match operation {
            Some(operation) => {
                slot.running.remove(operation);
            }
            None => {
                slot.cancelling.send_replace(false);
            }
        }
        if slot.running.is_empty() && !*slot.cancelling.borrow() {
            slots.remove(key);
        }
    }
}

/// A launch counted as running.
pub(crate) struct LaunchTicket {
    guard: LaunchGuard,
    key: LaunchKey,
    operation: String,
    cancelling: watch::Receiver<bool>,
}
impl LaunchTicket {
    /// Wait until no `CancelLaunch` of this key is in flight, so that cancel
    /// cannot reach this launch (ADR-0110).
    pub async fn dispatchable(&mut self) {
        let _ = self.cancelling.wait_for(|cancelling| !cancelling).await;
    }
}
impl Drop for LaunchTicket {
    fn drop(&mut self) {
        self.guard.release(&self.key, Some(&self.operation));
    }
}

/// A `CancelLaunch` in flight.
pub(crate) struct CancelTicket {
    guard: LaunchGuard,
    key: LaunchKey,
}
impl Drop for CancelTicket {
    fn drop(&mut self) {
        self.guard.release(&self.key, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn key(local_id: &str) -> LaunchKey {
        LaunchKey {
            provider: "desktop".into(),
            owner: ":1.7".into(),
            generation: "g1".into(),
            local_id: local_id.into(),
        }
    }

    #[tokio::test]
    async fn a_second_running_launch_of_the_id_refuses_cancel() {
        let guard = LaunchGuard::default();
        let first = guard.admit(key("game"), "/op/1");
        let second = guard.admit(key("game"), "/op/2");
        // Another application does not count.
        let _other = guard.admit(key("other"), "/op/3");
        assert!(matches!(
            guard.begin_cancel(&key("game"), "/op/1"),
            Err(ErrorCode::Conflict)
        ));
        assert!(matches!(
            guard.begin_cancel(&key("game"), "/op/2"),
            Err(ErrorCode::Conflict)
        ));
        // Once the other launch has ended, the cancel goes through.
        drop(second);
        let cancel = guard.begin_cancel(&key("game"), "/op/1").unwrap();
        // A second cancel while one is in flight stops nothing.
        assert!(matches!(
            guard.begin_cancel(&key("game"), "/op/1"),
            Err(ErrorCode::Conflict)
        ));
        drop(cancel);
        drop(first);
        // Ended: nothing to forward.
        assert!(matches!(
            guard.begin_cancel(&key("game"), "/op/1"),
            Err(ErrorCode::NotFound)
        ));
        assert!(guard.0.lock().unwrap().get(&key("game")).is_none());
    }

    #[tokio::test]
    async fn a_launch_waits_for_a_cancel_in_flight() {
        let guard = LaunchGuard::default();
        let first = guard.admit(key("game"), "/op/1");
        let cancel = guard.begin_cancel(&key("game"), "/op/1").unwrap();
        // Launch B is admitted while the cancel of A is on its way to the
        // provider: it is not dispatched until the provider has answered.
        let mut second = guard.admit(key("game"), "/op/2");
        let waiting = tokio::time::timeout(Duration::from_millis(100), second.dispatchable()).await;
        assert!(
            waiting.is_err(),
            "dispatched while CancelLaunch was in flight"
        );
        // A launch of another application is not held.
        let mut other = guard.admit(key("other"), "/op/3");
        tokio::time::timeout(Duration::from_millis(100), other.dispatchable())
            .await
            .unwrap();
        drop(first);
        drop(cancel);
        tokio::time::timeout(Duration::from_secs(1), second.dispatchable())
            .await
            .unwrap();
        // B alone now: its own cancel may be forwarded.
        drop(guard.begin_cancel(&key("game"), "/op/2").unwrap());
    }

    #[tokio::test]
    async fn generations_and_providers_are_separate() {
        let guard = LaunchGuard::default();
        let _old = guard.admit(key("game"), "/op/1");
        let mut renewed = key("game");
        renewed.generation = "g2".into();
        let _new = guard.admit(renewed.clone(), "/op/2");
        drop(guard.begin_cancel(&renewed, "/op/2").unwrap());
    }
}
