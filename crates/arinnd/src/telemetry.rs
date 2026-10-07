// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Opt-in technical diagnostics. Never pass provider payloads or free-form errors here.
//! Every field that leaves the process is one of the fixed values below (ADR-0092).
use arinn_protocol::validation::{CAPABILITIES, ErrorCode};
use sentry::protocol::{
    Breadcrumb, Context, EnvelopeItem, Event, Exception, Frame, ItemContainer, Level, Metric, Span,
    SpanStatus, Stacktrace, TraceContext, Transaction, Unit,
};
use sentry::{Envelope, Transport};
use serde::Deserialize;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

#[derive(Default, Deserialize)]
struct Configuration {
    #[serde(default)]
    telemetry: Telemetry,
}
#[derive(Default, Deserialize)]
struct Telemetry {
    #[serde(default)]
    enabled: bool,
    dsn: Option<String>,
}

fn config_path() -> Option<PathBuf> {
    let root = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(root) if !root.is_empty() => PathBuf::from(root),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    root.is_absolute()
        .then(|| root.join("arinn/telemetry.toml"))
}
fn parse_config(text: &str) -> Telemetry {
    toml::from_str::<Configuration>(text)
        .unwrap_or_default()
        .telemetry
}
fn default_dsn() -> &'static str {
    include_str!("../../../observability/sentry-default-dsn.txt")
        .lines()
        .find(|line| line.starts_with("https://"))
        .expect("the checked-in public Sentry DSN must exist")
}
fn resolve_dsn(environment: Option<String>, configuration: Option<String>) -> String {
    environment
        .or(configuration)
        .unwrap_or_else(|| default_dsn().to_owned())
}
fn valid_dsn(text: &str) -> bool {
    if text.len() > 2048 || text.parse::<sentry::types::Dsn>().is_err() {
        return false;
    }
    let Ok(url) = url::Url::parse(text) else {
        return false;
    };
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "[::1]"));
    !url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && (url.scheme() == "https" || (loopback && url.scheme() == "http"))
}
/// No SDK, transport or panic handler is created unless enabled=true and DSN is valid.
pub fn init() -> Option<sentry::ClientInitGuard> {
    let path = config_path()?;
    if std::fs::metadata(&path).ok()?.len() > 1024 * 1024 {
        return None;
    }
    let config = parse_config(&std::fs::read_to_string(path).ok()?);
    if !config.enabled {
        return None;
    }
    let environment = std::env::var_os("ARINN_SENTRY_DSN")
        .map(|value| value.into_string())
        .transpose()
        .ok()?;
    let dsn = resolve_dsn(environment, config.dsn);
    if !valid_dsn(&dsn) {
        return None;
    }
    let mut options = options(&dsn);
    options.transport = Some(Arc::new(FilteringFactory));
    let guard = sentry::init(options);
    record("startup", None, 0);
    Some(guard)
}
/// SDK options shared by the daemon and the tests; the caller sets the transport.
fn options(dsn: &str) -> sentry::ClientOptions {
    let mut options = sentry::ClientOptions::default();
    options.dsn = dsn.parse().ok();
    options.release = Some(RELEASE.into());
    options.default_integrations = false;
    options.integrations = vec![
        Arc::new(sentry::integrations::panic::PanicIntegration::default()),
        Arc::new(sentry::integrations::backtrace::AttachStacktraceIntegration),
    ];
    options.attach_stacktrace = true;
    options.send_default_pii = false;
    options.max_breadcrumbs = 100;
    options.shutdown_timeout = Duration::from_secs(2);
    options = options.traces_sample_rate(1.0);
    options.before_send = Some(Arc::new(|event| Some(sanitize(event))));
    options.before_breadcrumb = Some(Arc::new(safe_breadcrumb));
    options
}

struct FilteringFactory;
impl sentry::TransportFactory for FilteringFactory {
    fn create_transport_with_options(
        &self,
        options: sentry::TransportOptions,
    ) -> Arc<dyn Transport> {
        Arc::new(FilteringTransport(
            sentry::transports::ReqwestHttpTransportOptions::from(options).build(),
        ))
    }
}
struct FilteringTransport(sentry::transports::ReqwestHttpTransport);
impl Transport for FilteringTransport {
    fn send_envelope(&self, envelope: Envelope) {
        let clean = filter_envelope(envelope);
        if clean.items().next().is_some() {
            self.0.send_envelope(clean);
        }
    }
    fn flush(&self, timeout: Duration) -> bool {
        self.0.flush(timeout)
    }
    fn shutdown(&self, timeout: Duration) -> bool {
        self.0.shutdown(timeout)
    }
}
/// Release reported with every payload; #357 replaces it with the build id.
const RELEASE: &str = concat!("arinn@", env!("CARGO_PKG_VERSION"));
/// Operation kinds: fixed Arinn identifiers, never user content (ADR-0092).
pub const KINDS: &[&str] = &[
    "application.launch",
    "application.activate",
    "application.terminate",
    "browser.activate_tab",
    "browser.close_tab",
    "browse.open",
    "daemon.reload",
    "provider.restart",
    "download.pause",
    "download.resume",
    "download.start-now",
    "download.retry",
    "library.launch",
    "library.restart",
    "media.command:play",
    "media.command:pause",
    "media.command:play-pause",
    "media.command:seek",
    "media.command:next",
    "media.command:previous",
    "media.command:stop",
    "media.command:select-track",
    "media.command:disable-subtitles",
    "media.command:thumbnail",
    "media.present",
    "media.release",
    "platform.window.activate",
    "platform.window.close",
    "settings.set",
    "shortcut.open",
];
/// Child span kinds: a daemon → provider call or a wait for the platform adapter.
pub const SPAN_OPS: &[&str] = &["provider.call", "platform.wait"];
/// Child span names besides the operation kinds themselves.
pub const SPAN_NAMES: &[&str] = &[
    "media.open",
    "browser.launch",
    "browser.open_url",
    "browser.show_window",
];
/// Normalized failure reasons the daemon can observe itself.
pub const REASONS: &[&str] = &[
    "provider_changed",
    "platform_timeout",
    "platform_channel_closed",
    // Managed playback windows (#369).
    "platform_not_polling",
    "platform_window_missing",
    "platform_conflict",
    "session_locked",
    "platform_cancelled",
];
const OUTCOMES: &[&str] = &["succeeded", "failed"];
const MAX_SPANS: usize = 16;

fn listed(list: &[&'static str], value: &str) -> Option<&'static str> {
    list.iter().copied().find(|known| *known == value)
}
/// A manifest plugin id such as `mpv`; anything else is dropped.
fn safe_plugin(value: &str) -> Option<&str> {
    let bytes = value.as_bytes();
    (!bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_alphanumeric()
        && bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        }))
    .then_some(value)
}
fn safe_tag(key: &str, value: &str) -> Option<String> {
    match key {
        "operation" => safe_operation(value).map(Into::into),
        "error" => safe_error(value).map(Into::into),
        "kind" => listed(KINDS, value).map(Into::into),
        "capability" => listed(CAPABILITIES, value).map(Into::into),
        "plugin" => safe_plugin(value).map(Into::into),
        "outcome" => listed(OUTCOMES, value).map(Into::into),
        "reason" => listed(REASONS, value).map(Into::into),
        _ => None,
    }
}
const TAGS: &[&str] = &[
    "operation",
    "kind",
    "capability",
    "plugin",
    "outcome",
    "error",
    "reason",
];
fn safe_tags(
    tags: &sentry::protocol::Map<String, String>,
) -> sentry::protocol::Map<String, String> {
    let mut clean = sentry::protocol::Map::new();
    clean.insert("component".into(), "arinnd".into());
    for key in TAGS {
        if let Some(value) = tags.get(*key).and_then(|v| safe_tag(key, v)) {
            clean.insert((*key).into(), value);
        }
    }
    clean
}

/// A timed step of an operation: a provider call or a platform wait.
#[derive(Clone, Debug)]
pub struct Phase {
    pub op: &'static str,
    pub name: &'static str,
    pub start: SystemTime,
    pub end: SystemTime,
    /// `None` when the step was cut short, for example by the operation deadline.
    pub outcome: Option<Result<(), ErrorCode>>,
}
/// A completed operation, described only by fixed labels.
pub struct Completion<'a> {
    pub kind: &'static str,
    pub capability: &'a str,
    pub plugin: &'a str,
    pub error: Option<ErrorCode>,
    pub reason: Option<&'static str>,
    pub started: SystemTime,
    pub duration_ms: u64,
    pub phases: &'a [Phase],
}
fn status(error: Option<ErrorCode>) -> SpanStatus {
    match error {
        None => SpanStatus::Ok,
        Some(ErrorCode::Timeout) => SpanStatus::DeadlineExceeded,
        Some(ErrorCode::Cancelled) => SpanStatus::Cancelled,
        Some(ErrorCode::NotFound) => SpanStatus::NotFound,
        Some(ErrorCode::Unavailable) => SpanStatus::Unavailable,
        Some(ErrorCode::PermissionDenied) => SpanStatus::PermissionDenied,
        Some(ErrorCode::InvalidArgument) => SpanStatus::InvalidArgument,
        Some(ErrorCode::Conflict) => SpanStatus::AlreadyExists,
        Some(ErrorCode::Unsupported) => SpanStatus::Unimplemented,
        Some(ErrorCode::Internal) => SpanStatus::InternalError,
    }
}
/// Caller-side mistakes and races are warnings; daemon or provider faults are errors.
fn level(error: ErrorCode) -> Level {
    match error {
        ErrorCode::Timeout | ErrorCode::Unavailable | ErrorCode::Internal => Level::Error,
        _ => Level::Warning,
    }
}
/// One failure is one Sentry issue: component + operation + error code + reason.
fn fingerprint(tags: &sentry::protocol::Map<String, String>) -> Vec<String> {
    let Some(error) = tags.get("error") else {
        return Vec::new();
    };
    let Some(operation) = tags.get("kind").or_else(|| tags.get("operation")) else {
        return Vec::new();
    };
    vec![
        "arinnd".into(),
        operation.clone(),
        error.clone(),
        tags.get("reason").cloned().unwrap_or_else(|| "-".into()),
    ]
}
/// A readable title built only from allowlisted tags.
fn title(tags: &sentry::protocol::Map<String, String>) -> String {
    match (
        tags.get("kind").or_else(|| tags.get("operation")),
        tags.get("error"),
    ) {
        (Some(operation), Some(error)) => match tags.get("reason") {
            Some(reason) => format!("{operation} failed: {error} ({reason})"),
            None => format!("{operation} failed: {error}"),
        },
        _ => "Daemon technical diagnostic; private message omitted".into(),
    }
}

fn safe_span(span: Span) -> Option<Span> {
    let op = listed(SPAN_OPS, span.op.as_deref()?)?;
    let name = span.description.as_deref()?;
    let name = listed(KINDS, name).or_else(|| listed(SPAN_NAMES, name))?;
    Some(Span {
        span_id: span.span_id,
        trace_id: span.trace_id,
        parent_span_id: span.parent_span_id,
        same_process_as_parent: span.same_process_as_parent,
        op: Some(op.into()),
        description: Some(name.into()),
        timestamp: span.timestamp,
        start_timestamp: span.start_timestamp,
        status: span.status,
        ..Default::default()
    })
}
fn sanitize_transaction(transaction: Transaction<'static>) -> Option<Transaction<'static>> {
    let name = transaction.name.as_deref()?;
    let (name, op) = match name {
        "daemon.startup" => ("daemon.startup", "daemon.startup"),
        _ => (listed(KINDS, name)?, "daemon.operation"),
    };
    let mut clean = Transaction {
        event_id: transaction.event_id,
        name: Some(name.into()),
        release: Some(RELEASE.into()),
        platform: "rust".into(),
        timestamp: transaction.timestamp,
        start_timestamp: transaction.start_timestamp,
        tags: safe_tags(&transaction.tags),
        spans: transaction
            .spans
            .into_iter()
            .filter_map(safe_span)
            .take(MAX_SPANS)
            .collect(),
        ..Default::default()
    };
    if op == "daemon.operation" {
        clean.tags.insert("kind".into(), name.into());
    }
    if let Some(Context::Trace(trace)) = transaction.contexts.get("trace") {
        clean.contexts.insert(
            "trace".into(),
            TraceContext {
                trace_id: trace.trace_id,
                span_id: trace.span_id,
                parent_span_id: trace.parent_span_id,
                op: Some(op.into()),
                status: trace.status,
                ..Default::default()
            }
            .into(),
        );
    }
    Some(clean)
}
const METRIC_ATTRIBUTES: &[&str] = &["kind", "outcome", "error"];
fn sanitize_metric(mut metric: Metric) -> Option<Metric> {
    if !matches!(
        metric.name.as_ref(),
        "arinn.daemon.operations"
            | "arinn.daemon.errors"
            | "arinn.daemon.operation.duration"
            | "arinn.daemon.startup.duration"
    ) || !metric.value.is_finite()
        || metric.value < 0.0
    {
        return None;
    }
    metric.unit = metric
        .name
        .ends_with(".duration")
        .then_some(Unit::Millisecond);
    let original = std::mem::take(&mut metric.attributes);
    metric
        .attributes
        .insert("component".into(), "arinnd".into());
    for key in METRIC_ATTRIBUTES {
        let value = original.get(*key).and_then(|v| v.0.as_str());
        if let Some(value) = value.and_then(|v| safe_tag(key, v)) {
            metric.attributes.insert((*key).into(), value.into());
        }
    }
    Some(metric)
}
// Rebuild headers as well: SDK dynamic sampling context can contain scope data.
fn filter_envelope(envelope: Envelope) -> Envelope {
    let mut clean = Envelope::new();
    for item in envelope.into_items() {
        match item {
            EnvelopeItem::Event(event) => clean.add_item(sanitize(*event)),
            EnvelopeItem::Transaction(transaction) => {
                if let Some(transaction) = sanitize_transaction(*transaction) {
                    clean.add_item(transaction);
                }
            }
            EnvelopeItem::ItemContainer(ItemContainer::Metrics(metrics)) => {
                let metrics: Vec<_> = metrics
                    .into_iter()
                    .take(1000)
                    .filter_map(sanitize_metric)
                    .collect();
                if !metrics.is_empty() {
                    clean.add_item(ItemContainer::Metrics(metrics));
                }
            }
            _ => {} // Never send raw envelopes, attachments, sessions, logs or unknown items.
        }
    }
    clean
}
fn startup_trace(error: bool, duration_ms: u64) {
    let now = SystemTime::now();
    let transaction = sentry::start_transaction_with_timestamp(
        sentry::TransactionContext::new("daemon.startup", "daemon.startup"),
        now.checked_sub(Duration::from_millis(duration_ms))
            .unwrap_or(now),
    );
    transaction.set_status(if error {
        SpanStatus::InternalError
    } else {
        SpanStatus::Ok
    });
    transaction.finish();
}

fn safe_symbol(symbol: Option<String>) -> Option<String> {
    symbol.filter(|s| s.len() <= 512 && !s.contains(['/', '\\', '\n', '\r']))
}
fn safe_stack(stack: Stacktrace) -> Stacktrace {
    Stacktrace {
        frames: stack
            .frames
            .into_iter()
            .take(128)
            .map(|frame| Frame {
                function: safe_symbol(frame.function),
                symbol: safe_symbol(frame.symbol),
                module: safe_symbol(frame.module),
                lineno: frame.lineno,
                colno: frame.colno,
                in_app: frame.in_app,
                image_addr: frame.image_addr,
                instruction_addr: frame.instruction_addr,
                symbol_addr: frame.symbol_addr,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}
fn safe_operation(operation: &str) -> Option<&'static str> {
    match operation {
        "startup" => Some("startup"),
        "ready" => Some("ready"),
        "shutdown" => Some("shutdown"),
        "seed-web-shortcuts" => Some("seed-web-shortcuts"),
        "operation.complete" => Some("operation.complete"),
        _ => None,
    }
}
fn safe_error(error: &str) -> Option<&'static str> {
    match error {
        "NotFound" => Some("NotFound"),
        "Unavailable" => Some("Unavailable"),
        "Unsupported" => Some("Unsupported"),
        "PermissionDenied" => Some("PermissionDenied"),
        "Timeout" => Some("Timeout"),
        "InvalidArgument" => Some("InvalidArgument"),
        "Conflict" => Some("Conflict"),
        "Internal" => Some("Internal"),
        "Cancelled" => Some("Cancelled"),
        _ => None,
    }
}
fn safe_breadcrumb(crumb: Breadcrumb) -> Option<Breadcrumb> {
    if crumb.category.as_deref() != Some("arinnd.lifecycle") {
        return None;
    }
    let operation = safe_operation(crumb.message.as_deref()?)?;
    let mut clean = Breadcrumb {
        timestamp: crumb.timestamp,
        category: Some("arinnd.lifecycle".into()),
        message: Some(operation.into()),
        level: crumb.level,
        ..Default::default()
    };
    if let Some(duration) = crumb.data.get("duration_ms").and_then(|v| v.as_u64()) {
        clean.data.insert("duration_ms".into(), duration.into());
    }
    for key in ["kind", "outcome", "error", "reason"] {
        if let Some(value) = crumb
            .data
            .get(key)
            .and_then(|v| v.as_str())
            .and_then(|v| safe_tag(key, v))
        {
            clean.data.insert(key.into(), value.into());
        }
    }
    Some(clean)
}
fn sanitize(event: Event<'static>) -> Event<'static> {
    let tags = safe_tags(&event.tags);
    let mut clean = Event {
        event_id: event.event_id,
        timestamp: event.timestamp,
        level: event.level,
        platform: "rust".into(),
        release: Some(RELEASE.into()),
        message: Some(title(&tags)),
        logger: Some("arinnd".into()),
        stacktrace: event.stacktrace.map(safe_stack),
        // Never trust a supplied fingerprint: rebuild it from the clean tags.
        fingerprint: fingerprint(&tags).into_iter().map(Into::into).collect(),
        tags,
        ..Default::default()
    };
    if clean.fingerprint.is_empty() {
        clean.fingerprint = Event::default().fingerprint;
    }
    clean.exception.values = event
        .exception
        .values
        .into_iter()
        .take(4)
        .map(|e| Exception {
            ty: "RustPanic".into(),
            value: Some("Rust panic; private message omitted".into()),
            stacktrace: e.stacktrace.map(safe_stack),
            ..Default::default()
        })
        .collect();
    clean.breadcrumbs.values = event
        .breadcrumbs
        .values
        .into_iter()
        .filter_map(safe_breadcrumb)
        .take(100)
        .collect();
    clean
}
fn enabled() -> bool {
    sentry::Hub::current()
        .client()
        .is_some_and(|c| c.is_enabled())
}
/// Daemon lifecycle: `operation` is a fixed literal; `error` is the typed public error code.
pub fn record(operation: &'static str, error: Option<ErrorCode>, duration_ms: u64) {
    if !enabled() {
        return;
    }
    let Some(operation) = safe_operation(operation) else {
        return;
    };
    if operation == "ready" || (operation == "startup" && error.is_some()) {
        sentry::metrics::distribution("arinn.daemon.startup.duration", duration_ms as f64)
            .unit(Unit::Millisecond)
            .capture();
        startup_trace(error.is_some(), duration_ms);
    }
    if error.is_some() {
        sentry::metrics::counter("arinn.daemon.errors", 1).capture();
    }
    let mut crumb = Breadcrumb {
        category: Some("arinnd.lifecycle".into()),
        message: Some(operation.into()),
        ..Default::default()
    };
    crumb.data.insert("duration_ms".into(), duration_ms.into());
    if let Some(error) = error {
        crumb.data.insert("error".into(), error.to_string().into());
    }
    sentry::add_breadcrumb(crumb);
    if let Some(error) = error {
        let mut event = Event {
            level: level(error),
            ..Default::default()
        };
        event.tags.insert("operation".into(), operation.into());
        event.tags.insert("error".into(), error.to_string());
        sentry::capture_event(event);
    }
}
/// A completed daemon operation: one transaction with its provider and platform
/// spans, metrics by kind and, on failure, one event grouped by kind, error and reason.
pub fn operation(completion: &Completion<'_>) {
    if !enabled() {
        return;
    }
    let outcome = if completion.error.is_some() {
        "failed"
    } else {
        "succeeded"
    };
    let error = completion.error.map(|e| e.to_string());
    let mut tags = sentry::protocol::Map::new();
    tags.insert("kind".to_owned(), completion.kind.to_owned());
    tags.insert("capability".to_owned(), completion.capability.to_owned());
    tags.insert("plugin".to_owned(), completion.plugin.to_owned());
    tags.insert("outcome".to_owned(), outcome.to_owned());
    if let Some(error) = &error {
        tags.insert("error".to_owned(), error.clone());
    }
    if let Some(reason) = completion.reason {
        tags.insert("reason".to_owned(), reason.to_owned());
    }
    let tags = safe_tags(&tags);
    let Some(kind) = tags.get("kind").cloned() else {
        return;
    };

    let mut counter = sentry::metrics::counter("arinn.daemon.operations", 1)
        .attribute("kind", kind.clone())
        .attribute("outcome", outcome);
    if let Some(error) = &error {
        counter = counter.attribute("error", error.clone());
    }
    counter.capture();
    let mut duration = sentry::metrics::distribution(
        "arinn.daemon.operation.duration",
        completion.duration_ms as f64,
    )
    .unit(Unit::Millisecond)
    .attribute("kind", kind.clone())
    .attribute("outcome", outcome);
    if let Some(error) = &error {
        duration = duration.attribute("error", error.clone());
    }
    duration.capture();
    if let Some(error) = &error {
        sentry::metrics::counter("arinn.daemon.errors", 1)
            .attribute("kind", kind.clone())
            .attribute("error", error.clone())
            .capture();
    }

    let transaction = sentry::start_transaction_with_timestamp(
        sentry::TransactionContext::new(&kind, "daemon.operation"),
        completion.started,
    );
    for (key, value) in &tags {
        if key != "component" {
            transaction.set_tag(key, value);
        }
    }
    for phase in completion.phases.iter().take(MAX_SPANS) {
        let span = transaction.start_child_with_details(
            phase.op,
            phase.name,
            Default::default(),
            phase.start,
        );
        span.set_status(match phase.outcome {
            Some(result) => status(result.err()),
            None => SpanStatus::Cancelled,
        });
        span.finish_with_timestamp(phase.end);
    }
    transaction.set_status(status(completion.error));
    transaction
        .finish_with_timestamp(completion.started + Duration::from_millis(completion.duration_ms));

    let mut crumb = Breadcrumb {
        category: Some("arinnd.lifecycle".into()),
        message: Some("operation.complete".into()),
        ..Default::default()
    };
    crumb
        .data
        .insert("duration_ms".into(), completion.duration_ms.into());
    for key in ["kind", "outcome", "error", "reason"] {
        if let Some(value) = tags.get(key) {
            crumb.data.insert(key.into(), value.clone().into());
        }
    }
    sentry::add_breadcrumb(crumb);
    if let Some(error) = completion.error {
        let mut event = Event {
            level: level(error),
            ..Default::default()
        };
        event.tags = tags;
        sentry::capture_event(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enabled_is_explicit_and_defaults_to_false() {
        for config in [
            "",
            "[telemetry]",
            "[telemetry]\nenabled = false",
            "[telemetry]\nenabled = 'true'",
            "[telemetry]\nenabled = 1",
            "broken [",
        ] {
            assert!(!parse_config(config).enabled, "{config}");
        }
        assert!(parse_config("[telemetry]\nenabled = true").enabled);
        assert!(!parse_config("[telemetry]\ndsn = 'https://fixture@example.invalid/1'").enabled);
    }
    #[test]
    fn public_default_and_explicit_overrides() {
        assert!(valid_dsn(default_dsn()));
        assert_eq!(resolve_dsn(None, None), default_dsn());
        assert_eq!(resolve_dsn(None, Some("configured".into())), "configured");
        assert_eq!(
            resolve_dsn(Some("environment".into()), Some("configured".into())),
            "environment"
        );
        assert_eq!(resolve_dsn(Some(String::new()), None), "");
        assert_eq!(resolve_dsn(None, Some(String::new())), "");
        assert!(!parse_config("[telemetry]").enabled);
        assert!(!parse_config("[telemetry]\nenabled = false").enabled);
        assert!(!parse_config("[telemetry]\nenabled = true\ndsn = false").enabled);
    }
    #[test]
    fn dsn_rejects_remote_plaintext_and_credentials() {
        assert!(valid_dsn("https://fixture@example.invalid/1"));
        assert!(valid_dsn("http://fixture@127.0.0.1:9000/1"));
        for dsn in [
            "http://fixture@example.invalid/1",
            "https://fixture:secret@example.invalid/1",
            "https://fixture@example.invalid/1?private=true",
            "broken",
        ] {
            assert!(!valid_dsn(dsn));
        }
    }
    #[test]
    fn transport_rebuilds_transactions_metrics_and_drops_unknown_items() {
        let private = "private-token-fixture";
        let transaction: Transaction = serde_json::from_value(serde_json::json!({
            "transaction":"library.launch", "server_name":private,
            "tags":{"private":private}, "extra":{"private":private}, "user":{"username":private},
            "request":{"url":"https://private.invalid"},
            "spans":[{"description":private,"data":{"private":private}}],
            "contexts":{"trace":{"description":private,"data":{"private":private},"op":private}, "device":{"name":private}}
        })).unwrap();
        let metric: Metric = serde_json::from_value(serde_json::json!({
            "name":"arinn.daemon.operations", "type":"counter", "value":1,
            "timestamp":1, "trace_id":"0123456789abcdef0123456789abcdef",
            "attributes":{"private":{"type":"string", "value":private}}
        }))
        .unwrap();
        let mut envelope = Envelope::new();
        envelope.add_item(transaction);
        envelope.add_item(ItemContainer::Metrics(vec![metric]));
        envelope.add_item(ItemContainer::Logs(vec![]));
        let clean = filter_envelope(envelope);
        assert_eq!(clean.items().count(), 2);
        let mut bytes = Vec::new();
        clean.to_writer(&mut bytes).unwrap();
        let encoded = String::from_utf8(bytes).unwrap();
        assert!(!encoded.contains(private), "{encoded}");
        assert!(!encoded.contains("private.invalid"), "{encoded}");
        assert!(encoded.contains("library.launch"));
        assert!(encoded.contains("daemon.operation"));
        assert!(encoded.contains("arinn.daemon.operations"));
        let unknown = Transaction {
            name: Some(private.into()),
            ..Default::default()
        };
        assert!(sanitize_transaction(unknown).is_none());
    }
    #[test]
    fn sanitizes_every_sdk_payload_field() {
        let private = "private-token-fixture";
        let mut event: Event = serde_json::from_value(serde_json::json!({
            "message":private,"logentry":{"message":private},"server_name":private,
            "tags":{"user":private,"error":"Internal","operation":"startup"},
            "extra":{"secret":private},"contexts":{"device":{"name":private}},
            "request":{"url":"https://private.invalid/page"},"user":{"username":private},
            "exception":{"values":[{"type":private,"value":private,"module":private,"stacktrace":{"frames":[{"function":"arinnd::safe","filename":private,"abs_path":"/home/private-fixture/data","context_line":private,"vars":{"token":private}}]}}]},
            "breadcrumbs":{"values":[{"category":"http","message":private},{"category":"arinnd.lifecycle","message":"operation.complete","data":{"duration_ms":42,"error":"Unavailable","private":private}}]}
        })).unwrap();
        event.fingerprint = vec![private.into()].into();
        let clean = sanitize(event);
        let encoded = serde_json::to_string(&clean).unwrap();
        for secret in [
            private,
            "private.invalid",
            "private-fixture",
            "context_line",
            "username",
            "server_name",
        ] {
            assert!(!encoded.contains(secret), "{encoded}");
        }
        assert!(encoded.contains("arinnd::safe"));
        assert_eq!(clean.tags["error"], "Internal");
        assert_eq!(clean.breadcrumbs.values.len(), 1);
    }

    /// Runs `work` against a private hub whose transport applies the real filter.
    fn captured(work: impl FnOnce()) -> String {
        let sent = Arc::new(std::sync::Mutex::new(Vec::<Envelope>::new()));
        struct Recorder(Arc<std::sync::Mutex<Vec<Envelope>>>);
        impl Transport for Recorder {
            fn send_envelope(&self, envelope: Envelope) {
                self.0.lock().unwrap().push(filter_envelope(envelope));
            }
        }
        let mut options = options("https://fixture@example.invalid/1");
        let recorder = sent.clone();
        options.transport = Some(Arc::new(move |_: &sentry::ClientOptions| {
            Arc::new(Recorder(recorder.clone())) as Arc<dyn Transport>
        }));
        let client = Arc::new(sentry::Client::with_options(options));
        let hub = Arc::new(sentry::Hub::new(
            Some(client.clone()),
            Arc::new(Default::default()),
        ));
        sentry::Hub::run(hub, work);
        client.flush(Some(Duration::from_secs(2)));
        let mut text = String::new();
        for envelope in sent.lock().unwrap().iter() {
            let mut bytes = Vec::new();
            envelope.to_writer(&mut bytes).unwrap();
            text.push_str(&String::from_utf8(bytes).unwrap());
        }
        text
    }
    fn failed(kind: &'static str, plugin: &str, reason: Option<&'static str>) -> String {
        let start = SystemTime::now();
        let phases = [Phase {
            op: "platform.wait",
            name: kind,
            start,
            end: start + Duration::from_millis(5),
            outcome: None,
        }];
        captured(|| {
            operation(&Completion {
                kind,
                capability: "media",
                plugin,
                error: Some(ErrorCode::Timeout),
                reason,
                started: start,
                duration_ms: 5000,
                phases: &phases,
            })
        })
    }
    fn items<'a>(text: &'a str, kind: &str) -> Vec<serde_json::Value> {
        let lines: Vec<&'a str> = text.lines().collect();
        lines
            .windows(2)
            .filter(|pair| pair[0].contains(&format!("\"type\":\"{kind}\"")))
            .map(|pair| serde_json::from_str(pair[1]).unwrap())
            .collect()
    }
    #[test]
    fn failed_operation_is_named_tagged_traced_and_grouped() {
        let text = failed("media.present", "mpv", Some("platform_timeout"));
        let events = items(&text, "event");
        assert_eq!(events.len(), 1, "{text}");
        let event = &events[0];
        assert_eq!(
            event["message"]["formatted"]
                .as_str()
                .or(event["message"].as_str()),
            Some("media.present failed: Timeout (platform_timeout)"),
            "{event}"
        );
        // Error is the protocol default and is not serialized.
        assert!(
            event["level"].is_null() || event["level"] == "error",
            "{event}"
        );
        assert_eq!(event["tags"]["kind"], "media.present");
        assert_eq!(event["tags"]["capability"], "media");
        assert_eq!(event["tags"]["plugin"], "mpv");
        assert_eq!(event["tags"]["outcome"], "failed");
        assert_eq!(event["tags"]["reason"], "platform_timeout");
        assert_eq!(
            event["fingerprint"],
            serde_json::json!(["arinnd", "media.present", "Timeout", "platform_timeout"])
        );
        let transactions = items(&text, "transaction");
        assert_eq!(transactions.len(), 1, "{text}");
        let transaction = &transactions[0];
        assert_eq!(transaction["transaction"], "media.present");
        assert_eq!(transaction["contexts"]["trace"]["op"], "daemon.operation");
        assert_eq!(
            transaction["contexts"]["trace"]["status"],
            "deadline_exceeded"
        );
        assert_eq!(transaction["spans"][0]["op"], "platform.wait");
        assert_eq!(transaction["spans"][0]["description"], "media.present");
        assert_eq!(transaction["spans"][0]["status"], "cancelled");
        let metrics = items(&text, "trace_metric");
        let duration = metrics
            .iter()
            .flat_map(|batch| batch["items"].as_array().cloned().unwrap_or_default())
            .find(|metric| metric["name"] == "arinn.daemon.operation.duration")
            .unwrap_or_else(|| panic!("{text}"));
        assert_eq!(duration["attributes"]["kind"]["value"], "media.present");
        assert_eq!(duration["attributes"]["outcome"]["value"], "failed");
        assert_eq!(duration["attributes"]["error"]["value"], "Timeout");
    }
    #[test]
    fn identical_failures_share_a_fingerprint() {
        let print = |text: String| items(&text, "event")[0]["fingerprint"].clone();
        let first = print(failed("library.launch", "library", None));
        assert_eq!(first, print(failed("library.launch", "library", None)));
        assert_eq!(
            first,
            serde_json::json!(["arinnd", "library.launch", "Timeout", "-"])
        );
        assert_ne!(first, print(failed("media.present", "library", None)));
        assert_ne!(
            first,
            print(failed(
                "library.launch",
                "library",
                Some("provider_changed")
            ))
        );
    }
    #[test]
    fn private_labels_never_leave_the_process() {
        for plugin in [
            "/home/private-fixture/plugin",
            "file:///home/private-fixture/Movie.mkv",
            "Private Title",
            "private-token-fixture.mkv\n",
        ] {
            let text = failed("media.present", plugin, None);
            assert!(!text.contains("private-"), "{text}");
            assert!(!text.contains("Private "), "{text}");
            assert!(!items(&text, "event").is_empty());
        }
        let mut event = Event::default();
        for (key, value) in [
            ("kind", "https://private.invalid/watch"),
            ("capability", "private-token-fixture"),
            ("reason", "Private tab"),
            ("outcome", "/home/private-fixture"),
            ("error", "private-token-fixture"),
            ("title", "Private Title"),
        ] {
            event.tags.insert(key.into(), value.into());
        }
        event.fingerprint = vec!["private-token-fixture".into()].into();
        let encoded = serde_json::to_string(&sanitize(event)).unwrap();
        for marker in ["private-", "Private ", "private.invalid"] {
            assert!(!encoded.contains(marker), "{encoded}");
        }
    }
    #[test]
    fn spans_and_metric_attributes_are_allowlisted() {
        let transaction: Transaction = serde_json::from_value(serde_json::json!({
            "transaction":"library.launch",
            "tags":{"kind":"library.launch","plugin":"library","title":"Private Title"},
            "spans":[
                {"op":"provider.call","description":"media.open","tags":{"t":"Private"},"data":{"d":"Private"}},
                {"op":"provider.call","description":"/home/private-fixture/Movie.mkv"},
                {"op":"http.client","description":"library.launch"}
            ]
        }))
        .unwrap();
        let clean = sanitize_transaction(transaction).unwrap();
        let encoded = serde_json::to_string(&clean).unwrap();
        assert!(!encoded.contains("rivate"), "{encoded}");
        assert_eq!(clean.spans.len(), 1);
        assert_eq!(clean.tags["plugin"], "library");
        let metric: Metric = serde_json::from_value(serde_json::json!({
            "name":"arinn.daemon.operations", "type":"counter", "value":1,
            "timestamp":1, "trace_id":"0123456789abcdef0123456789abcdef",
            "attributes":{
                "kind":{"type":"string","value":"library.launch"},
                "outcome":{"type":"string","value":"Private tab"},
                "plugin":{"type":"string","value":"library"}
            }
        }))
        .unwrap();
        let clean = sanitize_metric(metric).unwrap();
        let keys: Vec<_> = clean.attributes.keys().map(|k| k.as_ref()).collect();
        assert_eq!(keys.len(), 2, "{keys:?}");
        assert!(keys.contains(&"kind") && keys.contains(&"component"));
    }
}
