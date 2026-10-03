// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
fn main() {
    let _guard = arinnd::telemetry::init();
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
