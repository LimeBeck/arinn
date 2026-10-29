// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::telemetry::{self, Completion, Phase};
use arinn_protocol::{
    production::{self, EntityRef, OperationInfo},
    validation::ErrorCode,
};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use zbus::{Connection, message::Header, object_server::SignalEmitter, zvariant::OwnedObjectPath};
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
/// Fixed diagnostic identity of an operation; never user content (ADR-0093).
#[derive(Clone, Debug, Default)]
pub struct Label {
    pub kind: &'static str,
    pub capability: &'static str,
    /// Manifest plugin id of the provider instance, not its instance id.
    pub plugin: String,
}
impl Label {
    pub fn new(kind: &'static str) -> Self {
        debug_assert!(telemetry::KINDS.contains(&kind), "unlisted kind {kind}");
        Self {
            kind,
            ..Default::default()
        }
    }
    pub fn provider(mut self, capability: &str, plugin: &str) -> Self {
        self.capability = arinn_protocol::validation::CAPABILITIES
            .iter()
            .copied()
            .find(|known| *known == capability)
            .unwrap_or_default();
        self.plugin = plugin.into();
        self
    }
}
#[derive(Default)]
struct Trace {
    label: Label,
    /// Provider instances chosen after admission (a URL open picks its
    /// browser, a library launch its media provider), so losing any of them
    /// still finishes the operation. Bounded: an operation uses few providers.
    chosen: Vec<String>,
    phases: Vec<Phase>,
    /// Steps still running, closed as cut short if the operation ends first.
    open: Vec<(u64, Phase)>,
    next: u64,
    reason: Option<&'static str>,
    done: bool,
}
const MAX_PHASES: usize = 16;
/// Asks the provider to stop the work behind a cancellable operation
/// (ADR-0110). Success means the provider confirmed the stop.
pub type Canceller =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send>> + Send + Sync>;
#[derive(Clone)]
pub struct Operation {
    pub value: Arc<Mutex<OperationInfo>>,
    pub caller: String,
    provider_id: String,
    request_id: String,
    admitted_at: Instant,
    admitted_wall: SystemTime,
    kind: &'static str,
    trace: Arc<std::sync::Mutex<Trace>>,
    canceller: Arc<std::sync::Mutex<Option<Canceller>>>,
}
/// Closes a phase when dropped, so a step cut by a deadline is still traced.
struct PhaseGuard<'a> {
    operation: &'a Operation,
    id: Option<u64>,
    outcome: Option<Result<(), ErrorCode>>,
}
impl Drop for PhaseGuard<'_> {
    fn drop(&mut self) {
        let Some(id) = self.id else {
            return;
        };
        if let Ok(mut trace) = self.operation.trace.lock()
            && let Some(index) = trace.open.iter().position(|(open, _)| *open == id)
        {
            let (_, mut phase) = trace.open.remove(index);
            phase.end = SystemTime::now();
            phase.outcome = self.outcome;
            trace.phases.push(phase);
        }
    }
}
impl Operation {
    pub fn kind(&self) -> &'static str {
        self.kind
    }
    /// Make the operation cancellable through `Operation1.Cancel` until it
    /// ends (ADR-0110).
    pub async fn cancellable(&self, canceller: Canceller) {
        *self.canceller.lock().unwrap() = Some(canceller);
        let mut value = self.value.lock().await;
        if ["queued", "running"].contains(&value.state.as_str()) {
            value.cancellable = true;
        }
    }
    /// Names the provider instance once the operation has chosen one, for
    /// diagnostics and for invalidation when that provider goes away.
    pub fn provider(&self, id: &str, capability: &str, plugin: &str) {
        if let Ok(mut trace) = self.trace.lock() {
            let label = std::mem::take(&mut trace.label);
            trace.label = label.provider(capability, plugin);
            if trace.chosen.len() < 4 {
                trace.chosen.push(id.into());
            }
        }
    }
    /// Records a further provider this operation depends on, keeping its label.
    pub fn depends_on(&self, id: &str) {
        if let Ok(mut trace) = self.trace.lock()
            && trace.chosen.len() < 4
        {
            trace.chosen.push(id.into());
        }
    }
    /// Ends a dependency recorded with `depends_on` once it is no longer needed.
    pub fn no_longer_depends_on(&self, id: &str) {
        if let Ok(mut trace) = self.trace.lock()
            && let Some(index) = trace.chosen.iter().position(|chosen| chosen == id)
        {
            trace.chosen.remove(index);
        }
    }
    fn chosen(&self, id: &str) -> bool {
        self.trace
            .lock()
            .is_ok_and(|trace| trace.chosen.iter().any(|chosen| chosen == id))
    }
    /// Times one step. `op` is a `telemetry::SPAN_OPS` entry and `name` a
    /// `telemetry::KINDS` or `telemetry::SPAN_NAMES` entry.
    pub async fn phase<T>(
        &self,
        op: &'static str,
        name: &'static str,
        work: impl std::future::Future<Output = Result<T, ErrorCode>>,
    ) -> Result<T, ErrorCode> {
        debug_assert!(telemetry::SPAN_OPS.contains(&op), "unlisted span {op}");
        debug_assert!(
            telemetry::KINDS.contains(&name) || telemetry::SPAN_NAMES.contains(&name),
            "unlisted span name {name}"
        );
        let id = self.trace.lock().ok().and_then(|mut trace| {
            if trace.done || trace.phases.len() + trace.open.len() >= MAX_PHASES {
                return None;
            }
            let id = trace.next;
            trace.next += 1;
            let now = SystemTime::now();
            trace.open.push((
                id,
                Phase {
                    op,
                    name,
                    start: now,
                    end: now,
                    outcome: None,
                },
            ));
            Some(id)
        });
        let mut guard = PhaseGuard {
            operation: self,
            id,
            outcome: None,
        };
        let result = work.await;
        guard.outcome = Some(result.as_ref().map(|_| ()).map_err(|e| *e));
        drop(guard);
        result
    }
    /// A normalized failure reason, one of `telemetry::REASONS`.
    pub fn reason(&self, reason: &'static str) {
        debug_assert!(
            telemetry::REASONS.contains(&reason),
            "unlisted reason {reason}"
        );
        if let Ok(mut trace) = self.trace.lock() {
            trace.reason = Some(reason);
        }
    }
    /// An operation without a D-Bus object, for routing tests.
    #[cfg(test)]
    pub(crate) fn detached(label: Label) -> Self {
        Self {
            value: Arc::new(Mutex::new(OperationInfo {
                state: "running".into(),
                error: String::new(),
                retryable: false,
                started_at: 0,
                ended_at: 0,
                results: Vec::new(),
                cancellable: false,
            })),
            caller: String::new(),
            provider_id: String::new(),
            request_id: String::new(),
            admitted_at: Instant::now(),
            admitted_wall: SystemTime::now(),
            kind: label.kind,
            trace: Arc::new(std::sync::Mutex::new(Trace {
                label,
                ..Default::default()
            })),
            canceller: Arc::default(),
        }
    }
    /// Capability, plugin, `op name` of each step and the reason, as telemetry would see them.
    #[cfg(test)]
    pub(crate) fn traced(&self) -> (&'static str, String, Vec<String>, Option<&'static str>) {
        let (label, phases, reason) = self.close_trace();
        let steps = phases
            .iter()
            .map(|phase| {
                let state = match phase.outcome {
                    None => " cut",
                    Some(Err(_)) => " failed",
                    Some(Ok(())) => "",
                };
                format!("{} {}{state}", phase.op, phase.name)
            })
            .collect();
        (label.capability, label.plugin, steps, reason)
    }
    /// Ends tracing: steps still running are reported as cut short.
    fn close_trace(&self) -> (Label, Vec<Phase>, Option<&'static str>) {
        let Ok(mut trace) = self.trace.lock() else {
            return (Label::new(self.kind), Vec::new(), None);
        };
        trace.done = true;
        let now = SystemTime::now();
        let mut phases = std::mem::take(&mut trace.phases);
        phases.extend(trace.open.drain(..).map(|(_, mut phase)| {
            phase.end = now;
            phase
        }));
        phases.sort_by_key(|phase| phase.start);
        (std::mem::take(&mut trace.label), phases, trace.reason)
    }
    pub async fn finish(
        &self,
        c: &Connection,
        path: &OwnedObjectPath,
        outcome: Result<Vec<EntityRef>, ErrorCode>,
    ) {
        let mut value = self.value.lock().await;
        if !["queued", "running"].contains(&value.state.as_str()) {
            return;
        }
        value.ended_at = now();
        if value.started_at == 0 {
            value.started_at = value.ended_at;
        }

        let safe_error = outcome.as_ref().err().copied();
        value.cancellable = false;
        match outcome {
            Ok(results) => {
                value.state = "succeeded".into();
                value.results = results;
            }
            Err(error) => {
                value.state = if error == ErrorCode::Cancelled {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                value.error = error.to_string();
                value.retryable = error == ErrorCode::Unavailable;
            }
        }
        let duration_ms = self.admitted_at.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let (label, phases, reason) = self.close_trace();
        eprintln!(
            "{}",
            serde_json::json!({"level":"info","component":"arinnd","provider_id":if self.provider_id.is_empty(){None}else{Some(&self.provider_id)},"request_id":self.request_id,"operation":"complete","kind":self.kind,"outcome":value.state,"error":if value.error.is_empty(){None}else{Some(&value.error)},"reason":reason,"duration_ms":duration_ms})
        );
        crate::telemetry::operation(&Completion {
            kind: self.kind,
            capability: label.capability,
            plugin: &label.plugin,
            error: safe_error,
            reason,
            started: self.admitted_wall,
            duration_ms,
            phases: &phases,
        });
        drop(value);
        if let Ok(emitter) = SignalEmitter::new(c, path.clone()) {
            let _ = Self::changed(&emitter).await;
        }
    }
}
#[zbus::interface(name = "org.arinn.Operation1")]
impl Operation {
    async fn get(&self) -> (OperationInfo,) {
        (self.value.lock().await.clone(),)
    }
    /// Only the caller that started the operation may cancel it. An ended
    /// operation is left as it is; a provider refusal leaves it running
    /// (ADR-0110).
    async fn cancel(
        &self,
        #[zbus(header)] h: Header<'_>,
        #[zbus(connection)] c: &Connection,
    ) -> production::Result<()> {
        if h.sender().map(|s| s.as_str()) != Some(self.caller.as_str()) {
            return Err(ErrorCode::PermissionDenied.into());
        }
        let path: OwnedObjectPath = h.path().ok_or(ErrorCode::Internal)?.to_owned().into();
        // Kept after the operation ends, so an ended launch still answers
        // success while other kinds stay Unsupported.
        let canceller = self
            .canceller
            .lock()
            .unwrap()
            .clone()
            .ok_or(ErrorCode::Unsupported)?;
        let ended = |state: &str| !["queued", "running"].contains(&state);
        if ended(&self.value.lock().await.state) {
            return Ok(());
        }
        match canceller().await {
            Ok(()) => {
                self.finish(c, &path, Err(ErrorCode::Cancelled)).await;
                Ok(())
            }
            // The launch ended some other way while the provider was asked.
            Err(_) if ended(&self.value.lock().await.state) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}
struct Entry {
    fingerprint: String,
    path: OwnedObjectPath,
    operation: Operation,
    provider: String,
}
#[derive(Clone, Default)]
pub struct Operations {
    entries: Arc<Mutex<HashMap<(String, String), Entry>>>,
}
impl Operations {
    pub async fn existing(
        &self,
        caller: &str,
        request_id: &str,
        fingerprint: &str,
    ) -> Result<Option<OwnedObjectPath>, ErrorCode> {
        uuid::Uuid::parse_str(request_id).map_err(|_| ErrorCode::InvalidArgument)?;
        let entries = self.entries.lock().await;
        if let Some(entry) = entries.get(&(caller.into(), request_id.into())) {
            if entry.fingerprint != fingerprint {
                return Err(ErrorCode::Conflict);
            }
            return Ok(Some(entry.path.clone()));
        }
        Ok(None)
    }

    pub async fn admit(
        &self,
        c: &Connection,
        caller: &str,
        request_id: &str,
        fingerprint: &str,
        provider: &str,
        label: Label,
    ) -> Result<(OwnedObjectPath, Operation, bool), ErrorCode> {
        uuid::Uuid::parse_str(request_id).map_err(|_| ErrorCode::InvalidArgument)?;
        let key = (caller.to_owned(), request_id.to_owned());
        let mut entries = self.entries.lock().await;
        if let Some(entry) = entries.get(&key) {
            if entry.fingerprint != fingerprint {
                return Err(ErrorCode::Conflict);
            }
            return Ok((entry.path.clone(), entry.operation.clone(), false));
        }
        if entries.len() >= 1024 {
            return Err(ErrorCode::Unavailable);
        }
        // Completed history stays available for deduplication, but must not
        // consume the caller's concurrency budget (for example, block Stop
        // after repeated seeks and thumbnail reads).
        let mut caller_active = 0;
        let mut provider_active = 0;
        for ((owner, _), entry) in entries.iter() {
            if owner != caller && entry.provider != provider {
                continue;
            }
            if ["queued", "running"].contains(&entry.operation.value.lock().await.state.as_str()) {
                if owner == caller {
                    caller_active += 1;
                }
                if entry.provider == provider {
                    provider_active += 1;
                }
            }
        }
        if caller_active >= 128 || provider_active >= 32 {
            return Err(ErrorCode::Unavailable);
        }
        let path: OwnedObjectPath = format!(
            "/org/arinn/Daemon1/operations/o_{}",
            uuid::Uuid::new_v4().simple()
        )
        .try_into()
        .unwrap();
        let operation = Operation {
            caller: caller.into(),
            provider_id: provider.into(),
            request_id: request_id.into(),
            admitted_at: Instant::now(),
            admitted_wall: SystemTime::now(),
            kind: label.kind,
            trace: Arc::new(std::sync::Mutex::new(Trace {
                label,
                ..Default::default()
            })),
            canceller: Arc::default(),
            value: Arc::new(Mutex::new(OperationInfo {
                state: "queued".into(),
                error: String::new(),
                retryable: false,
                started_at: 0,
                ended_at: 0,
                results: Vec::new(),
                cancellable: false,
            })),
        };
        c.object_server()
            .at(path.clone(), operation.clone())
            .await
            .map_err(|_| ErrorCode::Internal)?;
        entries.insert(
            key.clone(),
            Entry {
                fingerprint: fingerprint.into(),
                path: path.clone(),
                operation: operation.clone(),
                provider: provider.into(),
            },
        );
        let book = self.clone();
        let connection = c.clone();
        let object = path.clone();
        let op = operation.clone();
        tokio::spawn(async move {
            // Retention starts at completion, not admission. Polling here only
            // reaps objects; it does not retry or drive application commands.
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if op.value.lock().await.ended_at > 0 {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_secs(300)).await;
            book.entries.lock().await.remove(&key);
            let _ = connection
                .object_server()
                .remove::<Operation, _>(object)
                .await;
        });
        Ok((path, operation, true))
    }
    pub async fn invalidate(&self, c: &Connection, provider: Option<&str>, caller: Option<&str>) {
        let entries = self.entries.lock().await;
        for entry in entries.values() {
            let lost =
                provider.is_some_and(|id| id == entry.provider || entry.operation.chosen(id));
            if lost || caller.is_some_and(|id| id == entry.operation.caller) {
                if lost {
                    entry.operation.reason("provider_changed");
                }
                entry
                    .operation
                    .finish(c, &entry.path, Err(ErrorCode::Unavailable))
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod trace_tests {
    use super::*;

    #[tokio::test]
    async fn a_step_still_running_at_completion_is_reported_cut_short() {
        let op = Operation::detached(Label::new("library.launch").provider("library", "library"));
        op.phase("provider.call", "library.launch", async { Ok(()) })
            .await
            .unwrap();
        let (release, pending) = tokio::sync::oneshot::channel::<()>();
        let running = {
            let op = op.clone();
            tokio::spawn(async move {
                op.phase("provider.call", "media.open", async {
                    pending.await.map_err(|_| ErrorCode::Unavailable)
                })
                .await
            })
        };
        tokio::task::yield_now().await;
        // Invalidation finishes the operation while the provider call still waits.
        let (capability, plugin, steps, _) = op.traced();
        assert_eq!((capability, plugin.as_str()), ("library", "library"));
        assert_eq!(
            steps,
            [
                "provider.call library.launch",
                "provider.call media.open cut"
            ]
        );
        release.send(()).unwrap();
        running.await.unwrap().unwrap();
        // A step ending after completion is not reported twice.
        assert!(op.traced().2.is_empty());
    }
}
