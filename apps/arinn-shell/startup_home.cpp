// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "startup_home.h"
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>

StartupHome::StartupHome(SessionLifecycle *lifecycle, QObject *parent)
    : QObject(parent), m_lifecycle(lifecycle) {
  connect(&m_retry, &QTimer::timeout, this, &StartupHome::attempt);
  connect(lifecycle, &SessionLifecycle::changed, this, &StartupHome::attempt);
  m_retry.start(1000);
  QTimer::singleShot(0, this, &StartupHome::attempt);
}

void StartupHome::attempt() {
  if (m_done || m_pending || m_lifecycle->blocked())
    return;
  m_pending = true;
  auto request = QDBusMessage::createMethodCall(
      "org.arinn.Daemon1", "/org/arinn/Daemon1", "org.arinn.Daemon1", "Home");
  request.setAutoStartService(false);
  auto *call = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(request, 3000), this);
  connect(call, &QDBusPendingCallWatcher::finished, this, [this](auto *call) {
    const auto reply = call->reply();
    call->deleteLater();
    m_pending = false;
    // Home is accepted only when the platform is connected. Until then,
    // retry at the timer's bounded rate; never block the UI or fake visibility.
    if (reply.type() == QDBusMessage::ReplyMessage && reply.signature().isEmpty()) {
      m_done = true;
      m_retry.stop();
    }
  });
}
