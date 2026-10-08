// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
fn main() {
    let _guard = arinnd::telemetry::init();
    sentry::configure_scope(|scope| {
        scope.set_tag("private", "private-token-fixture");
        scope.set_extra("private", "private-token-fixture".into());
        scope.set_user(Some(sentry::protocol::User {
            username: Some("private-token-fixture".into()),
            ..Default::default()
        }));
    });
    sentry::metrics::counter("arinn.daemon.operations", 1)
        .attribute("private", "private-token-fixture")
        .capture();
    let started = std::time::SystemTime::now() - std::time::Duration::from_millis(42);
    let phases = [
        arinnd::telemetry::Phase {
            op: "provider.call",
            name: "library.launch",
            start: started,
            end: started + std::time::Duration::from_millis(10),
            outcome: Some(Ok(())),
        },
        arinnd::telemetry::Phase {
            op: "provider.call",
            name: "media.open",
            start: started + std::time::Duration::from_millis(10),
            end: started + std::time::Duration::from_millis(40),
            outcome: Some(Err(arinn_protocol::validation::ErrorCode::Timeout)),
        },
    ];
    // The plugin label is not a manifest id: it must be dropped, not sent.
    arinnd::telemetry::operation(&arinnd::telemetry::Completion {
        kind: "library.launch",
        capability: "library",
        plugin: "/home/private-fixture/private-token-fixture",
        error: Some(arinn_protocol::validation::ErrorCode::Timeout),
        reason: Some("provider_changed"),
        started,
        duration_ms: 42,
        phases: &phases,
    });
    arinnd::telemetry::record(
        "startup",
        Some(arinn_protocol::validation::ErrorCode::Internal),
        0,
    );
    if std::env::args().any(|arg| arg == "crash") {
        panic!(
            "private-token-fixture https://private.invalid/page Private tab /home/private-fixture/data"
        );
    }
}
