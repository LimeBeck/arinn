// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QByteArray>
#include <sentry.h>
namespace ArinnTelemetry {
QByteArray filterEnvelope(const QByteArray &body);
sentry_transport_t *transport();
}
