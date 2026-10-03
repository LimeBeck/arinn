// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "dbussessionlifecycle.h"
#include <QDBusError>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QDBusServiceWatcher>
#include <array>
namespace {
struct LockEndpoint {
  const char *service;
  const char *path;
  const char *interface;
};
constexpr std::array endpoints{
    LockEndpoint{"org.freedesktop.ScreenSaver", "/org/freedesktop/ScreenSaver",
                 "org.freedesktop.ScreenSaver"},
    LockEndpoint{"org.freedesktop.ScreenSaver", "/ScreenSaver",
                 "org.freedesktop.ScreenSaver"},
    LockEndpoint{"org.xfce.ScreenSaver", "/org/xfce/ScreenSaver",
                 "org.xfce.ScreenSaver"}};
} // namespace
DbusSessionLifecycle::DbusSessionLifecycle(QObject *parent,
                                           QDBusConnection session,
                                           QDBusConnection system)
    : SessionLifecycle(parent), m_session(session) {
  for (const auto &endpoint : endpoints)
    session.connect(endpoint.service, endpoint.path, endpoint.interface,
                    "ActiveChanged", this,
                    SLOT(lockChanged(bool, QDBusMessage)));
  system.connect("org.freedesktop.login1", "/org/freedesktop/login1",
                 "org.freedesktop.login1.Manager", "PrepareForSleep", this,
                 SLOT(prepareForSleep(bool)));
  auto watcher =
      new QDBusServiceWatcher("org.freedesktop.ScreenSaver", session,
                              QDBusServiceWatcher::WatchForOwnerChange, this);
  watcher->addWatchedService("org.xfce.ScreenSaver");
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this, [this] {
    m_locked = true;
    emit changed();
    refresh();
  });
  refresh();
}
void DbusSessionLifecycle::refresh() { probe(0, ++m_epoch); }
void DbusSessionLifecycle::probe(int index, quint64 epoch) {
  if (epoch != m_epoch)
    return;
  if (index >= static_cast<int>(endpoints.size())) {
    m_endpoint = -1;
    m_locked = true;
    emit changed();
    return;
  }
  m_endpoint = index;
  const auto &endpoint = endpoints[index];
  auto msg = QDBusMessage::createMethodCall(endpoint.service, endpoint.path,
                                            endpoint.interface, "GetActive");
  msg.setAutoStartService(false);
  auto pending =
      new QDBusPendingCallWatcher(m_session.asyncCall(msg, 1500), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch, index](auto *call) {
            QDBusPendingReply<bool> reply = *call;
            call->deleteLater();
            if (epoch != m_epoch)
              return;
            if (reply.isError()) {
              const auto error = reply.error().type();
              if (reply.error().name() ==
                      "org.freedesktop.DBus.Error.NameHasNoOwner" ||
                  error == QDBusError::ServiceUnknown ||
                  error == QDBusError::UnknownObject ||
                  error == QDBusError::UnknownInterface ||
                  error == QDBusError::UnknownMethod) {
                probe(index + 1, epoch);
                return;
              }
              // Timeout or malformed state cannot be bypassed with a weaker
              // answer.
              m_locked = true;
            } else {
              m_locked = reply.value();
            }
            emit changed();
          });
}
void DbusSessionLifecycle::lockChanged(bool locked,
                                       const QDBusMessage &message) {
  if (m_endpoint < 0 || message.path() != endpoints[m_endpoint].path ||
      message.interface() != endpoints[m_endpoint].interface)
    return;
  ++m_epoch; // a pending old GetActive must not undo this signal
  m_locked = locked;
  emit changed();
}
void DbusSessionLifecycle::prepareForSleep(bool sleeping) {
  ++m_epoch;
  m_sleeping = sleeping;
  m_locked = true;
  emit changed();
  if (!sleeping)
    refresh();
}

SessionLifecycle *createSessionLifecycle(QObject *parent) {
  return new DbusSessionLifecycle(parent);
}
