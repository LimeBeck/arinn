// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "sessionguard.h"
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QDBusServiceWatcher>
SessionGuard::SessionGuard(QObject *parent, QDBusConnection session,
                           QDBusConnection system)
    : QObject(parent), m_session(session) {
  session.connect("org.freedesktop.ScreenSaver", "/ScreenSaver",
                  "org.freedesktop.ScreenSaver", "ActiveChanged", this,
                  SLOT(lockChanged(bool)));
  system.connect("org.freedesktop.login1", "/org/freedesktop/login1",
                 "org.freedesktop.login1.Manager", "PrepareForSleep", this,
                 SLOT(prepareForSleep(bool)));
  auto watcher =
      new QDBusServiceWatcher("org.freedesktop.ScreenSaver", session,
                              QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this, [this] {
    m_locked = true;
    emit changed();
    refresh();
  });
  refresh();
}
void SessionGuard::refresh() {
  const auto epoch = ++m_epoch;
  auto msg = QDBusMessage::createMethodCall(
      "org.freedesktop.ScreenSaver", "/ScreenSaver",
      "org.freedesktop.ScreenSaver", "GetActive");
  auto pending =
      new QDBusPendingCallWatcher(m_session.asyncCall(msg, 1500), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch](auto *call) {
            QDBusPendingReply<bool> reply = *call;
            call->deleteLater();
            if (epoch != m_epoch)
              return;
            m_locked = reply.isError() ||
                       reply.value(); // fail closed on unknown state
            emit changed();
          });
}
void SessionGuard::lockChanged(bool locked) {
  ++m_epoch; // a pending old GetActive must not undo this signal
  m_locked = locked;
  emit changed();
}
void SessionGuard::prepareForSleep(bool sleeping) {
  ++m_epoch;
  m_sleeping = sleeping;
  m_locked = true;
  emit changed();
  if (!sleeping)
    refresh();
}
