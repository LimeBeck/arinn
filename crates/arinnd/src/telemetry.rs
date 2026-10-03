// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Opt-in technical diagnostics. Never pass provider payloads or free-form errors here.
use arinn_protocol::validation::ErrorCode;
use sentry::protocol::{
    Breadcrumb, Context, EnvelopeItem, Event, Exception, Frame, ItemContainer, Metric, SpanStatus,
    Stacktrace, TraceContext, Transaction, Unit,
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
    let dsn = std::env::var("ARINN_SENTRY_DSN").ok().or(config.dsn)?;
    if !valid_dsn(&dsn) {
        return None;
    }
    let mut options = sentry::ClientOptions::default();
    options.dsn = dsn.parse().ok();
    options.release = Some(concat!("arinn@", env!("CARGO_PKG_VERSION")).into());
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
    options.transport = Some(Arc::new(FilteringFactory));
    options.before_send = Some(Arc::new(|event| Some(sanitize(event))));
    options.before_breadcrumb = Some(Arc::new(safe_breadcrumb));
    let guard = sentry::init(options);
    record("startup", None, 0);
    Some(guard)
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
fn sanitize_transaction(transaction: Transaction<'static>) -> Option<Transaction<'static>> {
    let name = transaction.name.as_deref()?;
    if !matches!(name, "daemon.startup" | "daemon.operation") {
        return None;
    }
    let mut clean = Transaction {
        event_id: transaction.event_id,
        name: Some(name.into()),
        release: Some(concat!("arinn@", env!("CARGO_PKG_VERSION")).into()),
        platform: "rust".into(),
        timestamp: transaction.timestamp,
        start_timestamp: transaction.start_timestamp,
        ..Default::default()
    };
    clean.tags.insert("component".into(), "arinnd".into());
    if let Some(Context::Trace(trace)) = transaction.contexts.get("trace") {
        clean.contexts.insert(
            "trace".into(),
            TraceContext {
                trace_id: trace.trace_id,
                span_id: trace.span_id,
                parent_span_id: trace.parent_span_id,
                op: Some(name.into()),
                status: trace.status,
                ..Default::default()
            }
            .into(),
        );
    }
    Some(clean)
}
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
    metric.attributes.clear();
    metric
        .attributes
        .insert("component".into(), "arinnd".into());
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
fn trace(name: &'static str, error: bool, duration_ms: u64) {
    let now = SystemTime::now();
    let transaction = sentry::start_transaction_with_timestamp(
        sentry::TransactionContext::new(name, name),
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
fn safe_error(error: &str) -> Option<&str> {
    match error {
        "NotFound" | "Unavailable" | "Unsupported" | "PermissionDenied" | "Timeout"
        | "InvalidArgument" | "Conflict" | "Internal" | "Cancelled" => Some(error),
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
    if let Some(error) = crumb
        .data
        .get("error")
        .and_then(|v| v.as_str())
        .and_then(safe_error)
    {
        clean.data.insert("error".into(), error.into());
    }
    Some(clean)
}
fn sanitize(event: Event<'static>) -> Event<'static> {
    let mut clean = Event {
        event_id: event.event_id,
        timestamp: event.timestamp,
        level: event.level,
        platform: "rust".into(),
        release: Some(concat!("arinn@", env!("CARGO_PKG_VERSION")).into()),
        message: Some("Daemon technical diagnostic; private message omitted".into()),
        logger: Some("arinnd".into()),
        stacktrace: event.stacktrace.map(safe_stack),
        ..Default::default()
    };
    clean.tags.insert("component".into(), "arinnd".into());
    for key in ["operation", "error"] {
        if let Some(value) = event.tags.get(key) {
            let safe = if key == "operation" {
                safe_operation(value)
            } else {
                safe_error(value)
            };
            if let Some(safe) = safe {
                clean.tags.insert(key.into(), safe.into());
            }
        }
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
/// `operation` is a fixed adapter literal; `error` is the typed public safe error code.
pub fn record(operation: &'static str, error: Option<ErrorCode>, duration_ms: u64) {
    if !sentry::Hub::current()
        .client()
        .is_some_and(|c| c.is_enabled())
    {
        return;
    }
    let Some(operation) = safe_operation(operation) else {
        return;
    };
    if operation == "operation.complete" {
        sentry::metrics::counter("arinn.daemon.operations", 1).capture();
        sentry::metrics::distribution("arinn.daemon.operation.duration", duration_ms as f64)
            .unit(Unit::Millisecond)
            .capture();
        trace("daemon.operation", error.is_some(), duration_ms);
    }
    if operation == "ready" || (operation == "startup" && error.is_some()) {
        sentry::metrics::distribution("arinn.daemon.startup.duration", duration_ms as f64)
            .unit(Unit::Millisecond)
            .capture();
        trace("daemon.startup", error.is_some(), duration_ms);
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
        let mut event = Event::default();
        event.tags.insert("operation".into(), operation.into());
        event.tags.insert("error".into(), error.to_string());
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
            "transaction":"daemon.operation", "server_name":private,
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
}
