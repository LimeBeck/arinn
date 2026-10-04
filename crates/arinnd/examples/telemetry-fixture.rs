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
    arinnd::telemetry::record("operation.complete", None, 42);
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
